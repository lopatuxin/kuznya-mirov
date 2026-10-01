//! «Рельеф»: высота земли трёхмерной сцены. Точки высот лежат через полклетки или через четверть
//! клетки, крайние на краях сцены; между ними земля — плоские треугольники, по два на квадрат из четырёх соседних точек,
//! диагональ идёт от левой верхней точки к правой нижней. Высоты, ход, луч мыши и отрисовка считают
//! одну и ту же поверхность.

use std::sync::OnceLock;

use super::footprint::Footprint;
use super::math3::{self, Vec3};
use super::mountains::Mountain;
use super::value::Vec2;

/// Тангенс самого крутого склона, по которому идут: 45°.
pub const MAX_SLOPE: f64 = 1.0;

const EPS: f64 = 1e-9;

/// Вода: ровная гладь на высоте `level` над всей сценой.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Water {
    pub level: f64,
    pub color: [f32; 4],
}

/// Выпуклый многоугольник на плоскости `x`, `y` против часовой стрелки (в математическом смысле).
pub type Polygon = Vec<Vec2>;

/// Площадка меньше стольких клеток, со всех сторон окружённая кручей или водой, для ходьбы закрыта.
pub const MIN_ISLAND: f64 = 9.0;

/// Сколько точек высот бывает на клетку сцены: через полклетки или через четверть клетки.
pub const DENSITIES: [usize; 2] = [2, 4];

/// Сторона квадрата, которым меряется, где пройти нельзя, — полклетки при любой густоте сетки: сеть
/// обхода строится по углам закрытых мест, и лесенка в четверть клетки раздула бы её вчетверо.
const WALK_SQUARE: f64 = 0.5;

/// Сколько квадратов `WALK_SQUARE` по стороне склеиваются в один многоугольник, если закрыты все.
const MERGED_WALK_SQUARES: usize = 2;

/// Больше слоёв покрытий рельефа не бывает.
pub const MAX_COVERS: usize = 8;

/// Слой покрытия: материал по номеру в `files.materials`, маска по номеру в списке масок файла
/// рельефа и правило крутизны `slope` — градусы, от которых слой набирает силу. У нижнего слоя нет
/// ни маски, ни `slope`, у остальных есть хотя бы одно из двух.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cover {
    pub material: usize,
    pub mask: Option<usize>,
    pub slope: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Terrain {
    scene: [u32; 2],
    /// Точек высот на клетку сцены.
    density: usize,
    /// Итоговые высоты: `base` плюс наибольшая из гор в точке.
    heights: Vec<f64>,
    /// Высоты файла рельефа, без гор.
    base: Vec<f64>,
    mountains: Vec<Mountain>,
    water: Option<Water>,
    covers: Vec<Cover>,
    /// Есть ли карта цвета `tint`: она идёт после масок покрытий.
    tint: bool,
    flat: bool,
    blocked: OnceLock<Vec<Polygon>>,
}

/// Ровная земля высоты 0 без воды — для сцены без файла рельефа.
static FLAT: Terrain = Terrain {
    scene: [0, 0],
    density: 2,
    heights: Vec::new(),
    base: Vec::new(),
    mountains: Vec::new(),
    water: None,
    covers: Vec::new(),
    tint: false,
    flat: true,
    blocked: OnceLock::new(),
};

impl Terrain {
    pub fn flat() -> &'static Terrain {
        &FLAT
    }

    /// Точек на клетку у сетки из `rows` строк при высоте сцены `height` клеток: строк
    /// `d × высота + 1` при `d` из `DENSITIES`; `None`, если такого `d` нет.
    pub fn density_for(rows: usize, height: u32) -> Option<usize> {
        DENSITIES
            .into_iter()
            .find(|d| d * height as usize + 1 == rows)
    }

    /// Какие числа строк годятся при высоте сцены `height` клеток — для текста ошибки.
    pub fn allowed_rows(height: u32) -> String {
        DENSITIES
            .iter()
            .map(|d| format!("{} при {d} точках на клетку", d * height as usize + 1))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Рельеф сцены `scene` (клеток) из строк высот сверху вниз: строк `d × высота + 1`, чисел в
    /// строке `d × ширина + 1`, где `d` — точек на клетку из `DENSITIES`.
    pub fn from_rows(
        scene: [u32; 2],
        rows: &[Vec<f64>],
        water: Option<Water>,
    ) -> Result<Terrain, String> {
        let Some(density) = Terrain::density_for(rows.len(), scene[1]) else {
            return Err(format!(
                "heights: {} строк, а по высоте сцены {} клеток нужно {}",
                rows.len(),
                scene[1],
                Terrain::allowed_rows(scene[1])
            ));
        };
        let want_cols = density * scene[0] as usize + 1;
        if let Some((index, row)) = rows.iter().enumerate().find(|(_, r)| r.len() != want_cols) {
            return Err(format!(
                "heights[{index}]: {} чисел, а при {density} точках на клетку (строк {}) нужно {density} × {} + 1 = {want_cols}",
                row.len(),
                rows.len(),
                scene[0]
            ));
        }
        let heights: Vec<f64> = rows.iter().flatten().copied().collect();
        Ok(Terrain {
            scene,
            density,
            flat: heights.iter().all(|&h| h == 0.0),
            base: heights.clone(),
            heights,
            mountains: Vec::new(),
            water,
            covers: Vec::new(),
            tint: false,
            blocked: OnceLock::new(),
        })
    }

    /// Тот же рельеф под покрытиями `covers` и картой цвета, если `tint`: мазок кисти и правка воды
    /// меняют высоты, покрытия и карта цвета остаются.
    pub fn with_covers(mut self, covers: Vec<Cover>, tint: bool) -> Terrain {
        self.covers = covers;
        self.tint = tint;
        self
    }

    /// Тот же рельеф с горами `mountains`: итоговые высоты — `heights` файла плюс наибольшая из гор
    /// в точке сетки; считаются только точки под прямоугольниками гор.
    pub fn with_mountains(mut self, mountains: Vec<Mountain>) -> Terrain {
        let mut lift = vec![0.0; self.base.len()];
        for mountain in &mountains {
            self.lift_under(mountain, &mut lift);
        }
        self.heights = self.base.iter().zip(&lift).map(|(h, up)| h + up).collect();
        self.flat = self.heights.iter().all(|&h| h == 0.0);
        self.mountains = mountains;
        self
    }

    /// Поднимает `lift` в точках сетки под горой до её высоты, если та выше уже записанной.
    fn lift_under(&self, mountain: &Mountain, lift: &mut [f64]) {
        let [low, high] = mountain.bounds();
        let [columns, rows] = self.squares().map(|squares| squares + 1);
        for row in self.lattice_span(low[1], high[1], rows) {
            for column in self.lattice_span(low[0], high[0], columns) {
                let height = mountain.height_at(self.point_place(column, row));
                let slot = &mut lift[row * columns + column];
                *slot = slot.max(height);
            }
        }
    }

    /// Точки сетки из `count` по одной оси, что лежат на отрезке сцены от `low` до `high`.
    fn lattice_span(&self, low: f64, high: f64, count: usize) -> std::ops::Range<usize> {
        let d = self.density as f64;
        let first = (low * d - EPS).ceil().max(0.0) as usize;
        let last = ((high * d + EPS).floor() + 1.0).clamp(0.0, count as f64) as usize;
        first..last
    }

    /// Горы файла рельефа, в порядке записи.
    pub fn mountains(&self) -> &[Mountain] {
        &self.mountains
    }

    /// Номер горы с наибольшей высотой больше нуля в месте сцены; из равных — меньший.
    pub fn mountain_at(&self, x: f64, y: f64) -> Option<usize> {
        let mut best: Option<(usize, f64)> = None;
        for (index, mountain) in self.mountains.iter().enumerate() {
            let height = mountain.height_at([x, y]);
            if height > 0.0 && best.is_none_or(|(_, top)| height > top) {
                best = Some((index, height));
            }
        }
        best.map(|(index, _)| index)
    }

    /// Слои покрытий снизу вверх; пусто, пока файл рельефа не назвал их.
    pub fn covers(&self) -> &[Cover] {
        &self.covers
    }

    /// Названа ли в файле рельефа карта цвета `tint`.
    pub fn has_tint(&self) -> bool {
        self.tint
    }

    /// Точек высот на клетку сцены.
    pub fn density(&self) -> usize {
        self.density
    }

    /// Точек по ширине (`d × ширина + 1`); 0 у ровной земли без файла.
    pub fn columns(&self) -> usize {
        if self.heights.is_empty() {
            0
        } else {
            self.density * self.scene[0] as usize + 1
        }
    }

    pub fn rows(&self) -> usize {
        if self.heights.is_empty() {
            0
        } else {
            self.density * self.scene[1] as usize + 1
        }
    }

    /// Место сцены точки сетки `(column, row)`: `(column / d, row / d)`.
    pub fn point_place(&self, column: usize, row: usize) -> Vec2 {
        let d = self.density as f64;
        [column as f64 / d, row as f64 / d]
    }

    /// Высота точки сетки `(column, row)`; точка лежит в месте сцены `point_place`.
    pub fn point_height(&self, column: usize, row: usize) -> f64 {
        self.heights
            .get(row * self.columns() + column)
            .copied()
            .unwrap_or(0.0)
    }

    pub fn water(&self) -> Option<Water> {
        self.water
    }

    /// Итоговые высоты точек сетки с горами, строками сверху вниз; пусто у ровной земли без файла.
    pub fn heights(&self) -> &[f64] {
        &self.heights
    }

    /// Высоты точек сетки из файла рельефа, без гор; пусто у ровной земли без файла.
    pub fn base_heights(&self) -> &[f64] {
        &self.base
    }

    /// Вся земля на высоте 0.
    pub fn is_flat(&self) -> bool {
        self.flat
    }

    /// Земля ровная на нуле и воды нет — сцена ходит и выглядит, как без рельефа.
    pub fn is_trivial(&self) -> bool {
        self.water.is_none() && self.is_flat()
    }

    /// Самая низкая и самая высокая точка земли.
    pub fn height_range(&self) -> (f64, f64) {
        if self.heights.is_empty() {
            return (0.0, 0.0);
        }
        self.heights
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), &h| {
                (lo.min(h), hi.max(h))
            })
    }

    fn clamp_point(&self, x: f64, y: f64) -> Vec2 {
        [
            x.clamp(0.0, self.scene[0] as f64),
            y.clamp(0.0, self.scene[1] as f64),
        ]
    }

    /// Квадрат сетки, в котором лежит точка (уже внутри сцены), и доля пути по каждой оси.
    fn locate(&self, x: f64, y: f64) -> (usize, usize, f64, f64) {
        let squares = self.squares();
        let d = self.density as f64;
        let (gx, gy) = (x * d, y * d);
        let column = (gx.floor().max(0.0) as usize).min(squares[0].saturating_sub(1));
        let row = (gy.floor().max(0.0) as usize).min(squares[1].saturating_sub(1));
        (column, row, gx - column as f64, gy - row as f64)
    }

    /// Высота земли в точке; за краем сцены — высота ближайшей точки края.
    pub fn height_at(&self, x: f64, y: f64) -> f64 {
        if self.heights.is_empty() {
            return 0.0;
        }
        let [x, y] = self.clamp_point(x, y);
        let (column, row, u, v) = self.locate(x, y);
        let h = |dc: usize, dr: usize| self.point_height(column + dc, row + dr);
        let base = h(0, 0);
        if u >= v {
            base + u * (h(1, 0) - base) + v * (h(1, 1) - h(1, 0))
        } else {
            base + v * (h(0, 1) - base) + u * (h(1, 1) - h(0, 1))
        }
    }

    /// Самая низкая и самая высокая точка земли под прямоугольником: экстремум кусочно-линейной
    /// поверхности лежит в углу прямоугольника, в точке сетки внутри него или в пересечении его
    /// стороны с ребром треугольника.
    pub fn range_under(&self, footprint: &Footprint) -> (f64, f64) {
        if self.heights.is_empty() {
            return (0.0, 0.0);
        }
        let mut range = (f64::INFINITY, f64::NEG_INFINITY);
        let mut take = |point: Vec2| {
            let h = self.height_at(point[0], point[1]);
            range = (range.0.min(h), range.1.max(h));
        };
        let corners = footprint.corners();
        corners.iter().for_each(|&c| take(c));
        for point in self.lattice_points_in(&corners) {
            take(point);
        }
        for index in 0..4 {
            let (a, b) = (corners[index], corners[(index + 1) % 4]);
            for point in self.edge_breaks(a, b) {
                take(point);
            }
        }
        range
    }

    pub fn min_under(&self, footprint: &Footprint) -> f64 {
        self.range_under(footprint).0
    }

    pub fn max_under(&self, footprint: &Footprint) -> f64 {
        self.range_under(footprint).1
    }

    /// Точки сетки внутри выпуклого четырёхугольника (границы включены).
    fn lattice_points_in(&self, corners: &[Vec2; 4]) -> Vec<Vec2> {
        let squares = self.squares().map(|n| n as i64);
        let d = self.density as f64;
        let low = corners
            .iter()
            .fold([f64::INFINITY; 2], |m, c| [m[0].min(c[0]), m[1].min(c[1])]);
        let high = corners.iter().fold([f64::NEG_INFINITY; 2], |m, c| {
            [m[0].max(c[0]), m[1].max(c[1])]
        });
        let range = |low: f64, high: f64, max: i64| {
            let first = ((low * d - EPS).ceil() as i64).max(0);
            let last = ((high * d + EPS).floor() as i64).min(max);
            first..=last
        };
        let mut points = Vec::new();
        for row in range(low[1], high[1], squares[1]) {
            for column in range(low[0], high[0], squares[0]) {
                let point = [column as f64 / d, row as f64 / d];
                if inside_closed(corners, point) {
                    points.push(point);
                }
            }
        }
        points
    }

    /// Пересечения отрезка `a → b` с линиями, вдоль которых излом поверхности: `x = k / d`,
    /// `y = k / d` и диагоналями `x − y = k / d`.
    fn edge_breaks(&self, a: Vec2, b: Vec2) -> Vec<Vec2> {
        let squares = self.squares().map(|n| n as i64);
        let d = self.density as f64;
        let mut points = Vec::new();
        let mut lines = |start: f64, delta: f64, min: i64, max: i64| {
            if delta.abs() < EPS {
                return;
            }
            let (from, to) = (start.min(start + delta), start.max(start + delta));
            let first = ((from * d - EPS).ceil() as i64).max(min);
            let last = ((to * d + EPS).floor() as i64).min(max);
            for k in first..=last {
                let t = (k as f64 / d - start) / delta;
                if (-EPS..=1.0 + EPS).contains(&t) {
                    let t = t.clamp(0.0, 1.0);
                    points.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
                }
            }
        };
        lines(a[0], b[0] - a[0], 0, squares[0]);
        lines(a[1], b[1] - a[1], 0, squares[1]);
        lines(
            a[0] - a[1],
            (b[0] - a[0]) - (b[1] - a[1]),
            -squares[1],
            squares[0],
        );
        points
    }

    /// Сдвиги точек треугольника от левой верхней точки квадрата: `which` — 0 над диагональю (точки
    /// левая верхняя, правая верхняя, правая нижняя), 1 под ней (левая верхняя, правая нижняя, левая
    /// нижняя).
    fn corner_offsets(which: usize) -> [(usize, usize); 3] {
        if which == 0 {
            [(0, 0), (1, 0), (1, 1)]
        } else {
            [(0, 0), (1, 1), (0, 1)]
        }
    }

    /// Вершины треугольника `which` квадрата `(column, row)`.
    pub fn triangle(&self, column: usize, row: usize, which: usize) -> [[f64; 3]; 3] {
        Terrain::corner_offsets(which).map(|(dc, dr)| {
            let [x, y] = self.point_place(column + dc, row + dr);
            [x, y, self.point_height(column + dc, row + dr)]
        })
    }

    /// Нормали освещения вершин треугольника `which` квадрата `(column, row)` в порядке `triangle`.
    pub fn triangle_normals(&self, column: usize, row: usize, which: usize) -> [Vec3; 3] {
        Terrain::corner_offsets(which).map(|(dc, dr)| self.point_normal(column + dc, row + dr))
    }

    /// Нормаль освещения точки сетки `(column, row)` по наклону между её соседями слева и справа,
    /// сверху и снизу, на краю сцены — между точкой и соседом внутри. У соседних треугольников в общей
    /// точке она одна, поэтому свет по склону меняется плавно, а не скачком на ребре.
    fn point_normal(&self, column: usize, row: usize) -> Vec3 {
        let (left, right) = (
            column.saturating_sub(1),
            (column + 1).min(self.columns() - 1),
        );
        let (top, bottom) = (row.saturating_sub(1), (row + 1).min(self.rows() - 1));
        let d = self.density as f64;
        let along_x = (self.point_height(right, row) - self.point_height(left, row)) * d
            / (right - left) as f64;
        let along_y = (self.point_height(column, bottom) - self.point_height(column, top)) * d
            / (bottom - top) as f64;
        math3::normalize([-along_x, -along_y, 1.0])
    }

    /// Нормаль освещения в месте сцены: нормали точек треугольника, смешанные так же, как высоты в
    /// `height_at`; без файла высот — вверх.
    pub fn normal_at(&self, x: f64, y: f64) -> Vec3 {
        if self.heights.is_empty() {
            return [0.0, 0.0, 1.0];
        }
        let [x, y] = self.clamp_point(x, y);
        let (column, row, u, v) = self.locate(x, y);
        let n = |dc: usize, dr: usize| self.point_normal(column + dc, row + dr);
        let parts = if u >= v {
            [(n(0, 0), 1.0 - u), (n(1, 0), u - v), (n(1, 1), v)]
        } else {
            [(n(0, 0), 1.0 - v), (n(0, 1), v - u), (n(1, 1), u)]
        };
        let sum = parts.iter().fold([0.0; 3], |sum, &(normal, weight)| {
            math3::add(sum, math3::scale(normal, weight))
        });
        math3::normalize(sum)
    }

    /// Квадратов сетки по ширине и по высоте (`d × размер`); 0, пока рельефа нет.
    pub fn squares(&self) -> [usize; 2] {
        if self.heights.is_empty() {
            [0, 0]
        } else {
            [
                self.density * self.scene[0] as usize,
                self.density * self.scene[1] as usize,
            ]
        }
    }

    /// Квадраты сетки из `count` по одной оси, которые задевает отрезок сцены от `low` до `high`.
    pub fn square_span(&self, low: f64, high: f64, count: usize) -> std::ops::Range<usize> {
        let d = self.density as f64;
        let first = ((low * d).floor().max(0.0) as usize).min(count);
        let last = ((high * d).ceil().max(0.0) as usize).min(count);
        first..last
    }

    /// Единичная нормаль треугольника, смотрящая вверх: `x` вправо, `y` к игроку, `z` вверх. По ней
    /// считается крутизна склона для ходьбы; свет берёт нормали точек (`point_normal`).
    pub fn normal(triangle: &[[f64; 3]; 3]) -> Vec3 {
        let (ex, ey) = (
            [
                triangle[1][0] - triangle[0][0],
                triangle[1][1] - triangle[0][1],
                triangle[1][2] - triangle[0][2],
            ],
            [
                triangle[2][0] - triangle[0][0],
                triangle[2][1] - triangle[0][1],
                triangle[2][2] - triangle[0][2],
            ],
        );
        let mut normal = math3::normalize(math3::cross(ex, ey));
        if normal[2] < 0.0 {
            normal = math3::scale(normal, -1.0);
        }
        normal
    }

    /// Тангенс угла наклона треугольника.
    pub fn slope(triangle: &[[f64; 3]; 3]) -> f64 {
        let normal = Terrain::normal(triangle);
        (normal[0] * normal[0] + normal[1] * normal[1]).sqrt() / normal[2].max(EPS)
    }

    /// Куда по рельефу не пройти — выпуклые многоугольники: квадраты в `WALK_SQUARE` клетки, где есть
    /// треугольник круче 45° или вся земля ниже воды; части треугольников ниже воды в остальных
    /// квадратах; площадки меньше `MIN_ISLAND` клеток, со всех сторон
    /// окружённые такими квадратами, — туда не дойти, а углы вокруг них раздули бы сеть обхода.
    /// Считаются один раз.
    pub fn walk_blocked(&self) -> &[Polygon] {
        self.blocked.get_or_init(|| {
            let level = self.water.map(|w| w.level);
            let [columns, rows] = self.squares();
            let mut shut: Vec<bool> = (0..columns * rows)
                .map(|at| self.square_shut(at % columns, at / columns, level))
                .collect();
            self.widen_to_walk_squares(&mut shut, columns, rows);
            self.shut_islands(&mut shut, columns, rows);
            let mut pieces = Vec::new();
            if let Some(level) = level {
                for (at, _) in shut.iter().enumerate().filter(|(_, shut)| !**shut) {
                    for which in 0..2 {
                        let triangle = self.triangle(at % columns, at / columns, which);
                        if let Some(piece) = clip_below(&triangle, level) {
                            pieces.push(piece);
                        }
                    }
                }
            }
            pieces.extend(self.walk_squares(&shut, columns, rows));
            pieces
        })
    }

    /// Квадрат сетки закрыт для ходьбы: в нём есть треугольник круче 45° или вся земля ниже воды.
    fn square_shut(&self, column: usize, row: usize, level: Option<f64>) -> bool {
        let drowned = level.is_some_and(|level| {
            [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .all(|&(dc, dr)| self.point_height(column + dc, row + dr) < level)
        });
        drowned
            || (0..2)
                .any(|which| Terrain::slope(&self.triangle(column, row, which)) > MAX_SLOPE + EPS)
    }

    /// Квадрат сетки закрыт, если закрыт хоть один квадрат сетки в его квадрате `WALK_SQUARE`.
    fn widen_to_walk_squares(&self, shut: &mut [bool], columns: usize, rows: usize) {
        let step = self.walk_step();
        if step == 1 {
            return;
        }
        for top in (0..rows).step_by(step) {
            for left in (0..columns).step_by(step) {
                let block = || {
                    (top..(top + step).min(rows)).flat_map(move |row| {
                        (left..(left + step).min(columns)).map(move |column| row * columns + column)
                    })
                };
                if block().any(|at| shut[at]) {
                    block().for_each(|at| shut[at] = true);
                }
            }
        }
    }

    /// Закрывает площадки открытых квадратов `shut`, что меньше `MIN_ISLAND` клеток и не касаются
    /// края сцены: со всех сторон их обступают закрытые квадраты.
    fn shut_islands(&self, shut: &mut [bool], columns: usize, rows: usize) {
        let limit = (MIN_ISLAND * (self.density * self.density) as f64) as usize;
        let mut seen = vec![false; shut.len()];
        let mut stack = Vec::new();
        let mut region = Vec::new();
        for start in 0..shut.len() {
            if shut[start] || seen[start] {
                continue;
            }
            region.clear();
            let mut edge = false;
            seen[start] = true;
            stack.push(start);
            while let Some(at) = stack.pop() {
                region.push(at);
                let (column, row) = (at % columns, at / columns);
                edge |= column == 0 || row == 0 || column + 1 == columns || row + 1 == rows;
                let neighbours = [
                    (column > 0).then(|| at - 1),
                    (column + 1 < columns).then(|| at + 1),
                    (row > 0).then(|| at - columns),
                    (row + 1 < rows).then(|| at + columns),
                ];
                for next in neighbours.into_iter().flatten() {
                    if !shut[next] && !seen[next] {
                        seen[next] = true;
                        stack.push(next);
                    }
                }
            }
            if !edge && region.len() < limit {
                region.iter().for_each(|&at| shut[at] = true);
            }
        }
    }

    /// Закрытые места `shut` (`columns × rows` квадратов сетки, строками) многоугольниками: квадрат
    /// `MERGED_WALK_SQUARES` × `MERGED_WALK_SQUARES` квадратов `WALK_SQUARE`, закрытый целиком, — одним
    /// многоугольником, иначе — каждый закрытый квадрат `WALK_SQUARE` своим. Углов в горах меньше, а
    /// вдоль кромки они стоят не реже, чем через клетку, и путь к ближайшему месту у недостижимой цели
    /// не теряет точности.
    fn walk_squares(&self, shut: &[bool], columns: usize, rows: usize) -> Vec<Polygon> {
        let step = self.walk_step();
        let big = step * MERGED_WALK_SQUARES;
        let is_shut = |left: usize, top: usize| shut[top * columns + left];
        let square = |left: usize, top: usize, side: usize| {
            let (right, bottom) = ((left + side).min(columns), (top + side).min(rows));
            vec![
                self.point_place(left, top),
                self.point_place(right, top),
                self.point_place(right, bottom),
                self.point_place(left, bottom),
            ]
        };
        let mut squares = Vec::new();
        for top in (0..rows).step_by(big) {
            for left in (0..columns).step_by(big) {
                let parts: Vec<(usize, usize)> = (top..(top + big).min(rows))
                    .step_by(step)
                    .flat_map(|row| {
                        (left..(left + big).min(columns))
                            .step_by(step)
                            .map(move |column| (column, row))
                    })
                    .collect();
                let whole = parts.len() == MERGED_WALK_SQUARES * MERGED_WALK_SQUARES;
                if whole && parts.iter().all(|&(column, row)| is_shut(column, row)) {
                    squares.push(square(left, top, big));
                } else {
                    for &(column, row) in parts.iter().filter(|&&(c, r)| is_shut(c, r)) {
                        squares.push(square(column, row, step));
                    }
                }
            }
        }
        squares
    }

    /// Квадратов сетки в стороне квадрата `WALK_SQUARE`.
    fn walk_step(&self) -> usize {
        ((WALK_SQUARE * self.density as f64).round() as usize).max(1)
    }

    /// Части треугольников, где земля ниже `level`, в прямоугольнике `area` (`[низ, верх]`).
    pub fn pieces_below(&self, level: f64, area: [Vec2; 2]) -> Vec<Polygon> {
        let [columns, rows] = self.squares();
        if columns == 0 {
            return Vec::new();
        }
        let mut pieces = Vec::new();
        for row in self.square_span(area[0][1], area[1][1], rows) {
            for column in self.square_span(area[0][0], area[1][0], columns) {
                for which in 0..2 {
                    if let Some(piece) = clip_below(&self.triangle(column, row, which), level) {
                        pieces.push(piece);
                    }
                }
            }
        }
        pieces
    }

    /// Расстояние вдоль луча `origin + t · direction` до первой точки рельефа в пределах сцены.
    pub fn ray_hit(&self, extent: [f64; 2], origin: Vec3, direction: Vec3) -> Option<f64> {
        let [columns, rows] = self.squares();
        if columns == 0 {
            return plane_hit(origin, direction, 0.0, extent);
        }
        let (lo, hi) = self.height_range();
        let (t_enter, t_exit) = clip_ray_to_box(origin, direction, extent, lo, hi)?;
        let mut best: Option<f64> = None;
        let start = [
            origin[0] + t_enter * direction[0],
            origin[1] + t_enter * direction[1],
        ];
        let d = self.density as f64;
        let mut cell = [
            ((start[0] * d).floor().max(0.0) as i64).min(columns as i64 - 1),
            ((start[1] * d).floor().max(0.0) as i64).min(rows as i64 - 1),
        ];
        let step = [
            if direction[0] >= 0.0 { 1 } else { -1 },
            if direction[1] >= 0.0 { 1 } else { -1 },
        ];
        let next_boundary = |axis: usize, cell: i64| {
            let edge = if step[axis] > 0 { cell + 1 } else { cell };
            edge as f64 / d
        };
        let mut t_max = [f64::INFINITY; 2];
        let mut t_delta = [f64::INFINITY; 2];
        for axis in 0..2 {
            if direction[axis].abs() > EPS {
                t_max[axis] = (next_boundary(axis, cell[axis]) - origin[axis]) / direction[axis];
                t_delta[axis] = 1.0 / (d * direction[axis].abs());
            }
        }
        loop {
            for which in 0..2 {
                let triangle = self.triangle(cell[0] as usize, cell[1] as usize, which);
                if let Some(t) = triangle_hit(&triangle, origin, direction) {
                    best = Some(best.map_or(t, |b: f64| b.min(t)));
                }
            }
            if best.is_some() {
                return best;
            }
            let axis = if t_max[0] < t_max[1] { 0 } else { 1 };
            if t_max[axis] > t_exit {
                return None;
            }
            cell[axis] += step[axis];
            t_max[axis] += t_delta[axis];
            if cell[0] < 0 || cell[1] < 0 || cell[0] >= columns as i64 || cell[1] >= rows as i64 {
                return None;
            }
        }
    }

    /// Расстояние вдоль луча до глади воды там, где вода видна: над землёй ниже её уровня и в
    /// пределах сцены.
    pub fn water_hit(&self, origin: Vec3, direction: Vec3) -> Option<f64> {
        let water = self.water?;
        let extent = [self.scene[0] as f64, self.scene[1] as f64];
        let t = plane_hit(origin, direction, water.level, extent)?;
        let x = origin[0] + t * direction[0];
        let y = origin[1] + t * direction[1];
        (self.height_at(x, y) < water.level).then_some(t)
    }
}

/// Внутри выпуклого четырёхугольника, границы включены.
fn inside_closed(corners: &[Vec2; 4], point: Vec2) -> bool {
    let mut sign = 0.0_f64;
    for index in 0..4 {
        let (a, b) = (corners[index], corners[(index + 1) % 4]);
        let cross = (b[0] - a[0]) * (point[1] - a[1]) - (b[1] - a[1]) * (point[0] - a[0]);
        let length = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2))
            .sqrt()
            .max(EPS);
        let cross = cross / length;
        if cross.abs() <= 1e-7 {
            continue;
        }
        if sign == 0.0 {
            sign = cross.signum();
        } else if sign != cross.signum() {
            return false;
        }
    }
    true
}

/// Часть треугольника, где земля ниже `level`; `None`, если такой части нет или она вырождена.
pub fn clip_below(triangle: &[[f64; 3]; 3], level: f64) -> Option<Polygon> {
    let mut polygon: Polygon = Vec::with_capacity(4);
    for index in 0..3 {
        let (a, b) = (triangle[index], triangle[(index + 1) % 3]);
        let (a_in, b_in) = (a[2] < level, b[2] < level);
        if a_in {
            polygon.push([a[0], a[1]]);
        }
        if a_in != b_in {
            let t = (level - a[2]) / (b[2] - a[2]);
            polygon.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    (polygon.len() >= 3 && polygon_area(&polygon) > EPS).then(|| ccw(polygon))
}

fn polygon_area(polygon: &[Vec2]) -> f64 {
    let sum: f64 = (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    sum.abs() / 2.0
}

/// Многоугольник против часовой стрелки.
pub fn ccw(mut polygon: Polygon) -> Polygon {
    let signed: f64 = (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if signed < 0.0 {
        polygon.reverse();
    }
    polygon
}

/// Пересечение луча с плоскостью `z = level` внутри `[0, extent]`.
fn plane_hit(origin: Vec3, direction: Vec3, level: f64, extent: [f64; 2]) -> Option<f64> {
    if direction[2].abs() < EPS {
        return None;
    }
    let t = (level - origin[2]) / direction[2];
    if t < 0.0 {
        return None;
    }
    let x = origin[0] + t * direction[0];
    let y = origin[1] + t * direction[1];
    (x >= -EPS && y >= -EPS && x <= extent[0] + EPS && y <= extent[1] + EPS).then_some(t)
}

/// Отрезок луча внутри коробки `[0, extent] × [lo, hi]`.
fn clip_ray_to_box(
    origin: Vec3,
    direction: Vec3,
    extent: [f64; 2],
    lo: f64,
    hi: f64,
) -> Option<(f64, f64)> {
    let bounds = [(0.0, extent[0]), (0.0, extent[1]), (lo, hi)];
    let (mut near, mut far) = (0.0_f64, f64::INFINITY);
    for axis in 0..3 {
        let (min, max) = bounds[axis];
        if direction[axis].abs() < EPS {
            if origin[axis] < min - EPS || origin[axis] > max + EPS {
                return None;
            }
            continue;
        }
        let (t0, t1) = (
            (min - origin[axis]) / direction[axis],
            (max - origin[axis]) / direction[axis],
        );
        near = near.max(t0.min(t1));
        far = far.min(t0.max(t1));
        if near > far {
            return None;
        }
    }
    Some((near, far))
}

/// Пересечение луча с треугольником (Мёллер — Трумбор, обе стороны); `t ≥ 0`.
fn triangle_hit(triangle: &[[f64; 3]; 3], origin: Vec3, direction: Vec3) -> Option<f64> {
    use super::math3::{cross, dot, sub};
    let (e1, e2) = (sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
    let p = cross(direction, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-14 {
        return None;
    }
    let inv = 1.0 / det;
    let s = sub(origin, triangle[0]);
    let u = dot(s, p) * inv;
    if !(-EPS..=1.0 + EPS).contains(&u) {
        return None;
    }
    let q = cross(s, e1);
    let v = dot(direction, q) * inv;
    if v < -EPS || u + v > 1.0 + EPS {
        return None;
    }
    let t = dot(e2, q) * inv;
    (t >= 0.0).then_some(t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ramp() -> Terrain {
        // Сцена 2×1 клетки: точки 5 × 3, высота растёт вдоль x на 0,5 за полклетки.
        let rows = vec![
            vec![0.0, 0.5, 1.0, 1.5, 2.0],
            vec![0.0, 0.5, 1.0, 1.5, 2.0],
            vec![0.0, 0.5, 1.0, 1.5, 2.0],
        ];
        Terrain::from_rows([2, 1], &rows, None).expect("размеры сходятся")
    }

    #[test]
    fn a_ramp_reads_its_grid_points_and_the_space_between_them() {
        let terrain = ramp();
        assert_eq!(terrain.height_at(0.0, 0.0), 0.0);
        assert_eq!(terrain.height_at(2.0, 1.0), 2.0);
        assert!((terrain.height_at(0.75, 0.3) - 0.75 * 1.0).abs() < 1e-12);
        assert_eq!(terrain.height_at(-5.0, 0.5), 0.0, "за краем — по краю");
        assert_eq!(terrain.height_at(9.0, 0.5), 2.0);
    }

    #[test]
    fn the_lowest_and_highest_point_under_a_turned_rectangle_are_found_between_grid_points() {
        let terrain = ramp();
        let rect = Footprint::rotated(
            [0.6, 0.2],
            [0.8, 0.4],
            crate::core::value::Rotation::from_degrees(45.0),
        );
        let (lo, hi) = terrain.range_under(&rect);
        let corners = rect.corners();
        let expected_lo = corners.iter().map(|c| c[0]).fold(f64::INFINITY, f64::min);
        let expected_hi = corners
            .iter()
            .map(|c| c[0])
            .fold(f64::NEG_INFINITY, f64::max);
        assert!((lo - expected_lo).abs() < 1e-9, "{lo} против {expected_lo}");
        assert!((hi - expected_hi).abs() < 1e-9, "{hi} против {expected_hi}");
    }

    #[test]
    fn a_ray_from_above_meets_the_slope_and_a_ray_beside_the_scene_misses() {
        let terrain = ramp();
        let t = terrain
            .ray_hit([2.0, 1.0], [1.0, 0.5, 10.0], [0.0, 0.0, -1.0])
            .expect("попал");
        assert!((t - 9.0).abs() < 1e-9, "{t}");
        assert_eq!(
            terrain.ray_hit([2.0, 1.0], [5.0, 0.5, 10.0], [0.0, 0.0, -1.0]),
            None
        );
    }

    #[test]
    fn on_an_even_ramp_the_light_normal_of_every_point_is_the_normal_of_the_triangles() {
        let terrain = ramp();
        let along = Terrain::normal(&terrain.triangle(0, 0, 0));
        for row in 0..terrain.rows() {
            for column in 0..terrain.columns() {
                let normal = terrain.point_normal(column, row);
                assert!(
                    (0..3).all(|axis| (normal[axis] - along[axis]).abs() < 1e-12),
                    "точка ({column}, {row}): {normal:?} против {along:?}"
                );
            }
        }
    }

    #[test]
    fn across_a_ridge_the_light_normal_changes_smoothly_while_the_triangle_normal_jumps() {
        // Сцена 2×1 клетки: гребень вдоль x = 1, по обе стороны склоны в разные стороны.
        let row = vec![0.0, 0.5, 1.0, 0.5, 0.0];
        let terrain = Terrain::from_rows([2, 1], &vec![row; 3], None).expect("размеры сходятся");
        let (left, right) = (
            Terrain::normal(&terrain.triangle(1, 0, 0)),
            Terrain::normal(&terrain.triangle(2, 0, 1)),
        );
        assert!(left[0] < -0.5 && right[0] > 0.5, "{left:?} {right:?}");
        let (before, after) = (terrain.normal_at(0.999, 0.6), terrain.normal_at(1.001, 0.6));
        assert!(
            (0..3).all(|axis| (before[axis] - after[axis]).abs() < 1e-2),
            "{before:?} {after:?}"
        );
        assert_eq!(
            terrain.normal_at(1.0, 0.5),
            [0.0, 0.0, 1.0],
            "на гребне — вверх"
        );
        let (at_point, of_point) = (terrain.normal_at(0.5, 0.5), terrain.point_normal(1, 1));
        assert!(
            (0..3).all(|axis| (at_point[axis] - of_point[axis]).abs() < 1e-12),
            "в точке сетки — её нормаль: {at_point:?} {of_point:?}"
        );
    }

    #[test]
    fn a_wrong_row_count_is_refused() {
        let rows = vec![vec![0.0; 5]; 2];
        assert!(Terrain::from_rows([2, 1], &rows, None).is_err());
    }
}

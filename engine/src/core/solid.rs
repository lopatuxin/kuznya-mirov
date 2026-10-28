//! «Трёхмерная сцена» → «Здания»: сетка столбов твёрдой модели. Треугольники модели раскладываются по плану на
//! столбики густотой `CELL` в осях модели, как вокселизация Recast: у каждого столбика — занятые отрезки высоты и
//! для каждого верх, годный как пол (грань не круче 30°). Закрытая сетка заполняется между нижней и верхней
//! гранью, незакрытая остаётся поверхностью. Ни мира, ни видеокарты: место здания в мире — `core::building`.

use super::model::Part;

/// Густота сетки: сторона столбика в единицах модели.
pub const CELL: f32 = 0.1;

/// Столбиков вдоль большей стороны модели не больше этого: большая модель получает столбики крупнее `CELL`.
const MAX_ACROSS: f32 = 1024.0;

/// Косинус 30°: грань с вертикалью нормали не меньше него годится как пол.
const WALKABLE_NORMAL: f32 = 0.866_025_4;

/// Куски горизонтальной грани уже этой доли столбика — касание краем, а не заход в него.
const EDGE_SLACK: f32 = 1e-3;

/// Допуск на включение точки в треугольник при заполнении между гранями, доли единицы модели.
const INSIDE_SLACK: f64 = 1e-9;

/// Две грани с одной стороной ближе этого — одна и та же (луч прошёл по ребру двух треугольников).
const SAME_FACE: f32 = 1e-4;

/// Пересечение столбика с ячейкой прямоугольника глубже этого считается пересечением, касание — нет.
const TOUCH: f64 = 1e-5;

/// Занятый отрезок высоты столбика.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub low: f32,
    pub high: f32,
    /// Высота пола на верху отрезка: у отрезка, чей верх — грань не круче 30°.
    pub floor: Option<f32>,
}

/// Прямоугольник в осях модели: середина, полуразмеры вдоль своих осей, косинус и синус угла.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalRect {
    pub center: [f64; 2],
    pub half: [f64; 2],
    pub cos: f64,
    pub sin: f64,
}

impl LocalRect {
    /// Прямоугольник по осям `x`, `y` — для столбика под точкой.
    fn reach(&self) -> [f64; 2] {
        [
            self.cos.abs() * self.half[0] + self.sin.abs() * self.half[1],
            self.sin.abs() * self.half[0] + self.cos.abs() * self.half[1],
        ]
    }

    /// Задевает ли прямоугольник квадрат `[x0, x1] × [y0, y1]` глубже `TOUCH`.
    fn overlaps_square(&self, low: [f64; 2], size: f64) -> bool {
        let reach = self.reach();
        for axis in 0..2 {
            let top = (low[axis] + size).min(self.center[axis] + reach[axis]);
            let bottom = low[axis].max(self.center[axis] - reach[axis]);
            if top - bottom <= TOUCH {
                return false;
            }
        }
        let middle = [low[0] + size / 2.0, low[1] + size / 2.0];
        let offset = [middle[0] - self.center[0], middle[1] - self.center[1]];
        let radius = size / 2.0 * (self.cos.abs() + self.sin.abs());
        let along = offset[0] * self.cos + offset[1] * self.sin;
        let across = -offset[0] * self.sin + offset[1] * self.cos;
        self.half[0] + radius - along.abs() > TOUCH && self.half[1] + radius - across.abs() > TOUCH
    }
}

/// Сетка столбов модели: у каждого столбика отрезки высоты по возрастанию.
#[derive(Debug, Clone, PartialEq)]
pub struct SolidGrid {
    cell: f32,
    low: [f32; 2],
    across: [usize; 2],
    /// Начало отрезков каждого столбика в `spans`; на один больше, чем столбиков.
    starts: Vec<u32>,
    spans: Vec<Span>,
}

/// Кусок поверхности в одном столбике: от `low` до `high`; `floor` — верх грани, годной как пол.
struct Piece {
    column: u32,
    low: f32,
    high: f32,
    floor: Option<f32>,
}

/// Кусок столбика до слияния: низ, верх и верх грани-пола.
type Raw = (f32, f32, Option<f32>);

/// Пересечение луча над серединой столбика с гранью: высота и смотрит ли грань вверх.
struct Hit {
    column: u32,
    z: f32,
    up: bool,
}

type Point = [f32; 3];

fn sub(a: Point, b: Point) -> Point {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Point, b: Point) -> Point {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Point, b: Point) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Часть многоугольника по одну сторону плоскости `p[axis] = bound`: с большей стороны, если `keep_above`.
fn clip(polygon: &[Point], axis: usize, bound: f32, keep_above: bool, out: &mut Vec<Point>) {
    out.clear();
    let inside = |p: Point| {
        if keep_above {
            p[axis] >= bound
        } else {
            p[axis] <= bound
        }
    };
    for (at, &current) in polygon.iter().enumerate() {
        let previous = polygon[(at + polygon.len() - 1) % polygon.len()];
        match (inside(previous), inside(current)) {
            (true, true) => out.push(current),
            (true, false) | (false, true) => {
                let t = (bound - previous[axis]) / (current[axis] - previous[axis]);
                out.push(std::array::from_fn(|i| {
                    previous[i] + (current[i] - previous[i]) * t
                }));
                if inside(current) {
                    out.push(current);
                }
            }
            (false, false) => {}
        }
    }
}

struct Voxelizer {
    cell: f32,
    low: [f32; 2],
    across: [usize; 2],
    pieces: Vec<Piece>,
    hits: Vec<Hit>,
    inside: Vec<Point>,
    scratch: Vec<Point>,
}

impl Voxelizer {
    fn square_low(&self, column: [usize; 2]) -> [f32; 2] {
        [
            self.low[0] + column[0] as f32 * self.cell,
            self.low[1] + column[1] as f32 * self.cell,
        ]
    }

    fn column_range(&self, axis: usize, from: f32, to: f32) -> (usize, usize) {
        let index = |value: f32| {
            let at = ((value - self.low[axis]) / self.cell).floor();
            (at.max(0.0) as usize).min(self.across[axis] - 1)
        };
        (index(from), index(to))
    }

    /// Грань стены, что лежит ровно на границе столбиков, достаётся тому, что внутри тела, а не обоим соседям: стена,
    /// выровненная по сетке, не толстеет на столбик. Для этого треугольник с боковой нормалью чуть сдвигается внутрь.
    fn pulled_in(&self, corners: [Point; 3], normal: Point, length: f32) -> [Point; 3] {
        let side = normal[0].hypot(normal[1]);
        if normal[2].abs() >= 0.5 * length || side < 1e-12 {
            return corners;
        }
        let pull = self.cell * 1e-3 / side;
        corners.map(|p| [p[0] - normal[0] * pull, p[1] - normal[1] * pull, p[2]])
    }

    /// Попал ли в столбик кусок треугольника, что лежит в `scratch`: у треугольника с площадью в плане — только если
    /// кусок заходит в столбик глубже погрешности, касание краем не в счёт.
    fn covers(&self, upright: bool) -> bool {
        if !upright {
            return !self.scratch.is_empty();
        }
        let reach = |axis: usize| {
            let values = self.scratch.iter().map(|p| p[axis]);
            let high = values.clone().fold(f32::NEG_INFINITY, f32::max);
            let low = values.fold(f32::INFINITY, f32::min);
            high - low
        };
        reach(0) > self.cell * EDGE_SLACK && reach(1) > self.cell * EDGE_SLACK
    }

    /// Раскладывает один треугольник: куски поверхности по задетым столбикам и пересечения лучей над серединами
    /// столбиков, что внутри него в плане.
    fn triangle(&mut self, corners: [Point; 3], normals: [Point; 3]) {
        let mut normal = cross(sub(corners[1], corners[0]), sub(corners[2], corners[0]));
        let length = dot(normal, normal).sqrt();
        if length < 1e-12 {
            return;
        }
        let outward: Point = std::array::from_fn(|i| normals[0][i] + normals[1][i] + normals[2][i]);
        if dot(normal, outward) < 0.0 {
            normal = normal.map(|c| -c);
        }
        let up = normal[2] / length;
        let walkable = up >= WALKABLE_NORMAL;
        let placed = self.pulled_in(corners, normal, length);

        let range = |axis: usize| {
            let (mut from, mut to) = (f32::INFINITY, f32::NEG_INFINITY);
            for corner in placed {
                from = from.min(corner[axis]);
                to = to.max(corner[axis]);
            }
            self.column_range(axis, from, to)
        };
        let ((x_from, x_to), (y_from, y_to)) = (range(0), range(1));
        let upright = up.abs() > 1e-4;
        for cy in y_from..=y_to {
            for cx in x_from..=x_to {
                let low = self.square_low([cx, cy]);
                let column = (cy * self.across[0] + cx) as u32;
                let upper = [low[0] + self.cell * 0.9999, low[1] + self.cell * 0.9999];
                clip(&placed, 0, low[0], true, &mut self.inside);
                clip(&self.inside, 0, upper[0], false, &mut self.scratch);
                clip(&self.scratch, 1, low[1], true, &mut self.inside);
                clip(&self.inside, 1, upper[1], false, &mut self.scratch);
                if self.covers(upright) {
                    let z_low = self
                        .scratch
                        .iter()
                        .map(|p| p[2])
                        .fold(f32::INFINITY, f32::min);
                    let z_high = self
                        .scratch
                        .iter()
                        .map(|p| p[2])
                        .fold(f32::NEG_INFINITY, f32::max);
                    self.pieces.push(Piece {
                        column,
                        low: z_low,
                        high: z_high,
                        floor: walkable.then_some(z_high),
                    });
                }
                if upright {
                    let middle = [low[0] + self.cell / 2.0, low[1] + self.cell / 2.0];
                    if let Some(z) = height_over(corners, middle) {
                        self.hits.push(Hit {
                            column,
                            z,
                            up: up > 0.0,
                        });
                    }
                }
            }
        }
    }
}

/// Высота треугольника над точкой `at` плана; `None`, если точка вне него.
fn height_over(corners: [Point; 3], at: [f32; 2]) -> Option<f32> {
    let [a, b, c] = corners.map(|p| [f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
    let (px, py) = (f64::from(at[0]), f64::from(at[1]));
    let area = (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]);
    if area.abs() < 1e-12 {
        return None;
    }
    let wa = ((b[0] - px) * (c[1] - py) - (b[1] - py) * (c[0] - px)) / area;
    let wb = ((c[0] - px) * (a[1] - py) - (c[1] - py) * (a[0] - px)) / area;
    let wc = 1.0 - wa - wb;
    (wa >= -INSIDE_SLACK && wb >= -INSIDE_SLACK && wc >= -INSIDE_SLACK)
        .then(|| (wa * a[2] + wb * b[2] + wc * c[2]) as f32)
}

/// Отрезки закрытого объёма над серединой одного столбика: от каждой нижней грани до парной верхней. Пары — скобки:
/// верхняя грань без нижней под ней и нижняя без верхней над ней (оболочка, вывернутая наизнанку) не парятся и
/// не вычёркивают объём соседних оболочек.
fn filled(hits: &mut [Hit], out: &mut Vec<(f32, f32)>) {
    hits.sort_by(|a, b| a.z.total_cmp(&b.z).then(b.up.cmp(&a.up)));
    let mut kept: Vec<&Hit> = Vec::with_capacity(hits.len());
    for hit in hits.iter() {
        let repeat = kept
            .last()
            .is_some_and(|last| last.up == hit.up && (hit.z - last.z).abs() < SAME_FACE);
        if !repeat {
            kept.push(hit);
        }
    }
    let mut open: Vec<f32> = Vec::new();
    for hit in kept {
        if !hit.up {
            open.push(hit.z);
        } else if let Some(start) = open.pop() {
            out.push((start, hit.z));
        }
    }
}

impl SolidGrid {
    /// Сетка модели с частями `parts` и коробкой `size` (середина основания — в начале осей x, y; низ — на нуле).
    pub fn build(parts: &[Part], size: [f64; 3]) -> SolidGrid {
        let extent = [size[0] as f32, size[1] as f32];
        let cell = CELL.max(extent[0].max(extent[1]) / MAX_ACROSS);
        let low = [-extent[0] / 2.0, -extent[1] / 2.0];
        let across = [
            ((extent[0] / cell).ceil() as usize).max(1),
            ((extent[1] / cell).ceil() as usize).max(1),
        ];
        let mut voxelizer = Voxelizer {
            cell,
            low,
            across,
            pieces: Vec::new(),
            hits: Vec::new(),
            inside: Vec::new(),
            scratch: Vec::new(),
        };
        for part in parts {
            for triangle in part.indices.as_chunks::<3>().0 {
                let vertices = triangle.map(|index| part.vertices[index as usize]);
                voxelizer.triangle(
                    vertices.map(|vertex| vertex.position),
                    vertices.map(|vertex| vertex.normal),
                );
            }
        }
        let Voxelizer { pieces, hits, .. } = voxelizer;
        SolidGrid::collect(cell, low, across, pieces, hits)
    }

    fn collect(
        cell: f32,
        low: [f32; 2],
        across: [usize; 2],
        mut pieces: Vec<Piece>,
        mut hits: Vec<Hit>,
    ) -> SolidGrid {
        pieces.sort_by(|a, b| a.column.cmp(&b.column).then(a.low.total_cmp(&b.low)));
        hits.sort_by_key(|hit| hit.column);
        let columns = across[0] * across[1];
        let tolerance = cell * 0.6;
        let mut grid = SolidGrid {
            cell,
            low,
            across,
            starts: Vec::with_capacity(columns + 1),
            spans: Vec::new(),
        };
        let (mut next_piece, mut next_hit) = (0, 0);
        let mut raw: Vec<Raw> = Vec::new();
        let mut fills: Vec<(f32, f32)> = Vec::new();
        for column in 0..columns as u32 {
            grid.starts.push(grid.spans.len() as u32);
            raw.clear();
            while let Some(piece) = pieces
                .get(next_piece)
                .filter(|piece| piece.column == column)
            {
                raw.push((piece.low, piece.high, piece.floor));
                next_piece += 1;
            }
            let hit_end = next_hit
                + hits[next_hit..]
                    .iter()
                    .take_while(|h| h.column == column)
                    .count();
            fills.clear();
            filled(&mut hits[next_hit..hit_end], &mut fills);
            next_hit = hit_end;
            raw.extend(fills.iter().map(|&(low, high)| (low, high, None)));
            raw.sort_by(|a, b| a.0.total_cmp(&b.0));
            grid.merge(&raw, tolerance);
        }
        grid.starts.push(grid.spans.len() as u32);
        grid
    }

    /// Складывает куски одного столбика (по возрастанию низа) в отрезки: пересёкшиеся и соприкоснувшиеся сливаются;
    /// верх отрезка — пол, только если его дала грань не круче 30° на самом верху.
    fn merge(&mut self, raw: &[Raw], tolerance: f32) {
        let mut current: Option<Raw> = None;
        for &(low, high, floor) in raw {
            current = match current {
                Some((start, top, held)) if low <= top + 1e-5 => {
                    let floor = match (held, floor) {
                        (Some(a), Some(b)) => Some(a.max(b)),
                        (a, b) => a.or(b),
                    };
                    Some((start, top.max(high), floor))
                }
                Some(done) => {
                    self.close(done, tolerance);
                    Some((low, high, floor))
                }
                None => Some((low, high, floor)),
            };
        }
        if let Some(done) = current {
            self.close(done, tolerance);
        }
    }

    fn close(&mut self, (low, high, floor): Raw, tolerance: f32) {
        self.spans.push(Span {
            low,
            high,
            floor: floor.filter(|&top| top >= high - tolerance),
        });
    }

    /// Сторона столбика в единицах модели.
    pub fn cell(&self) -> f32 {
        self.cell
    }

    fn column(&self, x: usize, y: usize) -> &[Span] {
        let at = y * self.across[0] + x;
        &self.spans[self.starts[at] as usize..self.starts[at + 1] as usize]
    }

    /// Отрезки столбика под точкой `point` плана модели; пусто вне сетки.
    pub fn spans_at(&self, point: [f64; 2]) -> &[Span] {
        let index = |axis: usize| {
            let at = ((point[axis] - f64::from(self.low[axis])) / f64::from(self.cell)).floor();
            (at >= 0.0 && (at as usize) < self.across[axis]).then_some(at as usize)
        };
        match (index(0), index(1)) {
            (Some(x), Some(y)) => self.column(x, y),
            _ => &[],
        }
    }

    /// Обходит отрезки каждого столбика, что задевает `rect`; `visit` вернул `true` — обход кончен и ответ `true`.
    pub fn any_under(&self, rect: &LocalRect, mut visit: impl FnMut(&[Span]) -> bool) -> bool {
        let reach = rect.reach();
        let cell = f64::from(self.cell);
        let range = |axis: usize| {
            let origin = f64::from(self.low[axis]);
            let from = ((rect.center[axis] - reach[axis] - origin) / cell).floor();
            let to = ((rect.center[axis] + reach[axis] - origin) / cell).floor();
            let last = self.across[axis] as f64 - 1.0;
            (from.max(0.0), to.min(last))
        };
        let ((x_from, x_to), (y_from, y_to)) = (range(0), range(1));
        if x_from > x_to || y_from > y_to {
            return false;
        }
        for y in y_from as usize..=y_to as usize {
            for x in x_from as usize..=x_to as usize {
                let spans = self.column(x, y);
                if spans.is_empty() {
                    continue;
                }
                let corner = [
                    f64::from(self.low[0]) + x as f64 * cell,
                    f64::from(self.low[1]) + y as f64 * cell,
                ];
                if rect.overlaps_square(corner, cell) && visit(spans) {
                    return true;
                }
            }
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::ModelVertex;

    fn vertex(position: Point, normal: Point) -> ModelVertex {
        ModelVertex {
            position,
            normal,
            uv: [0.0; 2],
            tangent: [1.0, 0.0, 0.0, 1.0],
            color: [1.0; 4],
        }
    }

    /// Закрытая коробка `[low, high]` с наружными нормалями; треугольники обходятся против нормали, как после
    /// обмена осей файла.
    fn boxed(low: Point, high: Point) -> Part {
        let mut vertices = Vec::new();
        let mut indices = Vec::new();
        for axis in 0..3 {
            for sign in [-1.0f32, 1.0] {
                let (u, v) = ((axis + 1) % 3, (axis + 2) % 3);
                let at = if sign > 0.0 { high[axis] } else { low[axis] };
                let mut normal = [0.0; 3];
                normal[axis] = sign;
                let base = vertices.len() as u32;
                for (a, b) in [
                    (low[u], low[v]),
                    (high[u], low[v]),
                    (high[u], high[v]),
                    (low[u], high[v]),
                ] {
                    let mut position = [0.0; 3];
                    position[axis] = at;
                    position[u] = a;
                    position[v] = b;
                    vertices.push(vertex(position, normal));
                }
                indices.extend([base, base + 2, base + 1, base, base + 3, base + 2]);
            }
        }
        Part {
            vertices,
            indices,
            material: 0,
        }
    }

    /// Наклонная плита: пол ниже на `rise` по `x` на ширине `width`.
    fn ramp(width: f32, rise: f32) -> Part {
        let corners = [
            [-width / 2.0, -1.0, 0.0],
            [width / 2.0, -1.0, rise],
            [width / 2.0, 1.0, rise],
            [-width / 2.0, 1.0, 0.0],
        ];
        let slope = (-rise).atan2(width);
        let normal = [slope.sin(), 0.0, slope.cos()];
        Part {
            vertices: corners.map(|p| vertex(p, normal)).to_vec(),
            indices: vec![0, 1, 2, 0, 2, 3],
            material: 0,
        }
    }

    fn rect(x: f64, y: f64, half: f64) -> LocalRect {
        LocalRect {
            center: [x, y],
            half: [half, half],
            cos: 1.0,
            sin: 0.0,
        }
    }

    #[test]
    fn a_closed_box_is_one_filled_span_with_a_floor_on_top() {
        let grid = SolidGrid::build(
            &[boxed([-1.0, -1.0, 0.0], [1.0, 1.0, 2.0])],
            [2.0, 2.0, 2.0],
        );
        let spans = grid.spans_at([0.0, 0.0]);
        assert_eq!(spans.len(), 1);
        assert!(spans[0].low.abs() < 1e-5 && (spans[0].high - 2.0).abs() < 1e-5);
        assert_eq!(spans[0].floor, Some(2.0));
        assert!(grid.spans_at([5.0, 0.0]).is_empty());
    }

    #[test]
    fn a_wall_thinner_than_a_cell_still_fills_its_columns_and_a_room_between_walls_stays_empty() {
        let walls = [
            boxed([-2.0, -2.0, 0.0], [2.0, -1.96, 2.0]),
            boxed([-2.0, 1.96, 0.0], [2.0, 2.0, 2.0]),
            boxed([-2.0, -2.0, 0.0], [-1.96, 2.0, 2.0]),
            boxed([1.96, -2.0, 0.0], [2.0, 2.0, 2.0]),
        ];
        let grid = SolidGrid::build(&walls, [4.0, 4.0, 2.0]);
        assert!(grid.spans_at([0.0, 0.0]).is_empty());
        let wall = grid.spans_at([0.0, -1.98]);
        assert_eq!(wall.len(), 1);
        assert!(wall[0].low.abs() < 1e-5 && (wall[0].high - 2.0).abs() < 1e-5);
    }

    #[test]
    fn a_gentle_slope_is_a_floor_and_a_steep_one_is_a_blocker_only() {
        let gentle = SolidGrid::build(&[ramp(4.0, 1.0)], [4.0, 2.0, 1.0]);
        let spans = gentle.spans_at([0.0, 0.0]);
        assert!(spans.iter().any(|span| span.floor.is_some()));
        let steep = SolidGrid::build(&[ramp(2.0, 2.0)], [2.0, 2.0, 2.0]);
        let spans = steep.spans_at([0.0, 0.0]);
        assert!(!spans.is_empty() && spans.iter().all(|span| span.floor.is_none()));
    }

    #[test]
    fn stacked_boxes_that_touch_merge_into_one_span_and_the_floor_is_the_upper_top() {
        let stack = [
            boxed([-1.0, -1.0, 0.0], [1.0, 1.0, 1.0]),
            boxed([-1.0, -1.0, 1.0], [1.0, 1.0, 1.5]),
        ];
        let grid = SolidGrid::build(&stack, [2.0, 2.0, 1.5]);
        let spans = grid.spans_at([0.0, 0.0]);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].floor, Some(1.5));
    }

    #[test]
    fn a_rectangle_meets_only_the_columns_it_overlaps_and_touching_does_not_count() {
        let grid = SolidGrid::build(
            &[boxed([-0.5, -0.5, 0.0], [0.5, 0.5, 1.0])],
            [1.0, 1.0, 1.0],
        );
        let touched = |rect: &LocalRect| grid.any_under(rect, |_| true);
        assert!(touched(&rect(0.0, 0.0, 0.2)));
        let beside = LocalRect {
            center: [0.7, 0.0],
            half: [0.2, 0.2],
            cos: 1.0,
            sin: 0.0,
        };
        assert!(!touched(&beside), "край в край — не задевает");
        assert!(!touched(&rect(3.0, 3.0, 0.2)));
        let turned = LocalRect {
            center: [0.0, 0.0],
            half: [0.1, 0.05],
            cos: std::f64::consts::FRAC_1_SQRT_2,
            sin: std::f64::consts::FRAC_1_SQRT_2,
        };
        assert!(touched(&turned));
    }

    #[test]
    fn a_wall_that_lies_on_cell_edges_takes_exactly_its_own_columns() {
        let walls = [
            boxed([-2.0, -2.0, 0.0], [-1.8, 2.0, 2.0]),
            boxed([1.8, -2.0, 0.0], [2.0, 2.0, 2.0]),
        ];
        let grid = SolidGrid::build(&walls, [4.0, 4.0, 2.0]);
        assert!(!grid.spans_at([-1.95, 0.0]).is_empty());
        assert!(!grid.spans_at([-1.85, 0.0]).is_empty());
        assert!(grid.spans_at([-1.75, 0.0]).is_empty());
        assert!(grid.spans_at([1.75, 0.0]).is_empty());
        let top = grid.spans_at([-1.85, 0.0]);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].floor, Some(2.0), "верх стены в её столбиках — пол");
    }
    #[test]
    fn an_inside_out_shell_does_not_hollow_out_a_normal_shell_in_its_column() {
        let mut inverted = boxed([-1.0, -1.0, 0.0], [1.0, 1.0, 1.0]);
        for vertex in &mut inverted.vertices {
            vertex.normal = vertex.normal.map(|c| -c);
        }
        let normal = boxed([-1.0, -1.0, 2.0], [1.0, 1.0, 3.0]);
        let grid = SolidGrid::build(&[inverted, normal], [2.0, 2.0, 3.0]);
        let spans = grid.spans_at([0.0, 0.0]);
        assert!(
            spans
                .iter()
                .any(|span| (span.low - 2.0).abs() < 1e-5 && (span.high - 3.0).abs() < 1e-5),
            "{spans:?}"
        );
        assert!(
            spans
                .iter()
                .all(|span| span.high <= 1.0 + 1e-5 || span.low >= 2.0 - 1e-5),
            "и между оболочками пусто: {spans:?}"
        );
    }

    #[test]
    fn a_big_model_gets_coarser_columns_than_a_small_one() {
        let big = SolidGrid::build(
            &[boxed([-500.0, -1.0, 0.0], [500.0, 1.0, 1.0])],
            [1000.0, 2.0, 1.0],
        );
        assert!(big.cell > CELL);
        assert!(big.across[0] <= MAX_ACROSS as usize + 1);
    }
}

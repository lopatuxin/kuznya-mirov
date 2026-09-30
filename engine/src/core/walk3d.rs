//! «Рельеф» → «Ходьба»: путь по рельефу и настилам. Поверхностей несколько: рельеф и верх каждого
//! настила. На каждой поверхности путь — ломаная по углам раздутых препятствий, как на плоскости
//! (`core::pathfind`); между поверхностями идущий переходит на границе того места, где его
//! прямоугольник начинает или перестаёт задевать настил: местами перехода служат точки этой границы
//! через `PORTAL_SPACING` и концы её частей, где стоять можно, — так узкая часть не теряется между
//! точками. Что зависит только от земли, настилов и тела идущего — крутые и подводные места, стены
//! настилов, края настилов, переходы, — считается один раз, при сборке мира (`prepare`), и
//! запоминается. От объектов `avoid` зависят узлы вокруг них и то, кто кого из узлов видит: это тоже
//! посчитано заранее и при смене набора объектов пересчитывается лишь в затронутых ими местах
//! (`Field`), так что путь ищется по готовой видимости.

use std::cell::RefCell;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::rc::Rc;

use super::footprint::Footprint;
use super::grid::Rect;
use super::pathfind::{
    self, EPS, HeapEntry, Obstacle, PathKey, Poly, WalkCaches, dist, inflate, inflate_polygon,
};
use super::surface::{Pillar, SEAT_DEPTH, STEP};
use super::terrain::{Polygon, Terrain};
use super::value::{Rotation, Vec2};

const TOLERANCE: f64 = 1e-9;
/// Насколько одна поверхность над целью может быть дальше от названной высоты, чем ближайшая, чтобы всё ещё
/// считаться такой же близкой.
const GOAL_TIE: f64 = 1e-6;
/// Шаг по границе настила, с каким выбираются места перехода.
const PORTAL_SPACING: f64 = 0.5;
/// Сколько раз делится пополам промежуток, где меняется, разрешён ли переход.
const CHANGE_BISECTIONS: u32 = 24;
/// Шаг, с каким проверяется, можно ли пройти под настилом или встать на него.
const SAMPLE_SPACING: f64 = 0.25;
const INDEX_CELL: f64 = 1.0;
/// На сколько куски одной стены настила заходят друг на друга: чтобы отрезок вдоль их стыка не проходил
/// между ними (стык глубже допуска `EPS` в обоих).
const SEAM: f64 = 1e-5;
/// Сколько препятствий слоя может смениться разом, чтобы узлы и видимость слоя пересматривались по
/// затронутым парам; больше — слой считается заново целиком.
const REBUILD_LIMIT: usize = 16;

/// Настил как поверхность для ходьбы.
#[derive(Debug, Clone, PartialEq)]
pub struct Deck {
    pub id: u32,
    pub place: Footprint,
    /// Высота низа настила и его верха.
    pub bottom: f64,
    pub top: f64,
}

/// Объект `avoid`: место на земле и столб по высоте.
#[derive(Debug, Clone, PartialEq)]
pub struct Blocker {
    pub id: u32,
    pub place: Footprint,
    pub pillar: Pillar,
}

/// Всё, от чего зависит запомненный путь по поверхностям, кроме цели, размера и поворота идущего.
#[derive(Debug, Clone, PartialEq)]
pub struct SurfaceKey {
    decks: Vec<Deck>,
    blockers: Vec<Blocker>,
    height: f64,
    target_z: Option<f64>,
}

/// Земля, настилы и объекты, по которым идущий выбирает путь.
pub struct Surfaces<'a> {
    pub terrain: &'a Terrain,
    pub decks: Vec<Deck>,
    /// Объекты `avoid`, по возрастанию номера.
    pub blockers: Vec<Blocker>,
}

/// Идущий: тело, рост и высота основания сейчас.
pub struct Walker {
    pub id: u32,
    pub center: Vec2,
    pub size: Vec2,
    pub rotation: Option<Rotation>,
    pub height: f64,
    pub z: f64,
}

/// Куда идёт: точка, высота, если названа, и высота, к которой ближайшее место выбирается.
pub struct Goal {
    pub point: Vec2,
    pub named_z: Option<f64>,
    pub wanted_z: f64,
}

// -------------------------------------------------------------------------------------------
// Геометрия
// -------------------------------------------------------------------------------------------

/// Тело идущего с местом, где стоит его середина.
struct Body {
    size: Vec2,
    rotation: Option<Rotation>,
    height: f64,
    /// Тело с серединой в нуле — для раздувания препятствий.
    origin: Footprint,
}

impl Body {
    fn new(size: Vec2, rotation: Option<Rotation>, height: f64) -> Body {
        Body {
            size,
            rotation,
            height,
            origin: Footprint::rotated([-size[0] / 2.0, -size[1] / 2.0], size, rotation),
        }
    }

    fn at(&self, center: Vec2) -> Footprint {
        Footprint::rotated(
            [
                center[0] - self.size[0] / 2.0,
                center[1] - self.size[1] / 2.0,
            ],
            self.size,
            self.rotation,
        )
    }
}

fn inside_bounds(bounds: &Rect, p: Vec2) -> bool {
    p[0] >= bounds.x - EPS
        && p[0] <= bounds.x + bounds.w + EPS
        && p[1] >= bounds.y - EPS
        && p[1] <= bounds.y + bounds.h + EPS
}

/// Часть выпуклого многоугольника по одну сторону прямой `n · p = d`: с меньшей стороны (`n · p ≤ d`),
/// если `inside`, иначе с большей.
fn clip_half_plane(polygon: &[Vec2], n: Vec2, d: f64, inside: bool) -> Polygon {
    let side = |p: Vec2| {
        let value = d - (n[0] * p[0] + n[1] * p[1]);
        if inside { value } else { -value }
    };
    let mut clipped = Vec::with_capacity(polygon.len() + 1);
    for (i, &current) in polygon.iter().enumerate() {
        let previous = polygon[(i + polygon.len() - 1) % polygon.len()];
        let (before, now) = (side(previous), side(current));
        if (before >= 0.0) != (now >= 0.0) {
            let t = before / (before - now);
            clipped.push([
                previous[0] + t * (current[0] - previous[0]),
                previous[1] + t * (current[1] - previous[1]),
            ]);
        }
        if now >= 0.0 {
            clipped.push(current);
        }
    }
    clipped
}

/// Часть выпуклого `polygon` внутри выпуклого многоугольника `region` (вершины против часовой стрелки).
fn clip_to_convex(mut polygon: Polygon, region: &[Vec2]) -> Polygon {
    for i in 0..region.len() {
        let (a, b) = (region[i], region[(i + 1) % region.len()]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let length = (ex * ex + ey * ey).sqrt().max(TOLERANCE);
        let n = [ey / length, -ex / length];
        polygon = clip_half_plane(&polygon, n, n[0] * a[0] + n[1] * a[1], true);
        if polygon.len() < 3 {
            break;
        }
    }
    polygon
}

fn area(polygon: &[Vec2]) -> f64 {
    (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

/// Что остаётся от выпуклого `polygon`, если убрать выпуклый четырёхугольник `quad` (против часовой
/// стрелки): до четырёх выпуклых кусков. Четырёхугольник без площади (настил нулевого размера) ничего не
/// убирает: у его рёбер нет нормали, и каждое отдавало бы весь многоугольник и наружу, и внутрь.
fn subtract_convex(polygon: &[Vec2], quad: &[Vec2; 4]) -> Vec<Polygon> {
    if area(quad) <= EPS {
        return vec![polygon.to_vec()];
    }
    let mut rest: Polygon = polygon.to_vec();
    let mut parts = Vec::new();
    for i in 0..4 {
        let (a, b) = (quad[i], quad[(i + 1) % 4]);
        let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
        let length = (ex * ex + ey * ey).sqrt().max(TOLERANCE);
        let n = [ey / length, -ex / length];
        let d = n[0] * a[0] + n[1] * a[1];
        let outside = clip_half_plane(&rest, n, d, false);
        if outside.len() >= 3 && area(&outside) > EPS {
            parts.push(outside);
        }
        rest = clip_half_plane(&rest, n, d, true);
        if rest.len() < 3 {
            break;
        }
    }
    parts
}

/// Углы четырёхугольника места против часовой стрелки в обычных осях.
fn quad_ccw(place: &Footprint) -> [Vec2; 4] {
    let corners = place.corners();
    let signed: f64 = (0..4)
        .map(|i| {
            let (a, b) = (corners[i], corners[(i + 1) % 4]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum();
    if signed < 0.0 {
        [corners[3], corners[2], corners[1], corners[0]]
    } else {
        corners
    }
}

/// Точки вдоль границы многоугольника не реже, чем раз в `spacing`, углы включены.
fn boundary_samples(polygon: &Poly, spacing: f64) -> Vec<Vec2> {
    let mut samples = Vec::new();
    let count = polygon.verts.len();
    for i in 0..count {
        let (a, b) = (polygon.verts[i], polygon.verts[(i + 1) % count]);
        let steps = (dist(a, b) / spacing).ceil().max(1.0) as usize;
        for k in 0..steps {
            let t = k as f64 / steps as f64;
            samples.push([a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]);
        }
    }
    samples
}

fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> Vec2 {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length_squared = dx * dx + dy * dy;
    if length_squared < TOLERANCE {
        return a;
    }
    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / length_squared).clamp(0.0, 1.0);
    [a[0] + t * dx, a[1] + t * dy]
}

/// Что настил позволяет идущему в месте перехода: взойти на него и сойти с него на нижнюю поверхность.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Crossing {
    enter: bool,
    leave: bool,
}

impl Crossing {
    fn open(self) -> bool {
        self.enter || self.leave
    }
}

/// Части `spans` без открытого промежутка `(low, high)`.
fn subtract_open(spans: &[(f64, f64)], low: f64, high: f64) -> Vec<(f64, f64)> {
    let mut rest = Vec::with_capacity(spans.len() + 1);
    for &(from, to) in spans {
        if high <= from || low >= to {
            rest.push((from, to));
            continue;
        }
        if low > from {
            rest.push((from, low));
        }
        if high < to {
            rest.push((high, to));
        }
    }
    rest
}

/// Общие части двух наборов промежутков.
fn intersect_spans(a: &[(f64, f64)], b: &[(f64, f64)]) -> Vec<(f64, f64)> {
    a.iter()
        .flat_map(|&(a_low, a_high)| {
            b.iter().filter_map(move |&(b_low, b_high)| {
                let (low, high) = (a_low.max(b_low), a_high.min(b_high));
                (low <= high).then_some((low, high))
            })
        })
        .collect()
}

fn round_key(p: Vec2) -> (i64, i64) {
    ((p[0] * 1e6).round() as i64, (p[1] * 1e6).round() as i64)
}

/// Равномерная сетка охватывающих прямоугольников препятствий.
#[derive(Debug)]
struct Index {
    origin: Vec2,
    columns: i64,
    rows: i64,
    cells: Vec<Vec<u32>>,
}

impl Index {
    fn build(obstacles: &[Obstacle], area: &Rect) -> Index {
        let columns = ((area.w / INDEX_CELL).ceil() as i64).max(1);
        let rows = ((area.h / INDEX_CELL).ceil() as i64).max(1);
        let mut index = Index {
            origin: [area.x, area.y],
            columns,
            rows,
            cells: vec![Vec::new(); (columns * rows) as usize],
        };
        for (i, obstacle) in obstacles.iter().enumerate() {
            let bbox = obstacle.bbox();
            let (c0, r0) = index.cell_of(bbox.x, bbox.y);
            let (c1, r1) = index.cell_of(bbox.x + bbox.w, bbox.y + bbox.h);
            for r in r0..=r1 {
                for c in c0..=c1 {
                    index.cells[(r * columns + c) as usize].push(i as u32);
                }
            }
        }
        index
    }

    fn cell_of(&self, x: f64, y: f64) -> (i64, i64) {
        (
            (((x - self.origin[0]) / INDEX_CELL).floor() as i64).clamp(0, self.columns - 1),
            (((y - self.origin[1]) / INDEX_CELL).floor() as i64).clamp(0, self.rows - 1),
        )
    }

    /// Есть ли среди препятствий, чьи ячейки проходит отрезок, то, что его перегораживает. Ячейки
    /// идут по порядку вдоль отрезка.
    fn blocked_along(&self, obstacles: &[Obstacle], p0: Vec2, p1: Vec2) -> bool {
        let bbox = (
            p0[0].min(p1[0]),
            p0[0].max(p1[0]),
            p0[1].min(p1[1]),
            p0[1].max(p1[1]),
        );
        let (mut column, mut row) = self.cell_of(p0[0], p0[1]);
        let (last_column, last_row) = self.cell_of(p1[0], p1[1]);
        let (dx, dy) = (p1[0] - p0[0], p1[1] - p0[1]);
        let step_column = if dx >= 0.0 { 1 } else { -1 };
        let step_row = if dy >= 0.0 { 1 } else { -1 };
        let next = |cell: i64, step: i64, origin: f64| {
            origin + (if step > 0 { cell + 1 } else { cell }) as f64 * INDEX_CELL
        };
        let mut t_column = if dx.abs() < TOLERANCE {
            f64::INFINITY
        } else {
            (next(column, step_column, self.origin[0]) - p0[0]) / dx
        };
        let mut t_row = if dy.abs() < TOLERANCE {
            f64::INFINITY
        } else {
            (next(row, step_row, self.origin[1]) - p0[1]) / dy
        };
        let delta_column = if dx.abs() < TOLERANCE {
            f64::INFINITY
        } else {
            INDEX_CELL / dx.abs()
        };
        let delta_row = if dy.abs() < TOLERANCE {
            f64::INFINITY
        } else {
            INDEX_CELL / dy.abs()
        };
        loop {
            let hit = self.cells[(row * self.columns + column) as usize]
                .iter()
                .any(|&i| obstacles[i as usize].blocks_segment(p0, p1, bbox));
            if hit {
                return true;
            }
            if (column, row) == (last_column, last_row) {
                return false;
            }
            if t_column < t_row {
                column += step_column;
                t_column += delta_column;
            } else {
                row += step_row;
                t_row += delta_row;
            }
            if column < 0 || row < 0 || column >= self.columns || row >= self.rows {
                return false;
            }
        }
    }

    /// Номера препятствий из ячеек, которых касается охватывающий прямоугольник отрезка `a → b`, без
    /// повторов.
    fn candidates(&self, a: Vec2, b: Vec2) -> Vec<u32> {
        let (c0, r0) = self.cell_of(a[0].min(b[0]), a[1].min(b[1]));
        let (c1, r1) = self.cell_of(a[0].max(b[0]), a[1].max(b[1]));
        let mut found: Vec<u32> = (r0..=r1)
            .flat_map(|r| (c0..=c1).map(move |c| (r * self.columns + c) as usize))
            .flat_map(|cell| self.cells[cell].iter().copied())
            .collect();
        found.sort_unstable();
        found.dedup();
        found
    }

    /// Номера препятствий из ячейки точки.
    fn near(&self, p: Vec2) -> &[u32] {
        let (column, row) = self.cell_of(p[0], p[1]);
        &self.cells[(row * self.columns + column) as usize]
    }
}

/// Препятствия вместе с индексом их охватывающих прямоугольников.
#[derive(Debug)]
struct Indexed {
    obstacles: Vec<Obstacle>,
    index: Index,
}

impl Indexed {
    fn new(obstacles: Vec<Obstacle>, area: &Rect) -> Indexed {
        let index = Index::build(&obstacles, area);
        Indexed { obstacles, index }
    }

    /// Не заходит ли отрезок строго внутрь какого-нибудь препятствия.
    fn blocks_segment(&self, p0: Vec2, p1: Vec2) -> bool {
        self.index.blocked_along(&self.obstacles, p0, p1)
    }

    /// Глубже допуска `EPS` внутри какого-нибудь препятствия.
    fn holds_deeply(&self, p: Vec2) -> bool {
        self.index
            .near(p)
            .iter()
            .any(|&i| self.obstacles[i as usize].deeply_inside(p))
    }

    /// Строго внутри какого-нибудь препятствия, кроме `skip`.
    fn holds(&self, p: Vec2, skip: Option<usize>) -> bool {
        self.index
            .near(p)
            .iter()
            .any(|&i| Some(i as usize) != skip && self.obstacles[i as usize].strictly_inside(p))
    }
}

// -------------------------------------------------------------------------------------------
// Неизменное: рельеф, настилы, переходы
// -------------------------------------------------------------------------------------------

#[derive(Debug)]
struct TerrainLayer {
    fixed: Indexed,
    nodes: Vec<Vec2>,
    bounds: Rect,
}

#[derive(Debug)]
struct DeckLayer {
    deck: Deck,
    region: Option<Poly>,
    fixed: Indexed,
    nodes: Vec<Vec2>,
}

/// Переход между двумя поверхностями в точке `pos`: слои `a` и `b`, в какую сторону можно.
#[derive(Debug)]
struct Portal {
    pos: Vec2,
    a: usize,
    b: usize,
    a_to_b: bool,
    b_to_a: bool,
}

/// По чему запомненная сеть годна: тело, сцена, настилы и рост идущего.
#[derive(Debug, Clone, PartialEq)]
struct NavKey {
    size: Vec2,
    rotation: Option<Rotation>,
    height: f64,
    scene: (f64, f64),
    decks: Vec<Deck>,
    terrain: usize,
}

/// Слой 0 — рельеф, слой `1 + i` — верх настила `i`. Сеть и то, что зависит от объектов `avoid`
/// (`Field`), живут вместе и пересчитываются по-разному: сеть — при смене земли, настилов и тела
/// идущего, поле — только там, где изменились объекты.
#[derive(Debug)]
pub struct SurfaceNav {
    key: NavKey,
    terrain: TerrainLayer,
    decks: Vec<DeckLayer>,
    portals: Vec<Portal>,
    field: RefCell<Field>,
}

/// Что настил делает с идущим по земле в некоей точке под ним.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ground {
    /// Поднимает на себя.
    Lifted,
    /// Стена.
    Wall,
    /// Идущий проходит под ним.
    Under,
}

/// Область настила, разбитая на клетки: в углах и в середине каждой — как настил обходится с идущим по
/// земле там (`None` — точка вне области).
struct CellGrid {
    origin: Vec2,
    columns: usize,
    rows: usize,
    size: Vec2,
    samples: Vec<[Option<Ground>; 5]>,
}

impl CellGrid {
    /// Угол клеток на пересечении столбца `column` и строки `row` линий сетки.
    fn corner(&self, column: usize, row: usize) -> Vec2 {
        [
            self.origin[0] + column as f64 * self.size[0],
            self.origin[1] + row as f64 * self.size[1],
        ]
    }

    fn centre(&self, column: usize, row: usize) -> Vec2 {
        [
            self.origin[0] + (column as f64 + 0.5) * self.size[0],
            self.origin[1] + (row as f64 + 0.5) * self.size[1],
        ]
    }

    /// Помеченные клетки, слепленные в прямоугольники: `[левый, верхний, правый, нижний]` — номера линий
    /// сетки.
    fn rectangles(&self, marked: &[bool]) -> Vec<[usize; 4]> {
        let mut taken = vec![false; marked.len()];
        let free = |taken: &[bool], column: usize, row: usize| {
            let cell = row * self.columns + column;
            marked[cell] && !taken[cell]
        };
        let mut rectangles = Vec::new();
        for row in 0..self.rows {
            for column in 0..self.columns {
                if !free(&taken, column, row) {
                    continue;
                }
                let mut right = column + 1;
                while right < self.columns && free(&taken, right, row) {
                    right += 1;
                }
                let mut bottom = row + 1;
                while bottom < self.rows && (column..right).all(|c| free(&taken, c, bottom)) {
                    bottom += 1;
                }
                for r in row..bottom {
                    for c in column..right {
                        taken[r * self.columns + c] = true;
                    }
                }
                rectangles.push([column, row, right, bottom]);
            }
        }
        rectangles
    }

    /// Препятствие из прямоугольника клеток, разросшееся на `SEAM` в стороны (соседние заходят друг на
    /// друга) и обрезанное по области `region`.
    fn solid_piece(&self, rectangle: [usize; 4], region: &[Vec2]) -> Option<Obstacle> {
        let [left, top, right, bottom] = rectangle;
        let (low, high) = (self.corner(left, top), self.corner(right, bottom));
        let corners = vec![
            [low[0] - SEAM, low[1] - SEAM],
            [high[0] + SEAM, low[1] - SEAM],
            [high[0] + SEAM, high[1] + SEAM],
            [low[0] - SEAM, high[1] + SEAM],
        ];
        let clipped = clip_to_convex(corners, region);
        if clipped.len() < 3 || area(&clipped) <= EPS {
            return None;
        }
        Poly::hull(clipped).map(Obstacle::Poly)
    }
}

struct Builder<'a> {
    terrain: &'a Terrain,
    decks: &'a [Deck],
    body: &'a Body,
    bounds: Rect,
}

impl Builder<'_> {
    /// Высота, на которой стоит идущий на рельефе с серединой в `p`: самая нижняя точка под телом.
    fn ground_z(&self, p: Vec2) -> f64 {
        self.terrain.min_under(&self.body.at(p))
    }

    /// Как настил обходится с идущим по рельефу с серединой в `p`: на настил его поднимет (верх не выше
    /// ступеньки над землёй), настил ему стена (выше ступеньки, а просвет ниже роста) или он пройдёт под
    /// настилом.
    fn ground_under(&self, deck: &Deck, p: Vec2) -> Ground {
        let z = self.ground_z(p);
        if deck.top <= z + STEP + TOLERANCE {
            Ground::Lifted
        } else if deck.bottom - z >= self.body.height - TOLERANCE {
            Ground::Under
        } else {
            Ground::Wall
        }
    }

    fn deck_region(&self, deck: &Deck) -> Option<Poly> {
        let sums = deck
            .place
            .corners()
            .iter()
            .flat_map(|o| {
                self.body
                    .origin
                    .corners()
                    .into_iter()
                    .map(move |b| [o[0] + b[0], o[1] + b[1]])
            })
            .collect();
        Poly::hull(sums)
    }

    /// Область настила, разбитая на клетки `SAMPLE_SPACING`, и как настил обходится с идущим в углах и в
    /// середине каждой клетки, что лежат в области.
    fn sample_region(&self, deck: &Deck, region: &Poly) -> CellGrid {
        let bbox = region.bbox;
        let columns = ((bbox.w / SAMPLE_SPACING).ceil() as usize).max(1);
        let rows = ((bbox.h / SAMPLE_SPACING).ceil() as usize).max(1);
        let mut grid = CellGrid {
            origin: [bbox.x, bbox.y],
            columns,
            rows,
            size: [bbox.w / columns as f64, bbox.h / rows as f64],
            samples: Vec::with_capacity(columns * rows),
        };
        let probe = |p: Vec2| {
            region.contains_closed(p).then(|| {
                if inside_bounds(&self.bounds, p) {
                    self.ground_under(deck, p)
                } else {
                    Ground::Wall
                }
            })
        };
        let lattice: Vec<Option<Ground>> = (0..=rows)
            .flat_map(|row| (0..=columns).map(move |column| (column, row)))
            .map(|(column, row)| probe(grid.corner(column, row)))
            .collect();
        for row in 0..rows {
            for column in 0..columns {
                let corner =
                    |dc: usize, dr: usize| lattice[(row + dr) * (columns + 1) + column + dc];
                let samples = [
                    corner(0, 0),
                    corner(1, 0),
                    corner(0, 1),
                    corner(1, 1),
                    probe(grid.centre(column, row)),
                ];
                grid.samples.push(samples);
            }
        }
        grid
    }

    /// Точки на краю области настила, откуда идущий сошёл бы с него в пропасть глубже ступеньки и где ему
    /// можно стоять: не в крутых и подводных местах `ground`.
    fn drops_beside(&self, deck: &Deck, region: &Poly, ground: &Indexed) -> Vec<Vec2> {
        boundary_samples(region, PORTAL_SPACING)
            .into_iter()
            .filter(|&q| {
                inside_bounds(&self.bounds, q)
                    && !ground.holds(q, None)
                    && self.ground_under(deck, q) != Ground::Lifted
            })
            .collect()
    }

    /// Что настил отнимает у идущего по рельефу: те части его области, где идущий не проходит под ним, —
    /// туда он не пойдёт по земле, а взойдёт на настил через переход на его краю или обойдёт стену. Где
    /// земля под настилом такова, что просвет достаточен, отнимать нечего: идущий проходит под ним. Идущий,
    /// что шёл по земле там, где настил низок, взошёл бы на него и по нему дошёл бы до края, за которым
    /// глубже ступеньки, — такие низкие места, откуда по земле видно такой край, тоже отняты. Клетка
    /// отнята, если в её углах или в середине есть стена либо низкое место, откуда виден такой край.
    /// Настил, не оставляющий свободных клеток, — одна стена по его области, иначе — стена из слепленных
    /// отнятых клеток. `ground` — крутые и подводные места, что и так закрыты идущему по земле.
    fn deck_solids(&self, deck: &Deck, region: &Poly, ground: &Indexed) -> Vec<Obstacle> {
        let grid = self.sample_region(deck, region);
        let drops = self.drops_beside(deck, region, ground);
        let taken: Vec<bool> = grid
            .samples
            .iter()
            .enumerate()
            .map(|(cell, samples)| {
                let centre = grid.centre(cell % grid.columns, cell / grid.columns);
                samples.contains(&Some(Ground::Wall))
                    || (samples.contains(&Some(Ground::Lifted))
                        && drops.iter().any(|&q| !ground.blocks_segment(centre, q)))
            })
            .collect();
        if !taken.contains(&true) {
            return Vec::new();
        }
        let whole = grid
            .samples
            .iter()
            .zip(&taken)
            .all(|(samples, &taken)| taken || samples.iter().all(Option::is_none));
        if whole {
            return vec![inflate(&deck.place, &self.body.origin)];
        }
        grid.rectangles(&taken)
            .into_iter()
            .filter_map(|rectangle| grid.solid_piece(rectangle, &region.verts))
            .collect()
    }

    fn terrain_layer(&self) -> TerrainLayer {
        let mut obstacles: Vec<Obstacle> = self
            .terrain
            .walk_blocked()
            .iter()
            .filter_map(|piece| inflate_polygon(piece, &self.body.origin))
            .collect();
        let pieces = Indexed::new(std::mem::take(&mut obstacles), &self.bounds);
        let mut walls = Vec::new();
        for deck in self.decks {
            let Some(region) = self.deck_region(deck) else {
                continue;
            };
            walls.extend(self.deck_solids(deck, &region, &pieces));
        }
        let mut obstacles = pieces.obstacles;
        obstacles.extend(walls);
        let fixed = Indexed::new(obstacles, &self.bounds);
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        for (i, obstacle) in fixed.obstacles.iter().enumerate() {
            for corner in obstacle.corners() {
                if inside_bounds(&self.bounds, corner)
                    && !fixed.holds(corner, Some(i))
                    && seen.insert(round_key(corner))
                {
                    nodes.push(corner);
                }
            }
        }
        TerrainLayer {
            fixed,
            nodes,
            bounds: self.bounds,
        }
    }

    /// Куски земли, где идущий на настиле `deck` стоял бы над обрывом глубже ступеньки, с этого края
    /// настила не убрать: земля ниже верха настила больше чем на ступеньку и не прикрытая ни им
    /// самим, ни другим настилом в пределах ступеньки от него.
    fn drop_obstacles(&self, deck: &Deck, region: &Poly) -> Vec<Obstacle> {
        let margin = self.body.origin.aabb().w.max(self.body.origin.aabb().h);
        let area = [
            [region.bbox.x - margin, region.bbox.y - margin],
            [
                region.bbox.x + region.bbox.w + margin,
                region.bbox.y + region.bbox.h + margin,
            ],
        ];
        let mut floors: Vec<[Vec2; 4]> = vec![quad_ccw(&deck.place)];
        for other in self.decks {
            if other.id != deck.id
                && other.top >= deck.top - STEP - TOLERANCE
                && other.top <= deck.top + STEP + TOLERANCE
            {
                floors.push(quad_ccw(&other.place));
            }
        }
        let mut fragments = self.terrain.pieces_below(deck.top - STEP, area);
        for floor in &floors {
            fragments = fragments
                .iter()
                .flat_map(|piece| subtract_convex(piece, floor))
                .collect();
        }
        fragments
            .iter()
            .filter_map(|piece| inflate_polygon(piece, &self.body.origin))
            .collect()
    }

    fn deck_layer(&self, deck: &Deck) -> DeckLayer {
        let region = self.deck_region(deck);
        let Some(poly) = &region else {
            return DeckLayer {
                deck: deck.clone(),
                region,
                fixed: Indexed::new(Vec::new(), &self.bounds),
                nodes: Vec::new(),
            };
        };
        let mut obstacles = self.drop_obstacles(deck, poly);
        for other in self.decks {
            if other.id == deck.id {
                continue;
            }
            let door = other.top > deck.top + TOLERANCE && other.top <= deck.top + STEP + TOLERANCE;
            let wall = other.top > deck.top + STEP + TOLERANCE
                && other.bottom - deck.top < self.body.height - TOLERANCE;
            if door || wall {
                obstacles.push(inflate(&other.place, &self.body.origin));
            }
        }
        let mut seen = HashSet::new();
        let mut nodes = Vec::new();
        let candidates = obstacles
            .iter()
            .flat_map(Obstacle::corners)
            .chain(poly.verts.iter().copied());
        for corner in candidates {
            if poly.contains_closed(corner)
                && inside_bounds(&self.bounds, corner)
                && !obstacles.iter().any(|o| o.strictly_inside(corner))
                && seen.insert(round_key(corner))
            {
                nodes.push(corner);
            }
        }
        DeckLayer {
            deck: deck.clone(),
            region,
            fixed: Indexed::new(obstacles, &self.bounds),
            nodes,
        }
    }

    /// Настил, на который сажает идущего с серединой в `p` и основанием `from` правило сдвига:
    /// самый высокий из задетых, верх которого не выше `from` плюс ступенька, кроме `exclude`.
    fn seat_over(&self, p: Vec2, from: f64, exclude: usize) -> Option<usize> {
        let body = self.body.at(p);
        self.decks
            .iter()
            .enumerate()
            .filter(|&(i, other)| {
                i != exclude
                    && other.top <= from + STEP + TOLERANCE
                    && other.place.overlaps_deeper_than(&body, SEAT_DEPTH)
            })
            .fold(None, |best: Option<usize>, (i, other)| match best {
                Some(b) if self.decks[b].top >= other.top => Some(b),
                _ => Some(i),
            })
    }

    /// Не перебьёт ли другой настил выше `top` посадку на настил с верхом `top`.
    fn outranked(&self, seat: Option<usize>, top: f64) -> bool {
        seat.is_some_and(|i| self.decks[i].top > top + TOLERANCE)
    }

    /// Части стороны `a → b` области настила (параметр `t ∈ [0, 1]`), где идущему можно стоять на слое:
    /// внутри границ сцены, внутри `within`, если она названа, и не строго внутри неизменных препятствий
    /// слоя. Концы частей лежат на границах препятствий.
    fn free_spans(
        &self,
        a: Vec2,
        b: Vec2,
        fixed: &Indexed,
        within: Option<&Poly>,
    ) -> Vec<(f64, f64)> {
        let Some(mut whole) = pathfind::span_in_rect(&self.bounds, a, b) else {
            return Vec::new();
        };
        if let Some(region) = within {
            let Some(inner) = region.closed_span(a, b) else {
                return Vec::new();
            };
            whole = (whole.0.max(inner.0), whole.1.min(inner.1));
            if whole.0 > whole.1 {
                return Vec::new();
            }
        }
        let mut free = vec![whole];
        for id in fixed.index.candidates(a, b) {
            if let Some((low, high)) = fixed.obstacles[id as usize].inside_span(a, b) {
                free = subtract_open(&free, low, high);
            }
        }
        free
    }

    /// Места перехода на стороне `a → b`: точки сетки с шагом не больше `PORTAL_SPACING` и концы свободных
    /// частей, где `crossing` разрешает переход. Концы нужны, чтобы узкая свободная часть, между точками
    /// сетки не оказавшаяся, всё равно давала переход. Где разрешение меняется между двумя точками одной
    /// свободной части, добавляются и точки по обе стороны перемены.
    fn crossing_points(
        a: Vec2,
        b: Vec2,
        free: &[(f64, f64)],
        crossing: impl Fn(Vec2) -> Crossing,
    ) -> Vec<(Vec2, Crossing)> {
        let point = |t: f64| [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
        let steps = ((dist(a, b) / PORTAL_SPACING).ceil() as usize).max(1);
        let mut moments: Vec<f64> = (0..steps).map(|n| n as f64 / steps as f64).collect();
        for &(low, high) in free {
            moments.extend([low, high].into_iter().filter(|&t| t > 0.0 && t < 1.0));
        }
        moments.sort_by(f64::total_cmp);
        moments.dedup();
        let mut found = Vec::new();
        for &(low, high) in free {
            let mut previous: Option<(f64, Crossing)> = None;
            for &t in moments.iter().filter(|&&t| t >= low && t <= high) {
                let now = crossing(point(t));
                if let Some((before, was)) = previous.filter(|&(_, was)| was != now) {
                    let (near, far) = Self::locate_change(&point, &crossing, (before, was), t);
                    found.extend([near, far].map(|(t, c)| (point(t), c)));
                }
                found.push((point(t), now));
                previous = Some((t, now));
            }
        }
        found.retain(|&(_, c)| c.open());
        found
    }

    /// Перемена разрешения между `from` и `until`: последняя точка с прежним разрешением и первая с новым.
    fn locate_change(
        point: &impl Fn(f64) -> Vec2,
        crossing: &impl Fn(Vec2) -> Crossing,
        from: (f64, Crossing),
        until: f64,
    ) -> ((f64, Crossing), (f64, Crossing)) {
        let (mut low, mut high) = (from.0, until);
        for _ in 0..CHANGE_BISECTIONS {
            let middle = (low + high) / 2.0;
            if crossing(point(middle)) == from.1 {
                low = middle;
            } else {
                high = middle;
            }
        }
        ((low, crossing(point(low))), (high, crossing(point(high))))
    }

    fn ground_crossing(&self, upper: &DeckLayer, k: usize, q: Vec2) -> Crossing {
        let z = self.ground_z(q);
        Crossing {
            enter: upper.deck.top <= z + STEP + TOLERANCE
                && !self.outranked(self.seat_over(q, z, k), upper.deck.top),
            leave: self.seat_over(q, upper.deck.top, k).is_none(),
        }
    }

    fn deck_crossing(&self, upper: &DeckLayer, k: usize, lower: &DeckLayer, q: Vec2) -> Crossing {
        Crossing {
            enter: upper.deck.top <= lower.deck.top + STEP + TOLERANCE
                && !self.outranked(self.seat_over(q, lower.deck.top, k), upper.deck.top),
            leave: self
                .seat_over(q, upper.deck.top, k)
                .is_some_and(|i| (self.decks[i].top - lower.deck.top).abs() < TOLERANCE),
        }
    }

    fn portals(&self, terrain: &TerrainLayer, layers: &[DeckLayer]) -> Vec<Portal> {
        let mut portals = Vec::new();
        let mut seen = HashSet::new();
        let mut add = |pos: Vec2, a: usize, b: usize, crossing: Crossing| {
            if seen.insert((round_key(pos), a, b, crossing.enter, crossing.leave)) {
                portals.push(Portal {
                    pos,
                    a,
                    b,
                    a_to_b: crossing.enter,
                    b_to_a: crossing.leave,
                });
            }
        };
        for (k, upper) in layers.iter().enumerate() {
            let Some(region) = &upper.region else {
                continue;
            };
            for i in 0..region.verts.len() {
                let (a, b) = (region.verts[i], region.verts[(i + 1) % region.verts.len()]);
                let above = self.free_spans(a, b, &upper.fixed, None);
                if above.is_empty() {
                    continue;
                }
                let ground = intersect_spans(&above, &self.free_spans(a, b, &terrain.fixed, None));
                for (pos, crossing) in
                    Self::crossing_points(a, b, &ground, |q| self.ground_crossing(upper, k, q))
                {
                    add(pos, 0, 1 + k, crossing);
                }
                for (j, lower) in layers.iter().enumerate() {
                    let Some(lower_region) = &lower.region else {
                        continue;
                    };
                    if j == k || lower.deck.top > upper.deck.top + TOLERANCE {
                        continue;
                    }
                    let onto = intersect_spans(
                        &above,
                        &self.free_spans(a, b, &lower.fixed, Some(lower_region)),
                    );
                    for (pos, crossing) in Self::crossing_points(a, b, &onto, |q| {
                        self.deck_crossing(upper, k, lower, q)
                    }) {
                        add(pos, 1 + j, 1 + k, crossing);
                    }
                }
            }
        }
        portals
    }
}

impl SurfaceNav {
    fn build(
        surfaces: &Surfaces<'_>,
        walker: &Walker,
        scene: (f64, f64),
        key: NavKey,
    ) -> SurfaceNav {
        let frame = pathfind::frame(walker.center, walker.size, walker.rotation, scene);
        let body = Body::new(walker.size, walker.rotation, walker.height);
        let builder = Builder {
            terrain: surfaces.terrain,
            decks: &surfaces.decks,
            body: &body,
            bounds: frame.bounds,
        };
        let terrain = builder.terrain_layer();
        let decks: Vec<DeckLayer> = surfaces
            .decks
            .iter()
            .map(|deck| builder.deck_layer(deck))
            .collect();
        let portals = builder.portals(&terrain, &decks);
        SurfaceNav {
            key,
            terrain,
            decks,
            portals,
            field: RefCell::new(Field::default()),
        }
    }
}

// -------------------------------------------------------------------------------------------
// Объекты `avoid`: узлы и видимость
// -------------------------------------------------------------------------------------------

/// Слой сети вместе с объектами `avoid`, что мешают на нём: границы сцены, области настилов, преграды.
#[derive(Clone, Copy)]
struct View<'a> {
    nav: &'a SurfaceNav,
    overlay: &'a [Indexed],
}

impl View<'_> {
    fn in_region(&self, layer: usize, p: Vec2) -> bool {
        inside_bounds(&self.nav.terrain.bounds, p)
            && match layer {
                0 => true,
                _ => self.nav.decks[layer - 1]
                    .region
                    .as_ref()
                    .is_some_and(|region| region.contains_closed(p)),
            }
    }

    fn fixed(&self, layer: usize) -> &Indexed {
        match layer {
            0 => &self.nav.terrain.fixed,
            _ => &self.nav.decks[layer - 1].fixed,
        }
    }

    /// Строго внутри препятствия этого слоя — там идущему не встать.
    fn blocked(&self, layer: usize, p: Vec2, skip_dynamic: Option<usize>) -> bool {
        self.fixed(layer).holds(p, None) || self.overlay[layer].holds(p, skip_dynamic)
    }

    fn valid(&self, layer: usize, p: Vec2) -> bool {
        self.in_region(layer, p) && !self.blocked(layer, p, None)
    }

    /// Можно ли стоять в месте перехода: то же, но на границе препятствия, куда точку отнесла погрешность
    /// дробных чисел, — можно.
    fn valid_crossing(&self, layer: usize, p: Vec2) -> bool {
        self.in_region(layer, p)
            && !self.fixed(layer).holds_deeply(p)
            && !self.overlay[layer].holds_deeply(p)
    }

    fn sees(&self, layer: usize, a: Vec2, b: Vec2) -> bool {
        !self.overlay[layer].blocks_segment(a, b) && !self.fixed(layer).blocks_segment(a, b)
    }
}

type Key = (i64, i64);

/// Узлы одного слоя и кто кого из них видит: битовая матрица по слотам. Слот занят, пока узел есть.
#[derive(Debug, Default)]
struct LayerGraph {
    slots: Vec<Slot>,
    free: Vec<u32>,
    by_key: HashMap<Key, u32>,
    words: usize,
    edges: Vec<u64>,
}

#[derive(Debug, Clone, Copy)]
struct Slot {
    pos: Vec2,
    active: bool,
}

impl LayerGraph {
    fn row(&self, slot: u32) -> &[u64] {
        let start = slot as usize * self.words;
        &self.edges[start..start + self.words]
    }

    fn insert(&mut self, pos: Vec2) -> u32 {
        let slot = match self.free.pop() {
            Some(slot) => {
                self.slots[slot as usize] = Slot { pos, active: true };
                slot
            }
            None => {
                self.slots.push(Slot { pos, active: true });
                self.grow();
                (self.slots.len() - 1) as u32
            }
        };
        self.by_key.insert(round_key(pos), slot);
        slot
    }

    /// Строк матрицы столько же, сколько слотов; ширина строки растёт вдвое, когда слотов больше, чем
    /// битов в ней.
    fn grow(&mut self) {
        let needed = self.slots.len().div_ceil(64).max(1);
        if needed > self.words {
            let words = (self.words * 2).max(needed);
            let mut edges = vec![0; self.slots.len() * words];
            for row in 0..self.slots.len() - 1 {
                edges[row * words..row * words + self.words]
                    .copy_from_slice(&self.edges[row * self.words..(row + 1) * self.words]);
            }
            self.edges = edges;
            self.words = words;
        } else {
            self.edges.resize(self.slots.len() * self.words, 0);
        }
    }

    fn remove(&mut self, slot: u32) {
        for other in self.neighbors(slot).collect::<Vec<_>>() {
            self.set_bit(other, slot, false);
        }
        let start = slot as usize * self.words;
        self.edges[start..start + self.words].fill(0);
        let pos = self.slots[slot as usize].pos;
        self.slots[slot as usize].active = false;
        self.by_key.remove(&round_key(pos));
        self.free.push(slot);
    }

    fn clear(&mut self) {
        *self = LayerGraph::default();
    }

    fn set_bit(&mut self, row: u32, column: u32, on: bool) {
        let index = row as usize * self.words + column as usize / 64;
        let bit = 1_u64 << (column % 64);
        if on {
            self.edges[index] |= bit;
        } else {
            self.edges[index] &= !bit;
        }
    }

    fn link(&mut self, a: u32, b: u32, on: bool) {
        self.set_bit(a, b, on);
        self.set_bit(b, a, on);
    }

    fn linked(&self, a: u32, b: u32) -> bool {
        (self.row(a)[b as usize / 64] >> (b % 64)) & 1 == 1
    }

    fn neighbors(&self, slot: u32) -> impl Iterator<Item = u32> + '_ {
        let row = self.row(slot);
        (0..self.words).flat_map(move |word| {
            let mut bits = row[word];
            std::iter::from_fn(move || {
                if bits == 0 {
                    return None;
                }
                let bit = bits.trailing_zeros();
                bits &= bits - 1;
                Some((word * 64) as u32 + bit)
            })
        })
    }

    fn active(&self) -> impl Iterator<Item = u32> + '_ {
        (0..self.slots.len() as u32).filter(|&slot| self.slots[slot as usize].active)
    }
}

/// Что поменялось на слое: препятствия, что убраны (какими были), и что появились.
#[derive(Default)]
struct LayerChange {
    removed: Vec<Obstacle>,
    added: Vec<Obstacle>,
}

/// Переход между слоями в слотах сети: откуда, куда и в какую сторону можно.
#[derive(Debug, Clone, Copy)]
struct Twin {
    a: (usize, u32),
    b: (usize, u32),
    a_to_b: bool,
    b_to_a: bool,
}

/// Всё, что зависит от объектов `avoid`: раздутые препятствия по слоям, узлы и видимость. Сеть из
/// неизменного не пересчитывается; при смене набора объектов пересчитывается лишь затронутое ими:
/// узлы убранных и вставших объектов и пары узлов, чей отрезок они пересекают.
#[derive(Debug, Default)]
struct Field {
    ready: bool,
    blockers: Vec<Blocker>,
    /// Раздутые объекты `avoid`, что мешают идущему на слое, по возрастанию номера объекта.
    items: Vec<Vec<(u32, Obstacle)>>,
    overlay: Vec<Indexed>,
    graphs: Vec<LayerGraph>,
    twins: Vec<Twin>,
}

/// Объекты, что убраны или сменились (их прежний вид уходит), и что появились или сменились (новый вид
/// приходит). Оба списка по возрастанию номера.
fn changed_blockers(old: &[Blocker], new: &[Blocker]) -> (HashSet<u32>, Vec<Blocker>) {
    let mut gone = HashSet::new();
    let mut came = Vec::new();
    let (mut i, mut j) = (0, 0);
    while i < old.len() || j < new.len() {
        match (old.get(i), new.get(j)) {
            (Some(a), Some(b)) if a.id == b.id => {
                if a != b {
                    gone.insert(a.id);
                    came.push(b.clone());
                }
                i += 1;
                j += 1;
            }
            (Some(a), Some(b)) if a.id < b.id => {
                gone.insert(a.id);
                i += 1;
            }
            (Some(a), None) => {
                gone.insert(a.id);
                i += 1;
            }
            (_, Some(b)) => {
                came.push(b.clone());
                j += 1;
            }
            (None, None) => break,
        }
    }
    (gone, came)
}

/// Столб идущего, стоящего основанием на `z`.
fn standing_at(body: &Body, z: f64) -> Pillar {
    Pillar {
        low: z,
        high: z + body.height,
        closed: false,
    }
}

/// Задевает ли столб объекта `avoid` столб идущего по рельефу где-нибудь у раздутого препятствия:
/// высота, на которой идущий стоит на рельефе, меняется от места к месту, поэтому сперва прикидка по
/// границам высот под препятствием, а на спорном — проверка точек внутри него.
fn ground_walker_meets(
    terrain: &Terrain,
    body: &Body,
    inflated: &Obstacle,
    pillar: &Pillar,
) -> bool {
    let bbox = inflated.bbox();
    let half = body.origin.aabb();
    let area = Footprint::flat(
        [bbox.x - half.w, bbox.y - half.h],
        [bbox.w + 2.0 * half.w, bbox.h + 2.0 * half.h],
    );
    let (lowest, highest) = terrain.range_under(&area);
    let (from, to) = (pillar.low - body.height, pillar.high);
    if highest <= from || lowest >= to {
        return false;
    }
    if lowest > from && highest < to {
        return true;
    }
    let step = SAMPLE_SPACING;
    let (columns, rows) = (
        (bbox.w / step).ceil() as usize + 1,
        (bbox.h / step).ceil() as usize + 1,
    );
    (0..rows).any(|row| {
        (0..columns).any(|column| {
            let p = [bbox.x + column as f64 * step, bbox.y + row as f64 * step];
            inflated.strictly_inside(p)
                && standing_at(body, terrain.min_under(&body.at(p))).overlaps(pillar)
        })
    })
}

/// Раздутый объект `avoid` на слое `layer`, если он мешает идущему на этой поверхности.
fn blocker_obstacle(
    nav: &SurfaceNav,
    terrain: &Terrain,
    body: &Body,
    blocker: &Blocker,
    layer: usize,
) -> Option<Obstacle> {
    let inflated = inflate(&blocker.place, &body.origin);
    let meets = if layer == 0 {
        ground_walker_meets(terrain, body, &inflated, &blocker.pillar)
    } else {
        let top = nav.decks[layer - 1].deck.top;
        standing_at(body, top).overlaps(&blocker.pillar)
    };
    meets.then_some(inflated)
}

impl Field {
    /// Приводит поле к объектам `avoid` из `surfaces`. Ничего не делает, когда они те же.
    fn sync(&mut self, nav: &SurfaceNav, surfaces: &Surfaces<'_>, body: &Body) {
        if self.ready && self.blockers == surfaces.blockers {
            return;
        }
        let layers = 1 + nav.decks.len();
        if !self.ready {
            let area = nav.terrain.bounds;
            self.items = vec![Vec::new(); layers];
            self.overlay = (0..layers)
                .map(|_| Indexed::new(Vec::new(), &area))
                .collect();
            self.graphs = (0..layers).map(|_| LayerGraph::default()).collect();
            self.ready = true;
        }
        let (gone, came) = changed_blockers(&self.blockers, &surfaces.blockers);
        let changes = self.replace_obstacles(nav, surfaces.terrain, body, &gone, &came);
        self.blockers = surfaces.blockers.clone();
        let Field {
            overlay,
            graphs,
            twins,
            ..
        } = self;
        let view = View {
            nav,
            overlay: overlay.as_slice(),
        };
        let portal_valid: Vec<bool> = nav
            .portals
            .iter()
            .map(|portal| {
                view.valid_crossing(portal.a, portal.pos)
                    && view.valid_crossing(portal.b, portal.pos)
            })
            .collect();
        for (layer, graph) in graphs.iter_mut().enumerate() {
            let desired = desired_nodes(&view, layer, &portal_valid);
            update_layer(&view, graph, layer, &desired, &changes[layer]);
        }
        *twins = nav
            .portals
            .iter()
            .zip(&portal_valid)
            .filter(|&(_, &valid)| valid)
            .filter_map(|(portal, _)| {
                let key = round_key(portal.pos);
                Some(Twin {
                    a: (portal.a, *graphs[portal.a].by_key.get(&key)?),
                    b: (portal.b, *graphs[portal.b].by_key.get(&key)?),
                    a_to_b: portal.a_to_b,
                    b_to_a: portal.b_to_a,
                })
            })
            .collect();
    }

    /// Убирает препятствия ушедших объектов и добавляет пришедших; возвращает, что поменялось на
    /// каждом слое. Слой, где ничего не поменялось, остаётся как есть.
    fn replace_obstacles(
        &mut self,
        nav: &SurfaceNav,
        terrain: &Terrain,
        body: &Body,
        gone: &HashSet<u32>,
        came: &[Blocker],
    ) -> Vec<LayerChange> {
        let area = nav.terrain.bounds;
        (0..self.items.len())
            .map(|layer| {
                let removed: Vec<Obstacle> = self.items[layer]
                    .iter()
                    .filter(|(id, _)| gone.contains(id))
                    .map(|(_, obstacle)| obstacle.clone())
                    .collect();
                let added: Vec<(u32, Obstacle)> = came
                    .iter()
                    .filter_map(|blocker| {
                        blocker_obstacle(nav, terrain, body, blocker, layer)
                            .map(|obstacle| (blocker.id, obstacle))
                    })
                    .collect();
                if removed.is_empty() && added.is_empty() {
                    return LayerChange::default();
                }
                let items = &mut self.items[layer];
                items.retain(|(id, _)| !gone.contains(id));
                items.extend(added.iter().cloned());
                items.sort_by_key(|(id, _)| *id);
                self.overlay[layer] = Indexed::new(
                    items.iter().map(|(_, obstacle)| obstacle.clone()).collect(),
                    &area,
                );
                LayerChange {
                    removed,
                    added: added.into_iter().map(|(_, obstacle)| obstacle).collect(),
                }
            })
            .collect()
    }
}

/// Узлы слоя при нынешних объектах `avoid`: углы неизменных препятствий, что не под объектами, углы
/// самих объектов, что свободны, и концы переходов. Без повторов, в порядке появления.
fn desired_nodes(view: &View<'_>, layer: usize, portal_valid: &[bool]) -> Vec<(Key, Vec2)> {
    let mut seen = HashSet::new();
    let mut nodes = Vec::new();
    let mut push = |pos: Vec2| {
        let key = round_key(pos);
        if seen.insert(key) {
            nodes.push((key, pos));
        }
    };
    let overlay = &view.overlay[layer];
    let fixed_nodes = match layer {
        0 => &view.nav.terrain.nodes,
        _ => &view.nav.decks[layer - 1].nodes,
    };
    for &pos in fixed_nodes {
        if !overlay.holds(pos, None) {
            push(pos);
        }
    }
    for (i, obstacle) in overlay.obstacles.iter().enumerate() {
        for corner in obstacle.corners() {
            if view.in_region(layer, corner) && !view.blocked(layer, corner, Some(i)) {
                push(corner);
            }
        }
    }
    for (portal, &valid) in view.nav.portals.iter().zip(portal_valid) {
        if valid && (portal.a == layer || portal.b == layer) {
            push(portal.pos);
        }
    }
    nodes
}

/// Приводит узлы и видимость слоя к `desired`. Уцелевшие пары пересматриваются только там, где
/// изменившиеся препятствия могли что-то поменять; новые узлы смотрят на все узлы слоя.
fn update_layer(
    view: &View<'_>,
    graph: &mut LayerGraph,
    layer: usize,
    desired: &[(Key, Vec2)],
    change: &LayerChange,
) {
    if change.removed.len() + change.added.len() > REBUILD_LIMIT {
        graph.clear();
    }
    let wanted: HashSet<Key> = desired.iter().map(|&(key, _)| key).collect();
    let stale: Vec<u32> = graph
        .by_key
        .iter()
        .filter(|(key, _)| !wanted.contains(key))
        .map(|(_, &slot)| slot)
        .collect();
    for slot in stale {
        graph.remove(slot);
    }
    if !change.removed.is_empty() || !change.added.is_empty() {
        recheck_pairs(view, graph, layer, change);
    }
    for &(key, pos) in desired {
        if graph.by_key.contains_key(&key) {
            continue;
        }
        let others: Vec<u32> = graph.active().collect();
        let slot = graph.insert(pos);
        for other in others {
            if view.sees(layer, pos, graph.slots[other as usize].pos) {
                graph.link(slot, other, true);
            }
        }
    }
}

/// Направления от точки `from` на крайние углы выпуклого препятствия: отрезок из `from` задевает
/// препятствие, только если идёт между ними. `None`, когда `from` не снаружи препятствия и такого угла
/// нет.
fn cone_to(corners: &[Vec2], from: Vec2) -> Option<(Vec2, Vec2)> {
    let cross = |a: Vec2, b: Vec2| a[0] * b[1] - a[1] * b[0];
    let mut directions = corners.iter().map(|c| [c[0] - from[0], c[1] - from[1]]);
    let first = directions.next()?;
    let (mut low, mut high) = (first, first);
    for d in directions {
        if cross(low, d) < 0.0 {
            low = d;
        }
        if cross(d, high) < 0.0 {
            high = d;
        }
    }
    (cross(low, high) > TOLERANCE).then_some((low, high))
}

/// Пары уцелевших узлов: видимость, которую перегородил вставший объект, гаснет; видимость, которую
/// перегораживал убранный, считается заново. Пары, чей отрезок вставший или убранный объект не
/// задевает, не трогаются.
fn recheck_pairs(view: &View<'_>, graph: &mut LayerGraph, layer: usize, change: &LayerChange) {
    let survivors: Vec<u32> = graph.active().collect();
    let points: Vec<Vec2> = survivors
        .iter()
        .map(|&slot| graph.slots[slot as usize].pos)
        .collect();
    let mut index = vec![usize::MAX; graph.slots.len()];
    for (i, &slot) in survivors.iter().enumerate() {
        index[slot as usize] = i;
    }
    for obstacle in &change.added {
        let mut hidden = Vec::new();
        for (i, &a) in survivors.iter().enumerate() {
            for b in graph.neighbors(a) {
                let j = index[b as usize];
                if j > i && blocks_walk(obstacle, points[i], points[j]) {
                    hidden.push((a, b));
                }
            }
        }
        for (a, b) in hidden {
            graph.link(a, b, false);
        }
    }
    let (xs, ys): (Vec<f64>, Vec<f64>) = points.iter().map(|p| (p[0], p[1])).unzip();
    for obstacle in &change.removed {
        let corners = obstacle.corners();
        let mut opened = Vec::new();
        for (i, &a) in survivors.iter().enumerate() {
            let cone = cone_to(&corners, points[i]);
            for (j, &b) in survivors.iter().enumerate().skip(i + 1) {
                if let Some((low, high)) = cone {
                    let (tx, ty) = (xs[j] - xs[i], ys[j] - ys[i]);
                    if low[0] * ty - low[1] * tx < -TOLERANCE
                        || tx * high[1] - ty * high[0] < -TOLERANCE
                    {
                        continue;
                    }
                }
                if !graph.linked(a, b)
                    && blocks_walk(obstacle, points[i], points[j])
                    && view.sees(layer, points[i], points[j])
                {
                    opened.push((a, b));
                }
            }
        }
        for (a, b) in opened {
            graph.link(a, b, true);
        }
    }
}

// -------------------------------------------------------------------------------------------
// Путь
// -------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
struct Node {
    pos: Vec2,
    layer: usize,
}

struct Search<'a> {
    nav: &'a SurfaceNav,
    field: &'a Field,
    surfaces: &'a Surfaces<'a>,
    body: Body,
    nodes: Vec<Node>,
    /// Слой и слот сети каждого узла, кроме начала и цели пути.
    slots: Vec<(usize, u32)>,
    layer_nodes: Vec<Vec<usize>>,
    /// Номер узла по слою и слоту: узлы идут по слоям, а в слое — по `x`, потом по `y`, чтобы путь не
    /// зависел от того, в каком порядке сеть получала узлы.
    rank: Vec<Vec<u32>>,
    twins: Vec<Vec<usize>>,
}

impl<'a> Search<'a> {
    fn new(
        nav: &'a SurfaceNav,
        field: &'a Field,
        surfaces: &'a Surfaces<'a>,
        walker: &Walker,
    ) -> Search<'a> {
        let layers = 1 + nav.decks.len();
        let mut search = Search {
            nav,
            field,
            surfaces,
            body: Body::new(walker.size, walker.rotation, walker.height),
            nodes: Vec::new(),
            slots: Vec::new(),
            layer_nodes: vec![Vec::new(); layers],
            rank: field
                .graphs
                .iter()
                .map(|graph| vec![u32::MAX; graph.slots.len()])
                .collect(),
            twins: Vec::new(),
        };
        for (layer, graph) in field.graphs.iter().enumerate() {
            let mut slots: Vec<u32> = graph.active().collect();
            slots.sort_by(|&a, &b| {
                let (p, q) = (graph.slots[a as usize].pos, graph.slots[b as usize].pos);
                p[0].total_cmp(&q[0]).then(p[1].total_cmp(&q[1]))
            });
            for slot in slots {
                let id = search.push_node(graph.slots[slot as usize].pos, layer);
                search.slots.push((layer, slot));
                search.rank[layer][slot as usize] = id as u32;
            }
        }
        for twin in &field.twins {
            let a = search.rank[twin.a.0][twin.a.1 as usize] as usize;
            let b = search.rank[twin.b.0][twin.b.1 as usize] as usize;
            if twin.a_to_b {
                search.twins[a].push(b);
            }
            if twin.b_to_a {
                search.twins[b].push(a);
            }
        }
        search
    }

    fn view(&self) -> View<'a> {
        View {
            nav: self.nav,
            overlay: self.field.overlay.as_slice(),
        }
    }

    fn layer_z(&self, layer: usize, p: Vec2) -> f64 {
        match layer {
            0 => self.surfaces.terrain.min_under(&self.body.at(p)),
            _ => self.nav.decks[layer - 1].deck.top,
        }
    }

    fn push_node(&mut self, pos: Vec2, layer: usize) -> usize {
        let id = self.nodes.len();
        self.nodes.push(Node { pos, layer });
        self.layer_nodes[layer].push(id);
        self.twins.push(Vec::new());
        id
    }

    /// Стороны области слоя: края сцены для рельефа, многоугольник настила для настила.
    fn region_edges(&self, layer: usize) -> Vec<(Vec2, Vec2)> {
        if layer == 0 {
            return Obstacle::Rect(self.nav.terrain.bounds).edges();
        }
        let Some(region) = &self.nav.decks[layer - 1].region else {
            return Vec::new();
        };
        (0..region.verts.len())
            .map(|i| (region.verts[i], region.verts[(i + 1) % region.verts.len()]))
            .collect()
    }

    /// Ближайшая к `p` точка слоя, где идущему можно стоять: сама `p`, если она свободна, иначе
    /// ближайшая точка на краю препятствий или области слоя. Сперва `p` выходит из держащих её
    /// препятствий по прямой к ближайшему краю, и так, пока не выйдет; эта точка ограничивает поиск, а
    /// ближе неё — ближайшие к `p` точки сторон окрестных препятствий и области, что не лежат внутри
    /// других препятствий. Когда идущий сам выходит из препятствия (`walking_out`), точки, к которым он
    /// шёл бы сквозь препятствия, что его не держат, не годятся: выйти из одной стены, зайдя в другую,
    /// нельзя, пока есть выход без этого.
    fn escape(&self, layer: usize, p: Vec2, walking_out: bool) -> Vec2 {
        let view = self.view();
        if view.valid(layer, p) {
            return p;
        }
        let (fixed, dynamic) = (&view.fixed(layer).obstacles, &view.overlay[layer].obstacles);
        let mut exit = p;
        for _ in 0..fixed.len() + dynamic.len() + 1 {
            let holder = fixed
                .iter()
                .chain(dynamic.iter())
                .find(|o| o.strictly_inside(exit));
            match holder {
                Some(o) => exit = o.exit(exit),
                None => break,
            }
        }
        let others: Vec<&Obstacle> = fixed
            .iter()
            .chain(dynamic.iter())
            .filter(|o| !o.strictly_inside(p))
            .collect();
        let walkable = |q: Vec2| {
            view.valid(layer, q) && !(walking_out && others.iter().any(|o| blocks_walk(o, p, q)))
        };
        let mut best = if walkable(exit) {
            (dist(p, exit), exit)
        } else {
            (f64::INFINITY, p)
        };
        let reach = best.0;
        let obstacle_edges = fixed
            .iter()
            .chain(dynamic.iter())
            .filter(|o| {
                let bbox = o.bbox();
                bbox.x <= p[0] + reach
                    && bbox.x + bbox.w >= p[0] - reach
                    && bbox.y <= p[1] + reach
                    && bbox.y + bbox.h >= p[1] - reach
            })
            .flat_map(Obstacle::edges);
        for (a, b) in self.region_edges(layer).into_iter().chain(obstacle_edges) {
            let q = closest_on_segment(p, a, b);
            let d = dist(p, q);
            if d < best.0 - TOLERANCE && walkable(q) {
                best = (d, q);
            }
        }
        if best.0.is_infinite() && view.valid(layer, exit) {
            return exit;
        }
        best.1
    }

    /// Слой, в котором идущий стоит сейчас.
    fn start_layer(&self, walker: &Walker) -> usize {
        let body = self.body.at(walker.center);
        self.nav
            .decks
            .iter()
            .enumerate()
            .filter(|(_, layer)| {
                (layer.deck.top - walker.z).abs() < 1e-6
                    && layer.deck.place.overlaps_deeper_than(&body, SEAT_DEPTH)
            })
            .max_by(|(_, a), (_, b)| a.deck.top.total_cmp(&b.deck.top))
            .map_or(0, |(i, _)| 1 + i)
    }

    /// Слои и точки, куда идти: среди поверхностей над целью — ближайшие к названной высоте. Если таких
    /// несколько одинаково близких (вровень с землёй лежит верх настила), годится любая: путь выберет ту,
    /// до которой короче.
    fn goal_places(&self, goal: &Goal) -> Vec<Node> {
        let view = self.view();
        let mut found: Vec<(f64, Node)> = Vec::new();
        for layer in 0..=self.nav.decks.len() {
            if layer > 0 && !view.in_region(layer, goal.point) {
                continue;
            }
            let clamped = if layer == 0 {
                let bounds = &self.nav.terrain.bounds;
                [
                    goal.point[0].clamp(bounds.x, bounds.x + bounds.w),
                    goal.point[1].clamp(bounds.y, bounds.y + bounds.h),
                ]
            } else {
                goal.point
            };
            let place = self.escape(layer, clamped, false);
            if !view.in_region(layer, place) {
                continue;
            }
            let along = dist(place, goal.point);
            let up = self.layer_z(layer, place) - goal.wanted_z;
            let distance = (along * along + up * up).sqrt();
            found.push((distance, Node { pos: place, layer }));
        }
        let closest = found.iter().map(|&(d, _)| d).fold(f64::INFINITY, f64::min);
        let places: Vec<Node> = found
            .into_iter()
            .filter(|&(d, _)| d <= closest + GOAL_TIE)
            .map(|(_, node)| node)
            .collect();
        if places.is_empty() {
            vec![Node {
                pos: goal.point,
                layer: 0,
            }]
        } else {
            places
        }
    }

    /// Кратчайший путь от `start` до любой из `goals`; когда до цели не дойти — до ближайшего достигнутого
    /// места (по трёхмерному расстоянию до цели).
    fn search(&mut self, start: Node, goals: &[Node], goal_wanted: (Vec2, f64)) -> Vec<Vec2> {
        let view = self.view();
        let fixed = self.nodes.len();
        let start_id = self.push_node(start.pos, start.layer);
        let goal_ids: Vec<usize> = goals
            .iter()
            .map(|goal| self.push_node(goal.pos, goal.layer))
            .collect();
        let start_sees: Vec<usize> = self.layer_nodes[start.layer]
            .iter()
            .copied()
            .filter(|&j| j < fixed && view.sees(start.layer, start.pos, self.nodes[j].pos))
            .collect();
        let mut sees_goal: Vec<Vec<usize>> = vec![Vec::new(); fixed];
        for (&goal_id, goal) in goal_ids.iter().zip(goals) {
            for &j in &self.layer_nodes[goal.layer] {
                if j < fixed && view.sees(goal.layer, self.nodes[j].pos, goal.pos) {
                    sees_goal[j].push(goal_id);
                }
            }
        }
        let start_sees_goals: Vec<usize> = goal_ids
            .iter()
            .zip(goals)
            .filter(|&(_, goal)| {
                start.layer == goal.layer && view.sees(start.layer, start.pos, goal.pos)
            })
            .map(|(&goal_id, _)| goal_id)
            .collect();
        let count = self.nodes.len();
        let mut g = vec![f64::INFINITY; count];
        let mut came_from = vec![usize::MAX; count];
        let mut closed = vec![false; count];
        let mut open = BinaryHeap::new();
        let mut neighbors: Vec<usize> = Vec::new();
        let to_goal = |pos: Vec2| {
            goals
                .iter()
                .map(|goal| dist(pos, goal.pos))
                .fold(f64::INFINITY, f64::min)
        };
        let best_goal = |g: &[f64]| {
            goal_ids
                .iter()
                .map(|&id| g[id])
                .fold(f64::INFINITY, f64::min)
        };
        g[start_id] = 0.0;
        open.push(HeapEntry {
            cost: to_goal(start.pos),
            node: start_id,
        });
        while let Some(HeapEntry { node, .. }) = open.pop() {
            if closed[node] {
                continue;
            }
            closed[node] = true;
            if goal_ids.contains(&node) {
                break;
            }
            let here = self.nodes[node];
            neighbors.clear();
            if node == start_id {
                neighbors.extend(&start_sees);
                neighbors.extend(&start_sees_goals);
            } else {
                let (layer, slot) = self.slots[node];
                neighbors.extend(
                    self.field.graphs[layer]
                        .neighbors(slot)
                        .map(|other| self.rank[layer][other as usize] as usize),
                );
                neighbors.sort_unstable();
                neighbors.extend(&sees_goal[node]);
            }
            for &other in &neighbors {
                if closed[other] {
                    continue;
                }
                let there = self.nodes[other];
                let tentative = g[node] + dist(here.pos, there.pos);
                if tentative >= g[other] - TOLERANCE
                    || tentative + to_goal(there.pos) >= best_goal(&g) - TOLERANCE
                {
                    continue;
                }
                g[other] = tentative;
                came_from[other] = node;
                open.push(HeapEntry {
                    cost: tentative + to_goal(there.pos),
                    node: other,
                });
            }
            for &twin in &self.twins[node] {
                if closed[twin]
                    || g[node] >= g[twin] - TOLERANCE
                    || g[node] + to_goal(self.nodes[twin].pos) >= best_goal(&g) - TOLERANCE
                {
                    continue;
                }
                g[twin] = g[node];
                came_from[twin] = node;
                open.push(HeapEntry {
                    cost: g[node] + to_goal(self.nodes[twin].pos),
                    node: twin,
                });
            }
        }
        let reached = goal_ids
            .iter()
            .copied()
            .filter(|&id| g[id].is_finite())
            .min_by(|&a, &b| g[a].total_cmp(&g[b]).then(a.cmp(&b)));
        let end = reached.unwrap_or_else(|| self.closest_reached(&g, start_id, goal_wanted));
        let mut path = vec![self.nodes[end].pos];
        let mut node = end;
        while came_from[node] != usize::MAX {
            node = came_from[node];
            path.push(self.nodes[node].pos);
        }
        path.reverse();
        path.remove(0);
        path.dedup_by(|a, b| dist(*a, *b) < 1e-9);
        path
    }

    fn closest_reached(&self, g: &[f64], start: usize, (point, z): (Vec2, f64)) -> usize {
        (0..g.len())
            .filter(|&i| i != start && g[i].is_finite())
            .min_by(|&a, &b| {
                let score = |i: usize| {
                    let node = self.nodes[i];
                    let up = self.layer_z(node.layer, node.pos) - z;
                    let along = dist(node.pos, point);
                    (along * along + up * up).sqrt()
                };
                score(a).total_cmp(&score(b)).then_with(|| a.cmp(&b))
            })
            .unwrap_or(start)
    }
}

/// Заходит ли отрезок `p → q` строго внутрь препятствия.
fn blocks_walk(obstacle: &Obstacle, p: Vec2, q: Vec2) -> bool {
    let bbox = (
        p[0].min(q[0]),
        p[0].max(q[0]),
        p[1].min(q[1]),
        p[1].max(q[1]),
    );
    obstacle.blocks_segment(p, q, bbox)
}

fn nav_key(surfaces: &Surfaces<'_>, walker: &Walker, scene: (f64, f64)) -> NavKey {
    NavKey {
        size: walker.size,
        rotation: walker.rotation,
        height: walker.height,
        scene,
        decks: surfaces.decks.clone(),
        terrain: std::ptr::from_ref(surfaces.terrain) as usize,
    }
}

/// Сеть для этого идущего и этих земли и настилов: запомненная или, если такой нет, новая.
fn navigation(
    surfaces: &Surfaces<'_>,
    walker: &Walker,
    scene: (f64, f64),
    caches: &mut WalkCaches,
) -> Rc<SurfaceNav> {
    let key = nav_key(surfaces, walker, scene);
    if let Some(known) = caches.navigations().iter().find(|nav| nav.key == key) {
        return Rc::clone(known);
    }
    let nav = Rc::new(SurfaceNav::build(surfaces, walker, scene, key));
    caches.add_navigation(Rc::clone(&nav));
    nav
}

/// Сеть для этого идущего с узлами и видимостью при нынешних объектах `avoid`.
fn ready_navigation(
    surfaces: &Surfaces<'_>,
    walker: &Walker,
    scene: (f64, f64),
    caches: &mut WalkCaches,
) -> Rc<SurfaceNav> {
    let nav = navigation(surfaces, walker, scene, caches);
    let body = Body::new(walker.size, walker.rotation, walker.height);
    nav.field.borrow_mut().sync(&nav, surfaces, &body);
    nav
}

/// Готовит идущему всё, что не зависит от цели: сеть из земли и настилов, узлы и видимость при нынешних
/// объектах `avoid`. Зовётся при сборке мира, чтобы первый путь не платил за это.
pub fn prepare(
    surfaces: &Surfaces<'_>,
    walker: &Walker,
    scene: (f64, f64),
    caches: &mut WalkCaches,
) {
    ready_navigation(surfaces, walker, scene, caches);
}

/// Ломаная от середины идущего до цели по поверхностям; пустая, если идти некуда.
pub fn plan(
    surfaces: &Surfaces<'_>,
    walker: &Walker,
    goal: &Goal,
    scene: (f64, f64),
    caches: &mut WalkCaches,
) -> Vec<Vec2> {
    let nav = ready_navigation(surfaces, walker, scene, caches);
    let field = nav.field.borrow();
    let mut search = Search::new(&nav, &field, surfaces, walker);
    let start_layer = search.start_layer(walker);
    let start = Node {
        pos: search.escape(start_layer, walker.center, true),
        layer: start_layer,
    };
    let goals = search.goal_places(goal);
    let mut path = search.search(start, &goals, (goal.point, goal.wanted_z));
    if dist(start.pos, walker.center) > 1e-9 {
        path.insert(0, start.pos);
    }
    path
}

/// Идёт к цели по поверхностям на `budget` клеток этого шага: новая середина и «дошёл ли». Путь
/// не пересчитывается, пока настилы, объекты `avoid`, цель и тело идущего те же.
pub fn advance(
    walker: &Walker,
    goal: &Goal,
    surfaces: &Surfaces<'_>,
    scene: (f64, f64),
    budget: f64,
    caches: &mut WalkCaches,
) -> (Vec2, bool) {
    let frame = pathfind::frame(walker.center, walker.size, walker.rotation, scene);
    let target = [
        goal.point[0].clamp(frame.bounds.x, frame.bounds.x + frame.bounds.w),
        goal.point[1].clamp(frame.bounds.y, frame.bounds.y + frame.bounds.h),
    ];
    let key = PathKey {
        target: goal.point,
        size: walker.size,
        rotation: walker.rotation,
        obstacles: Vec::new(),
        surface: Some(SurfaceKey {
            decks: surfaces.decks.clone(),
            blockers: surfaces.blockers.clone(),
            height: walker.height,
            target_z: goal.named_z,
        }),
    };
    let goal = Goal {
        point: target,
        named_z: goal.named_z,
        wanted_z: goal.wanted_z,
    };
    pathfind::ensure_path(walker.id, key, caches, |caches| {
        plan(surfaces, walker, &goal, scene, caches)
    });
    pathfind::follow(walker.id, walker.center, budget, &frame, caches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, w: f64, h: f64) -> [Vec2; 4] {
        [[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
    }

    #[test]
    fn what_is_left_of_a_polygon_after_a_rectangle_is_cut_out_of_it_covers_the_rest() {
        let polygon = square(0.0, 0.0, 4.0, 4.0);
        let parts = subtract_convex(&polygon, &square(1.0, 1.0, 2.0, 2.0));
        let total: f64 = parts.iter().map(|p| area(p)).sum();
        assert!((total - 12.0).abs() < 1e-9, "{total}");
        let untouched = subtract_convex(&polygon, &square(10.0, 10.0, 1.0, 1.0));
        assert_eq!(untouched.len(), 1);
        assert!((area(&untouched[0]) - 16.0).abs() < 1e-9);
        assert!(subtract_convex(&polygon, &square(-1.0, -1.0, 6.0, 6.0)).is_empty());
    }

    #[test]
    fn a_rectangle_without_area_cuts_nothing_out() {
        let polygon = square(0.0, 0.0, 4.0, 4.0);
        for empty in [square(1.0, 1.0, 0.0, 0.0), square(1.0, 1.0, 2.0, 0.0)] {
            let parts = subtract_convex(&polygon, &empty);
            assert_eq!(parts.len(), 1);
            assert!((area(&parts[0]) - 16.0).abs() < 1e-9);
        }
    }

    #[test]
    fn samples_along_a_boundary_are_no_further_apart_than_the_spacing() {
        let poly = Poly::hull(square(0.0, 0.0, 2.0, 1.0).to_vec()).expect("квадрат");
        let samples = boundary_samples(&poly, 0.5);
        assert_eq!(samples.len(), 12);
    }

    /// Обход ячеек вдоль отрезка находит ровно те преграды, что нашёл бы перебор всех.
    #[test]
    fn the_indexed_walk_along_a_segment_agrees_with_checking_every_obstacle() {
        let mut seed = 12_345_u64;
        let mut random = move |low: f64, high: f64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            low + (high - low) * ((seed >> 33) as f64 / (1_u64 << 31) as f64)
        };
        let area = Rect {
            x: 0.0,
            y: 0.0,
            w: 32.0,
            h: 24.0,
        };
        let obstacles: Vec<Obstacle> = (0..40)
            .map(|_| {
                let (x, y) = (random(0.0, 31.0), random(0.0, 23.0));
                Obstacle::Rect(Rect {
                    x,
                    y,
                    w: random(0.2, 3.0),
                    h: random(0.2, 3.0),
                })
            })
            .collect();
        let indexed = Indexed::new(obstacles, &area);
        let mut blocked = 0;
        for _ in 0..400 {
            let a = [random(0.0, 32.0), random(0.0, 24.0)];
            let b = [random(0.0, 32.0), random(0.0, 24.0)];
            let bbox = (
                a[0].min(b[0]),
                a[0].max(b[0]),
                a[1].min(b[1]),
                a[1].max(b[1]),
            );
            let expected = indexed
                .obstacles
                .iter()
                .any(|o| o.blocks_segment(a, b, bbox));
            assert_eq!(indexed.blocks_segment(a, b), expected, "{a:?} → {b:?}");
            blocked += usize::from(expected);
        }
        assert!(
            blocked > 50 && blocked < 350,
            "проверка различает: {blocked}"
        );
    }

    fn any_crossing() -> Crossing {
        Crossing {
            enter: true,
            leave: true,
        }
    }

    /// Свободная часть стороны, что уже шага сетки и между двумя её точками, всё равно даёт переходы — на
    /// своих концах: иначе узкая ступень, чья часть у края настила короче `PORTAL_SPACING`, была бы стеной.
    #[test]
    fn a_free_part_of_a_side_between_two_grid_points_gets_a_crossing_at_each_end() {
        let points =
            Builder::crossing_points([0.0, 0.0], [2.0, 0.0], &[(0.3, 0.34)], |_| any_crossing());
        let mut xs: Vec<f64> = points.iter().map(|(p, _)| p[0]).collect();
        xs.sort_by(f64::total_cmp);
        assert_eq!(xs.len(), 2, "{xs:?}");
        assert!(
            (xs[0] - 0.6).abs() < 1e-9 && (xs[1] - 0.68).abs() < 1e-9,
            "{xs:?}"
        );
    }

    #[test]
    fn a_side_without_obstacles_gets_crossings_on_the_grid_and_none_are_added_at_its_ends() {
        let points =
            Builder::crossing_points([0.0, 0.0], [2.0, 0.0], &[(0.0, 1.0)], |_| any_crossing());
        let xs: Vec<f64> = points.iter().map(|(p, _)| p[0]).collect();
        assert_eq!(xs, [0.0, 0.5, 1.0, 1.5]);
    }

    /// Разрешение перехода меняется между двумя точками сетки: переход есть в самой перемене, а не только
    /// в следующей точке сетки.
    #[test]
    fn a_crossing_is_placed_where_the_permission_begins_between_two_grid_points() {
        let points =
            Builder::crossing_points([0.0, 0.0], [2.0, 0.0], &[(0.0, 1.0)], |q| Crossing {
                enter: q[0] >= 0.6,
                leave: false,
            });
        let first = points
            .iter()
            .map(|(p, _)| p[0])
            .fold(f64::INFINITY, f64::min);
        assert!((first - 0.6).abs() < 1e-6, "{first}");
    }

    #[test]
    fn spans_are_cut_by_open_intervals_and_intersected() {
        let cut = subtract_open(&[(0.0, 1.0)], 0.2, 0.4);
        assert_eq!(cut, [(0.0, 0.2), (0.4, 1.0)]);
        assert_eq!(subtract_open(&cut, 0.0, 1.0), []);
        assert_eq!(subtract_open(&cut, 0.2, 0.4), cut, "по пустому промежутку");
        assert_eq!(
            intersect_spans(&[(0.0, 0.2), (0.4, 1.0)], &[(0.1, 0.5)]),
            [(0.1, 0.2), (0.4, 0.5)]
        );
    }

    fn random_blockers(random: &mut impl FnMut(f64, f64) -> f64, count: usize) -> Vec<Blocker> {
        (0..count as u32)
            .map(|id| Blocker {
                id,
                place: Footprint::flat(
                    [random(0.5, 10.0), random(0.5, 6.5)],
                    [random(0.3, 2.0), random(0.3, 2.0)],
                ),
                pillar: Pillar {
                    low: 0.0,
                    high: random(1.0, 3.0),
                    closed: false,
                },
            })
            .collect()
    }

    /// Узлы и рёбра видимости всех слоёв: что видно, а не в каком порядке сеть получала узлы.
    fn snapshot(field: &Field) -> Vec<(usize, Key, Key)> {
        let mut shot = Vec::new();
        for (layer, graph) in field.graphs.iter().enumerate() {
            for a in graph.active() {
                let ka = round_key(graph.slots[a as usize].pos);
                shot.push((layer, ka, ka));
                for b in graph.neighbors(a) {
                    let kb = round_key(graph.slots[b as usize].pos);
                    if ka < kb {
                        shot.push((layer, ka, kb));
                    }
                }
            }
        }
        shot.sort_unstable();
        shot
    }

    /// Поле, которое подгоняли под меняющиеся объекты `avoid`, видит то же, что поле, построенное с
    /// нуля под последний набор: убранные, вставшие и сдвинувшиеся объекты, мало и много разом.
    #[test]
    fn a_field_kept_up_to_date_sees_what_a_fresh_one_would() {
        let mut seed = 987_654_321_u64;
        let mut random = move |low: f64, high: f64| {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            low + (high - low) * ((seed >> 33) as f64 / (1_u64 << 31) as f64)
        };
        let terrain =
            Terrain::from_rows([12, 8], &vec![vec![0.0; 25]; 17], None).expect("размеры сходятся");
        let decks = vec![Deck {
            id: 100,
            place: Footprint::flat([4.0, 2.0], [3.0, 2.0]),
            bottom: 0.0,
            top: 0.3,
        }];
        let walker = Walker {
            id: 0,
            center: [1.0, 1.0],
            size: [0.6, 0.6],
            rotation: None,
            height: 1.8,
            z: 0.0,
        };
        let scene = (12.0, 8.0);
        let surfaces = |blockers: Vec<Blocker>| Surfaces {
            terrain: &terrain,
            decks: decks.clone(),
            blockers,
        };
        let body = Body::new(walker.size, walker.rotation, walker.height);
        let mut caches = WalkCaches::new();
        let mut blockers = random_blockers(&mut random, 10);
        let nav = ready_navigation(&surfaces(blockers.clone()), &walker, scene, &mut caches);
        for round in 0..40 {
            match round % 4 {
                0 if !blockers.is_empty() => {
                    blockers.remove(
                        (random(0.0, blockers.len() as f64) as usize).min(blockers.len() - 1),
                    );
                }
                1 => {
                    let id = blockers.iter().map(|b| b.id).max().map_or(0, |id| id + 1);
                    let mut fresh = random_blockers(&mut random, 1);
                    fresh[0].id = id;
                    blockers.extend(fresh);
                }
                2 if !blockers.is_empty() => {
                    let at = (random(0.0, blockers.len() as f64) as usize).min(blockers.len() - 1);
                    blockers[at].place = Footprint::flat(
                        [random(0.5, 10.0), random(0.5, 6.5)],
                        [random(0.3, 2.0), random(0.3, 2.0)],
                    );
                }
                _ => {
                    let count = (random(0.0, 30.0) as usize).max(1);
                    blockers = random_blockers(&mut random, count);
                }
            }
            let now = surfaces(blockers.clone());
            nav.field.borrow_mut().sync(&nav, &now, &body);
            let mut fresh = Field::default();
            fresh.sync(&nav, &now, &body);
            assert_eq!(
                snapshot(&nav.field.borrow()),
                snapshot(&fresh),
                "круг {round}"
            );
            assert_eq!(
                nav.field.borrow().twins.len(),
                fresh.twins.len(),
                "круг {round}"
            );
        }
    }

    /// Стену обходят, пока она стоит, и идут прямо, когда её убрали, и снова обходят, когда она вернулась.
    #[test]
    fn a_wall_is_walked_around_while_it_stands_and_not_once_it_is_gone() {
        let terrain =
            Terrain::from_rows([10, 6], &vec![vec![0.0; 21]; 13], None).expect("размеры сходятся");
        let wall = Blocker {
            id: 1,
            place: Footprint::flat([4.0, 1.0], [1.0, 4.0]),
            pillar: Pillar {
                low: 0.0,
                high: 2.0,
                closed: false,
            },
        };
        let surfaces = |blockers: Vec<Blocker>| Surfaces {
            terrain: &terrain,
            decks: Vec::new(),
            blockers,
        };
        let walker = Walker {
            id: 0,
            center: [1.0, 3.0],
            size: [0.6, 0.6],
            rotation: None,
            height: 1.8,
            z: 0.0,
        };
        let goal = Goal {
            point: [8.0, 3.0],
            named_z: None,
            wanted_z: 0.0,
        };
        let scene = (10.0, 6.0);
        let mut caches = WalkCaches::new();
        let walled = surfaces(vec![wall]);
        let around = plan(&walled, &walker, &goal, scene, &mut caches);
        assert!(around.len() > 1, "стену обходят: {around:?}");
        assert_eq!(plan(&walled, &walker, &goal, scene, &mut caches), around);
        let open = plan(&surfaces(Vec::new()), &walker, &goal, scene, &mut caches);
        assert_eq!(open, vec![[8.0, 3.0]], "без стены идут прямо");
        assert_eq!(plan(&walled, &walker, &goal, scene, &mut caches), around);
        assert_eq!(caches.navigations().len(), 1, "сеть одна на все три пути");
    }

    /// Что зависит от земли, настилов и объектов `avoid`, готово после `prepare`: путь берёт готовое, а не
    /// строит ни сеть, ни поле, — ни для того же набора объектов, ни для того же идущего.
    #[test]
    fn what_prepare_readies_is_all_a_path_needs() {
        let terrain =
            Terrain::from_rows([10, 6], &vec![vec![0.0; 21]; 13], None).expect("размеры сходятся");
        let wall = Blocker {
            id: 1,
            place: Footprint::flat([4.0, 1.0], [1.0, 4.0]),
            pillar: Pillar {
                low: 0.0,
                high: 2.0,
                closed: false,
            },
        };
        let surfaces = Surfaces {
            terrain: &terrain,
            decks: Vec::new(),
            blockers: vec![wall],
        };
        let walker = Walker {
            id: 0,
            center: [1.0, 3.0],
            size: [0.6, 0.6],
            rotation: None,
            height: 1.8,
            z: 0.0,
        };
        let goal = Goal {
            point: [8.0, 3.0],
            named_z: None,
            wanted_z: 0.0,
        };
        let scene = (10.0, 6.0);
        let mut caches = WalkCaches::new();
        prepare(&surfaces, &walker, scene, &mut caches);
        assert_eq!(caches.navigations().len(), 1);
        let nav = Rc::clone(&caches.navigations()[0]);
        assert!(nav.field.borrow().ready, "поле готово до первого пути");
        let ready = snapshot(&nav.field.borrow());
        assert!(ready.len() > 4, "узлы и видимость посчитаны: {ready:?}");
        let around = plan(&surfaces, &walker, &goal, scene, &mut caches);
        assert!(around.len() > 1, "стену обходят: {around:?}");
        assert!(Rc::ptr_eq(&nav, &caches.navigations()[0]), "сеть та же");
        assert_eq!(snapshot(&nav.field.borrow()), ready, "поле то же");
    }
}

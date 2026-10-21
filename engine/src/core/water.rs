//! «Рельеф» → «Вода»: записи воды файла рельефа — море, озёра и реки — и гладь над каждой точкой
//! сетки рельефа, которую движок считает по ним один раз при сборке рельефа: какие точки залиты,
//! на какой высоте стоит вода, куда она течёт и где пенится. Ход, луч мыши, камера и отрисовка
//! читают эту одну гладь.

use std::collections::{HashMap, VecDeque};

use super::value::Vec2;

/// `clarity` и `ripple`, когда в записи их нет.
pub const DEFAULT_CLARITY: f64 = 1.0;
pub const DEFAULT_RIPPLE: f64 = 1.0;

// Числа течения и пены (требования 10 и 11): подбираются на глаз.
/// Скорость течения реки в клетках в секунду: `ripple × (FLOW_BASE + FLOW_PER_SLOPE × уклон)`, но не
/// больше `ripple × FLOW_CAP`.
const FLOW_BASE: f64 = 0.5;
const FLOW_PER_SLOPE: f64 = 6.0;
const FLOW_CAP: f64 = 4.0;
/// Уклон глади, круче которого вода падающая (45°), и скорость падения в долях `ripple`.
const FALL_SLOPE: f64 = 1.0;
const FALL_FLOW: f64 = 4.0;
/// Ближе стольких клеток к вершине ломаной течение реки переходит к соседнему звену.
const COURSE_BLEND: f64 = 1.0;
/// На стольких ширинах русла перед устьем течение реки гаснет, и на стольких от истока набирает силу.
const END_CALM: f64 = 1.0;
/// Уклон, от которого начинается пена, и уклон, при котором она полная.
const FOAM_FROM_SLOPE: f64 = 0.02;
const FOAM_FULL_SLOPE: f64 = 0.08;
/// На сколько длин ряби пена расходится от точки с пеной.
const FOAM_REACH: f64 = 2.0;

/// Озеро проходит русло реки, гладь которой в точке ниже его уровня не больше чем на столько.
const PASS_TOLERANCE: f64 = 0.001;
/// Точка суши берёт гладь залитой точки не дальше `LAND_REACH_CELLS` клеток и не дальше
/// `LAND_REACH_STEPS` шагов сетки — что больше.
const LAND_REACH_CELLS: f64 = 1.0;
const LAND_REACH_STEPS: f64 = 2.0;

const EPS: f64 = 1e-9;
/// Гладь точки, у которой её нет.
const NO_SURFACE: f64 = f64::NEG_INFINITY;

/// Озеро: точка `at` в чаше и высота глади `level`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lake {
    pub at: Vec2,
    pub level: f64,
}

/// Река: точки `[x, y, высота глади, ширина русла]` от истока к устью.
#[derive(Debug, Clone, PartialEq)]
pub struct River {
    pub points: Vec<[f64; 4]>,
}

/// Запись `water` файла рельефа: поля, которых автор не написал, остаются пустыми.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterSpec {
    pub color: [f32; 4],
    pub level: Option<f64>,
    pub lakes: Vec<Lake>,
    pub rivers: Vec<River>,
    pub clarity: Option<f64>,
    pub ripple: Option<f64>,
}

impl WaterSpec {
    /// На какой глубине в клетках дна уже не видно.
    pub fn clarity(&self) -> f64 {
        self.clarity.unwrap_or(DEFAULT_CLARITY)
    }

    /// Длина ряби в клетках.
    pub fn ripple(&self) -> f64 {
        self.ripple.unwrap_or(DEFAULT_RIPPLE)
    }
}

/// Сетка точек рельефа: точек по ширине и по высоте и точек на клетку.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    pub columns: usize,
    pub rows: usize,
    pub density: usize,
}

/// Соседи точки сетки: ребра треугольников — четыре по сторонам и два по диагонали от левой верхней
/// точки квадрата к правой нижней.
const LINKS: [(isize, isize); 6] = [(-1, 0), (1, 0), (0, -1), (0, 1), (-1, -1), (1, 1)];

impl Grid {
    fn len(self) -> usize {
        self.columns * self.rows
    }

    fn column(self, point: usize) -> usize {
        point % self.columns
    }

    fn row(self, point: usize) -> usize {
        point / self.columns
    }

    fn place(self, point: usize) -> Vec2 {
        let d = self.density as f64;
        [self.column(point) as f64 / d, self.row(point) as f64 / d]
    }

    /// Точка сетки, ближайшая к месту сцены.
    fn nearest(self, place: Vec2) -> usize {
        let d = self.density as f64;
        let column = (place[0] * d).round().clamp(0.0, (self.columns - 1) as f64) as usize;
        let row = (place[1] * d).round().clamp(0.0, (self.rows - 1) as f64) as usize;
        row * self.columns + column
    }

    fn on_edge(self, point: usize) -> bool {
        let (column, row) = (self.column(point), self.row(point));
        column == 0 || row == 0 || column + 1 == self.columns || row + 1 == self.rows
    }

    fn neighbours(self, point: usize) -> impl Iterator<Item = usize> {
        let (column, row) = (self.column(point) as isize, self.row(point) as isize);
        LINKS.into_iter().filter_map(move |(dc, dr)| {
            let (c, r) = (column + dc, row + dr);
            ((0..self.columns as isize).contains(&c) && (0..self.rows as isize).contains(&r))
                .then(|| r as usize * self.columns + c as usize)
        })
    }

    /// Точки сетки по одной оси из `count`, что лежат на отрезке сцены от `low` до `high`.
    fn span(self, low: f64, high: f64, count: usize) -> std::ops::Range<usize> {
        let d = self.density as f64;
        let first = (low * d - EPS).ceil().max(0.0) as usize;
        let last = ((high * d + EPS).floor() + 1.0).clamp(0.0, count as f64) as usize;
        first.min(last)..last
    }
}

/// Чем залита точка сетки.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Body {
    Dry,
    River,
    Sea,
    Lake,
}

/// Предупреждение о воде: путь к записи и текст с номером озера или реки.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterWarning {
    pub path: String,
    pub message: String,
}

/// Гладь, течение и пена над точками сетки рельефа.
#[derive(Debug, Clone, PartialEq)]
pub struct WaterField {
    body: Vec<Body>,
    /// Высота глади над точкой; у точки без глади `NO_SURFACE`.
    surface: Vec<f64>,
    /// Течение в клетках в секунду.
    flow: Vec<[f32; 2]>,
    foam: Vec<f32>,
    range: Option<(f64, f64)>,
    warnings: Vec<WaterWarning>,
}

/// Русло реки над точкой сетки: гладь, направление течения к устью и падение глади на клетку.
#[derive(Debug, Clone, Copy)]
struct Channel {
    surface: f64,
    direction: Vec2,
    slope: f64,
    /// Доля течения: у устья оно гаснет до нуля, у истока набирает силу.
    current: f64,
}

impl WaterField {
    /// Гладь над точками сетки `grid` с высотами земли `ground`, строками сверху вниз.
    pub fn compute(spec: &WaterSpec, ground: &[f64], grid: Grid) -> WaterField {
        let channels = river_channels(&spec.rivers, grid);
        let mut body = vec![Body::Dry; grid.len()];
        let mut surface = vec![NO_SURFACE; grid.len()];
        flood_rivers(ground, &channels, &mut body, &mut surface);
        let mut warnings = Vec::new();
        flood_lakes(
            &spec.lakes,
            ground,
            grid,
            &channels,
            (&mut body, &mut surface),
            &mut warnings,
        );
        if let Some(level) = spec.level {
            flood_sea(level, ground, &channels, &mut body, &mut surface);
        }
        let steep = steepness(grid, &body, &surface);
        let ripple = spec.ripple();
        let flow = flows(&body, &channels, &steep, ripple);
        let foam = foams(grid, &body, &steep, ripple);
        warn_dry_rivers(&spec.rivers, grid, &body, &mut warnings);
        extend_to_land(grid, ground, &body, &mut surface);
        let range = surface_range(&surface);
        WaterField {
            body,
            surface,
            flow,
            foam,
            range,
            warnings,
        }
    }

    pub fn body(&self, point: usize) -> Body {
        self.body[point]
    }

    pub fn is_flooded(&self, point: usize) -> bool {
        self.body[point] != Body::Dry
    }

    /// Высота глади над точкой сетки; у залитой точки — глади её водоёма, у точки суши рядом с водой —
    /// ближайшей залитой, но не выше своей земли; `None` вдали от воды.
    pub fn surface(&self, point: usize) -> Option<f64> {
        Some(self.surface[point]).filter(|surface| surface.is_finite())
    }

    /// Течение в залитой точке, клеток в секунду по осям сцены.
    pub fn flow(&self, point: usize) -> Vec2 {
        self.flow[point].map(f64::from)
    }

    /// Пена в залитой точке, от 0 до 1.
    pub fn foam(&self, point: usize) -> f64 {
        f64::from(self.foam[point])
    }

    /// Наименьшая и наибольшая гладь, которая рисуется, — у залитых точек и у кромки, где гладь сходит к
    /// земле; `None`, когда ничего не залито.
    pub fn range(&self) -> Option<(f64, f64)> {
        self.range
    }

    pub fn warnings(&self) -> &[WaterWarning] {
        &self.warnings
    }

    /// Нормаль глади в точке сетки по глади соседей, которые тоже залиты; вверх, когда соседей нет.
    pub fn normal(&self, grid: Grid, point: usize) -> [f64; 3] {
        let d = grid.density as f64;
        let (column, row) = (grid.column(point), grid.row(point));
        let along = |before: Option<usize>, after: Option<usize>| {
            let wet = |at: Option<usize>| at.filter(|&at| self.is_flooded(at));
            match (wet(before), wet(after)) {
                (Some(b), Some(a)) => (self.surface[a] - self.surface[b]) * d / 2.0,
                (None, Some(a)) => (self.surface[a] - self.surface[point]) * d,
                (Some(b), None) => (self.surface[point] - self.surface[b]) * d,
                (None, None) => 0.0,
            }
        };
        let at = |c: usize, r: usize| r * grid.columns + c;
        let slope_x = along(
            column.checked_sub(1).map(|c| at(c, row)),
            (column + 1 < grid.columns).then(|| at(column + 1, row)),
        );
        let slope_y = along(
            row.checked_sub(1).map(|r| at(column, r)),
            (row + 1 < grid.rows).then(|| at(column, row + 1)),
        );
        let length = (slope_x * slope_x + slope_y * slope_y + 1.0).sqrt();
        [-slope_x / length, -slope_y / length, 1.0 / length]
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// Часть треугольника, где глубина больше нуля, вершинами — долями трёх его точек, по кругу. Глубина
/// меняется по треугольнику по прямой, поэтому кромка — отрезок, где она равна нулю. Одна функция для
/// хода и для рисования.
pub fn clip_wet(depth: [f64; 3]) -> Option<Vec<[f64; 3]>> {
    const CORNERS: [[f64; 3]; 3] = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
    let mut polygon = Vec::with_capacity(4);
    for index in 0..3 {
        let next = (index + 1) % 3;
        let (inside, next_inside) = (depth[index] > 0.0, depth[next] > 0.0);
        if inside {
            polygon.push(CORNERS[index]);
        }
        if inside != next_inside {
            let t = depth[index] / (depth[index] - depth[next]);
            polygon.push([0, 1, 2].map(|axis| lerp(CORNERS[index][axis], CORNERS[next][axis], t)));
        }
    }
    (polygon.len() >= 3).then_some(polygon)
}

// --- Русла рек ---------------------------------------------------------------------------------

/// Ближайшая к точке сетки точка ломаной реки: звено, доля пути по нему и квадрат расстояния.
#[derive(Debug, Clone, Copy)]
struct Nearest {
    segment: usize,
    t: f64,
    distance2: f64,
}

/// Русла рек над точками сетки; точку в русле двух рек берёт та, что раньше в файле.
fn river_channels(rivers: &[River], grid: Grid) -> Vec<Option<Channel>> {
    let mut channels = vec![None; grid.len()];
    for river in rivers {
        for (point, channel) in river_hits(&river.points, grid) {
            channels[point].get_or_insert(channel);
        }
    }
    channels
}

/// Точки сетки в русле одной реки. Для точки берётся ближайшая точка ломаной; она в русле, если
/// расстояние до неё не больше половины ширины там, у концов русло скруглено.
fn river_hits(points: &[[f64; 4]], grid: Grid) -> Vec<(usize, Channel)> {
    let reach = points.iter().map(|p| p[3]).fold(0.0, f64::max) / 2.0;
    let mut nearest: HashMap<usize, Nearest> = HashMap::new();
    // Звено нулевой длины — ступенька водопада: его ближние точки — те же вершины соседних звеньев, и в ничьей
    // оно не должно забирать их, ведь своего направления у него нет. Пропускается, если в ломаной есть звенья длиннее.
    let zero = |pair: &[[f64; 4]]| (pair[1][0] - pair[0][0]).hypot(pair[1][1] - pair[0][1]) <= EPS;
    let has_length = points.windows(2).any(|pair| !zero(pair));
    for (segment, pair) in points.windows(2).enumerate() {
        if has_length && zero(pair) {
            continue;
        }
        let (a, b) = (pair[0], pair[1]);
        let columns = grid.span(a[0].min(b[0]) - reach, a[0].max(b[0]) + reach, grid.columns);
        let rows = grid.span(a[1].min(b[1]) - reach, a[1].max(b[1]) + reach, grid.rows);
        for row in rows {
            for column in columns.clone() {
                let point = row * grid.columns + column;
                let (t, distance2) = project(a, b, grid.place(point));
                let closer = nearest
                    .get(&point)
                    .is_none_or(|best| distance2 < best.distance2);
                if closer {
                    nearest.insert(
                        point,
                        Nearest {
                            segment,
                            t,
                            distance2,
                        },
                    );
                }
            }
        }
    }
    // Путь по ломаной от каждой её точки до устья.
    let mut to_mouth = vec![0.0; points.len()];
    for at in (0..points.len().saturating_sub(1)).rev() {
        let (a, b) = (points[at], points[at + 1]);
        to_mouth[at] = to_mouth[at + 1] + (b[0] - a[0]).hypot(b[1] - a[1]);
    }
    nearest
        .into_iter()
        .filter_map(|(point, near)| {
            channel_of(points, &to_mouth, near).map(|channel| (point, channel))
        })
        .collect()
}

/// Ближайшая к `place` точка звена `a → b`: доля пути и квадрат расстояния.
fn project(a: [f64; 4], b: [f64; 4], place: Vec2) -> (f64, f64) {
    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
    let length2 = dx * dx + dy * dy;
    let t = if length2 > EPS {
        (((place[0] - a[0]) * dx + (place[1] - a[1]) * dy) / length2).clamp(0.0, 1.0)
    } else {
        0.0
    };
    let (rx, ry) = (place[0] - (a[0] + t * dx), place[1] - (a[1] + t * dy));
    (t, rx * rx + ry * ry)
}

/// Русло реки в ближайшей точке ломаной; `None`, если точка сетки дальше половины ширины.
/// У концов течение тихое: на последних `END_CALM` ширинах русла перед устьем оно плавно сходит на нет, за концом его
/// нет — вода реки сливается с морем или озером, и узор её ряби не отличается от узора их ряби; так же от истока оно
/// набирает силу на первых `END_CALM` ширинах — река, вытекающая из озера, не начинается полосой у его кромки.
fn channel_of(points: &[[f64; 4]], to_mouth: &[f64], near: Nearest) -> Option<Channel> {
    let (a, b) = (points[near.segment], points[near.segment + 1]);
    if near.distance2.sqrt() > lerp(a[3], b[3], near.t) / 2.0 + EPS {
        return None;
    }
    let (direction, slope) = course_at(points, near.segment, near.t);
    let length = (b[0] - a[0]).hypot(b[1] - a[1]);
    let remaining = (1.0 - near.t) * length + to_mouth[near.segment + 1];
    let travelled = to_mouth[0] - remaining;
    let rise = |distance: f64, width: f64| {
        let calm = END_CALM * width;
        if calm > EPS {
            let x = (distance / calm).clamp(0.0, 1.0);
            x * x * (3.0 - 2.0 * x)
        } else {
            1.0
        }
    };
    let current = rise(remaining, points[points.len() - 1][3]) * rise(travelled, points[0][3]);
    Some(Channel {
        surface: lerp(a[2], b[2], near.t),
        direction,
        slope,
        current,
    })
}

/// Направление течения к устью и падение глади на клетку в точке ломаной `points` на звене `segment`, доля
/// пути `t`. Вдали от вершин — как у самого звена; ближе `COURSE_BLEND` клетки к вершине течение переходит к
/// соседнему звену и в самой вершине равно среднему двух. Иначе течение менялось бы скачком на стыке звеньев, и узор
/// ряби и пены, который оно сносит, рвался бы там полосой поперёк реки.
fn course_at(points: &[[f64; 4]], segment: usize, t: f64) -> (Vec2, f64) {
    let along = |s: usize| {
        let (a, b) = (points[s], points[s + 1]);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        if length > EPS {
            (
                [(b[0] - a[0]) / length, (b[1] - a[1]) / length],
                ((a[2] - b[2]) / length).max(0.0),
                length,
            )
        } else {
            ([0.0, 0.0], 0.0, 0.0)
        }
    };
    let (direction, slope, length) = along(segment);
    let blend = (length / 2.0).min(COURSE_BLEND);
    // Соседом берётся ближайшее звено ненулевой длины: две точки в одном месте — ступенька водопада, у неё нет
    // своего направления, и звенья по обе стороны от неё сходятся в одной вершине.
    let previous = (0..segment).rev().find(|&s| along(s).2 > EPS);
    let next = (segment + 1..points.len() - 1).find(|&s| along(s).2 > EPS);
    let neighbour = match (previous, next) {
        _ if blend <= EPS => None,
        (Some(previous), _) if t * length < blend => {
            Some((along(previous), 0.5 * (1.0 - t * length / blend)))
        }
        (_, Some(next)) if (1.0 - t) * length < blend => {
            Some((along(next), 0.5 * (1.0 - (1.0 - t) * length / blend)))
        }
        _ => None,
    };
    let Some(((other, other_slope, _), weight)) = neighbour else {
        return (direction, slope);
    };
    let sum = [
        lerp(direction[0], other[0], weight),
        lerp(direction[1], other[1], weight),
    ];
    let norm = sum[0].hypot(sum[1]);
    let direction = if norm > EPS {
        [sum[0] / norm, sum[1] / norm]
    } else {
        direction
    };
    (direction, lerp(slope, other_slope, weight))
}

// --- Заливка -----------------------------------------------------------------------------------

/// Залита рекой точка её русла, где земля ниже глади.
fn flood_rivers(
    ground: &[f64],
    channels: &[Option<Channel>],
    body: &mut [Body],
    surface: &mut [f64],
) {
    for (point, channel) in channels.iter().enumerate() {
        if let Some(channel) = channel.filter(|channel| ground[point] < channel.surface) {
            body[point] = Body::River;
            surface[point] = channel.surface;
        }
    }
}

/// Море залило каждую точку вне русел рек и не залитую озером, где земля ниже `level`.
fn flood_sea(
    level: f64,
    ground: &[f64],
    channels: &[Option<Channel>],
    body: &mut [Body],
    surface: &mut [f64],
) {
    for point in 0..ground.len() {
        if channels[point].is_none() && body[point] == Body::Dry && ground[point] < level {
            body[point] = Body::Sea;
            surface[point] = level;
        }
    }
}

/// Что заливка озера делает с точкой.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Entry {
    Take,
    Pass,
    Wall,
}

/// Озёра по порядку в файле: заливка от точки, ближайшей к `at`, по соседям, пока земля ниже `level`.
/// Чужая вода — стена; русло реки заливка проходит, только если гладь реки там не ниже
/// `level − PASS_TOLERANCE`, и само русло не берёт.
fn flood_lakes(
    lakes: &[Lake],
    ground: &[f64],
    grid: Grid,
    channels: &[Option<Channel>],
    (body, surface): (&mut [Body], &mut [f64]),
    warnings: &mut Vec<WaterWarning>,
) {
    let mut visited = vec![usize::MAX; grid.len()];
    for (index, lake) in lakes.iter().enumerate() {
        let number = index + 1;
        let path = format!("water → lakes[{index}]");
        let mut warn = |message: String| {
            warnings.push(WaterWarning {
                path: path.clone(),
                message,
            });
        };
        let entry = |point: usize, body: &[Body]| {
            if ground[point] >= lake.level {
                return Entry::Wall;
            }
            match channels[point] {
                Some(channel) if channel.surface >= lake.level - PASS_TOLERANCE => Entry::Pass,
                Some(_) => Entry::Wall,
                None if body[point] == Body::Dry => Entry::Take,
                None => Entry::Wall,
            }
        };
        let start = grid.nearest(lake.at);
        if ground[start] >= lake.level {
            warn(format!(
                "озеро {number}: земля в точке at ({}, {}) не ниже level {} — воды в озере нет",
                lake.at[0], lake.at[1], lake.level
            ));
            continue;
        }
        if entry(start, body) == Entry::Wall {
            warn(format!(
                "озеро {number}: в точке at уже вода другого водоёма или русло реки с водой ниже level {} — воды в озере нет",
                lake.level
            ));
            continue;
        }
        let mut reached_edge = false;
        let mut queue = VecDeque::from([start]);
        visited[start] = index;
        while let Some(point) = queue.pop_front() {
            match entry(point, body) {
                Entry::Wall => continue,
                Entry::Pass => {}
                Entry::Take => {
                    body[point] = Body::Lake;
                    surface[point] = lake.level;
                    reached_edge |= grid.on_edge(point);
                }
            }
            for next in grid.neighbours(point) {
                if visited[next] != index {
                    visited[next] = index;
                    queue.push_back(next);
                }
            }
        }
        if reached_edge {
            warn(format!(
                "озеро {number}: чаша не замкнута — вода дошла до края сцены"
            ));
        }
    }
}

fn surface_range(surface: &[f64]) -> Option<(f64, f64)> {
    let mut range: Option<(f64, f64)> = None;
    for &height in surface.iter().filter(|height| height.is_finite()) {
        range = Some(range.map_or((height, height), |(low, high)| {
            (low.min(height), high.max(height))
        }));
    }
    range
}

/// Точки суши рядом с водой берут гладь ближайшей залитой точки, при равном расстоянии — большую, но
/// не выше своей земли. Ближайшая залитая точка всегда граничная — у неё есть сосед суши, — поэтому
/// ищут от граничных.
fn extend_to_land(grid: Grid, ground: &[f64], body: &[Body], surface: &mut [f64]) {
    let reach = (LAND_REACH_CELLS * grid.density as f64).max(LAND_REACH_STEPS);
    let steps = reach.floor() as isize;
    let limit = (reach * reach).floor() as isize;
    let mut nearest = vec![isize::MAX; grid.len()];
    let mut higher = vec![NO_SURFACE; grid.len()];
    for point in (0..grid.len()).filter(|&point| body[point] != Body::Dry) {
        let borders_land = grid_ring(grid, point).any(|near| body[near] == Body::Dry);
        if !borders_land {
            continue;
        }
        let (column, row) = (grid.column(point) as isize, grid.row(point) as isize);
        for dr in -steps..=steps {
            for dc in -steps..=steps {
                let distance2 = dc * dc + dr * dr;
                let (c, r) = (column + dc, row + dr);
                let inside =
                    (0..grid.columns as isize).contains(&c) && (0..grid.rows as isize).contains(&r);
                if !inside || distance2 > limit {
                    continue;
                }
                let target = r as usize * grid.columns + c as usize;
                if body[target] != Body::Dry {
                    continue;
                }
                let closer = distance2 < nearest[target]
                    || (distance2 == nearest[target] && surface[point] > higher[target]);
                if closer {
                    nearest[target] = distance2;
                    higher[target] = surface[point];
                }
            }
        }
    }
    for target in (0..grid.len()).filter(|&target| nearest[target] != isize::MAX) {
        surface[target] = higher[target].min(ground[target]);
    }
}

/// Восемь соседей точки сетки, что лежат в сетке.
fn grid_ring(grid: Grid, point: usize) -> impl Iterator<Item = usize> {
    let (column, row) = (grid.column(point) as isize, grid.row(point) as isize);
    (-1isize..=1)
        .flat_map(|dr| (-1isize..=1).map(move |dc| (dc, dr)))
        .filter(|&(dc, dr)| (dc, dr) != (0, 0))
        .filter_map(move |(dc, dr)| {
            let (c, r) = (column + dc, row + dr);
            ((0..grid.columns as isize).contains(&c) && (0..grid.rows as isize).contains(&r))
                .then(|| r as usize * grid.columns + c as usize)
        })
}

// --- Течение и пена ----------------------------------------------------------------------------

/// Крутизна глади в залитой точке — наибольший наклон треугольников глади вокруг неё, у которых все
/// три точки залиты, — и направление вниз по этому наклону.
struct Steep {
    slope: Vec<f32>,
    down: Vec<[f32; 2]>,
}

fn steepness(grid: Grid, body: &[Body], surface: &[f64]) -> Steep {
    let mut steep = Steep {
        slope: vec![0.0; grid.len()],
        down: vec![[0.0; 2]; grid.len()],
    };
    let d = grid.density as f64;
    let at = |column: usize, row: usize| row * grid.columns + column;
    for row in 0..grid.rows.saturating_sub(1) {
        for column in 0..grid.columns.saturating_sub(1) {
            let (top_left, top_right) = (at(column, row), at(column + 1, row));
            let (bottom_left, bottom_right) = (at(column, row + 1), at(column + 1, row + 1));
            let triangles = [
                (
                    [top_left, top_right, bottom_right],
                    (surface[top_right] - surface[top_left]) * d,
                    (surface[bottom_right] - surface[top_right]) * d,
                ),
                (
                    [top_left, bottom_right, bottom_left],
                    (surface[bottom_right] - surface[bottom_left]) * d,
                    (surface[bottom_left] - surface[top_left]) * d,
                ),
            ];
            for (corners, slope_x, slope_y) in triangles {
                if corners.iter().any(|&corner| body[corner] == Body::Dry) {
                    continue;
                }
                let slope = slope_x.hypot(slope_y);
                for corner in corners {
                    if slope > f64::from(steep.slope[corner]) {
                        steep.slope[corner] = slope as f32;
                        steep.down[corner] = [(-slope_x / slope) as f32, (-slope_y / slope) as f32];
                    }
                }
            }
        }
    }
    steep
}

/// Течение: у реки — вдоль ломаной к устью со скоростью по уклону, где гладь круче 45° — вниз по
/// наклону глади. У моря и озёр течения нет.
fn flows(body: &[Body], channels: &[Option<Channel>], steep: &Steep, ripple: f64) -> Vec<[f32; 2]> {
    (0..body.len())
        .map(|point| {
            if f64::from(steep.slope[point]) > FALL_SLOPE {
                let speed = ripple * FALL_FLOW;
                return steep.down[point].map(|c| (f64::from(c) * speed) as f32);
            }
            match (body[point], channels[point]) {
                (Body::River, Some(channel)) => {
                    let speed = ripple
                        * (FLOW_BASE + FLOW_PER_SLOPE * channel.slope).min(FLOW_CAP)
                        * channel.current;
                    channel.direction.map(|c| (c * speed) as f32)
                }
                _ => [0.0; 2],
            }
        })
        .collect()
}

/// Пена: от уклона `FOAM_FROM_SLOPE` она начинается, к `FOAM_FULL_SLOPE` полная, и от каждой точки с
/// пеной расходится на `FOAM_REACH × ripple` клеток по залитым точкам, слабея к краю по прямой.
fn foams(grid: Grid, body: &[Body], steep: &Steep, ripple: f64) -> Vec<f32> {
    let mut foam = vec![0.0_f32; grid.len()];
    let reach = FOAM_REACH * ripple;
    let d = grid.density as f64;
    let steps = (reach * d).ceil() as isize;
    for point in 0..grid.len() {
        let slope = f64::from(steep.slope[point]);
        let seed =
            ((slope - FOAM_FROM_SLOPE) / (FOAM_FULL_SLOPE - FOAM_FROM_SLOPE)).clamp(0.0, 1.0);
        if seed <= 0.0 {
            continue;
        }
        let (column, row) = (grid.column(point) as isize, grid.row(point) as isize);
        for r in (row - steps).max(0)..=(row + steps).min(grid.rows as isize - 1) {
            for c in (column - steps).max(0)..=(column + steps).min(grid.columns as isize - 1) {
                let target = r as usize * grid.columns + c as usize;
                if body[target] == Body::Dry {
                    continue;
                }
                let distance = ((c - column) as f64).hypot((r - row) as f64) / d;
                if distance < reach {
                    let value = (seed * (1.0 - distance / reach)) as f32;
                    foam[target] = foam[target].max(value);
                }
            }
        }
    }
    foam
}

// --- Предупреждения о реках --------------------------------------------------------------------

/// Участки линии каждой реки, вдоль которых ни одна точка не залита никаким водоёмом: русло там выше
/// глади. Участок внутри озера воды не лишён.
fn warn_dry_rivers(rivers: &[River], grid: Grid, body: &[Body], warnings: &mut Vec<WaterWarning>) {
    for (index, river) in rivers.iter().enumerate() {
        for (first, last) in dry_sections(&river.points, grid, body) {
            warnings.push(WaterWarning {
                path: format!("water → rivers[{index}]"),
                message: format!(
                    "река {}: между точками {first} и {last} воды нет — русло там выше глади",
                    index + 1
                ),
            });
        }
    }
}

/// Номера (с единицы) первой и последней точки каждого участка линии без воды. Линия снимается через
/// шаг сетки; место с водой — то, у чьего русла (половина ширины и шаг сетки вокруг места) залита хоть
/// одна точка: русло внутри озера, куда озеро не заходит, воды не лишено.
fn dry_sections(points: &[[f64; 4]], grid: Grid, body: &[Body]) -> Vec<(usize, usize)> {
    let d = grid.density as f64;
    let wet_at = |place: Vec2, width: f64| {
        let reach = width / 2.0 + 1.0 / d;
        let columns = grid.span(place[0] - reach, place[0] + reach, grid.columns);
        let rows = grid.span(place[1] - reach, place[1] + reach, grid.rows);
        rows.flat_map(|row| columns.clone().map(move |column| (column, row)))
            .any(|(column, row)| {
                let point = row * grid.columns + column;
                let [x, y] = grid.place(point);
                body[point] != Body::Dry && (x - place[0]).hypot(y - place[1]) <= reach
            })
    };
    let mut sections = Vec::new();
    let mut open: Option<(usize, usize)> = None;
    for (segment, pair) in points.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let length = (b[0] - a[0]).hypot(b[1] - a[1]);
        let samples = ((length * d).ceil() as usize).max(1);
        for k in usize::from(segment > 0)..=samples {
            let t = k as f64 / samples as f64;
            let place = [lerp(a[0], b[0], t), lerp(a[1], b[1], t)];
            if wet_at(place, lerp(a[3], b[3], t)) {
                if let Some((from, to)) = open.take() {
                    sections.push((from + 1, to + 2));
                }
            } else {
                open = Some(open.map_or((segment, segment), |(from, _)| (from, segment)));
            }
        }
    }
    if let Some((from, to)) = open {
        sections.push((from + 1, to + 2));
    }
    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Сетка в `width × height` клеток с `density` точками на клетку.
    fn grid(width: usize, height: usize, density: usize) -> Grid {
        Grid {
            columns: width * density + 1,
            rows: height * density + 1,
            density,
        }
    }

    fn ground_of(grid: Grid, height: impl Fn(f64, f64) -> f64) -> Vec<f64> {
        (0..grid.len())
            .map(|point| {
                let [x, y] = grid.place(point);
                height(x, y)
            })
            .collect()
    }

    fn spec() -> WaterSpec {
        WaterSpec {
            color: [0.2, 0.4, 0.5, 1.0],
            level: None,
            lakes: Vec::new(),
            rivers: Vec::new(),
            clarity: None,
            ripple: None,
        }
    }

    fn at(grid: Grid, x: f64, y: f64) -> usize {
        grid.nearest([x, y])
    }

    #[test]
    fn the_sea_floods_every_point_below_its_level_and_lends_its_surface_to_the_land_beside_it() {
        let grid = grid(10, 6, 2);
        let ground = ground_of(grid, |x, _| x - 4.0);
        let field = WaterField::compute(
            &WaterSpec {
                level: Some(0.0),
                ..spec()
            },
            &ground,
            grid,
        );
        for point in 0..grid.len() {
            let [x, _] = grid.place(point);
            assert_eq!(field.is_flooded(point), x < 4.0, "x = {x}");
        }
        assert_eq!(field.surface(at(grid, 1.0, 3.0)), Some(0.0));
        assert_eq!(
            field.surface(at(grid, 4.5, 3.0)),
            Some(0.0f64.min(0.5)),
            "суша у воды: гладь моря, но не выше земли"
        );
        assert_eq!(
            field.surface(at(grid, 6.0, 3.0)),
            None,
            "дальше клетки от воды глади нет"
        );
    }

    #[test]
    fn a_land_point_takes_the_surface_of_the_nearest_wet_point_and_the_higher_one_on_a_tie() {
        let grid = grid(8, 4, 2);
        let wet_low = at(grid, 2.0, 2.0);
        let wet_high = at(grid, 3.0, 2.0);
        let ground = ground_of(grid, |x, y| {
            if (y - 2.0).abs() < 0.1 && (x == 2.0 || x == 3.0) {
                -1.0
            } else {
                9.0
            }
        });
        let mut body = vec![Body::Dry; grid.len()];
        let mut surface = vec![NO_SURFACE; grid.len()];
        body[wet_low] = Body::Lake;
        surface[wet_low] = 1.0;
        body[wet_high] = Body::Lake;
        surface[wet_high] = 2.0;
        extend_to_land(grid, &ground, &body, &mut surface);
        assert_eq!(surface[at(grid, 2.5, 2.0)], 2.0, "поровну — большая гладь");
        assert_eq!(surface[at(grid, 1.5, 2.0)], 1.0, "ближе к низкой");
        assert_eq!(surface[at(grid, 3.5, 2.0)], 2.0);
        let low_ground = ground_of(grid, |x, y| {
            if (y - 2.0).abs() < 0.1 && (x == 2.0 || x == 3.0) {
                -1.0
            } else {
                0.5
            }
        });
        let mut capped = vec![NO_SURFACE; grid.len()];
        capped[wet_high] = 2.0;
        extend_to_land(grid, &low_ground, &body, &mut capped);
        assert_eq!(capped[at(grid, 3.5, 2.0)], 0.5, "не выше своей земли");
    }

    fn bowl(grid: Grid, rim: f64) -> Vec<f64> {
        ground_of(grid, |x, y| {
            let distance = (x - 6.0).hypot(y - 5.0);
            if distance < 2.0 { 0.0 } else { rim }
        })
    }

    #[test]
    fn a_lake_floods_only_its_own_bowl() {
        let grid = grid(16, 10, 2);
        let mut ground = bowl(grid, 5.0);
        for (point, height) in ground.iter_mut().enumerate() {
            let [x, y] = grid.place(point);
            if (x - 13.0).hypot(y - 5.0) < 1.5 {
                *height = 0.0;
            }
        }
        let field = WaterField::compute(
            &WaterSpec {
                lakes: vec![Lake {
                    at: [6.0, 5.0],
                    level: 1.0,
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        assert_eq!(field.body(at(grid, 6.0, 5.0)), Body::Lake);
        assert_eq!(field.surface(at(grid, 6.0, 5.0)), Some(1.0));
        assert_eq!(field.body(at(grid, 13.0, 5.0)), Body::Dry, "чужая яма суха");
        assert!(field.warnings().is_empty(), "{:?}", field.warnings());
    }

    #[test]
    fn a_bowl_with_a_low_notch_spills_through_it_and_a_lake_reaching_the_edge_warns() {
        let grid = grid(16, 10, 2);
        let ground = ground_of(grid, |x, y| {
            let distance = (x - 6.0).hypot(y - 5.0);
            if distance < 2.0 || ((y - 5.0).abs() < 0.3 && x > 6.0) {
                0.0
            } else {
                5.0
            }
        });
        let field = WaterField::compute(
            &WaterSpec {
                lakes: vec![Lake {
                    at: [6.0, 5.0],
                    level: 1.0,
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        assert_eq!(
            field.body(at(grid, 12.0, 5.0)),
            Body::Lake,
            "вода ушла по низине"
        );
        let warnings = field.warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].message.contains("озеро 1"), "{warnings:?}");
        assert!(warnings[0].message.contains("не замкнута"), "{warnings:?}");
    }

    #[test]
    fn a_lake_whose_point_is_on_dry_land_is_empty_and_warns() {
        let grid = grid(16, 10, 2);
        let ground = bowl(grid, 5.0);
        let field = WaterField::compute(
            &WaterSpec {
                lakes: vec![Lake {
                    at: [1.0, 1.0],
                    level: 1.0,
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        assert!((0..grid.len()).all(|point| !field.is_flooded(point)));
        assert_eq!(field.warnings().len(), 1);
        assert!(field.warnings()[0].message.contains("озеро 1"));
        assert_eq!(field.warnings()[0].path, "water → lakes[0]");
    }

    fn straight_river(heights: [f64; 2], width: f64) -> River {
        River {
            points: vec![
                [2.0, 5.0, heights[0], width],
                [18.0, 5.0, heights[1], width],
            ],
        }
    }

    #[test]
    fn a_river_surface_follows_its_line_and_is_flat_across_and_absent_outside_the_channel() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(grid, |_, _| -3.0);
        let field = WaterField::compute(
            &WaterSpec {
                rivers: vec![straight_river([4.0, 2.0], 2.0)],
                ..spec()
            },
            &ground,
            grid,
        );
        let middle = field.surface(at(grid, 10.0, 5.0)).expect("в русле");
        assert!((middle - 3.0).abs() < 1e-9, "{middle}");
        let across = field.surface(at(grid, 10.0, 5.75)).expect("в русле");
        assert!((across - middle).abs() < 1e-9, "поперёк ровная");
        assert_eq!(field.body(at(grid, 10.0, 6.5)), Body::Dry, "вне русла нет");
        let cap = field.body(at(grid, 1.5, 5.0));
        assert_eq!(cap, Body::River, "у конца русло скруглено");
        assert_eq!(field.body(at(grid, 0.5, 5.0)), Body::Dry);
    }

    #[test]
    fn a_river_width_changes_along_the_line_by_a_straight_line() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(grid, |_, _| -3.0);
        let river = River {
            points: vec![[2.0, 5.0, 1.0, 1.0], [18.0, 5.0, 1.0, 5.0]],
        };
        let field = WaterField::compute(
            &WaterSpec {
                rivers: vec![river],
                ..spec()
            },
            &ground,
            grid,
        );
        let wet_across = |x: f64| {
            (0..grid.len())
                .filter(|&point| grid.place(point)[0] == x && field.body(point) == Body::River)
                .count()
        };
        assert!(wet_across(4.0) < wet_across(10.0), "шире к устью");
        assert!(wet_across(10.0) < wet_across(16.0));
    }

    #[test]
    fn the_sea_does_not_enter_a_river_channel_above_its_level_and_the_earlier_river_wins() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(grid, |_, _| 0.0);
        let field = WaterField::compute(
            &WaterSpec {
                level: Some(2.0),
                rivers: vec![
                    straight_river([5.0, 5.0], 2.0),
                    River {
                        points: vec![[10.0, 3.0, 1.0, 1.0], [10.0, 7.0, 1.0, 1.0]],
                    },
                ],
                ..spec()
            },
            &ground,
            grid,
        );
        let in_channel = at(grid, 6.0, 5.4);
        assert_eq!(field.body(in_channel), Body::River);
        assert_eq!(field.surface(in_channel), Some(5.0), "не уровень моря");
        assert_eq!(field.body(at(grid, 6.0, 7.0)), Body::Sea);
        let junction = at(grid, 10.0, 5.2);
        assert_eq!(
            field.surface(junction),
            Some(5.0),
            "приток у слияния стоит на воде реки, что раньше в файле"
        );
        assert_eq!(
            field.surface(at(grid, 10.0, 3.2)),
            Some(1.0),
            "в стороне от русла первой реки — вода притока"
        );
    }

    /// Чаша озера на уровне 3 вокруг (6, 6) и река через неё на высоте `river_level`.
    fn bowl_crossed_by_river(river_level: f64) -> (Grid, Vec<f64>, WaterSpec) {
        let grid = grid(30, 12, 4);
        let ground = ground_of(grid, |x, y| {
            if (x - 6.0).hypot(y - 6.0) < 3.0 {
                0.0
            } else {
                9.0
            }
        });
        let spec = WaterSpec {
            lakes: vec![Lake {
                at: [6.0, 4.5],
                level: 3.0,
            }],
            rivers: vec![River {
                points: vec![[2.0, 6.0, river_level, 1.0], [10.0, 6.0, river_level, 1.0]],
            }],
            ..spec()
        };
        (grid, ground, spec)
    }

    #[test]
    fn a_river_lower_than_the_lake_divides_it_and_the_half_without_its_point_stays_dry() {
        let (grid, ground, spec) = bowl_crossed_by_river(0.5);
        let field = WaterField::compute(&spec, &ground, grid);
        assert_eq!(field.body(at(grid, 6.0, 4.5)), Body::Lake);
        assert_eq!(field.body(at(grid, 6.0, 6.0)), Body::River);
        assert_eq!(field.surface(at(grid, 6.0, 6.0)), Some(0.5));
        assert_eq!(field.body(at(grid, 6.0, 7.8)), Body::Dry);
    }

    #[test]
    fn a_river_through_a_lake_at_its_level_does_not_divide_it() {
        let (grid, ground, spec) = bowl_crossed_by_river(3.0);
        let field = WaterField::compute(&spec, &ground, grid);
        assert_eq!(field.body(at(grid, 6.0, 4.5)), Body::Lake, "верх чаши");
        assert_eq!(field.body(at(grid, 6.0, 7.8)), Body::Lake, "низ чаши");
        assert_eq!(field.surface(at(grid, 6.0, 7.8)), Some(3.0));
        assert_eq!(field.body(at(grid, 6.0, 6.0)), Body::River, "русло — реки");
    }

    #[test]
    fn a_river_flowing_out_of_a_lake_down_a_valley_keeps_the_lake_from_pouring_after_it() {
        let grid = grid(30, 12, 4);
        let ground = ground_of(grid, |x, y| {
            let bowl = (x - 6.0).hypot(y - 6.0) < 3.0;
            let valley = (y - 6.0).abs() < 0.6 && x >= 6.0;
            if bowl || valley { 0.0 } else { 9.0 }
        });
        let spec = WaterSpec {
            lakes: vec![Lake {
                at: [6.0, 5.0],
                level: 3.0,
            }],
            rivers: vec![River {
                points: vec![[8.0, 6.0, 0.5, 1.0], [28.0, 6.0, 0.5, 1.0]],
            }],
            ..spec()
        };
        let field = WaterField::compute(&spec, &ground, grid);
        assert_eq!(field.body(at(grid, 6.0, 5.0)), Body::Lake);
        assert_eq!(field.body(at(grid, 14.0, 6.0)), Body::River);
        let spilled = (0..grid.len())
            .filter(|&point| grid.place(point)[0] > 9.5)
            .any(|point| field.body(point) == Body::Lake);
        assert!(!spilled, "озеро не перелилось в долину");
    }

    #[test]
    fn the_triangle_with_depths_one_minus_one_minus_one_keeps_the_part_to_the_midpoints_of_its_edges()
     {
        let polygon = clip_wet([1.0, -1.0, -1.0]).expect("есть вода");
        assert_eq!(polygon.len(), 3);
        assert!(polygon.contains(&[1.0, 0.0, 0.0]));
        assert!(polygon.contains(&[0.5, 0.5, 0.0]));
        assert!(polygon.contains(&[0.5, 0.0, 0.5]));
        assert!(clip_wet([-1.0, 0.0, -2.0]).is_none());
        assert_eq!(clip_wet([1.0, 2.0, 3.0]).expect("целиком").len(), 3);
    }

    /// Ровная долина с рекой вдоль x: перекат на отрезке от x = 14 до 16.
    fn river_with_riffle() -> (Grid, WaterField) {
        let grid = grid(30, 10, 4);
        let ground = ground_of(grid, |_, _| -5.0);
        let spec = WaterSpec {
            rivers: vec![River {
                points: vec![
                    [2.0, 5.0, 2.0, 3.0],
                    [14.0, 5.0, 1.9, 3.0],
                    [16.0, 5.0, 1.0, 3.0],
                    [28.0, 5.0, 0.9, 3.0],
                ],
            }],
            ..spec()
        };
        let field = WaterField::compute(&spec, &ground, grid);
        (grid, field)
    }

    #[test]
    fn a_river_flows_toward_its_mouth_faster_where_it_is_steeper_and_foams_on_a_riffle() {
        let (grid, field) = river_with_riffle();
        let calm = field.flow(at(grid, 8.0, 5.0));
        let steep = field.flow(at(grid, 15.0, 5.0));
        assert!(calm[0] > 0.0 && calm[1].abs() < 1e-6, "{calm:?}");
        assert!(
            (calm[0] - (0.5 + 6.0 * (0.1 / 12.0))).abs() < 1e-5,
            "{calm:?}"
        );
        assert!(steep[0] > calm[0] * 2.0, "{steep:?} против {calm:?}");
        assert!(steep[0] <= 4.0 + 1e-6);
        assert!(field.foam(at(grid, 15.0, 5.0)) > 0.5, "на перекате");
        assert!(field.foam(at(grid, 17.0, 5.0)) > 0.0, "ниже переката");
        assert_eq!(field.foam(at(grid, 6.0, 5.0)), 0.0, "ровная река без пены");
        assert_eq!(field.foam(at(grid, 26.0, 5.0)), 0.0);
    }

    /// Ревью фазы 24: течение не меняется скачком на стыке звеньев — в вершине обе стороны дают среднее двух
    /// звеньев, дальше `COURSE_BLEND` от вершин — значения самого звена; ступенька нулевой длины стык не рвёт.
    #[test]
    fn the_river_course_is_continuous_across_vertices_and_its_own_away_from_them() {
        let close = |a: (Vec2, f64), b: (Vec2, f64)| {
            (a.0[0] - b.0[0]).abs() < 1e-9
                && (a.0[1] - b.0[1]).abs() < 1e-9
                && (a.1 - b.1).abs() < 1e-9
        };
        let bend = [
            [0.0, 0.0, 3.0, 2.0],
            [6.0, 0.0, 2.7, 2.0],
            [6.0, 0.0, 1.0, 2.0],
            [6.0, 6.0, 0.4, 2.0],
            [7.0, 6.5, 0.3, 2.0],
        ];
        // Звено 1 нулевой длины: звенья 0 и 2 сходятся в одной вершине.
        assert!(close(course_at(&bend, 0, 1.0), course_at(&bend, 2, 0.0)));
        // Короткое звено 3 стыкуется со звеном 2.
        assert!(close(course_at(&bend, 2, 1.0), course_at(&bend, 3, 0.0)));
        let (direction, slope) = course_at(&bend, 0, 1.0);
        let half = std::f64::consts::FRAC_1_SQRT_2;
        assert!(
            (direction[0] - half).abs() < 1e-9 && (direction[1] - half).abs() < 1e-9,
            "{direction:?}"
        );
        assert!(
            (slope - (0.3 / 6.0 + 0.6 / 6.0) / 2.0).abs() < 1e-9,
            "{slope}"
        );
        // Посередине длинного звена — его собственные направление и уклон.
        assert!(close(course_at(&bend, 0, 0.5), ([1.0, 0.0], 0.3 / 6.0)));
        assert!(close(course_at(&bend, 2, 0.5), ([0.0, 1.0], 0.6 / 6.0)));
        // Начало и конец ломаной — без соседей, как у звена.
        assert!(close(course_at(&bend, 0, 0.0), ([1.0, 0.0], 0.3 / 6.0)));
    }

    /// Ревью фазы 24: ступенька нулевой длины у истока не забирает вершину — гладь там у звена за ступенькой; у самого
    /// истока тихо, по правилу концов, ниже течение идёт к устью; река из одних совпавших точек — круглое русло.
    #[test]
    fn a_zero_length_segment_neither_steals_the_vertex_nor_stops_the_current() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(grid, |_, _| -3.0);
        let stepped = WaterField::compute(
            &WaterSpec {
                rivers: vec![River {
                    points: vec![
                        [5.0, 5.0, 2.2, 2.0],
                        [5.0, 5.0, 2.0, 2.0],
                        [15.0, 5.0, 1.0, 2.0],
                    ],
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        assert_eq!(
            stepped.surface(at(grid, 5.0, 5.0)),
            Some(2.0),
            "гладь звена за ступенькой"
        );
        assert_eq!(
            stepped.flow(at(grid, 5.0, 5.0)),
            [0.0, 0.0],
            "у истока тихо"
        );
        let below = stepped.flow(at(grid, 5.5, 5.0));
        assert!(
            below[0] > 0.0 && below[1].abs() < 1e-9,
            "ниже истока течение к устью {below:?}"
        );
        let pool = WaterField::compute(
            &WaterSpec {
                rivers: vec![River {
                    points: vec![[5.0, 5.0, 2.0, 2.0], [5.0, 5.0, 2.0, 2.0]],
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        assert_eq!(pool.body(at(grid, 5.0, 5.0)), Body::River);
        assert_eq!(pool.body(at(grid, 5.75, 5.0)), Body::River, "внутри круга");
        assert_eq!(pool.body(at(grid, 6.5, 5.0)), Body::Dry, "за кругом");
    }

    /// К устью течение гаснет: в середине реки оно полное, у самого конца слабее, за концом его нет; от истока оно
    /// так же набирает силу — река, вытекающая из озера, не начинается полосой.
    #[test]
    fn the_current_calms_down_at_the_mouth() {
        let grid = grid(30, 10, 4);
        let ground = ground_of(grid, |_, _| -3.0);
        let field = WaterField::compute(
            &WaterSpec {
                rivers: vec![straight_river([2.0, 1.0], 4.0)],
                ..spec()
            },
            &ground,
            grid,
        );
        let middle = field.flow(at(grid, 10.0, 5.0))[0];
        let near_mouth = field.flow(at(grid, 17.0, 5.0))[0];
        assert!(middle > 0.0);
        assert!(
            near_mouth > 0.0 && near_mouth < middle,
            "{near_mouth} против {middle}"
        );
        assert_eq!(
            field.flow(at(grid, 19.0, 5.0)),
            [0.0, 0.0],
            "за концом реки"
        );
        let near_source = field.flow(at(grid, 3.0, 5.0))[0];
        assert!(
            near_source > 0.0 && near_source < middle,
            "{near_source} против {middle}"
        );
        assert_eq!(field.flow(at(grid, 1.0, 5.0)), [0.0, 0.0], "перед истоком");
    }

    #[test]
    fn a_still_lake_and_the_sea_have_no_current() {
        let grid = grid(16, 10, 2);
        let ground = bowl(grid, 5.0);
        let field = WaterField::compute(
            &WaterSpec {
                level: Some(-1.0),
                lakes: vec![Lake {
                    at: [6.0, 5.0],
                    level: 1.0,
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        let flowing = (0..grid.len())
            .filter(|&point| field.is_flooded(point))
            .any(|point| field.flow(point) != [0.0, 0.0] || field.foam(point) > 0.0);
        assert!(!flowing);
    }

    #[test]
    fn a_lake_bowl_deeper_than_the_sea_stays_a_lake_at_the_lake_level() {
        let grid = grid(16, 10, 2);
        let ground = ground_of(grid, |x, y| {
            let distance = (x - 6.0).hypot(y - 5.0);
            if distance < 2.0 {
                -1.0
            } else if distance < 3.0 {
                3.5
            } else {
                0.0
            }
        });
        let field = WaterField::compute(
            &WaterSpec {
                level: Some(0.5),
                lakes: vec![Lake {
                    at: [6.0, 5.0],
                    level: 3.0,
                }],
                ..spec()
            },
            &ground,
            grid,
        );
        assert_eq!(field.body(at(grid, 6.0, 5.0)), Body::Lake);
        assert_eq!(field.surface(at(grid, 6.0, 5.0)), Some(3.0));
        assert_eq!(field.body(at(grid, 6.0, 9.0)), Body::Sea, "вне чаши — море");
        assert!(field.warnings().is_empty(), "{:?}", field.warnings());
    }

    #[test]
    fn a_waterfall_steeper_than_forty_five_degrees_flows_down_its_slope_at_four_ripples() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(grid, |_, _| -9.0);
        let spec = WaterSpec {
            ripple: Some(0.5),
            rivers: vec![River {
                points: vec![
                    [2.0, 5.0, 6.0, 4.0],
                    [9.0, 5.0, 6.0, 4.0],
                    [10.0, 5.0, 1.0, 4.0],
                    [18.0, 5.0, 1.0, 4.0],
                ],
            }],
            ..spec()
        };
        let field = WaterField::compute(&spec, &ground, grid);
        let flow = field.flow(at(grid, 9.5, 5.0));
        assert!(
            (flow[0] - 2.0).abs() < 1e-5 && flow[1].abs() < 1e-5,
            "{flow:?}"
        );
    }

    #[test]
    fn a_stretch_of_the_river_line_with_no_water_warns_with_its_point_numbers() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(
            grid,
            |x, _| if (8.0..12.0).contains(&x) { 5.0 } else { -3.0 },
        );
        let spec = WaterSpec {
            rivers: vec![River {
                points: vec![
                    [2.0, 5.0, 1.0, 1.0],
                    [6.0, 5.0, 1.0, 1.0],
                    [14.0, 5.0, 1.0, 1.0],
                    [18.0, 5.0, 1.0, 1.0],
                ],
            }],
            ..spec()
        };
        let field = WaterField::compute(&spec, &ground, grid);
        let warnings = field.warnings();
        assert_eq!(warnings.len(), 1, "{warnings:?}");
        assert!(warnings[0].message.contains("река 1"), "{warnings:?}");
        assert!(
            warnings[0].message.contains("точками 2 и 3"),
            "{warnings:?}"
        );
        let lake_bed = ground_of(grid, |x, y| {
            if (4.5..=5.5).contains(&y) && (8.0..=12.0).contains(&x) {
                5.0
            } else {
                -3.0
            }
        });
        let inside_lake = WaterField::compute(
            &WaterSpec {
                lakes: vec![Lake {
                    at: [4.0, 5.0],
                    level: 6.0,
                }],
                rivers: vec![River {
                    points: vec![[9.0, 5.0, 1.0, 1.0], [11.0, 5.0, 1.0, 1.0]],
                }],
                ..spec.clone()
            },
            &lake_bed,
            grid,
        );
        assert!(
            inside_lake
                .warnings()
                .iter()
                .all(|warning| !warning.message.contains("река")),
            "участок внутри озера предупреждения не даёт: {:?}",
            inside_lake.warnings()
        );
    }

    /// Русла по ломаной, найденные перебором всех звеньев для каждой точки сетки.
    fn brute_channels(points: &[[f64; 4]], grid: Grid) -> Vec<Option<f64>> {
        (0..grid.len())
            .map(|point| {
                let place = grid.place(point);
                let mut best: Option<(f64, usize, f64)> = None;
                for (segment, pair) in points.windows(2).enumerate() {
                    let (t, distance2) = project(pair[0], pair[1], place);
                    if best.is_none_or(|(near, _, _)| distance2 < near) {
                        best = Some((distance2, segment, t));
                    }
                }
                let (distance2, segment, t) = best?;
                let (a, b) = (points[segment], points[segment + 1]);
                let half = lerp(a[3], b[3], t) / 2.0;
                (distance2.sqrt() <= half + EPS).then(|| lerp(a[2], b[2], t))
            })
            .collect()
    }

    #[test]
    fn the_channels_found_near_each_segment_agree_with_a_scan_of_every_segment() {
        let grid = grid(24, 16, 4);
        let mut seed = 99_u64;
        let mut random = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 11) as f64 / (1_u64 << 53) as f64
        };
        for _ in 0..40 {
            let count = 2 + (random() * 5.0) as usize;
            let mut height = 6.0;
            let points: Vec<[f64; 4]> = (0..count)
                .map(|_| {
                    height -= random() * 1.5;
                    [
                        24.0 * random(),
                        16.0 * random(),
                        height,
                        0.3 + 3.0 * random(),
                    ]
                })
                .collect();
            let fast = river_channels(
                &[River {
                    points: points.clone(),
                }],
                grid,
            );
            let slow = brute_channels(&points, grid);
            for point in 0..grid.len() {
                assert_eq!(
                    fast[point].map(|c| c.surface),
                    slow[point],
                    "точка {point} ({:?}) ломаной {points:?}",
                    grid.place(point)
                );
            }
        }
    }

    #[test]
    fn the_surface_range_covers_every_flooded_point() {
        let grid = grid(10, 6, 2);
        let ground = ground_of(grid, |x, _| x - 4.0);
        let field = WaterField::compute(
            &WaterSpec {
                level: Some(0.5),
                ..spec()
            },
            &ground,
            grid,
        );
        assert_eq!(field.range(), Some((0.5, 0.5)));
        let dry = WaterField::compute(&spec(), &ground, grid);
        assert_eq!(dry.range(), None);
    }

    /// Ревью фазы 24: моря внизу нет, за краем русла земля ниже глади — гладь кромки сходит к земле, и
    /// нижняя граница воды её не отрезает, иначе луч мыши пройдёт мимо кромки в дно.
    #[test]
    fn the_surface_range_reaches_down_to_a_bank_cut_by_the_channel_edge() {
        let grid = grid(20, 10, 4);
        let ground = ground_of(grid, |_, _| -3.0);
        let field = WaterField::compute(
            &WaterSpec {
                rivers: vec![straight_river([4.0, 2.0], 2.0)],
                ..spec()
            },
            &ground,
            grid,
        );
        let (low, high) = field.range().expect("вода есть");
        for point in 0..grid.len() {
            if let Some(surface) = field.surface(point) {
                assert!(
                    (low..=high).contains(&surface),
                    "гладь {surface} в точке {point} вне {low}..{high}"
                );
            }
        }
        assert_eq!(low, -3.0);
        assert_eq!(high, 4.0);
    }
}

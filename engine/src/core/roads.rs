//! «Правила игры» → «Езда по дорогам»: сеть из дорог-прямоугольников, самый быстрый путь по ней и
//! движение машины по своей стороне со скруглёнными углами. Модуль не знает о `World`: дороги ему
//! приносят готовыми прямоугольниками, как `pathfind` приносит препятствия.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::rc::Rc;

use super::value::Vec2;

/// Дороги связаны, если их высоты расходятся не больше чем на это.
pub const MAX_HEIGHT_GAP: f64 = 0.4;
/// Доля ширины дороги, на которую своя сторона сдвинута от оси.
pub const SIDE_SHIFT_SHARE: f64 = 0.25;
/// Поворот на стыке больше этого — разворот: машина идёт по прямой от одной стороны к другой.
pub const MAX_JOINT_TURN_DEGREES: f64 = 150.0;
/// Доля ширины дороги, дальше которой от излома дуга скругления не начинается.
pub const ARC_WIDTH_SHARE: f64 = 0.5;

const EPS: f64 = 1e-9;

pub(super) fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub(super) fn add_scaled(a: Vec2, direction: Vec2, k: f64) -> Vec2 {
    [a[0] + direction[0] * k, a[1] + direction[1] * k]
}

pub(super) fn dot(a: Vec2, b: Vec2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub(super) fn cross(a: Vec2, b: Vec2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub(super) fn length(a: Vec2) -> f64 {
    dot(a, a).sqrt()
}

pub(super) fn distance(a: Vec2, b: Vec2) -> f64 {
    length(sub(a, b))
}

fn unit(a: Vec2) -> Vec2 {
    let len = length(a);
    if len < EPS {
        [1.0, 0.0]
    } else {
        [a[0] / len, a[1] / len]
    }
}

/// Ближайшая к `p` точка отрезка `a`–`b` и её доля пути от `a` к `b`.
fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f64) {
    let along = sub(b, a);
    let len_sq = dot(along, along);
    if len_sq < EPS * EPS {
        return (a, 0.0);
    }
    let share = (dot(sub(p, a), along) / len_sq).clamp(0.0, 1.0);
    (add_scaled(a, along, share), share)
}

/// Дорога, как её видит сеть: прямоугольник `position`/`size`, повёрнутый вокруг середины, и высота
/// основания. `sin_cos` — синус и косинус `rotation`, как их считает `Rotation::sin_cos`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadShape {
    pub id: u32,
    pub position: Vec2,
    pub size: Vec2,
    pub sin_cos: (f64, f64),
    pub z: f64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Road {
    pub(super) id: u32,
    pub(super) center: Vec2,
    pub(super) dir: Vec2,
    pub(super) half_length: f64,
    pub(super) half_width: f64,
    pub(super) z: f64,
}

impl Road {
    /// Ось идёт вдоль длинной стороны; у квадрата — вдоль ширины `size`.
    pub(super) fn from_shape(shape: &RoadShape) -> Road {
        let (sin, cos) = shape.sin_cos;
        let center = [
            shape.position[0] + shape.size[0] / 2.0,
            shape.position[1] + shape.size[1] / 2.0,
        ];
        let (dir, long, short) = if shape.size[1] > shape.size[0] {
            ([-sin, cos], shape.size[1], shape.size[0])
        } else {
            ([cos, sin], shape.size[0], shape.size[1])
        };
        Road {
            id: shape.id,
            center,
            dir,
            half_length: long / 2.0,
            half_width: short / 2.0,
            z: shape.z,
        }
    }

    pub(super) fn length(&self) -> f64 {
        self.half_length * 2.0
    }

    pub(super) fn point_at(&self, along: f64) -> Vec2 {
        add_scaled(self.center, self.dir, along - self.half_length)
    }

    pub(super) fn end(&self, which: usize) -> Vec2 {
        self.point_at(if which == 0 { 0.0 } else { self.length() })
    }

    fn axis(&self) -> [Vec2; 2] {
        [self.end(0), self.end(1)]
    }

    /// Лежит ли точка в прямоугольнике дороги, края включая.
    fn contains(&self, p: Vec2) -> bool {
        let local = sub(p, self.center);
        dot(local, self.dir).abs() <= self.half_length + EPS
            && cross(self.dir, local).abs() <= self.half_width + EPS
    }

    /// Ближайшая к `p` точка оси и её расстояние от начала оси.
    fn closest_on_axis(&self, p: Vec2) -> (Vec2, f64) {
        let (point, share) = closest_on_segment(p, self.end(0), self.end(1));
        (point, share * self.length())
    }

    fn bounds(&self) -> [Vec2; 2] {
        let reach = |axis: usize| {
            self.dir[axis].abs() * self.half_length
                + self.dir[1 - axis].abs() * self.half_width
                + EPS
        };
        let (rx, ry) = (reach(0), reach(1));
        [
            [self.center[0] - rx, self.center[1] - ry],
            [self.center[0] + rx, self.center[1] + ry],
        ]
    }
}

/// Связь двух дорог: точка на оси каждой. Концом `end_of_a` называется тот конец оси дороги `a`,
/// что лежит внутри прямоугольника `b`; у пересечения осей его нет.
#[derive(Debug, Clone, Copy)]
pub(super) struct Link {
    pub(super) a: usize,
    pub(super) b: usize,
    pub(super) end_of_a: Option<usize>,
    pub(super) point_a: Vec2,
    pub(super) point_b: Vec2,
}

#[derive(Debug, Clone, Copy)]
struct Piece {
    a: usize,
    b: usize,
    road: usize,
    length: f64,
}

/// Пары дорог, чьи прямоугольники могут пересекаться, — через сетку, а не каждая с каждой.
fn candidate_pairs(roads: &[Road]) -> Vec<(usize, usize)> {
    let cell = (roads
        .iter()
        .map(|road| road.half_length.max(road.half_width) * 2.0)
        .sum::<f64>()
        / roads.len().max(1) as f64)
        .max(1.0);
    let mut cells: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (index, road) in roads.iter().enumerate() {
        let [low, high] = road.bounds();
        let (x0, y0) = (
            (low[0] / cell).floor() as i64,
            (low[1] / cell).floor() as i64,
        );
        let (x1, y1) = (
            (high[0] / cell).floor() as i64,
            (high[1] / cell).floor() as i64,
        );
        for x in x0..=x1 {
            for y in y0..=y1 {
                cells.entry((x, y)).or_default().push(index);
            }
        }
    }
    let mut pairs = Vec::new();
    for members in cells.values() {
        for (k, &first) in members.iter().enumerate() {
            for &second in &members[k + 1..] {
                pairs.push((first, second));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

/// Точка пересечения осей двух дорог, если они пересекаются.
fn axes_cross(first: &Road, second: &Road) -> Option<Vec2> {
    let (a1, a2) = (first.end(0), second.end(0));
    let r = sub(first.end(1), a1);
    let s = sub(second.end(1), a2);
    let denominator = cross(r, s);
    if denominator.abs() < EPS * EPS {
        return None;
    }
    let between = sub(a2, a1);
    let along_first = cross(between, s) / denominator;
    let along_second = cross(between, r) / denominator;
    let within = |share: f64, len: f64| share * len >= -EPS && (share - 1.0) * len <= EPS;
    (within(along_first, length(r)) && within(along_second, length(s)))
        .then(|| add_scaled(a1, r, along_first))
}

fn links_between(roads: &[Road], a: usize, b: usize, links: &mut Vec<Link>) {
    if (roads[a].z - roads[b].z).abs() > MAX_HEIGHT_GAP + EPS {
        return;
    }
    if let Some(point) = axes_cross(&roads[a], &roads[b]) {
        links.push(Link {
            a,
            b,
            end_of_a: None,
            point_a: point,
            point_b: point,
        });
    }
    for (carried, carrier) in [(a, b), (b, a)] {
        for end in 0..2 {
            let point = roads[carried].end(end);
            if roads[carrier].contains(point) {
                links.push(Link {
                    a: carried,
                    b: carrier,
                    end_of_a: Some(end),
                    point_a: point,
                    point_b: roads[carrier].closest_on_axis(point).0,
                });
            }
        }
    }
}

pub(super) fn find_links(roads: &[Road]) -> Vec<Link> {
    let mut links = Vec::new();
    for (a, b) in candidate_pairs(roads) {
        links_between(roads, a, b, &mut links);
    }
    links
}

/// Узлы сети и куски между ними: узлы — концы осей и точки связей, куски — части осей между
/// соседними узлами и короткие куски связей «конец в прямоугольнике другой дороги».
fn cut_into_pieces(roads: &[Road], links: &[Link]) -> (Vec<Vec2>, Vec<Piece>) {
    let mut nodes: Vec<Vec2> = Vec::new();
    let mut cuts: Vec<Vec<(f64, usize)>> = Vec::with_capacity(roads.len());
    let mut ends: Vec<[usize; 2]> = Vec::with_capacity(roads.len());
    for road in roads {
        let first = nodes.len();
        nodes.push(road.end(0));
        nodes.push(road.end(1));
        ends.push([first, first + 1]);
        cuts.push(vec![(0.0, first), (road.length(), first + 1)]);
    }
    let mut pieces = Vec::new();
    for link in links {
        let along_b = roads[link.b].closest_on_axis(link.point_b).1;
        match link.end_of_a {
            None => {
                let node = nodes.len();
                nodes.push(link.point_a);
                cuts[link.a].push((roads[link.a].closest_on_axis(link.point_a).1, node));
                cuts[link.b].push((along_b, node));
            }
            Some(end) if distance(link.point_a, link.point_b) <= EPS => {
                cuts[link.b].push((along_b, ends[link.a][end]));
            }
            Some(end) => {
                let node = nodes.len();
                nodes.push(link.point_b);
                cuts[link.b].push((along_b, node));
                pieces.push(Piece {
                    a: ends[link.a][end],
                    b: node,
                    road: link.a,
                    length: distance(link.point_a, link.point_b),
                });
            }
        }
    }
    for (road, road_cuts) in cuts.iter_mut().enumerate() {
        road_cuts.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        for pair in road_cuts.windows(2) {
            if pair[0].1 != pair[1].1 {
                pieces.push(Piece {
                    a: pair[0].1,
                    b: pair[1].1,
                    road,
                    length: pair[1].0 - pair[0].0,
                });
            }
        }
    }
    (nodes, pieces)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Right,
    Left,
    Middle,
}

impl Side {
    /// Куда от оси сдвинута сторона: `1` — вправо по ходу, `-1` — влево, `0` — по оси.
    fn share(self) -> f64 {
        match self {
            Side::Right => 1.0,
            Side::Left => -1.0,
            Side::Middle => 0.0,
        }
    }
}

/// Часть пути по дороге: от `from` до `to` вдоль одной дороги, предельная скорость на ней `cap`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leg {
    pub from: Vec2,
    pub to: Vec2,
    pub road: usize,
    pub width: f64,
    pub cap: Option<f64>,
}

impl Leg {
    fn length(&self) -> f64 {
        distance(self.from, self.to)
    }
}

/// Путь по осям: въезд, части по дорогам и съезд.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub entry: Vec2,
    pub legs: Vec<Leg>,
    pub exit: Vec2,
}

/// Длина и время пути по осям — то, что узнаёт код вызовом `road_route`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    pub length: f64,
    pub time: f64,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    piece: usize,
    share: f64,
    point: Vec2,
}

#[derive(Debug, Clone, Copy)]
enum Via {
    Unreached,
    Entry,
    Piece { piece: usize, from: usize },
}

struct Search {
    time: Vec<f64>,
    via: Vec<Via>,
}

#[derive(Debug, Clone, Copy)]
struct Exit {
    piece: usize,
    point: Vec2,
    gap: f64,
    time: f64,
    /// Узел, с которого съезжают на кусок; `None` — съезд на том же куске, что и въезд.
    from_node: Option<usize>,
}

#[derive(PartialEq)]
struct Waiting {
    time: f64,
    node: usize,
}

impl Eq for Waiting {}

impl Ord for Waiting {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .time
            .total_cmp(&self.time)
            .then(other.node.cmp(&self.node))
    }
}

impl PartialOrd for Waiting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Что сеть рассказывает редактору о выбранной дороге.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadLinks {
    pub z: f64,
    pub axis: [Vec2; 2],
    pub links: Vec<(Vec2, u32)>,
    pub ends: Vec<Vec2>,
    pub neighbours: Vec<(u32, [Vec2; 2], f64)>,
}

/// Сеть дорог: узлы, куски и связи. Собирается заново, когда меняются дороги, а предельные скорости
/// дорог — только время на их кусках.
#[derive(Debug, Default)]
pub struct RoadNetwork {
    shapes: Vec<RoadShape>,
    speeds: Vec<Option<f64>>,
    roads: Vec<Road>,
    nodes: Vec<Vec2>,
    pieces: Vec<Piece>,
    around: Vec<Vec<usize>>,
    links: Vec<Link>,
    version: u64,
    fresh_shapes: Vec<RoadShape>,
    fresh_speeds: Vec<Option<f64>>,
}

impl RoadNetwork {
    /// Номер состояния сети: растёт, когда сеть пересобрана или сменилась скорость на дороге.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn is_empty(&self) -> bool {
        self.roads.is_empty()
    }

    /// Приводит сеть к дорогам `shapes` (по возрастанию номера) и их скоростям `speeds`; `true`, если
    /// что-то изменилось.
    pub fn sync(&mut self, shapes: &[RoadShape], speeds: &[Option<f64>]) -> bool {
        if self.shapes != shapes {
            self.rebuild(shapes);
            self.speeds = speeds.to_vec();
            self.version += 1;
            return true;
        }
        if self.speeds != speeds {
            self.speeds = speeds.to_vec();
            self.version += 1;
            return true;
        }
        false
    }

    /// То же, что `sync`, для дорог, которые `fill` кладёт в переданные буферы: буферы живут между
    /// вызовами, и шаг без перемен ничего не выделяет.
    pub fn sync_with(
        &mut self,
        fill: impl FnOnce(&mut Vec<RoadShape>, &mut Vec<Option<f64>>),
    ) -> bool {
        let mut shapes = std::mem::take(&mut self.fresh_shapes);
        let mut speeds = std::mem::take(&mut self.fresh_speeds);
        fill(&mut shapes, &mut speeds);
        let changed = self.sync(&shapes, &speeds);
        self.fresh_shapes = shapes;
        self.fresh_speeds = speeds;
        changed
    }

    pub fn clear(&mut self) {
        self.rebuild(&[]);
        self.speeds.clear();
        self.version += 1;
    }

    fn rebuild(&mut self, shapes: &[RoadShape]) {
        self.shapes = shapes.to_vec();
        self.roads = shapes.iter().map(Road::from_shape).collect();
        self.links = find_links(&self.roads);
        (self.nodes, self.pieces) = cut_into_pieces(&self.roads, &self.links);
        self.around = vec![Vec::new(); self.nodes.len()];
        for (index, piece) in self.pieces.iter().enumerate() {
            self.around[piece.a].push(index);
            self.around[piece.b].push(index);
        }
    }

    fn cap(&self, road: usize) -> Option<f64> {
        self.speeds.get(road).copied().flatten()
    }

    fn is_open(&self, road: usize) -> bool {
        self.cap(road).is_none_or(|cap| cap > 0.0)
    }

    /// Скорость машины со скоростью `speed` на дороге; `None` — дорога закрыта.
    fn travel_speed(&self, road: usize, speed: f64) -> Option<f64> {
        self.is_open(road)
            .then(|| self.cap(road).map_or(speed, |cap| cap.min(speed)))
    }

    fn piece_time(&self, piece: &Piece, speed: f64) -> Option<f64> {
        Some(piece.length / self.travel_speed(piece.road, speed)?)
    }

    fn ends_of(&self, piece: &Piece) -> [Vec2; 2] {
        [self.nodes[piece.a], self.nodes[piece.b]]
    }

    /// Въезд: ближайшая к `start` точка осей открытых дорог; с `z` — среди дорог не выше и не ниже
    /// ступеньки, а если таких нет, среди всех.
    fn find_entry(&self, start: Vec2, z: Option<f64>) -> Option<Entry> {
        let nearest = |near_z: Option<f64>| {
            let mut best: Option<(f64, Entry)> = None;
            for (index, piece) in self.pieces.iter().enumerate() {
                if !self.is_open(piece.road)
                    || near_z.is_some_and(|z| {
                        (self.roads[piece.road].z - z).abs() > MAX_HEIGHT_GAP + EPS
                    })
                {
                    continue;
                }
                let [a, b] = self.ends_of(piece);
                let (point, share) = closest_on_segment(start, a, b);
                let gap = distance(start, point);
                if best.is_none_or(|(known, _)| gap < known - 1e-12) {
                    best = Some((
                        gap,
                        Entry {
                            piece: index,
                            share,
                            point,
                        },
                    ));
                }
            }
            best.map(|(_, entry)| entry)
        };
        z.and_then(|z| nearest(Some(z))).or_else(|| nearest(None))
    }

    fn search(&self, entry: &Entry, speed: f64) -> Search {
        let mut time = vec![f64::INFINITY; self.nodes.len()];
        let mut via = vec![Via::Unreached; self.nodes.len()];
        let mut queue = BinaryHeap::new();
        let first = self.pieces[entry.piece];
        if let Some(whole) = self.piece_time(&first, speed) {
            for (node, share) in [(first.a, entry.share), (first.b, 1.0 - entry.share)] {
                time[node] = whole * share;
                via[node] = Via::Entry;
                queue.push(Waiting {
                    time: time[node],
                    node,
                });
            }
        }
        while let Some(Waiting {
            time: reached,
            node,
        }) = queue.pop()
        {
            if reached > time[node] {
                continue;
            }
            for &index in &self.around[node] {
                let piece = self.pieces[index];
                let Some(cost) = self.piece_time(&piece, speed) else {
                    continue;
                };
                let next = if piece.a == node { piece.b } else { piece.a };
                if reached + cost < time[next] {
                    time[next] = reached + cost;
                    via[next] = Via::Piece {
                        piece: index,
                        from: node,
                    };
                    queue.push(Waiting {
                        time: time[next],
                        node: next,
                    });
                }
            }
        }
        Search { time, via }
    }

    /// Съезд: ближайшая к `goal` точка осей среди тех, куда от въезда можно доехать; при равных —
    /// та, до которой быстрее.
    fn find_exit(&self, entry: &Entry, search: &Search, goal: Vec2, speed: f64) -> Option<Exit> {
        let mut best: Option<Exit> = None;
        for (index, piece) in self.pieces.iter().enumerate() {
            let Some(whole) = self.piece_time(piece, speed) else {
                continue;
            };
            let [a, b] = self.ends_of(piece);
            let (point, share) = closest_on_segment(goal, a, b);
            let mut arrival: Option<(f64, Option<usize>)> = None;
            let mut consider = |time: f64, from_node: Option<usize>| {
                if arrival.is_none_or(|(known, _)| time < known) {
                    arrival = Some((time, from_node));
                }
            };
            if index == entry.piece {
                consider((share - entry.share).abs() * whole, None);
            }
            consider(search.time[piece.a] + whole * share, Some(piece.a));
            consider(search.time[piece.b] + whole * (1.0 - share), Some(piece.b));
            let Some((time, from_node)) = arrival.filter(|(time, _)| time.is_finite()) else {
                continue;
            };
            let candidate = Exit {
                piece: index,
                point,
                gap: distance(goal, point),
                time,
                from_node,
            };
            let better = best.is_none_or(|known| {
                candidate.gap < known.gap - EPS
                    || (candidate.gap <= known.gap + EPS && candidate.time < known.time - EPS)
            });
            if better {
                best = Some(candidate);
            }
        }
        best
    }

    fn leg(&self, from: Vec2, to: Vec2, road: usize) -> Leg {
        Leg {
            from,
            to,
            road,
            width: self.roads[road].half_width * 2.0,
            cap: self.cap(road),
        }
    }

    /// Части пути от въезда до съезда: куски, которыми едет машина, без нулевых и со слитыми
    /// продолжениями одной дороги.
    fn legs(&self, entry: &Entry, search: &Search, exit: &Exit) -> Vec<Leg> {
        let mut raw: Vec<Leg> = Vec::new();
        let first_piece = self.pieces[entry.piece];
        match exit.from_node {
            None => raw.push(self.leg(entry.point, exit.point, first_piece.road)),
            Some(last) => {
                let mut chain = Vec::new();
                let mut node = last;
                while let Via::Piece { piece, from } = search.via[node] {
                    chain.push((piece, from, node));
                    node = from;
                }
                raw.push(self.leg(entry.point, self.nodes[node], first_piece.road));
                for &(piece, from, to) in chain.iter().rev() {
                    raw.push(self.leg(self.nodes[from], self.nodes[to], self.pieces[piece].road));
                }
                raw.push(self.leg(self.nodes[last], exit.point, self.pieces[exit.piece].road));
            }
        }
        let mut legs: Vec<Leg> = Vec::new();
        for leg in raw.into_iter().filter(|leg| leg.length() > EPS) {
            if let Some(previous) = legs.last_mut()
                && previous.road == leg.road
                && cross(
                    unit(sub(previous.to, previous.from)),
                    unit(sub(leg.to, leg.from)),
                )
                .abs()
                    < EPS
                && dot(sub(previous.to, previous.from), sub(leg.to, leg.from)) > 0.0
            {
                previous.to = leg.to;
            } else {
                legs.push(leg);
            }
        }
        legs
    }

    /// Самый быстрый путь по сети от `start` к `goal` для машины со скоростью `speed`; `None` —
    /// открытых дорог нет, и машина едет по прямой. `z` — высота машины: дороги её высоты предпочтены.
    pub fn route(&self, start: Vec2, goal: Vec2, speed: f64, z: Option<f64>) -> Option<Route> {
        if speed.is_nan() || speed <= 0.0 {
            return None;
        }
        let entry = self.find_entry(start, z)?;
        let search = self.search(&entry, speed);
        let exit = self.find_exit(&entry, &search, goal, speed)?;
        Some(Route {
            entry: entry.point,
            legs: self.legs(&entry, &search, &exit),
            exit: exit.point,
        })
    }

    /// Длина и время пути по осям из `from` в `to` со скоростью `speed`, прямые куски до въезда и
    /// после съезда вместе; `None` без скорости.
    pub fn measure(&self, from: Vec2, to: Vec2, speed: f64) -> Option<Measure> {
        if speed.is_nan() || speed <= 0.0 {
            return None;
        }
        let Some(route) = self.route(from, to, speed, None) else {
            let straight = distance(from, to);
            return Some(Measure {
                length: straight,
                time: straight / speed,
            });
        };
        let ends = distance(from, route.entry) + distance(route.exit, to);
        let mut length = ends;
        let mut time = ends / speed;
        for leg in &route.legs {
            length += leg.length();
            time += leg.length() / leg.cap.map_or(speed, |cap| cap.min(speed));
        }
        Some(Measure { length, time })
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.roads.binary_search_by_key(&id, |road| road.id).ok()
    }

    /// Связи дороги `id`; `None`, если такой дороги в сети нет.
    pub fn links_of(&self, id: u32) -> Option<RoadLinks> {
        let index = self.index_of(id)?;
        let road = &self.roads[index];
        let mut links = Vec::new();
        let mut neighbours: Vec<(u32, [Vec2; 2], f64)> = Vec::new();
        for link in &self.links {
            let (point, other) = if link.a == index {
                (link.point_a, link.b)
            } else if link.b == index {
                (link.point_b, link.a)
            } else {
                continue;
            };
            let other_road = &self.roads[other];
            if links
                .iter()
                .any(|(known, id)| *id == other_road.id && distance(*known, point) <= EPS)
            {
                continue;
            }
            links.push((point, other_road.id));
            if !neighbours
                .iter()
                .any(|(known, _, _)| *known == other_road.id)
            {
                neighbours.push((other_road.id, other_road.axis(), other_road.z));
            }
        }
        let ends = (0..2)
            .map(|end| road.end(end))
            .filter(|&end| links.iter().all(|(point, _)| distance(*point, end) > EPS))
            .collect();
        Some(RoadLinks {
            z: road.z,
            axis: road.axis(),
            links,
            ends,
            neighbours,
        })
    }
}

/// Часть пути машины: прямая или дуга окружности. `cap` — предельная скорость на ней.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Primitive {
    Line {
        from: Vec2,
        to: Vec2,
        cap: Option<f64>,
    },
    Arc {
        center: Vec2,
        start: Vec2,
        radius: f64,
        /// Поворот по дуге в радианах со знаком: куда повернут путь.
        sweep: f64,
        cap: Option<f64>,
    },
}

impl Primitive {
    fn length(&self) -> f64 {
        match self {
            Primitive::Line { from, to, .. } => distance(*from, *to),
            Primitive::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }

    fn cap(&self) -> Option<f64> {
        match self {
            Primitive::Line { cap, .. } | Primitive::Arc { cap, .. } => *cap,
        }
    }

    fn point_at(&self, travelled: f64) -> Vec2 {
        match self {
            Primitive::Line { from, to, .. } => {
                let len = distance(*from, *to);
                if len < EPS {
                    *to
                } else {
                    add_scaled(*from, sub(*to, *from), travelled / len)
                }
            }
            Primitive::Arc {
                center,
                start,
                radius,
                sweep,
                ..
            } => {
                let angle = sweep.signum() * travelled / radius;
                let (sin, cos) = (libm::sin(angle), libm::cos(angle));
                let spoke = sub(*start, *center);
                [
                    center[0] + spoke[0] * cos - spoke[1] * sin,
                    center[1] + spoke[0] * sin + spoke[1] * cos,
                ]
            }
        }
    }
}

fn min_cap(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Линия своей стороны одной части пути: начало, конец и направление.
fn side_line(leg: &Leg, side: Side) -> (Vec2, Vec2, Vec2) {
    let dir = unit(sub(leg.to, leg.from));
    let shift = side.share() * SIDE_SHIFT_SHARE * leg.width;
    let right = [-dir[1], dir[0]];
    (
        add_scaled(leg.from, right, shift),
        add_scaled(leg.to, right, shift),
        dir,
    )
}

/// Точка, где линии сторон двух частей продолжаются до пересечения; `None` — на стыке машина идёт
/// по прямой: разворот, параллельные линии или пересечение дальше ширины дороги от узла.
fn joint_point(previous: Vec2, first: &Leg, second: &Leg, side: Side) -> Option<Vec2> {
    let (a1, _, d1) = side_line(first, side);
    let (a2, b2, d2) = side_line(second, side);
    let turn = libm::atan2(cross(d1, d2).abs(), dot(d1, d2));
    if turn > MAX_JOINT_TURN_DEGREES.to_radians() || cross(d1, d2).abs() < EPS {
        return None;
    }
    let along_first = cross(sub(a2, a1), d2) / cross(d1, d2);
    let point = add_scaled(a1, d1, along_first);
    let reach = first.width.max(second.width);
    (distance(point, first.to) <= reach
        && dot(sub(point, previous), d1) >= -EPS
        && dot(sub(b2, point), d2) >= -EPS)
        .then_some(point)
}

/// Ломаная пути: вершины, ширина дороги у каждой и предельная скорость на отрезке, что к ней идёт.
struct Polyline {
    points: Vec<Vec2>,
    widths: Vec<f64>,
    caps: Vec<Option<f64>>,
}

impl Polyline {
    fn push(&mut self, point: Vec2, width: f64, cap: Option<f64>) {
        let last = self.points[self.points.len() - 1];
        if distance(last, point) <= EPS {
            return;
        }
        self.points.push(point);
        self.widths.push(width);
        self.caps.push(cap);
    }
}

fn polyline_of(start: Vec2, goal: Vec2, legs: &[Leg], side: Side) -> Polyline {
    let mut line = Polyline {
        points: vec![start],
        widths: vec![0.0],
        caps: vec![None],
    };
    let first = &legs[0];
    line.push(side_line(first, side).0, first.width, None);
    for pair in legs.windows(2) {
        let (before, after) = (&pair[0], &pair[1]);
        let previous = line.points[line.points.len() - 1];
        match joint_point(previous, before, after, side) {
            Some(point) => line.push(point, before.width.min(after.width), before.cap),
            None => {
                line.push(side_line(before, side).1, before.width, before.cap);
                line.push(
                    side_line(after, side).0,
                    after.width,
                    min_cap(before.cap, after.cap),
                );
            }
        }
    }
    let last = &legs[legs.len() - 1];
    line.push(side_line(last, side).1, last.width, last.cap);
    line.push(goal, 0.0, None);
    line
}

/// Дуга, скругляющая излом `corner` между отрезками с направлениями `before` и `after`; касается обоих
/// на расстоянии `reach` от излома.
fn fillet(corner: Vec2, before: Vec2, after: Vec2, reach: f64, cap: Option<f64>) -> Primitive {
    let turn = libm::atan2(cross(before, after).abs(), dot(before, after));
    let side = cross(before, after).signum();
    let radius = reach / libm::tan(turn / 2.0);
    let start = add_scaled(corner, before, -reach);
    let center = add_scaled(start, [-before[1], before[0]], side * radius);
    Primitive::Arc {
        center,
        start,
        radius,
        sweep: side * turn,
        cap,
    }
}

fn round_corners(line: &Polyline) -> Vec<Primitive> {
    let points = &line.points;
    let mut primitives = Vec::new();
    let mut from = points[0];
    for (before_index, corner) in points.windows(3).enumerate() {
        let index = before_index + 1;
        let before = unit(sub(corner[1], corner[0]));
        let after = unit(sub(corner[2], corner[1]));
        let turn = libm::atan2(cross(before, after).abs(), dot(before, after));
        let reach = (line.widths[index] * ARC_WIDTH_SHARE)
            .min(distance(corner[1], corner[0]) / 2.0)
            .min(distance(corner[2], corner[1]) / 2.0);
        if turn < EPS || turn > std::f64::consts::PI - EPS || reach < EPS {
            primitives.push(Primitive::Line {
                from,
                to: corner[1],
                cap: line.caps[index],
            });
            from = corner[1];
            continue;
        }
        primitives.push(Primitive::Line {
            from,
            to: add_scaled(corner[1], before, -reach),
            cap: line.caps[index],
        });
        primitives.push(fillet(
            corner[1],
            before,
            after,
            reach,
            min_cap(line.caps[index], line.caps[index + 1]),
        ));
        from = add_scaled(corner[1], after, reach);
    }
    primitives.push(Primitive::Line {
        from,
        to: points[points.len() - 1],
        cap: line.caps[points.len() - 1],
    });
    primitives
}

/// Путь машины: прямые и дуги от её середины до точки назначения.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    primitives: Vec<Primitive>,
    goal: Vec2,
}

impl Path {
    /// Путь по `route` со своей стороной `side`; без пути по сети — прямая от `start` к `goal`.
    pub fn new(start: Vec2, goal: Vec2, route: Option<&Route>, side: Side) -> Path {
        let legs = route.map_or(&[][..], |route| &route.legs);
        let primitives = if legs.is_empty() {
            vec![Primitive::Line {
                from: start,
                to: goal,
                cap: None,
            }]
        } else {
            round_corners(&polyline_of(start, goal, legs, side))
        };
        Path { primitives, goal }
    }

    /// Длина пути с дугами.
    pub fn length(&self) -> f64 {
        self.primitives.iter().map(Primitive::length).sum()
    }
}

/// Где машина на пути: номер части и сколько по ней проехано.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Cursor {
    index: usize,
    travelled: f64,
}

impl Cursor {
    /// Проезжает путь за `seconds` со скоростью `speed`; остаток времени на границе частей идёт на
    /// следующую со скоростью следующей. Возвращает, доехала ли машина.
    fn advance(&mut self, path: &Path, speed: f64, seconds: f64) -> bool {
        let mut left = seconds;
        while let Some(primitive) = path.primitives.get(self.index) {
            let pace = primitive.cap().map_or(speed, |cap| cap.min(speed));
            let reach = pace * left;
            let remaining = primitive.length() - self.travelled;
            if reach < remaining {
                self.travelled += reach;
                return false;
            }
            left -= remaining / pace;
            self.index += 1;
            self.travelled = 0.0;
        }
        true
    }

    fn position(&self, path: &Path) -> Vec2 {
        path.primitives
            .get(self.index)
            .map_or(path.goal, |primitive| primitive.point_at(self.travelled))
    }
}

#[derive(Debug)]
struct CarPath {
    target: Vec2,
    position: Vec2,
    version: u64,
    side: Side,
    path: Path,
    cursor: Cursor,
}

/// Машина, какой её видит езда: что нужно, чтобы решить, нужен ли новый путь, и куда она едет.
#[derive(Debug, Clone, Copy)]
pub struct Driver {
    pub position: Vec2,
    pub size: Vec2,
    pub z: f64,
    pub target: Vec2,
    pub speed: f64,
    pub side: Side,
}

/// Куда машина попала за шаг: новое `position` и приехала ли.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moved {
    pub position: Vec2,
    pub arrived: bool,
}

/// Сеть дорог и пути машин — всё, что езда держит между шагами. Сеть одна на игру: ею пользуются и правило
/// `drive`, и вызов кода `road_route`, поэтому у неё два хозяина (`network_handle`); обращаются к ней по
/// очереди, в одном потоке и не вложенно.
#[derive(Debug, Default)]
pub struct RoadState {
    network: Rc<RefCell<RoadNetwork>>,
    paths: HashMap<u32, CarPath>,
}

impl RoadState {
    pub fn new() -> Self {
        RoadState::default()
    }

    /// Сеть игры для исполнителя кода.
    pub fn network_handle(&self) -> Rc<RefCell<RoadNetwork>> {
        Rc::clone(&self.network)
    }

    /// Сверяет сеть с дорогами мира, которые `fill` кладёт в переданные списки; `true`, если сеть
    /// изменилась.
    pub fn sync_with(&self, fill: impl FnOnce(&mut Vec<RoadShape>, &mut Vec<Option<f64>>)) -> bool {
        self.network.borrow_mut().sync_with(fill)
    }

    /// Связи дороги `id` по нынешней сети.
    pub fn links_of(&self, id: u32) -> Option<RoadLinks> {
        self.network.borrow().links_of(id)
    }

    /// Забывает сеть и пути: мир собран заново.
    pub fn clear(&mut self) {
        self.network.borrow_mut().clear();
        self.paths.clear();
    }

    /// Убирает пути машин, которых нет среди `keep`.
    pub fn prune(&mut self, keep: &HashSet<u32>) {
        self.paths.retain(|id, _| keep.contains(id));
    }

    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    fn plan(&self, driver: &Driver) -> CarPath {
        let center = [
            driver.position[0] + driver.size[0] / 2.0,
            driver.position[1] + driver.size[1] / 2.0,
        ];
        let network = self.network.borrow();
        let route = network.route(center, driver.target, driver.speed, Some(driver.z));
        CarPath {
            target: driver.target,
            position: driver.position,
            version: network.version(),
            side: driver.side,
            path: Path::new(center, driver.target, route.as_ref(), driver.side),
            cursor: Cursor::default(),
        }
    }

    /// Один шаг машины `id`: за `seconds` по пути, найденному заново от того места, где она стоит,
    /// если цель, сеть, сторона или место машины не те, с которыми путь был найден. Приехавшей путь
    /// не запоминается.
    pub fn drive(&mut self, id: u32, driver: &Driver, seconds: f64) -> Moved {
        let version = self.network.borrow().version();
        let mut car = match self.paths.remove(&id) {
            Some(known)
                if known.target == driver.target
                    && known.position == driver.position
                    && known.version == version
                    && known.side == driver.side =>
            {
                known
            }
            _ => self.plan(driver),
        };
        let arrived = car.cursor.advance(&car.path, driver.speed, seconds);
        let center = car.cursor.position(&car.path);
        car.position = [
            center[0] - driver.size[0] / 2.0,
            center[1] - driver.size[1] / 2.0,
        ];
        let position = car.position;
        if !arrived {
            self.paths.insert(id, car);
        }
        Moved { position, arrived }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f64 = 1.0 / 60.0;

    fn shape(id: u32, position: Vec2, size: Vec2) -> RoadShape {
        RoadShape {
            id,
            position,
            size,
            sin_cos: (0.0, 1.0),
            z: 0.0,
        }
    }

    fn crossing() -> Vec<RoadShape> {
        vec![
            shape(0, [0.0, 9.5], [20.0, 1.0]),
            shape(1, [9.5, 0.0], [1.0, 20.0]),
        ]
    }

    fn network(shapes: &[RoadShape], speeds: &[Option<f64>]) -> RoadNetwork {
        let mut network = RoadNetwork::default();
        network.sync(shapes, speeds);
        network
    }

    fn near(actual: f64, expected: f64, eps: f64) {
        assert!(
            (actual - expected).abs() <= eps,
            "ожидалось {expected} ± {eps}, вышло {actual}"
        );
    }

    fn near_point(actual: Vec2, expected: Vec2) {
        near(actual[0], expected[0], 1e-9);
        near(actual[1], expected[1], 1e-9);
    }

    fn steps_to_arrive(path: &Path, speed: f64) -> usize {
        let mut cursor = Cursor::default();
        (1..=100_000)
            .find(|_| cursor.advance(path, speed, STEP))
            .expect("машина доехала")
    }

    fn corner_path(side: Side, speeds: &[Option<f64>]) -> Path {
        let network = network(&crossing(), speeds);
        let route = network.route([2.0, 10.0], [10.0, 2.0], 4.0, None);
        Path::new([2.0, 10.0], [10.0, 2.0], route.as_ref(), side)
    }

    #[test]
    fn axes_run_along_the_long_side_and_a_square_along_its_width() {
        let long = Road::from_shape(&shape(0, [0.0, 9.5], [20.0, 1.0]));
        assert_eq!(long.axis(), [[0.0, 10.0], [20.0, 10.0]]);
        let tall = Road::from_shape(&shape(1, [9.5, 0.0], [1.0, 20.0]));
        assert_eq!(tall.axis(), [[10.0, 0.0], [10.0, 20.0]]);
        let square = Road::from_shape(&shape(2, [4.0, 4.0], [2.0, 2.0]));
        assert_eq!(square.axis(), [[4.0, 5.0], [6.0, 5.0]]);
    }

    #[test]
    fn a_rotated_road_has_a_rotated_axis() {
        let turned = RoadShape {
            sin_cos: (1.0, 0.0),
            ..shape(0, [0.0, 9.5], [20.0, 1.0])
        };
        let axis = Road::from_shape(&turned).axis();
        near_point(axis[0], [10.0, 0.0]);
        near_point(axis[1], [10.0, 20.0]);
    }

    #[test]
    fn crossing_axes_link_at_the_crossing_point() {
        let network = network(&crossing(), &[None, None]);
        let links = network.links_of(0).expect("дорога");
        assert_eq!(links.links, vec![([10.0, 10.0], 1)]);
        assert_eq!(links.neighbours.len(), 1);
        assert_eq!(links.neighbours[0].0, 1);
        assert_eq!(links.ends, vec![[0.0, 10.0], [20.0, 10.0]]);
    }

    #[test]
    fn an_end_inside_another_rectangle_links_with_a_zero_length_piece() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [20.0, 1.0]),
            shape(1, [20.0, 9.6], [8.0, 0.8]),
        ];
        let network = network(&shapes, &[None, None]);
        let links = network.links_of(1).expect("дорога");
        assert_eq!(links.links, vec![([20.0, 10.0], 0)]);
        assert_eq!(links.ends, vec![[28.0, 10.0]]);
        let route = network
            .route([2.0, 10.0], [27.0, 10.0], 4.0, None)
            .expect("путь");
        assert_eq!(route.legs.len(), 2);
        assert_eq!(route.legs[1].road, 1);
        near_point(route.legs[1].to, [27.0, 10.0]);
    }

    #[test]
    fn a_side_road_ending_beside_the_axis_links_through_a_short_piece() {
        let shapes = vec![
            shape(0, [0.0, 9.0], [20.0, 2.0]),
            shape(1, [10.0, 0.0], [1.0, 9.5]),
        ];
        let network = network(&shapes, &[None, None]);
        let links = network.links_of(1).expect("дорога");
        assert_eq!(links.links.len(), 1);
        near_point(links.links[0].0, [10.5, 9.5]);
        let route = network
            .route([2.0, 10.0], [10.5, 1.0], 4.0, None)
            .expect("путь");
        assert!(route.legs.iter().any(|leg| leg.road == 1));
        near_point(route.exit, [10.5, 1.0]);
    }

    #[test]
    fn roads_on_different_heights_are_not_linked() {
        let mut shapes = crossing();
        shapes[1].z = 1.2;
        let network = network(&shapes, &[None, None]);
        assert!(network.links_of(0).expect("дорога").links.is_empty());
        shapes[1].z = 0.4;
        let network = self::network(&shapes, &[None, None]);
        assert_eq!(network.links_of(0).expect("дорога").links.len(), 1);
    }

    #[test]
    fn a_square_on_a_junction_is_a_node_for_every_road_ending_in_it() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [9.5, 1.0]),
            shape(1, [9.5, 9.5], [1.0, 1.0]),
            shape(2, [10.5, 9.5], [9.5, 1.0]),
            shape(3, [9.5, 0.0], [1.0, 9.5]),
        ];
        let network = network(&shapes, &[None; 4]);
        let route = network
            .route([1.0, 10.0], [10.0, 1.0], 4.0, None)
            .expect("путь");
        let measure = network
            .measure([1.0, 10.0], [10.0, 1.0], 4.0)
            .expect("мера");
        near(measure.length, 18.0, 1e-9);
        near_point(route.exit, [10.0, 1.0]);
    }

    #[test]
    fn the_example_route_is_sixteen_cells_in_four_seconds() {
        let network = network(&crossing(), &[None, None]);
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 16.0, 1e-9);
        near(measure.time, 4.0, 1e-9);
    }

    #[test]
    fn a_slow_road_changes_the_time_but_not_the_length() {
        let network = network(&crossing(), &[None, Some(2.0)]);
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 16.0, 1e-9);
        near(measure.time, 6.0, 1e-9);
    }

    #[test]
    fn a_closed_road_is_out_of_the_network() {
        let network = network(&crossing(), &[None, Some(0.0)]);
        let route = network
            .route([2.0, 10.0], [10.0, 2.0], 4.0, None)
            .expect("путь");
        near_point(route.exit, [10.0, 10.0]);
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 8.0 + 8.0, 1e-9);
    }

    #[test]
    fn with_every_road_closed_the_car_goes_straight() {
        let network = network(&crossing(), &[Some(0.0), Some(0.0)]);
        assert!(network.route([2.0, 10.0], [10.0, 2.0], 4.0, None).is_none());
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, (128.0_f64).sqrt(), 1e-9);
    }

    #[test]
    fn the_faster_of_two_ways_wins_even_when_it_is_longer() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [10.0, 1.0]),
            shape(1, [10.0, 9.5], [10.0, 1.0]),
            shape(2, [0.0, 0.0], [1.0, 10.0]),
            shape(3, [0.0, 0.0], [20.0, 1.0]),
            shape(4, [19.0, 0.0], [1.0, 10.0]),
        ];
        let fast_around = network(&shapes, &[Some(1.0), Some(1.0), None, None, None]);
        let route = fast_around
            .route([0.5, 10.0], [19.5, 10.0], 5.0, None)
            .expect("путь");
        assert!(route.legs.iter().any(|leg| leg.road == 3));
        let slow_around = network(&shapes, &[None, None, Some(1.0), Some(1.0), Some(1.0)]);
        let route = slow_around
            .route([0.5, 10.0], [19.5, 10.0], 5.0, None)
            .expect("путь");
        assert!(route.legs.iter().all(|leg| leg.road <= 1));
    }

    #[test]
    fn the_exit_is_the_nearest_reachable_point_to_the_goal() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [10.0, 1.0]),
            shape(1, [12.0, 4.5], [10.0, 1.0]),
        ];
        let network = network(&shapes, &[None, None]);
        let route = network
            .route([2.0, 10.0], [12.0, 5.0], 4.0, None)
            .expect("путь");
        near_point(route.exit, [10.0, 10.0]);
    }

    #[test]
    fn the_entry_prefers_the_road_on_the_height_of_the_car() {
        let mut shapes = crossing();
        shapes[1].z = 1.2;
        let network = network(&shapes, &[None, None]);
        let low = network
            .route([10.0, 9.0], [3.0, 10.0], 4.0, Some(0.0))
            .expect("путь");
        near_point(low.entry, [10.0, 10.0]);
        let high = network
            .route([10.0, 9.0], [10.0, 3.0], 4.0, Some(1.2))
            .expect("путь");
        near_point(high.entry, [10.0, 9.0]);
    }

    #[test]
    fn the_middle_path_is_fifteen_point_seven_nine_cells_and_237_steps() {
        let path = corner_path(Side::Middle, &[None, None]);
        near(path.length(), 15.0 + std::f64::consts::FRAC_PI_4, 1e-9);
        near(path.length(), 15.79, 0.01);
        near(steps_to_arrive(&path, 4.0) as f64, 237.0, 3.0);
    }

    #[test]
    fn a_slow_second_road_slows_the_arc_and_the_rest_of_the_path() {
        let path = corner_path(Side::Middle, &[None, Some(2.0)]);
        near(steps_to_arrive(&path, 4.0) as f64, 361.0, 4.0);
    }

    #[test]
    fn the_right_side_runs_a_quarter_width_from_the_axis_with_a_wide_left_turn() {
        let path = corner_path(Side::Right, &[None, None]);
        let arcs: Vec<Primitive> = path
            .primitives
            .iter()
            .copied()
            .filter(|primitive| matches!(primitive, Primitive::Arc { .. }))
            .collect();
        let corner = arcs
            .iter()
            .find(|arc| matches!(arc, Primitive::Arc { radius, .. } if (radius - 0.5).abs() < 1e-9))
            .expect("дуга на перекрёстке");
        let Primitive::Arc { start, .. } = corner else {
            panic!("дуга");
        };
        near_point(*start, [9.75, 10.25]);
        near_point(corner.point_at(corner.length()), [10.25, 9.75]);
        let mut cursor = Cursor::default();
        let mut straight_part = f64::NEG_INFINITY;
        while !cursor.advance(&path, 4.0, STEP) {
            let at = cursor.position(&path);
            if (3.0..9.0).contains(&at[0]) {
                straight_part = straight_part.max((at[1] - 10.25).abs());
            }
        }
        near(straight_part, 0.0, 1e-9);
    }

    #[test]
    fn a_right_turn_passes_nearer_to_the_near_corner_than_a_left_turn() {
        let network = network(&crossing(), &[None, None]);
        let nearest_to_corner = |goal: Vec2| {
            let route = network.route([2.0, 10.0], goal, 4.0, None);
            let path = Path::new([2.0, 10.0], goal, route.as_ref(), Side::Right);
            let mut cursor = Cursor::default();
            let mut closest = f64::INFINITY;
            while !cursor.advance(&path, 4.0, STEP) {
                closest = closest.min(distance(cursor.position(&path), [9.5, 10.5]));
            }
            closest
        };
        assert!(nearest_to_corner([10.0, 18.0]) < nearest_to_corner([10.0, 2.0]));
    }

    #[test]
    fn an_arc_touches_both_segments_no_farther_than_the_reach_from_the_corner() {
        let arc = fillet([10.25, 10.25], [1.0, 0.0], [0.0, -1.0], 0.5, None);
        let Primitive::Arc { start, radius, .. } = arc else {
            panic!("дуга");
        };
        near_point(start, [9.75, 10.25]);
        near(radius, 0.5, 1e-9);
        near_point(arc.point_at(arc.length()), [10.25, 9.75]);
        for step in 0..=10 {
            let point = arc.point_at(arc.length() * step as f64 / 10.0);
            assert!(distance(point, [10.25, 10.25]) <= 0.5 + 1e-9);
        }
    }

    #[test]
    fn a_uturn_goes_straight_across_from_one_side_to_the_other() {
        let first = Leg {
            from: [0.0, 10.0],
            to: [10.0, 10.0],
            road: 0,
            width: 1.0,
            cap: None,
        };
        let back = Leg {
            from: [10.0, 10.0],
            to: [0.0, 10.0],
            road: 0,
            width: 1.0,
            cap: None,
        };
        assert!(joint_point([0.0, 10.25], &first, &back, Side::Right).is_none());
        let line = polyline_of([0.0, 10.0], [0.0, 10.0], &[first, back], Side::Right);
        assert!(line.points.contains(&[10.0, 10.25]));
        assert!(line.points.contains(&[10.0, 9.75]));
    }

    #[test]
    fn a_car_far_from_the_roads_first_drives_straight_to_the_nearest_one() {
        let network = network(&crossing(), &[None, None]);
        let route = network
            .route([2.0, 14.0], [10.0, 2.0], 4.0, None)
            .expect("путь");
        near_point(route.entry, [2.0, 10.0]);
        let measure = network
            .measure([2.0, 14.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 4.0 + 16.0, 1e-9);
    }

    #[test]
    fn a_goal_far_from_the_roads_ends_with_a_straight_piece() {
        let network = network(&crossing(), &[None, None]);
        let measure = network
            .measure([2.0, 10.0], [10.0, -6.0], 4.0)
            .expect("мера");
        near(measure.length, 8.0 + 10.0 + 6.0, 1e-9);
    }

    #[test]
    fn a_goal_on_a_cut_off_network_goes_to_the_nearest_point_of_its_own_piece() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [10.0, 1.0]),
            shape(1, [30.0, 9.5], [10.0, 1.0]),
        ];
        let network = network(&shapes, &[None, None]);
        let route = network
            .route([2.0, 10.0], [29.0, 10.0], 4.0, None)
            .expect("путь");
        near_point(route.exit, [10.0, 10.0]);
    }

    #[test]
    fn driving_on_the_example_arrives_exactly_and_forgets_the_path() {
        let mut state = RoadState::new();
        state.network.borrow_mut().sync(&crossing(), &[None, None]);
        let mut driver = Driver {
            position: [1.9, 9.7],
            size: [0.2, 0.6],
            z: 0.0,
            target: [10.0, 2.0],
            speed: 4.0,
            side: Side::Middle,
        };
        let mut arrived_after = None;
        for step in 1..=400 {
            let moved = state.drive(0, &driver, STEP);
            driver.position = moved.position;
            if moved.arrived {
                arrived_after = Some(step);
                break;
            }
        }
        near(arrived_after.expect("доехала") as f64, 237.0, 3.0);
        near_point(
            [
                driver.position[0] + driver.size[0] / 2.0,
                driver.position[1] + driver.size[1] / 2.0,
            ],
            [10.0, 2.0],
        );
        assert_eq!(state.path_count(), 0);
    }

    #[test]
    fn a_moved_car_gets_a_new_path_from_where_it_stands() {
        let mut state = RoadState::new();
        state.network.borrow_mut().sync(&crossing(), &[None, None]);
        let mut driver = Driver {
            position: [1.9, 9.7],
            size: [0.2, 0.6],
            z: 0.0,
            target: [10.0, 2.0],
            speed: 4.0,
            side: Side::Middle,
        };
        state.drive(0, &driver, STEP);
        driver.position = [14.9, 9.7];
        let again = state.drive(0, &driver, STEP);
        near_point(
            [again.position[0] + 0.1, again.position[1] + 0.3],
            [15.0 - 4.0 / 60.0, 10.0],
        );
    }

    #[test]
    fn many_roads_find_their_links_through_the_grid() {
        let mut shapes = Vec::new();
        for row in 0..30 {
            shapes.push(shape(row, [-5.0, row as f64 * 3.0 + 1.0], [70.0, 1.0]));
        }
        for column in 0..20 {
            shapes.push(shape(
                30 + column,
                [column as f64 * 3.0, -5.0],
                [1.0, 100.0],
            ));
        }
        let speeds = vec![None; shapes.len()];
        let network = network(&shapes, &speeds);
        assert_eq!(network.links_of(0).expect("дорога").links.len(), 20);
        assert_eq!(network.links_of(30).expect("дорога").links.len(), 30);
    }
}

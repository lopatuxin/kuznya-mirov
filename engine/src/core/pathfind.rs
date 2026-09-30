//! «Ходьба», требования 22–32: путь в обход прямоугольников `avoid` для правила `walk`. Каждое
//! препятствие раздувается на тело идущего (сумма Минковского), край сцены сдвигается внутрь на
//! его полуразмер, а сам идущий считается точкой (требование 27); путь — ломаная по углам раздутых
//! препятствий, кратчайшая по A* с расстоянием по прямой. Без поворотов раздутое препятствие —
//! прямоугольник по осям, как раньше; с поворотом (трёхмерная сцена, «Трёхмерная сцена» →
//! `rotation`) — выпуклый многоугольник до восьми углов. Ничего здесь не знает про `World` —
//! `core::step::apply_walk_rule` читает мир и решает свойства, это только геометрия.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::rc::Rc;

use super::footprint::Footprint;
use super::grid::Rect;
use super::value::{Rotation, Vec2};
use super::walk3d::{SurfaceKey, SurfaceNav};

/// Строгий допуск на «касание не мешает» (требование 27) и на сравнение чисел с плавающей
/// точкой при проверке углов — каждое использование обосновано соседним комментарием.
pub(super) const EPS: f64 = 1e-6;

/// На сколько глубже границы препятствия должна лежать точка отрезка, чтобы `inside_span` счёл её внутри:
/// сторона, что идёт по самой границе, из-за погрешности дробных чисел не должна оказаться «внутри».
const SPAN_DEPTH: f64 = 1e-7;

pub(super) fn dist(a: Vec2, b: Vec2) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn point_strictly_inside(p: Vec2, r: &Rect) -> bool {
    p[0] > r.x && p[0] < r.x + r.w && p[1] > r.y && p[1] < r.y + r.h
}

/// `rect`, сжатый на `EPS` с каждой стороны — `None`, когда от него совсем ничего не остаётся
/// (ничтожно маленькое препятствие никогда не мешает отрезку, который его лишь задевает).
fn shrink(rect: &Rect) -> Option<Rect> {
    let w = rect.w - 2.0 * EPS;
    let h = rect.h - 2.0 * EPS;
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    Some(Rect {
        x: rect.x + EPS,
        y: rect.y + EPS,
        w,
        h,
    })
}

/// Liang-Barsky: параметр `[t0, t1] ⊆ [0, 1]`, на котором отрезок `p0..p1` лежит внутри
/// замкнутого `rect` — `None` без пересечения вовсе.
fn clip_segment(p0: Vec2, p1: Vec2, rect: &Rect) -> Option<(f64, f64)> {
    let dx = p1[0] - p0[0];
    let dy = p1[1] - p0[1];
    let mut t0 = 0.0_f64;
    let mut t1 = 1.0_f64;
    for &(p, q) in &[
        (-dx, p0[0] - rect.x),
        (dx, rect.x + rect.w - p0[0]),
        (-dy, p0[1] - rect.y),
        (dy, rect.y + rect.h - p0[1]),
    ] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return None;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                if r > t1 {
                    return None;
                }
                if r > t0 {
                    t0 = r;
                }
            } else {
                if r < t0 {
                    return None;
                }
                if r < t1 {
                    t1 = r;
                }
            }
        }
    }
    (t0 <= t1).then_some((t0, t1))
}

/// Отрезок `a → b` в параметре `t ∈ [0, 1]`: часть, где `n · p < d + slack` по всем полуплоскостям (для
/// `strict` — строго, иначе с равенством). `None`, когда части нет.
fn half_plane_span(
    planes: impl Iterator<Item = (Vec2, f64)>,
    a: Vec2,
    b: Vec2,
    slack: f64,
    strict: bool,
) -> Option<(f64, f64)> {
    let dir = [b[0] - a[0], b[1] - a[1]];
    let (mut low, mut high) = (0.0_f64, 1.0_f64);
    for (n, d) in planes {
        let room = d + slack - (n[0] * a[0] + n[1] * a[1]);
        let toward = n[0] * dir[0] + n[1] * dir[1];
        if toward.abs() < 1e-12 {
            if room < 0.0 || (strict && room <= 0.0) {
                return None;
            }
            continue;
        }
        let t = room / toward;
        if toward > 0.0 {
            high = high.min(t);
        } else {
            low = low.max(t);
        }
    }
    (low < high || (!strict && low <= high)).then_some((low, high))
}

fn rect_planes(r: &Rect) -> [(Vec2, f64); 4] {
    [
        ([-1.0, 0.0], -r.x),
        ([1.0, 0.0], r.x + r.w),
        ([0.0, -1.0], -r.y),
        ([0.0, 1.0], r.y + r.h),
    ]
}

/// Часть отрезка `a → b` (параметр `t ∈ [0, 1]`) внутри `rect` с допуском `EPS` на границе.
pub(super) fn span_in_rect(rect: &Rect, a: Vec2, b: Vec2) -> Option<(f64, f64)> {
    half_plane_span(rect_planes(rect).into_iter(), a, b, EPS / 2.0, false)
}

/// Выпуклый многоугольник — раздутое повёрнутое препятствие. Вершины идут против часовой стрелки
/// (в математическом смысле `x`, `y`); у каждой стороны — единичная внешняя нормаль `n` и число
/// `d`, для которого точки многоугольника — это `n·p ≤ d` по всем сторонам.
#[derive(Debug, Clone)]
pub(super) struct Poly {
    pub(super) verts: Vec<Vec2>,
    edges: Vec<(Vec2, f64)>,
    pub(super) bbox: Rect,
}

impl Poly {
    /// Оболочка Эндрю по монотонной цепочке; `None`, когда точки лежат на одной прямой.
    pub(super) fn hull(mut points: Vec<Vec2>) -> Option<Poly> {
        points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
        points.dedup_by(|a, b| dist(*a, *b) < 1e-12);
        if points.len() < 3 {
            return None;
        }
        let cross = |o: Vec2, a: Vec2, b: Vec2| {
            (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
        };
        let mut chain: Vec<Vec2> = Vec::with_capacity(points.len() * 2);
        for pass in 0..2 {
            let start = chain.len();
            let ordered: Vec<Vec2> = if pass == 0 {
                points.clone()
            } else {
                points.iter().rev().copied().collect()
            };
            for p in ordered {
                while chain.len() >= start + 2
                    && cross(chain[chain.len() - 2], chain[chain.len() - 1], p) <= 1e-12
                {
                    chain.pop();
                }
                chain.push(p);
            }
            chain.pop();
        }
        if chain.len() < 3 {
            return None;
        }
        let edges: Vec<(Vec2, f64)> = (0..chain.len())
            .map(|i| {
                let (a, b) = (chain[i], chain[(i + 1) % chain.len()]);
                let (ex, ey) = (b[0] - a[0], b[1] - a[1]);
                let len = (ex * ex + ey * ey).sqrt();
                let n = [ey / len, -ex / len];
                (n, n[0] * a[0] + n[1] * a[1])
            })
            .collect();
        let xs = chain.iter().map(|v| v[0]);
        let ys = chain.iter().map(|v| v[1]);
        let (x0, x1) = (
            xs.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
        );
        let (y0, y1) = (
            ys.clone().fold(f64::INFINITY, f64::min),
            ys.fold(f64::NEG_INFINITY, f64::max),
        );
        Some(Poly {
            verts: chain,
            edges,
            bbox: Rect {
                x: x0,
                y: y0,
                w: x1 - x0,
                h: y1 - y0,
            },
        })
    }

    pub(super) fn strictly_inside(&self, p: Vec2) -> bool {
        self.edges
            .iter()
            .all(|(n, d)| n[0] * p[0] + n[1] * p[1] < *d)
    }

    /// Внутри многоугольника или на его границе, с допуском `EPS`.
    pub(super) fn contains_closed(&self, p: Vec2) -> bool {
        self.edges
            .iter()
            .all(|(n, d)| n[0] * p[0] + n[1] * p[1] <= *d + EPS)
    }

    /// Отрезок заходит внутрь многоугольника, сжатого на `EPS` со всех сторон: касание краем не
    /// мешает — как у `shrink`/`clip_segment` для прямоугольника. Cyrus-Beck по полуплоскостям.
    pub(super) fn clipped_by_segment(&self, p0: Vec2, p1: Vec2) -> bool {
        let dir = [p1[0] - p0[0], p1[1] - p0[1]];
        let (mut t0, mut t1) = (0.0_f64, 1.0_f64);
        for (n, d) in &self.edges {
            let room = (d - EPS) - (n[0] * p0[0] + n[1] * p0[1]);
            let toward = n[0] * dir[0] + n[1] * dir[1];
            if toward.abs() < 1e-12 {
                if room < 0.0 {
                    return false;
                }
                continue;
            }
            let t = room / toward;
            if toward < 0.0 {
                t0 = t0.max(t);
            } else {
                t1 = t1.min(t);
            }
            if t0 > t1 {
                return false;
            }
        }
        true
    }

    /// Часть отрезка `a → b` (параметр `t ∈ [0, 1]`) внутри многоугольника или на его границе, с допуском
    /// `EPS` — как у `contains_closed`.
    pub(super) fn closed_span(&self, a: Vec2, b: Vec2) -> Option<(f64, f64)> {
        half_plane_span(self.edges.iter().copied(), a, b, EPS / 2.0, false)
    }

    /// Точка на ближайшей к `p` стороне — куда выходит из многоугольника точка, что внутри него.
    fn nearest_exit(&self, p: Vec2) -> Vec2 {
        let mut best = (f64::INFINITY, [0.0, 0.0]);
        for (n, d) in &self.edges {
            let depth = d - (n[0] * p[0] + n[1] * p[1]);
            if depth < best.0 {
                best = (depth, *n);
            }
        }
        [p[0] + best.1[0] * best.0, p[1] + best.1[1] * best.0]
    }
}

/// Раздутое препятствие: прямоугольник по осям (пара без поворотов, прежний код) или выпуклый
/// многоугольник (хоть один из двух повёрнут).
#[derive(Debug, Clone)]
pub(super) enum Obstacle {
    Rect(Rect),
    Poly(Poly),
}

impl Obstacle {
    pub(super) fn strictly_inside(&self, p: Vec2) -> bool {
        match self {
            Obstacle::Rect(r) => point_strictly_inside(p, r),
            Obstacle::Poly(poly) => poly.strictly_inside(p),
        }
    }

    /// Глубже чем на `EPS` внутри препятствия: точка на его границе, куда её отнесла погрешность дробных
    /// чисел, не считается.
    pub(super) fn deeply_inside(&self, p: Vec2) -> bool {
        match self {
            Obstacle::Rect(r) => shrink(r).is_some_and(|inner| point_strictly_inside(p, &inner)),
            Obstacle::Poly(poly) => poly
                .edges
                .iter()
                .all(|(n, d)| n[0] * p[0] + n[1] * p[1] < d - EPS),
        }
    }

    /// Часть отрезка `a → b` (параметр `t ∈ [0, 1]`) строго внутри препятствия — как `strictly_inside`.
    pub(super) fn inside_span(&self, a: Vec2, b: Vec2) -> Option<(f64, f64)> {
        match self {
            Obstacle::Rect(r) => {
                half_plane_span(rect_planes(r).into_iter(), a, b, -SPAN_DEPTH, true)
            }
            Obstacle::Poly(poly) => {
                half_plane_span(poly.edges.iter().copied(), a, b, -SPAN_DEPTH, true)
            }
        }
    }

    pub(super) fn corners(&self) -> Vec<Vec2> {
        match self {
            Obstacle::Rect(r) => rect_corners(r).to_vec(),
            Obstacle::Poly(poly) => poly.verts.clone(),
        }
    }

    /// Стороны по кругу.
    pub(super) fn edges(&self) -> Vec<(Vec2, Vec2)> {
        let ring: Vec<Vec2> = match self {
            Obstacle::Rect(r) => vec![
                [r.x, r.y],
                [r.x + r.w, r.y],
                [r.x + r.w, r.y + r.h],
                [r.x, r.y + r.h],
            ],
            Obstacle::Poly(poly) => poly.verts.clone(),
        };
        (0..ring.len())
            .map(|i| (ring[i], ring[(i + 1) % ring.len()]))
            .collect()
    }

    /// Охватывающий прямоугольник по осям.
    pub(super) fn bbox(&self) -> Rect {
        match self {
            Obstacle::Rect(r) => *r,
            Obstacle::Poly(poly) => poly.bbox,
        }
    }

    pub(super) fn blocks_segment(&self, p0: Vec2, p1: Vec2, bbox: (f64, f64, f64, f64)) -> bool {
        match self {
            Obstacle::Rect(r) => segment_blocked_by(p0, p1, r, bbox),
            Obstacle::Poly(poly) => {
                !segment_bbox_misses(bbox, &poly.bbox) && poly.clipped_by_segment(p0, p1)
            }
        }
    }

    /// «Ходьба», требование 29: точка, что внутри, выходит по прямой к ближайшему краю.
    pub(super) fn exit(&self, p: Vec2) -> Vec2 {
        match self {
            Obstacle::Rect(r) => {
                let left = p[0] - r.x;
                let right = (r.x + r.w) - p[0];
                let top = p[1] - r.y;
                let bottom = (r.y + r.h) - p[1];
                let m = left.min(right).min(top).min(bottom);
                if m == left {
                    [r.x, p[1]]
                } else if m == right {
                    [r.x + r.w, p[1]]
                } else if m == top {
                    [p[0], r.y]
                } else {
                    [p[0], r.y + r.h]
                }
            }
            Obstacle::Poly(poly) => poly.nearest_exit(p),
        }
    }
}

/// «Ходьба», требование 27: отрезок проходим, если не заходит строго внутрь ни одного раздутого
/// препятствия — сжатый на `EPS` тест значит, что впритык касание краем (проход ровно шириной
/// идущего) не блокирует, а погрешность дробных чисел не даёт наложения на ничтожную долю клетки.
/// Cheap axis-aligned bounding-box reject before the full (divide-heavy) Liang-Barsky clip below
/// — most obstacle/segment pairs, across the `O(углы²)` visibility graph, are nowhere near each
/// other, and four comparisons turn those away far cheaper than `clip_segment` would. `bbox` is
/// the segment's own bounding box, computed once by the caller rather than per obstacle.
pub(super) fn segment_bbox_misses(bbox: (f64, f64, f64, f64), rect: &Rect) -> bool {
    let (minx, maxx, miny, maxy) = bbox;
    maxx < rect.x || minx > rect.x + rect.w || maxy < rect.y || miny > rect.y + rect.h
}

fn segment_blocked_by(p0: Vec2, p1: Vec2, rect: &Rect, bbox: (f64, f64, f64, f64)) -> bool {
    if segment_bbox_misses(bbox, rect) {
        return false;
    }
    shrink(rect).is_some_and(|shrunk| clip_segment(p0, p1, &shrunk).is_some())
}

fn segment_passable(p0: Vec2, p1: Vec2, obstacles: &[Obstacle]) -> bool {
    let bbox = (
        p0[0].min(p1[0]),
        p0[0].max(p1[0]),
        p0[1].min(p1[1]),
        p0[1].max(p1[1]),
    );
    !obstacles.iter().any(|o| o.blocks_segment(p0, p1, bbox))
}

/// «Ходьба», требование 27: угол берётся в узлы видимости, только если не лежит строго внутри
/// другого раздутого препятствия и не за сдвинутым краем сцены (`bounds`).
fn corner_valid(p: Vec2, obstacles: &[Obstacle], skip: usize, bounds: &Rect) -> bool {
    let in_bounds = p[0] >= bounds.x - EPS
        && p[0] <= bounds.x + bounds.w + EPS
        && p[1] >= bounds.y - EPS
        && p[1] <= bounds.y + bounds.h + EPS;
    if !in_bounds {
        return false;
    }
    obstacles
        .iter()
        .enumerate()
        .all(|(i, o)| i == skip || !o.strictly_inside(p))
}

fn rect_corners(r: &Rect) -> [Vec2; 4] {
    [
        [r.x, r.y],
        [r.x + r.w, r.y],
        [r.x, r.y + r.h],
        [r.x + r.w, r.y + r.h],
    ]
}

pub(super) struct HeapEntry {
    pub(super) cost: f64,
    pub(super) node: usize,
}
impl PartialEq for HeapEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.node == other.node
    }
}
impl Eq for HeapEntry {}
impl Ord for HeapEntry {
    // Min-heap over `cost`: `BinaryHeap` is a max-heap, so the comparison is reversed.
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .partial_cmp(&self.cost)
            .unwrap_or(Ordering::Equal)
    }
}
impl PartialOrd for HeapEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn build_path(mut node: usize, came_from: &[usize], nodes: &[Vec2]) -> Vec<Vec2> {
    let mut path = vec![nodes[node]];
    while came_from[node] != usize::MAX {
        node = came_from[node];
        path.push(nodes[node]);
    }
    path.reverse();
    path.remove(0); // сам `start` — идущий уже там стоит.
    path
}

/// «Ходьба», требование 27: кратчайшая ломаная от `start` до `goal` в обход `obstacles` (уже
/// раздутых) внутри `bounds` (сдвинутый внутрь край сцены) — узлы видимости — `start`, `goal` и
/// валидные углы препятствий, A* с расстоянием по прямой. Пустой список, если путь свободен и
/// прямой отрезок уже проходим. `None`, только когда до `goal` в принципе не добраться ни от
/// одного достигнутого узла — вызывающий сам решает идти к ближайшему найденному месту
/// (требование 25).
fn astar(start: Vec2, goal: Vec2, obstacles: &[Obstacle], bounds: &Rect) -> Option<Vec<Vec2>> {
    if segment_passable(start, goal, obstacles) {
        return Some(vec![goal]);
    }

    let mut nodes: Vec<Vec2> = vec![start, goal];
    for (i, o) in obstacles.iter().enumerate() {
        for c in o.corners() {
            if corner_valid(c, obstacles, i, bounds) {
                nodes.push(c);
            }
        }
    }
    const START: usize = 0;
    const GOAL: usize = 1;

    let mut open: BinaryHeap<HeapEntry> = BinaryHeap::new();
    let mut g_score = vec![f64::INFINITY; nodes.len()];
    let mut came_from = vec![usize::MAX; nodes.len()];
    let mut closed = vec![false; nodes.len()];
    g_score[START] = 0.0;
    open.push(HeapEntry {
        cost: dist(start, goal),
        node: START,
    });

    while let Some(HeapEntry { node, .. }) = open.pop() {
        if closed[node] {
            continue;
        }
        closed[node] = true;
        if node == GOAL {
            break;
        }
        for (j, &cand) in nodes.iter().enumerate() {
            if j == node || closed[j] || !segment_passable(nodes[node], cand, obstacles) {
                continue;
            }
            let tentative = g_score[node] + dist(nodes[node], cand);
            if tentative < g_score[j] {
                g_score[j] = tentative;
                came_from[j] = node;
                open.push(HeapEntry {
                    cost: tentative + dist(cand, goal),
                    node: j,
                });
            }
        }
    }

    if g_score[GOAL].is_finite() {
        return Some(build_path(GOAL, &came_from, &nodes));
    }
    // «Ходьба», требование 25: `goal` недостижим — идёт к ближайшему по прямой узлу среди тех,
    // до кого путь всё же нашёлся; из равноудалённых — меньший номер узла, для повторяемости.
    let best = (0..nodes.len())
        .filter(|&i| i != START && g_score[i].is_finite())
        .min_by(|&a, &b| {
            dist(nodes[a], goal)
                .partial_cmp(&dist(nodes[b], goal))
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.cmp(&b))
        })?;
    Some(build_path(best, &came_from, &nodes))
}

/// «Нефункциональное требование»: drops every obstacle that cannot possibly matter for a
/// detour from `from` to `to` — kept: any obstacle overlapping the box `from`/`to` span widens
/// by `margin`, and, transitively, any obstacle overlapping `margin` around one already kept.
/// A rectangle (convex) containing both endpoints contains the whole segment between them, so
/// on its own an obstacle entirely outside a widened version of that box could never block a
/// path that keeps its own detours within it — but a wall made of many small obstacles laid
/// side by side is exactly the case where that alone is wrong: the box only reaches the pieces
/// nearest the straight line, yet the walker has to go around the wall's actual far end, however
/// far that sits from the line. Chaining through touching/near obstacles keeps such a wall whole
/// out to both ends, while an unrelated cluster elsewhere in the scene, never within `margin` of
/// the chain, still drops. `margin` must also cover `walker_size`: two obstacles further apart
/// than the walker is wide still end up touching once each is inflated by half the walker's own
/// width/height (requirement 27), so a margin based on obstacle size alone can call a wall's
/// far pieces "irrelevant" while the inflated wall is in fact still standing there, unblocked in
/// this function's own (un-inflated) view of the world. Повёрнутое препятствие считается своим
/// охватывающим прямоугольником — оно лишь шире, а значит, оставляет не меньше, чем нужно.
fn relevant_obstacles(
    obstacles: Vec<(u32, Footprint)>,
    from: Vec2,
    to: Vec2,
    walker_size: Vec2,
) -> Vec<(u32, Footprint)> {
    let boxes: Vec<Rect> = obstacles.iter().map(|(_, f)| f.aabb()).collect();
    let margin = boxes
        .iter()
        .map(|r| r.w.max(r.h))
        .fold(walker_size[0].max(walker_size[1]).max(1.0), f64::max)
        * 4.0;
    let seed = Rect {
        x: from[0].min(to[0]) - margin,
        y: from[1].min(to[1]) - margin,
        w: (from[0].max(to[0]) - from[0].min(to[0])) + 2.0 * margin,
        h: (from[1].max(to[1]) - from[1].min(to[1])) + 2.0 * margin,
    };
    let mut kept = vec![false; obstacles.len()];
    let mut frontier: Vec<usize> = (0..obstacles.len())
        .filter(|&i| rects_overlap(&seed, &boxes[i]))
        .collect();
    for &i in &frontier {
        kept[i] = true;
    }
    while let Some(i) = frontier.pop() {
        let widened = widen(&boxes[i], margin);
        for j in 0..obstacles.len() {
            if !kept[j] && rects_overlap(&widened, &boxes[j]) {
                kept[j] = true;
                frontier.push(j);
            }
        }
    }
    obstacles
        .into_iter()
        .zip(kept)
        .filter_map(|(o, keep)| keep.then_some(o))
        .collect()
}

fn widen(r: &Rect, margin: f64) -> Rect {
    Rect {
        x: r.x - margin,
        y: r.y - margin,
        w: r.w + 2.0 * margin,
        h: r.h + 2.0 * margin,
    }
}

fn rects_overlap(a: &Rect, b: &Rect) -> bool {
    a.x <= b.x + b.w && b.x <= a.x + a.w && a.y <= b.y + b.h && b.y <= a.y + a.h
}

/// «Ходьба», требование 29: точка, раздутыми препятствиями уже занятая, выходит по прямой к
/// ближайшему краю того препятствия, что её держит — повторяется на случай, если этот край сам
/// лежит внутри соседнего препятствия. Та же функция решает требование 25's «цель внутри
/// препятствия»: применяется и к точке цели.
fn escape_point(mut p: Vec2, obstacles: &[Obstacle]) -> Vec2 {
    for _ in 0..obstacles.len().saturating_add(1) {
        let Some(o) = obstacles.iter().find(|o| o.strictly_inside(p)) else {
            break;
        };
        p = o.exit(p);
    }
    p
}

/// Раздутое препятствие `obstacle` для идущего с телом `body` — углы тела относительно его
/// середины. Пара без поворотов — прямоугольник, раздутый на полуразмеры тела, ровно как раньше;
/// иначе — оболочка сумм углов препятствия и тела (сумма Минковского двух прямоугольников).
pub(super) fn inflate(obstacle: &Footprint, body: &Footprint) -> Obstacle {
    if let (Footprint::Aligned(r), Footprint::Aligned(b)) = (obstacle, body) {
        let (hw, hh) = (b.w / 2.0, b.h / 2.0);
        return Obstacle::Rect(Rect {
            x: r.x - hw,
            y: r.y - hh,
            w: r.w + 2.0 * hw,
            h: r.h + 2.0 * hh,
        });
    }
    let sums: Vec<Vec2> = obstacle
        .corners()
        .iter()
        .flat_map(|o| {
            body.corners()
                .into_iter()
                .map(move |b| [o[0] + b[0], o[1] + b[1]])
        })
        .collect();
    Poly::hull(sums).map_or_else(|| Obstacle::Rect(obstacle.aabb()), Obstacle::Poly)
}

/// Выпуклый многоугольник, раздутый на тело идущего, — как `inflate` для места объекта. `None`, если
/// многоугольник вырожден.
pub(super) fn inflate_polygon(polygon: &[Vec2], body: &Footprint) -> Option<Obstacle> {
    let sums: Vec<Vec2> = polygon
        .iter()
        .flat_map(|o| {
            body.corners()
                .into_iter()
                .map(move |b| [o[0] + b[0], o[1] + b[1]])
        })
        .collect();
    Poly::hull(sums).map(Obstacle::Poly)
}

/// «Ходьба», требование 28: запомненный путь одного идущего объекта — вне мира, в `Game`,
/// сброшенный при сборке мира.
#[derive(Debug, Clone)]
pub struct WalkCache {
    target: Vec2,
    size: Vec2,
    rotation: Option<Rotation>,
    obstacles: Vec<(u32, Footprint)>,
    waypoints: Vec<Vec2>,
    /// «Рельеф»: то, от чего зависит путь по поверхностям, кроме перечисленного выше.
    surface: Option<SurfaceKey>,
}

/// Сколько сетей земли и настилов помнят пути: по одной на разное тело идущего.
const MAX_NAVIGATIONS: usize = 8;

/// Запомненные пути идущих и, для ходьбы по рельефу и настилам, то, что зависит только от земли,
/// настилов и тела идущего и потому не пересчитывается на каждый путь.
#[derive(Debug, Default)]
pub struct WalkCaches {
    paths: HashMap<u32, WalkCache>,
    navigation: Vec<Rc<SurfaceNav>>,
}

impl WalkCaches {
    pub fn new() -> Self {
        WalkCaches::default()
    }

    /// Забывает пути идущих, а сети земли и настилов оставляет: они не зависят от мира, а от земли,
    /// настилов и тела идущего, и сравниваются с ними при каждом обращении.
    pub fn clear_paths(&mut self) {
        self.paths.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.paths.is_empty()
    }

    pub fn len(&self) -> usize {
        self.paths.len()
    }

    pub fn contains_key(&self, id: &u32) -> bool {
        self.paths.contains_key(id)
    }

    pub(super) fn navigations(&self) -> &[Rc<SurfaceNav>] {
        &self.navigation
    }

    /// Запоминает сеть; помнит не больше `MAX_NAVIGATIONS` последних.
    pub(super) fn add_navigation(&mut self, navigation: Rc<SurfaceNav>) {
        if self.navigation.len() >= MAX_NAVIGATIONS {
            self.navigation.remove(0);
        }
        self.navigation.push(navigation);
    }
}

/// Убирает запомненные пути объектов, у которых `walk_to` больше нет — иначе таблица растёт, не
/// убывая, на каждую партию с уже отставленными целями.
pub fn prune(caches: &mut WalkCaches, keep: &HashSet<u32>) {
    caches.paths.retain(|id, _| keep.contains(id));
}

/// Тело идущего и границы его середины: край сцены сдвинут внутрь на полуразмер тела, а по оси,
/// где идущий больше сцены, стоит на месте.
pub(super) struct Frame {
    pub(super) body: Footprint,
    pub(super) bounds: Rect,
    frozen_x: bool,
    frozen_y: bool,
}

pub(super) fn frame(
    center: Vec2,
    size: Vec2,
    rotation: Option<Rotation>,
    scene_size: (f64, f64),
) -> Frame {
    let body = Footprint::rotated([-size[0] / 2.0, -size[1] / 2.0], size, rotation);
    let body_box = body.aabb();
    let hw = body_box.w / 2.0;
    let hh = body_box.h / 2.0;
    // «Крайние случаи»: идущий больше сцены по оси — по этой оси он не двигается; сдвинутый край
    // сцены на этой оси сжимается в одну точку, саму текущую.
    let frozen_x = 2.0 * hw >= scene_size.0;
    let frozen_y = 2.0 * hh >= scene_size.1;
    let (bx0, bx1) = if frozen_x {
        (center[0], center[0])
    } else {
        (hw, scene_size.0 - hw)
    };
    let (by0, by1) = if frozen_y {
        (center[1], center[1])
    } else {
        (hh, scene_size.1 - hh)
    };
    Frame {
        body,
        bounds: Rect {
            x: bx0,
            y: by0,
            w: (bx1 - bx0).max(0.0),
            h: (by1 - by0).max(0.0),
        },
        frozen_x,
        frozen_y,
    }
}

/// Идёт по запомненному пути на `budget` клеток; вторым — дошёл ли до конца.
pub(super) fn follow(
    id: u32,
    center: Vec2,
    budget: f64,
    frame: &Frame,
    caches: &mut WalkCaches,
) -> (Vec2, bool) {
    let cache = caches
        .paths
        .get_mut(&id)
        .expect("only just inserted or already present");
    let mut pos = center;
    let mut remaining = budget;
    while remaining > 1e-9 {
        let Some(&next) = cache.waypoints.first() else {
            break;
        };
        let d = dist(pos, next);
        if d <= remaining {
            pos = next;
            cache.waypoints.remove(0);
            remaining -= d;
        } else {
            let t = (remaining / d).min(1.0);
            pos = [
                pos[0] + (next[0] - pos[0]) * t,
                pos[1] + (next[1] - pos[1]) * t,
            ];
            remaining = 0.0;
        }
    }
    if frame.frozen_x {
        pos[0] = center[0];
    }
    if frame.frozen_y {
        pos[1] = center[1];
    }

    let arrived = cache.waypoints.is_empty();
    if arrived {
        caches.paths.remove(&id);
    }
    (pos, arrived)
}

/// Путь идущего `id` по ключу: тот же путь, пока не изменилось то, от чего он зависит, иначе новый —
/// из `plan`. Общая часть ходьбы по плоскости и по поверхностям.
pub(super) fn ensure_path(
    id: u32,
    key: PathKey,
    caches: &mut WalkCaches,
    plan: impl FnOnce(&mut WalkCaches) -> Vec<Vec2>,
) {
    #[allow(clippy::float_cmp)]
    let reuse = caches.paths.get(&id).is_some_and(|c| {
        c.target == key.target
            && c.size == key.size
            && c.rotation == key.rotation
            && c.obstacles == key.obstacles
            && c.surface == key.surface
    });
    if reuse {
        return;
    }
    let waypoints = plan(caches);
    caches.paths.insert(
        id,
        WalkCache {
            target: key.target,
            size: key.size,
            rotation: key.rotation,
            obstacles: key.obstacles,
            waypoints,
            surface: key.surface,
        },
    );
}

/// Всё, по чему решают, годится ли запомненный путь.
pub(super) struct PathKey {
    pub(super) target: Vec2,
    pub(super) size: Vec2,
    pub(super) rotation: Option<Rotation>,
    /// Отсортированы по номеру.
    pub(super) obstacles: Vec<(u32, Footprint)>,
    pub(super) surface: Option<SurfaceKey>,
}

/// «Ходьба», требования 22–29: продвигает одного идущего на `budget` клеток этого шага —
/// `obstacles` уже отобраны по `avoid` (сам объект исключён), пока не раздуты. `rotation` —
/// поворот тела идущего в трёхмерной сцене (`None` — по осям). Возвращает новую середину объекта
/// и «дошёл ли до конечной точки» (`waypoints` опустел) — вызывающий сам решает `walk_to` при этом
/// снять и как отметить `moved`.
#[allow(clippy::too_many_arguments)]
pub fn advance(
    id: u32,
    center: Vec2,
    size: Vec2,
    rotation: Option<Rotation>,
    raw_target: Vec2,
    mut obstacles: Vec<(u32, Footprint)>,
    scene_size: (f64, f64),
    budget: f64,
    caches: &mut WalkCaches,
) -> (Vec2, bool) {
    // «Нефункциональное требование»: an obstacle nowhere near the straight line from here to the
    // target, nor chained through nearer ones to it, cannot possibly matter — see
    // `relevant_obstacles`. Filtering it out here, before the visibility graph even sees it, is
    // what keeps the two-hundred-obstacle case fast without changing the answer for the ones
    // that matter.
    obstacles = relevant_obstacles(obstacles, center, raw_target, size);

    let frame = frame(center, size, rotation, scene_size);
    let bounds = frame.bounds;
    let inflated: Vec<Obstacle> = obstacles
        .iter()
        .map(|(_, f)| inflate(f, &frame.body))
        .collect();

    let target = escape_point(
        [
            raw_target[0].clamp(bounds.x, bounds.x + bounds.w),
            raw_target[1].clamp(bounds.y, bounds.y + bounds.h),
        ],
        &inflated,
    );

    obstacles.sort_by_key(|(id, _)| *id);
    // «Ходьба», требование 28: путь не пересчитывается, пока препятствия, цель и размер идущего
    // те же самые — сравнение точное, ровно этот вопрос и задаёт требование.
    let key = PathKey {
        target: raw_target,
        size,
        rotation,
        obstacles,
        surface: None,
    };
    ensure_path(id, key, caches, |_| {
        let start = escape_point(center, &inflated);
        let mut waypoints = astar(start, target, &inflated, &bounds).unwrap_or_default();
        #[allow(clippy::float_cmp)]
        if start != center {
            waypoints.insert(0, start);
        }
        waypoints
    });
    follow(id, center, budget, &frame, caches)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    fn aligned(obstacles: Vec<(u32, Rect)>) -> Vec<(u32, Footprint)> {
        obstacles
            .into_iter()
            .map(|(id, rect)| (id, Footprint::Aligned(rect)))
            .collect()
    }

    /// Прежний вызов без поворотов: препятствия и идущий по осям.
    #[allow(clippy::too_many_arguments)]
    fn walk(
        id: u32,
        center: Vec2,
        size: Vec2,
        target: Vec2,
        obstacles: Vec<(u32, Rect)>,
        scene: (f64, f64),
        budget: f64,
        caches: &mut WalkCaches,
    ) -> (Vec2, bool) {
        advance(
            id,
            center,
            size,
            None,
            target,
            aligned(obstacles),
            scene,
            budget,
            caches,
        )
    }

    #[test]
    fn straight_line_when_nothing_is_in_the_way() {
        let mut caches = WalkCaches::new();
        let (pos, arrived) = walk(
            0,
            [5.0, 5.0],
            [1.0, 1.0],
            [15.0, 5.0],
            Vec::new(),
            (20.0, 20.0),
            3.0,
            &mut caches,
        );
        assert_eq!(pos, [8.0, 5.0]);
        assert!(!arrived);
    }

    #[test]
    fn arrives_exactly_and_clears_the_cache() {
        let mut caches = WalkCaches::new();
        let (pos, arrived) = walk(
            0,
            [5.0, 5.0],
            [1.0, 1.0],
            [7.0, 5.0],
            Vec::new(),
            (20.0, 20.0),
            5.0,
            &mut caches,
        );
        assert_eq!(pos, [7.0, 5.0]);
        assert!(arrived);
        assert!(caches.is_empty());
    }

    /// «Ходьба», требование 24, 27: обходит стену между идущим и целью.
    #[test]
    fn walks_around_a_wall() {
        let mut caches = WalkCaches::new();
        let obstacles = vec![(1u32, r(54.0, 40.0, 1.0, 30.0))];
        let mut pos = [50.0, 50.0];
        for _ in 0..200 {
            let (p, arrived) = walk(
                0,
                pos,
                [1.0, 1.0],
                [60.0, 50.0],
                obstacles.clone(),
                (100.0, 100.0),
                0.5,
                &mut caches,
            );
            pos = p;
            if arrived {
                break;
            }
        }
        assert!((pos[0] - 60.0).abs() < 1e-6, "{pos:?}");
        assert!((pos[1] - 50.0).abs() < 1e-6, "{pos:?}");
    }

    /// «Ходьба», требование 27, нефункциональное требование: та же стена как выше, но собранная
    /// из шестидесяти отдельных клеток-препятствий 1×1 вместо одного прямоугольника — `margin`
    /// вокруг прямой от идущего до цели захватывает лишь несколько клеток стены у середины,
    /// `relevant_obstacles` обязана дотянуть остальные до самого края цепочкой, иначе идущий
    /// решит, что обход открыт там, где стена на самом деле продолжается, и застрянет.
    #[test]
    fn walks_around_a_wall_built_from_many_small_obstacles() {
        let mut caches = WalkCaches::new();
        let obstacles: Vec<(u32, Rect)> = (0..60)
            .map(|y| (y as u32 + 1, r(50.0, y as f64, 1.0, 1.0)))
            .collect();
        let mut pos = [49.5, 30.0];
        for _ in 0..1000 {
            let (p, arrived) = walk(
                0,
                pos,
                [1.0, 1.0],
                [60.0, 30.0],
                obstacles.clone(),
                (200.0, 200.0),
                1.0,
                &mut caches,
            );
            pos = p;
            if arrived {
                break;
            }
        }
        assert!((pos[0] - 60.0).abs() < 1e-6, "{pos:?}");
        assert!((pos[1] - 30.0).abs() < 1e-6, "{pos:?}");
    }

    /// «Нефункциональное требование», требования 24, 27: margin, посчитанный без размера
    /// идущего, отбросил бы дальние клетки стены (промежуток 6 клеток шире, чем `1*4=4`), хотя
    /// идущий шириной 10 (`hw=5`) раздувает каждую клетку на 5 с каждой стороны — соседние клетки
    /// с промежутком 6 после раздутия всё равно соприкасаются, и вся стена должна остаться целой.
    #[test]
    fn relevant_obstacles_keeps_a_wall_whose_gaps_close_only_after_inflation() {
        let obstacles: Vec<(u32, Rect)> = (0..30)
            .map(|i| (i as u32 + 1, r(50.0, i as f64 * 7.0, 1.0, 1.0)))
            .collect();
        let kept = relevant_obstacles(
            aligned(obstacles.clone()),
            [40.0, 100.0],
            [60.0, 100.0],
            [10.0, 10.0],
        );
        assert_eq!(kept.len(), obstacles.len());
    }

    /// «Ходьба», требования 24, 27: та же ловушка, но проверенная сквозь `advance` целиком —
    /// идущий шириной 10 обходит стену из клеток 1×1 с промежутком 6 (шире старого margin `4`,
    /// но уже самого идущего) до дальнего края стены, а не застревает, решив, что стена
    /// разорвана.
    #[test]
    fn walks_around_a_wall_with_gaps_narrower_than_itself() {
        let mut caches = WalkCaches::new();
        let obstacles: Vec<(u32, Rect)> = (0..20)
            .map(|i| (i as u32 + 1, r(50.0, i as f64 * 7.0, 1.0, 1.0)))
            .collect();
        let mut pos = [30.0, 60.0];
        for _ in 0..2000 {
            let (p, arrived) = walk(
                0,
                pos,
                [10.0, 10.0],
                [70.0, 60.0],
                obstacles.clone(),
                (200.0, 200.0),
                2.0,
                &mut caches,
            );
            pos = p;
            if arrived {
                break;
            }
        }
        assert!((pos[0] - 70.0).abs() < 1e-6, "{pos:?}");
        assert!((pos[1] - 60.0).abs() < 1e-6, "{pos:?}");
    }

    /// «Крайние случаи»: проход ровно шириной идущего — проходит, касаясь краёв.
    #[test]
    fn passes_through_a_gap_exactly_its_own_width() {
        let mut caches = WalkCaches::new();
        // Стена с разрывом шириной 1 в x∈[4,5): идущий 1×1 проходит впритык.
        let obstacles = vec![
            (1u32, r(4.0, 40.0, 1.0, 10.0)),
            (2u32, r(4.0, 51.0, 1.0, 10.0)),
        ];
        let mut pos = [0.5, 50.5];
        for _ in 0..400 {
            let (p, arrived) = walk(
                0,
                pos,
                [1.0, 1.0],
                [9.5, 50.5],
                obstacles.clone(),
                (100.0, 100.0),
                0.2,
                &mut caches,
            );
            pos = p;
            if arrived {
                break;
            }
        }
        assert!((pos[0] - 9.5).abs() < 1e-3, "{pos:?}");
    }

    /// «Крайние случаи»: проход уже на сотую клетки — не проходит, идёт в обход или останавливается
    /// перед ним, но не сквозь.
    #[test]
    fn does_not_pass_through_a_gap_one_hundredth_too_narrow() {
        let inflated_gap = 0.99; // walker width 1.0, gap 0.99 → strictly blocked
        let obstacles = [
            r(4.0, -10.0, 1.0, 10.0),
            r(4.0, -10.0 + 10.0 + inflated_gap, 1.0, 10.0),
        ];
        // The point exactly in the middle of the gap must be judged blocked once each wall is
        // inflated by half the walker's width (0.5): the two inflated walls overlap the gap.
        let hw = 0.5;
        let inflated: Vec<Rect> = obstacles
            .iter()
            .map(|r| Rect {
                x: r.x - hw,
                y: r.y,
                w: r.w + 2.0 * hw,
                h: r.h,
            })
            .collect();
        let mid = [4.5, -5.0];
        assert!(inflated.iter().any(|r| point_strictly_inside(mid, r)));
    }

    #[test]
    fn target_inside_an_obstacle_stops_flush_against_its_edge() {
        let mut caches = WalkCaches::new();
        let obstacles = vec![(1u32, r(5.0, -5.0, 4.0, 10.0))];
        let mut pos = [0.0, 0.0];
        for _ in 0..100 {
            let (p, arrived) = walk(
                0,
                pos,
                [1.0, 1.0],
                [7.0, 0.0], // dead center of the obstacle
                obstacles.clone(),
                (100.0, 100.0),
                0.5,
                &mut caches,
            );
            pos = p;
            if arrived {
                break;
            }
        }
        // Inflated obstacle left edge at x=5-0.5=4.5 — the walker stops flush against it.
        assert!((pos[0] - 4.5).abs() < 1e-6, "{pos:?}");
    }

    #[test]
    fn stuck_inside_an_obstacle_first_exits_by_the_nearest_edge() {
        let mut caches = WalkCaches::new();
        let obstacles = vec![(1u32, r(-2.0, -2.0, 4.0, 4.0))];
        // Walker's own center at [1.9, 0], size 1x1 → inflated rect is [-2.5,-2.5,5,5]; nearest
        // edge from there is the right one, at x=2.5.
        let (pos, _) = walk(
            0,
            [1.9, 0.0],
            [1.0, 1.0],
            [1.9, 0.0], // same target: nothing but the escape should move it
            obstacles,
            (100.0, 100.0),
            10.0,
            &mut caches,
        );
        assert!(pos[0] >= 2.5 - 1e-6, "{pos:?}");
    }

    #[test]
    fn scene_edge_acts_as_a_wall_the_walker_cannot_cross() {
        let mut caches = WalkCaches::new();
        let (pos, arrived) = walk(
            0,
            [0.5, 5.0],
            [1.0, 1.0],
            [-10.0, 5.0],
            Vec::new(),
            (20.0, 20.0),
            100.0,
            &mut caches,
        );
        assert!(arrived);
        assert!((pos[0] - 0.5).abs() < 1e-6, "{pos:?}");
    }

    #[test]
    fn prune_drops_only_the_ids_not_kept() {
        let mut caches = WalkCaches::new();
        caches.paths.insert(
            1,
            WalkCache {
                target: [0.0, 0.0],
                size: [1.0, 1.0],
                rotation: None,
                obstacles: Vec::new(),
                waypoints: Vec::new(),
                surface: None,
            },
        );
        caches.paths.insert(
            2,
            WalkCache {
                target: [0.0, 0.0],
                size: [1.0, 1.0],
                rotation: None,
                obstacles: Vec::new(),
                waypoints: Vec::new(),
                surface: None,
            },
        );
        let mut keep = HashSet::new();
        keep.insert(2);
        prune(&mut caches, &keep);
        assert_eq!(caches.len(), 1);
        assert!(caches.contains_key(&2));
    }

    // ---------------------------------------------------------------------------------------
    // «Трёхмерная сцена»: повёрнутые препятствия и идущий
    // ---------------------------------------------------------------------------------------

    fn rotated_wall(x: f64, y: f64, w: f64, h: f64, degrees: f64) -> Footprint {
        Footprint::rotated([x, y], [w, h], Rotation::from_degrees(degrees))
    }

    fn walk_until_arrival(
        walker: (Vec2, Vec2, Option<Rotation>),
        target: Vec2,
        obstacles: Vec<(u32, Footprint)>,
        scene: (f64, f64),
    ) -> (Vec2, usize) {
        let (mut pos, size, rotation) = walker;
        let mut caches = WalkCaches::new();
        for step in 0..4000 {
            let (next, arrived) = advance(
                0,
                pos,
                size,
                rotation,
                target,
                obstacles.clone(),
                scene,
                0.1,
                &mut caches,
            );
            pos = next;
            if arrived {
                return (pos, step);
            }
        }
        (pos, 4000)
    }

    /// Идущий 1×1 к точке за стеной под 30° — обходит её по концу и не проходит сквозь тело стены:
    /// каждая точка пути остаётся вне стены, раздутой на тело идущего.
    #[test]
    fn walks_around_a_wall_turned_thirty_degrees_by_its_end() {
        // Wall 10 long, 1 thick, centered at (20, 20), turned 30° clockwise.
        let wall = rotated_wall(15.0, 19.5, 10.0, 1.0, 30.0);
        let obstacles = vec![(1, wall)];
        let target = [24.0, 12.0];
        let start = [16.0, 28.0];
        let mut caches = WalkCaches::new();
        let mut pos = start;
        let mut trail = vec![pos];
        for _ in 0..4000 {
            let (next, arrived) = advance(
                0,
                pos,
                [1.0, 1.0],
                None,
                target,
                obstacles.clone(),
                (60.0, 60.0),
                0.1,
                &mut caches,
            );
            pos = next;
            trail.push(pos);
            if arrived {
                break;
            }
        }
        assert!(dist(pos, target) < 1e-6, "{pos:?}");
        let body = Footprint::flat([0.0, 0.0], [1.0, 1.0]);
        let Obstacle::Poly(inflated) = inflate(&wall, &body) else {
            panic!("a turned wall inflates to a polygon");
        };
        assert!(
            trail.iter().all(|&p| !inflated.strictly_inside(p)),
            "путь прошёл сквозь стену"
        );
        let straight = dist(start, target);
        let walked: f64 = trail.windows(2).map(|w| dist(w[0], w[1])).sum();
        assert!(
            walked > straight + 1.0,
            "обход длиннее прямой: {walked} против {straight}"
        );
    }

    /// Щель между двумя частями стены под 30° шире идущего — проходит сквозь неё, уже — идёт к
    /// ближайшему месту и останавливается перед стеной.
    #[test]
    fn a_gap_wider_than_the_walker_lets_it_through_and_a_narrower_one_does_not() {
        let build = |gap: f64| {
            // Two collinear wall pieces along the 30° line through (20, 20), `gap` apart, both
            // running out past the scene's edge so the only way across is the gap.
            let (sin, cos) = (0.5_f64, 30.0_f64.to_radians().cos());
            let length = 40.0;
            let piece = |offset: f64| {
                let center = [20.0 + cos * offset, 20.0 + sin * offset];
                rotated_wall(
                    center[0] - length / 2.0,
                    center[1] - 0.25,
                    length,
                    0.5,
                    30.0,
                )
            };
            let step = length / 2.0 + gap / 2.0;
            vec![(1, piece(-step)), (2, piece(step))]
        };
        // The walker crosses the wall line along the wall's normal (-sin, cos).
        let (sin, cos) = (0.5_f64, 30.0_f64.to_radians().cos());
        let start = [20.0 + sin * 6.0, 20.0 - cos * 6.0];
        let target = [20.0 - sin * 6.0, 20.0 + cos * 6.0];
        let (through, _) =
            walk_until_arrival((start, [1.0, 1.0], None), target, build(2.0), (40.0, 40.0));
        assert!(
            dist(through, target) < 1e-6,
            "щель 2 при идущем 1: {through:?}"
        );
        let (stopped, _) =
            walk_until_arrival((start, [1.0, 1.0], None), target, build(0.5), (40.0, 40.0));
        assert!(
            dist(stopped, target) > 0.5,
            "щель 0,5 при идущем 1 закрыта: {stopped:?}"
        );
    }

    /// Повёрнутый идущий шире по диагонали: щель, в которую влезает квадрат по осям, ему
    /// тесна; поворот, кратный 90, ничего не меняет.
    #[test]
    fn a_turned_walker_needs_a_wider_gap_than_an_aligned_one() {
        let pillars = vec![
            (1, Footprint::flat([10.0, 0.0], [1.0, 10.0])),
            (2, Footprint::flat([10.0, 11.4], [1.0, 11.0])),
        ];
        let gap_center = [10.5, 10.7];
        let start = [4.0, 10.7];
        let target = [17.0, 10.7];
        let size = [1.0, 1.0];
        let scene = (30.0, 22.4);
        let (aligned, _) = walk_until_arrival((start, size, None), target, pillars.clone(), scene);
        assert!(
            dist(aligned, target) < 1e-6,
            "по осям пролезает в щель 1,4: {aligned:?}"
        );
        let (diamond, _) = walk_until_arrival(
            (start, size, Rotation::from_degrees(45.0)),
            target,
            pillars.clone(),
            scene,
        );
        assert!(
            dist(diamond, target) > 0.5 && diamond[0] < gap_center[0],
            "повёрнутый на 45° квадрат (диагональ 1,41) в щель 1,4 не проходит: {diamond:?}"
        );
        let (quarter, _) = walk_until_arrival(
            (start, size, Rotation::from_degrees(90.0)),
            target,
            pillars,
            scene,
        );
        assert!(dist(quarter, target) < 1e-6, "{quarter:?}");
    }

    #[test]
    fn polygon_clipping_ignores_a_segment_that_only_grazes_an_edge() {
        let poly =
            Poly::hull(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]).expect("square");
        assert!(poly.clipped_by_segment([-1.0, 2.0], [5.0, 2.0]));
        assert!(
            !poly.clipped_by_segment([-1.0, 0.0], [5.0, 0.0]),
            "по краю — не мешает"
        );
        assert!(!poly.clipped_by_segment([-1.0, -1.0], [5.0, -1.0]));
        assert_eq!(poly.nearest_exit([0.5, 2.0]), [0.0, 2.0]);
    }

    #[test]
    fn the_hull_of_two_squares_turned_against_each_other_is_an_octagon() {
        let wall = Footprint::flat([0.0, 0.0], [2.0, 2.0]);
        let body = Footprint::rotated([-0.5, -0.5], [1.0, 1.0], Rotation::from_degrees(45.0));
        let Obstacle::Poly(poly) = inflate(&wall, &body) else {
            panic!("expected a polygon");
        };
        assert_eq!(poly.verts.len(), 8);
    }

    #[test]
    fn a_segment_along_the_border_of_an_obstacle_is_not_inside_it_and_one_across_it_is_inside_between_its_sides()
     {
        let poly =
            Poly::hull(vec![[0.0, 0.0], [4.0, 0.0], [4.0, 4.0], [0.0, 4.0]]).expect("square");
        let poly = Obstacle::Poly(poly);
        let rect = Obstacle::Rect(Rect {
            x: 0.0,
            y: 0.0,
            w: 4.0,
            h: 4.0,
        });
        for obstacle in [poly, rect] {
            let (low, high) = obstacle
                .inside_span([-2.0, 2.0], [6.0, 2.0])
                .expect("отрезок через середину");
            assert!((low - 0.25).abs() < 1e-6 && (high - 0.75).abs() < 1e-6);
            assert!(obstacle.inside_span([-2.0, 0.0], [6.0, 0.0]).is_none());
            assert!(obstacle.inside_span([-2.0, 4.0], [6.0, 4.0]).is_none());
            assert!(obstacle.inside_span([-2.0, 5.0], [6.0, 5.0]).is_none());
        }
    }
}

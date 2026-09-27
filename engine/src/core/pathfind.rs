//! «Ходьба», требования 22–32: путь в обход прямоугольников `avoid` для правила `walk`. Каждое
//! препятствие раздувается на половину ширины и высоты идущего, край сцены сдвигается внутрь на
//! столько же, а сам идущий считается точкой (требование 27); путь — ломаная по углам раздутых
//! препятствий, кратчайшая по A* с расстоянием по прямой. Ничего здесь не знает про `World` —
//! `core::step::apply_walk_rule` читает мир и решает свойства, это только геометрия.

use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};

use super::grid::Rect;
use super::value::Vec2;

/// Строгий допуск на «касание не мешает» (требование 27) и на сравнение чисел с плавающей
/// точкой при проверке углов — каждое использование обосновано соседним комментарием.
const EPS: f64 = 1e-6;

fn dist(a: Vec2, b: Vec2) -> f64 {
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

/// «Ходьба», требование 27: отрезок проходим, если не заходит строго внутрь ни одного раздутого
/// препятствия — сжатый на `EPS` тест значит, что впритык касание краем (проход ровно шириной
/// идущего) не блокирует, а погрешность дробных чисел не даёт наложения на ничтожную долю клетки.
/// Cheap axis-aligned bounding-box reject before the full (divide-heavy) Liang-Barsky clip below
/// — most obstacle/segment pairs, across the `O(углы²)` visibility graph, are nowhere near each
/// other, and four comparisons turn those away far cheaper than `clip_segment` would. `bbox` is
/// the segment's own bounding box, computed once by the caller rather than per obstacle.
fn segment_bbox_misses(bbox: (f64, f64, f64, f64), rect: &Rect) -> bool {
    let (minx, maxx, miny, maxy) = bbox;
    maxx < rect.x || minx > rect.x + rect.w || maxy < rect.y || miny > rect.y + rect.h
}

fn segment_blocked_by(p0: Vec2, p1: Vec2, rect: &Rect, bbox: (f64, f64, f64, f64)) -> bool {
    if segment_bbox_misses(bbox, rect) {
        return false;
    }
    shrink(rect).is_some_and(|shrunk| clip_segment(p0, p1, &shrunk).is_some())
}

fn segment_passable(p0: Vec2, p1: Vec2, obstacles: &[Rect]) -> bool {
    let bbox = (
        p0[0].min(p1[0]),
        p0[0].max(p1[0]),
        p0[1].min(p1[1]),
        p0[1].max(p1[1]),
    );
    !obstacles
        .iter()
        .any(|r| segment_blocked_by(p0, p1, r, bbox))
}

/// «Ходьба», требование 27: угол берётся в узлы видимости, только если не лежит строго внутри
/// другого раздутого препятствия и не за сдвинутым краем сцены (`bounds`).
fn corner_valid(p: Vec2, obstacles: &[Rect], skip: usize, bounds: &Rect) -> bool {
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
        .all(|(i, r)| i == skip || !point_strictly_inside(p, r))
}

fn rect_corners(r: &Rect) -> [Vec2; 4] {
    [
        [r.x, r.y],
        [r.x + r.w, r.y],
        [r.x, r.y + r.h],
        [r.x + r.w, r.y + r.h],
    ]
}

struct HeapEntry {
    cost: f64,
    node: usize,
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
fn astar(start: Vec2, goal: Vec2, obstacles: &[Rect], bounds: &Rect) -> Option<Vec<Vec2>> {
    if segment_passable(start, goal, obstacles) {
        return Some(vec![goal]);
    }

    let mut nodes: Vec<Vec2> = vec![start, goal];
    for (i, r) in obstacles.iter().enumerate() {
        for c in rect_corners(r) {
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
/// this function's own (un-inflated) view of the world.
fn relevant_obstacles(
    obstacles: Vec<(u32, Rect)>,
    from: Vec2,
    to: Vec2,
    walker_size: Vec2,
) -> Vec<(u32, Rect)> {
    let margin = obstacles
        .iter()
        .map(|(_, r)| r.w.max(r.h))
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
        .filter(|&i| rects_overlap(&seed, &obstacles[i].1))
        .collect();
    for &i in &frontier {
        kept[i] = true;
    }
    while let Some(i) = frontier.pop() {
        let widened = widen(&obstacles[i].1, margin);
        for j in 0..obstacles.len() {
            if !kept[j] && rects_overlap(&widened, &obstacles[j].1) {
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
fn escape_point(mut p: Vec2, obstacles: &[Rect]) -> Vec2 {
    for _ in 0..obstacles.len().saturating_add(1) {
        let Some(r) = obstacles.iter().find(|r| point_strictly_inside(p, r)) else {
            break;
        };
        let left = p[0] - r.x;
        let right = (r.x + r.w) - p[0];
        let top = p[1] - r.y;
        let bottom = (r.y + r.h) - p[1];
        let m = left.min(right).min(top).min(bottom);
        p = if m == left {
            [r.x, p[1]]
        } else if m == right {
            [r.x + r.w, p[1]]
        } else if m == top {
            [p[0], r.y]
        } else {
            [p[0], r.y + r.h]
        };
    }
    p
}

/// «Ходьба», требование 28: запомненный путь одного идущего объекта — вне мира, в `Game`,
/// сброшенный при сборке мира.
#[derive(Debug, Clone)]
pub struct WalkCache {
    target: Vec2,
    size: Vec2,
    obstacles: Vec<(u32, Rect)>,
    waypoints: Vec<Vec2>,
}

pub type WalkCaches = HashMap<u32, WalkCache>;

/// Убирает запомненные пути объектов, у которых `walk_to` больше нет — иначе таблица растёт, не
/// убывая, на каждую партию с уже отставленными целями.
pub fn prune(caches: &mut WalkCaches, keep: &HashSet<u32>) {
    caches.retain(|id, _| keep.contains(id));
}

/// «Ходьба», требования 22–29: продвигает одного идущего на `budget` клеток этого шага —
/// `obstacles` уже отобраны по `avoid` (сам объект исключён), пока не раздуты. Возвращает новую
/// середину объекта и «дошёл ли до конечной точки» (`waypoints` опустел) — вызывающий сам решает
/// `walk_to` при этом снять и как отметить `moved`.
#[allow(clippy::too_many_arguments)]
pub fn advance(
    id: u32,
    center: Vec2,
    size: Vec2,
    raw_target: Vec2,
    mut obstacles: Vec<(u32, Rect)>,
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

    let hw = size[0] / 2.0;
    let hh = size[1] / 2.0;
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
    let bounds = Rect {
        x: bx0,
        y: by0,
        w: (bx1 - bx0).max(0.0),
        h: (by1 - by0).max(0.0),
    };

    let inflated: Vec<Rect> = obstacles
        .iter()
        .map(|(_, r)| Rect {
            x: r.x - hw,
            y: r.y - hh,
            w: r.w + 2.0 * hw,
            h: r.h + 2.0 * hh,
        })
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
    #[allow(clippy::float_cmp)]
    let reuse = caches
        .get(&id)
        .is_some_and(|c| c.target == raw_target && c.size == size && c.obstacles == obstacles);
    if !reuse {
        let start = escape_point(center, &inflated);
        let mut waypoints = astar(start, target, &inflated, &bounds).unwrap_or_default();
        #[allow(clippy::float_cmp)]
        if start != center {
            waypoints.insert(0, start);
        }
        caches.insert(
            id,
            WalkCache {
                target: raw_target,
                size,
                obstacles,
                waypoints,
            },
        );
    }

    let cache = caches
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
    if frozen_x {
        pos[0] = center[0];
    }
    if frozen_y {
        pos[1] = center[1];
    }

    let arrived = cache.waypoints.is_empty();
    if arrived {
        caches.remove(&id);
    }
    (pos, arrived)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(x: f64, y: f64, w: f64, h: f64) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn straight_line_when_nothing_is_in_the_way() {
        let mut caches = WalkCaches::new();
        let (pos, arrived) = advance(
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
        let (pos, arrived) = advance(
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
            let (p, arrived) = advance(
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
            let (p, arrived) = advance(
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
            obstacles.clone(),
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
            let (p, arrived) = advance(
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
            let (p, arrived) = advance(
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
            let (p, arrived) = advance(
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
        let (pos, _) = advance(
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
        let (pos, arrived) = advance(
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
        caches.insert(
            1,
            WalkCache {
                target: [0.0, 0.0],
                size: [1.0, 1.0],
                obstacles: Vec::new(),
                waypoints: Vec::new(),
            },
        );
        caches.insert(
            2,
            WalkCache {
                target: [0.0, 0.0],
                size: [1.0, 1.0],
                obstacles: Vec::new(),
                waypoints: Vec::new(),
            },
        );
        let mut keep = HashSet::new();
        keep.insert(2);
        prune(&mut caches, &keep);
        assert_eq!(caches.len(), 1);
        assert!(caches.contains_key(&2));
    }
}

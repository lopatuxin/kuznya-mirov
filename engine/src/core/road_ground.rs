//! «Рельеф» → «Дороги на рельефе»: дорога без `z` стоит концами на земле и идёт одним уклоном, а землю под собой
//! движок правит сам — ядро по профилю и откос с плавным сходом к земле, как отпечатком, не трогая файл рельефа.
//! Модуль не знает о `World`: дороги ему приносят готовыми (`RoadInput`), а поправки отдаются участком
//! (`Patch`), по которому `Terrain` меняет свои точки.

use std::ops::Range;

use super::roads::{Road, RoadForm, RoadShape};
use super::terrain::Terrain;
use super::value::Vec2;

/// Запас ядра за полуширину дороги, в шагах сетки.
pub const CORE_MARGIN_STEPS: f64 = 1.0;
/// Откос не уже стольких шагов сетки.
pub const MIN_SLOPE_STEPS: f64 = 2.0;
/// Ширина откоса на единицу разницы высот профиля и земли. Smoothstep в середине круче среднего в
/// полтора раза, поэтому ширина три на единицу высоты даёт самое крутое место откоса один к двум.
pub const SLOPE_RUN_PER_RISE: f64 = 3.0;
/// Конец дороги, ближе этого к концу другой дороги, — у её конца, а не на её оси, клеток.
const HOST_EDGE: f64 = 1e-6;
/// Сколько раз охват откоса уточняется по высотам земли в нём.
const REACH_ROUNDS: usize = 4;
const EPS: f64 = 1e-9;

/// Дорога, как её передаёт мир: прямоугольник, изгиб в градусах и ровная ли она. У ровной дороги (`z` записан) высота
/// — `shape.z`; у остальных `shape.z` не читается.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadInput {
    pub shape: RoadShape,
    pub bend: f64,
    pub level: bool,
}

impl RoadInput {
    fn form(&self, start: f64, end: f64) -> RoadForm {
        RoadForm {
            shape: RoadShape {
                z: start,
                ..self.shape
            },
            bend: self.bend,
            end_z: end,
        }
    }
}

/// Дорога, на чьей оси лежит конец другой дороги: номер и расстояние по её оси.
type Host = (usize, f64);

/// Высоты концов дорог `inputs` (по возрастанию номера). Ровная дорога — на своей высоте. Остальные — концом на
/// земле без поправок дорог; а конец, что лежит в полосе другой дороги не у её конца (буква Т), берёт высоту той
/// дороги в этой точке. Если высоты так зависят друг от друга по кругу, концы круга берут высоту земли.
pub fn resolve_forms(inputs: &[RoadInput], ground: &Terrain) -> Vec<RoadForm> {
    let drafts: Vec<Road> = inputs
        .iter()
        .map(|input| Road::from_form(&input.form(input.shape.z, input.shape.z)))
        .collect();
    let hosts: Vec<[Option<Host>; 2]> = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            [0, 1].map(|end| {
                (!input.level)
                    .then(|| host_of(&drafts, index, end))
                    .flatten()
            })
        })
        .collect();
    let on_ground = |index: usize, end: usize| {
        let point = drafts[index].end(end);
        ground.ground_height_at(point[0], point[1])
    };
    let mut heights: Vec<[Option<f64>; 2]> = inputs
        .iter()
        .enumerate()
        .map(|(index, input)| {
            [0, 1].map(|end| {
                if input.level {
                    Some(input.shape.z)
                } else if hosts[index][end].is_none() {
                    Some(on_ground(index, end))
                } else {
                    None
                }
            })
        })
        .collect();
    loop {
        let mut progressed = false;
        for index in 0..inputs.len() {
            for end in 0..2 {
                let (None, Some((host, along))) = (heights[index][end], hosts[index][end]) else {
                    continue;
                };
                if let [Some(low), Some(high)] = heights[host] {
                    let share = along / drafts[host].length().max(EPS);
                    heights[index][end] = Some(low + (high - low) * share);
                    progressed = true;
                }
            }
        }
        if progressed {
            continue;
        }
        let stuck: Vec<(usize, usize)> = (0..inputs.len())
            .flat_map(|index| [(index, 0), (index, 1)])
            .filter(|&(index, end)| heights[index][end].is_none())
            .collect();
        if stuck.is_empty() {
            break;
        }
        let cyclic: Vec<(usize, usize)> = stuck
            .iter()
            .copied()
            .filter(|&node| reaches_itself(node, &hosts, &heights))
            .collect();
        for (index, end) in if cyclic.is_empty() { stuck } else { cyclic } {
            heights[index][end] = Some(on_ground(index, end));
        }
    }
    inputs
        .iter()
        .zip(&heights)
        .map(|(input, ends)| input.form(ends[0].unwrap_or_default(), ends[1].unwrap_or_default()))
        .collect()
}

/// Дорога, в чьей полосе лежит конец `end` дороги `index` не у её конца; из нескольких — ближайшая по оси.
fn host_of(drafts: &[Road], index: usize, end: usize) -> Option<Host> {
    let point = drafts[index].end(end);
    let mut best: Option<(f64, Host)> = None;
    for (other, road) in drafts.iter().enumerate() {
        if other == index || !road.contains(point) {
            continue;
        }
        let near = road.closest(point);
        if near.s <= HOST_EDGE || near.s >= road.length() - HOST_EDGE {
            continue;
        }
        if best.is_none_or(|(known, _)| near.distance < known - EPS) {
            best = Some((near.distance, (other, near.s)));
        }
    }
    best.map(|(_, host)| host)
}

/// Зависит ли высота конца `node` от самой себя через ещё не решённые концы.
fn reaches_itself(
    node: (usize, usize),
    hosts: &[[Option<Host>; 2]],
    heights: &[[Option<f64>; 2]],
) -> bool {
    let mut seen: Vec<(usize, usize)> = Vec::new();
    let mut stack = vec![node];
    while let Some((index, end)) = stack.pop() {
        let Some((host, _)) = hosts[index][end] else {
            continue;
        };
        for (next_end, known) in heights[host].iter().enumerate() {
            let next = (host, next_end);
            if next == node {
                return true;
            }
            if known.is_none() && !seen.contains(&next) {
                seen.push(next);
                stack.push(next);
            }
        }
    }
    false
}

/// Дорога без `z` и охват её поправок: прямоугольник сцены, за которым земля её не слышит.
#[derive(Debug, Clone, Copy)]
struct Reach {
    road: Road,
    rect: [Vec2; 2],
}

fn expand(rect: [Vec2; 2], by: f64) -> [Vec2; 2] {
    [
        [rect[0][0] - by, rect[0][1] - by],
        [rect[1][0] + by, rect[1][1] + by],
    ]
}

fn overlaps(a: &[Vec2; 2], b: &[Vec2; 2]) -> bool {
    a[0][0] <= b[1][0] && b[0][0] <= a[1][0] && a[0][1] <= b[1][1] && b[0][1] <= a[1][1]
}

fn smoothstep(share: f64) -> f64 {
    let t = share.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

impl Reach {
    /// Охват дороги: полоса, запас ядра и откос такой ширины, какую дают высоты земли в самом охвате.
    fn of(road: Road, ground: &Terrain) -> Reach {
        let step = 1.0 / ground.density() as f64;
        let margin = CORE_MARGIN_STEPS * step;
        let low = road.z[0].min(road.z[1]);
        let high = road.z[0].max(road.z[1]);
        let narrowest = MIN_SLOPE_STEPS * step;
        let mut run = narrowest;
        for _ in 0..REACH_ROUNDS {
            let rect = expand(road.bounds(), margin + run);
            let (ground_low, ground_high) = ground.ground_range_in(rect);
            let rise = (high - ground_low).abs().max((low - ground_high).abs());
            let wider = narrowest.max(SLOPE_RUN_PER_RISE * rise);
            if wider <= run + EPS {
                break;
            }
            run = wider;
        }
        Reach {
            road,
            rect: expand(road.bounds(), margin + run),
        }
    }
}

/// Дороги без `z`, чьи поправки надо наложить на участок точек сетки.
#[derive(Debug, Clone)]
pub struct Patch {
    /// Точки сетки участка: столбцы и строки.
    pub area: [Range<usize>; 2],
    roads: Vec<Reach>,
}

impl Patch {
    /// Высота точки сетки `(column, row)` после поправок дорог участка; `None` — ни одна дорога её не правит.
    /// `ground` — рельеф без поправок: по нему мерятся высоты концов и разница с профилем. Точку под водой этого
    /// рельефа дорога только срезает, но не насыпает: конец дороги у берега не сыплет мыс в море.
    pub fn lift(&self, ground: &Terrain, column: usize, row: usize) -> Option<f64> {
        let point = ground.point_place(column, row);
        let height = ground.point_height(column, row);
        let wet = ground
            .water_field()
            .is_some_and(|field| field.is_flooded(row * ground.columns() + column));
        let step = 1.0 / ground.density() as f64;
        let mut core: Option<(f64, f64)> = None;
        let mut slope: Option<(f64, f64)> = None;
        for reach in &self.roads {
            if point[0] < reach.rect[0][0]
                || point[0] > reach.rect[1][0]
                || point[1] < reach.rect[0][1]
                || point[1] > reach.rect[1][1]
            {
                continue;
            }
            let near = reach.road.closest(point);
            let profile = reach.road.height_at(near.s);
            let edge = reach.road.half_width + CORE_MARGIN_STEPS * step;
            if near.distance <= edge + EPS {
                if core.is_none_or(|(known, _)| near.distance < known - EPS) {
                    core = Some((near.distance, profile));
                }
                continue;
            }
            let rise = profile - height;
            let run = (MIN_SLOPE_STEPS * step).max(SLOPE_RUN_PER_RISE * rise.abs());
            let outside = near.distance - edge;
            if outside >= run {
                continue;
            }
            let share = 1.0 - smoothstep(outside / run);
            if slope.is_none_or(|(known, _)| share > known + EPS) {
                slope = Some((share, height + share * rise));
            }
        }
        let lifted = core.map(|(_, z)| z).or(slope.map(|(_, z)| z))?;
        Some(if wet { lifted.min(height) } else { lifted })
    }
}

/// Дорога мира, какой её видели в последний раз: вход, итоговая форма и охват поправок, если она без `z`.
#[derive(Debug, Clone, Copy)]
struct Settled {
    input: RoadInput,
    form: RoadForm,
    reach: Option<Reach>,
}

/// Высоты дорог мира и охваты их поправок земли — всё, что мир держит между правками дорог.
#[derive(Debug, Default)]
pub struct RoadGround {
    settled: Vec<Settled>,
}

impl RoadGround {
    /// Забывает дороги: земля под ними сброшена, и всё считается заново.
    pub fn reset(&mut self) {
        self.settled.clear();
    }

    pub fn is_empty(&self) -> bool {
        self.settled.is_empty()
    }

    /// Форма дороги `id` с высотами обоих концов; `None`, если такой дороги нет.
    pub fn form_of(&self, id: u32) -> Option<&RoadForm> {
        self.settled
            .binary_search_by_key(&id, |road| road.form.shape.id)
            .ok()
            .map(|at| &self.settled[at].form)
    }

    /// Формы дорог по возрастанию номера.
    pub fn forms(&self) -> impl Iterator<Item = &RoadForm> {
        self.settled.iter().map(|road| &road.form)
    }

    /// Приводит высоты к дорогам `inputs` (по возрастанию номера) и считает, какой участок земли надо поправить
    /// заново: объединение охватов дорог, что появились, исчезли или изменились. `None` — ничего не менялось.
    pub fn settle(&mut self, inputs: &[RoadInput], ground: &Terrain) -> Option<Patch> {
        let forms = resolve_forms(inputs, ground);
        let mut dirty: Option<[Vec2; 2]> = None;
        let mut mark = |rect: [Vec2; 2]| {
            dirty = Some(match dirty {
                None => rect,
                Some(known) => [
                    [known[0][0].min(rect[0][0]), known[0][1].min(rect[0][1])],
                    [known[1][0].max(rect[1][0]), known[1][1].max(rect[1][1])],
                ],
            });
        };
        let mut next: Vec<Settled> = Vec::with_capacity(inputs.len());
        for (input, form) in inputs.iter().zip(&forms) {
            let known = self
                .settled
                .binary_search_by_key(&input.shape.id, |road| road.form.shape.id)
                .ok()
                .map(|at| self.settled[at]);
            if let Some(same) = known.filter(|road| road.form == *form && road.input == *input) {
                next.push(same);
                continue;
            }
            let reach = (!input.level).then(|| Reach::of(Road::from_form(form), ground));
            for rect in [known.and_then(|road| road.reach), reach]
                .into_iter()
                .flatten()
                .map(|reach| reach.rect)
            {
                mark(rect);
            }
            next.push(Settled {
                input: *input,
                form: *form,
                reach,
            });
        }
        for gone in &self.settled {
            if inputs
                .binary_search_by_key(&gone.form.shape.id, |input| input.shape.id)
                .is_err()
                && let Some(reach) = gone.reach
            {
                mark(reach.rect);
            }
        }
        self.settled = next;
        let rect = dirty?;
        let area = ground.points_in(rect[0], rect[1]);
        let roads = self
            .settled
            .iter()
            .filter_map(|road| road.reach)
            .filter(|reach| overlaps(&reach.rect, &rect))
            .collect();
        Some(Patch { area, roads })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Земля `width × height` клеток с высотой `height_at(x, y)` в каждой точке сетки через полклетки.
    fn ground_of(width: u32, height: u32, height_at: impl Fn(f64, f64) -> f64) -> Terrain {
        let rows: Vec<Vec<f64>> = (0..=2 * height)
            .map(|row| {
                (0..=2 * width)
                    .map(|column| height_at(f64::from(column) / 2.0, f64::from(row) / 2.0))
                    .collect()
            })
            .collect();
        Terrain::from_rows([width, height], &rows, None).expect("размеры сходятся")
    }

    /// Холм высотой `top` над ровной землёй с вершиной в `(x, y)` и подножием в `radius`.
    fn hill(x: f64, y: f64, radius: f64, top: f64) -> impl Fn(f64, f64) -> f64 {
        move |px, py| (top * (1.0 - (px - x).hypot(py - y) / radius)).max(0.0)
    }

    fn input(id: u32, from: Vec2, to: Vec2, width: f64, bend: f64) -> RoadInput {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let chord = dx.hypot(dy);
        let middle = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
        RoadInput {
            shape: RoadShape {
                id,
                position: [middle[0] - chord / 2.0, middle[1] - width / 2.0],
                size: [chord, width],
                sin_cos: (dy / chord, dx / chord),
                z: 0.0,
            },
            bend,
            level: false,
        }
    }

    fn near(actual: f64, expected: f64) {
        assert!(
            (actual - expected).abs() < 1e-9,
            "ожидалось {expected}, вышло {actual}"
        );
    }

    /// Земля с поправками дорог `inputs`, как её собирает мир.
    fn corrected(ground: &Terrain, inputs: &[RoadInput]) -> Terrain {
        let mut state = RoadGround::default();
        let mut terrain = ground.clone();
        if let Some(patch) = state.settle(inputs, ground) {
            terrain.adjust_for_roads(patch.area.clone(), |column, row| {
                patch.lift(ground, column, row)
            });
        }
        terrain
    }

    /// Охват и участок дороги `form` над землёй `ground`.
    fn patch_of(ground: &Terrain, forms: &[RoadForm]) -> Patch {
        let roads: Vec<Reach> = forms
            .iter()
            .map(|form| Reach::of(Road::from_form(form), ground))
            .collect();
        Patch {
            area: ground.points_in([0.0, 0.0], [1e6, 1e6]),
            roads,
        }
    }

    #[test]
    fn the_ends_of_a_road_stand_on_the_ground_and_the_profile_is_one_slope() {
        let ground = ground_of(20, 10, |x, _| x * 0.1);
        let forms = resolve_forms(&[input(0, [2.0, 5.0], [12.0, 5.0], 1.0, 0.0)], &ground);
        near(forms[0].shape.z, 0.2);
        near(forms[0].end_z, 1.2);
    }

    #[test]
    fn a_level_road_keeps_its_height_and_a_bent_one_stands_by_the_ends_of_its_chord() {
        let ground = ground_of(20, 10, |x, _| x * 0.1);
        let mut level = input(0, [2.0, 5.0], [12.0, 5.0], 1.0, 0.0);
        level.level = true;
        level.shape.z = 3.0;
        let forms = resolve_forms(&[level], &ground);
        assert_eq!((forms[0].shape.z, forms[0].end_z), (3.0, 3.0));
        let curved = resolve_forms(&[input(0, [2.0, 5.0], [12.0, 5.0], 1.0, 60.0)], &ground);
        near(curved[0].shape.z, 0.2);
        near(curved[0].end_z, 1.2);
    }

    #[test]
    fn an_end_on_the_axis_of_another_road_takes_the_height_of_that_road_there() {
        let ground = ground_of(30, 12, hill(12.0, 6.0, 4.0, 2.0));
        let trunk = input(0, [2.0, 6.0], [22.0, 6.0], 1.0, 0.0);
        let street = input(1, [12.0, 6.0], [12.0, 1.0], 1.0, 0.0);
        let forms = resolve_forms(&[trunk, street], &ground);
        assert!(ground.ground_height_at(12.0, 6.0) > 1.9);
        near(forms[0].shape.z, 0.0);
        near(forms[0].end_z, 0.0);
        near(forms[1].shape.z, 0.0);
        near(forms[1].end_z, 0.0);
    }

    #[test]
    fn a_street_end_beside_the_axis_but_in_the_strip_also_leans_on_the_road() {
        let ground = ground_of(30, 12, hill(12.0, 6.0, 4.0, 2.0));
        let trunk = input(0, [2.0, 6.0], [22.0, 6.0], 2.0, 0.0);
        let street = input(1, [12.0, 6.5], [12.0, 1.0], 1.0, 0.0);
        let forms = resolve_forms(&[trunk, street], &ground);
        near(forms[1].shape.z, 0.0);
    }

    #[test]
    fn an_end_at_the_end_of_another_road_stays_on_the_ground() {
        let ground = ground_of(30, 12, hill(12.0, 6.0, 4.0, 2.0));
        let first = input(0, [2.0, 6.0], [12.0, 6.0], 1.0, 0.0);
        let second = input(1, [12.0, 6.0], [22.0, 6.0], 1.0, 0.0);
        let forms = resolve_forms(&[first, second], &ground);
        let on_hill = ground.ground_height_at(12.0, 6.0);
        near(forms[0].end_z, on_hill);
        near(forms[1].shape.z, on_hill);
    }

    #[test]
    fn ends_that_lean_on_each_other_in_a_circle_take_the_ground() {
        let ground = ground_of(30, 20, |x, y| 0.1 * x + 0.05 * y);
        let lift = |mut road: RoadInput| {
            road.shape.position[0] += 8.0;
            road.shape.position[1] += 5.0;
            road
        };
        let roads = [
            lift(input(0, [-2.0, 0.0], [8.0, 0.0], 1.0, 0.0)),
            lift(input(1, [10.0, -3.0], [4.0, 6.0], 1.0, 0.0)),
            lift(input(2, [5.0, 9.0], [2.0, 0.0], 1.0, 0.0)),
        ];
        let forms = resolve_forms(&roads, &ground);
        for (form, road) in forms.iter().zip(&roads) {
            let end = Road::from_form(&road.form(0.0, 0.0)).end(1);
            near(form.end_z, ground.ground_height_at(end[0], end[1]));
        }
    }

    #[test]
    fn a_chain_of_leaning_ends_takes_the_height_down_the_chain() {
        let ground = ground_of(30, 20, hill(14.0, 10.0, 5.0, 3.0));
        let trunk = input(0, [4.0, 10.0], [24.0, 10.0], 1.0, 0.0);
        let branch = input(1, [14.0, 10.0], [14.0, 16.0], 1.0, 0.0);
        let twig = input(2, [14.0, 14.0], [20.0, 14.0], 1.0, 0.0);
        let forms = resolve_forms(&[trunk, branch, twig], &ground);
        near(forms[1].shape.z, 0.0);
        let on_branch = forms[1].shape.z + (forms[1].end_z - forms[1].shape.z) * (4.0 / 6.0);
        near(forms[2].shape.z, on_branch);
    }

    #[test]
    fn the_core_lies_on_the_profile_and_the_slope_meets_the_ground_within_its_run() {
        let ground = ground_of(40, 24, hill(20.0, 12.0, 8.0, 2.0));
        let road = input(0, [4.0, 12.0], [36.0, 12.0], 2.0, 0.0);
        let terrain = corrected(&ground, &[road]);
        let core = 1.0 + 0.5;
        for column in 0..terrain.columns() {
            for row in 0..terrain.rows() {
                let at = terrain.point_place(column, row);
                let lateral = (at[1] - 12.0).abs();
                let height = terrain.point_height(column, row);
                let base = ground.point_height(column, row);
                if at[0] > 4.5 && at[0] < 35.5 && lateral <= core + 1e-9 {
                    near(height, 0.0);
                }
                let run = (2.0 * 0.5_f64).max(SLOPE_RUN_PER_RISE * base.abs());
                let beyond_ends = at[0] < 4.0 - core - run || at[0] > 36.0 + core + run;
                if lateral > core + run + 1e-9 || beyond_ends {
                    near(height, base);
                }
                let (low, high) = (base.min(0.0), base.max(0.0));
                assert!(
                    height >= low - 1e-9 && height <= high + 1e-9,
                    "{at:?}: {height} вне {low}..{high}"
                );
            }
        }
        assert_ne!(terrain.heights(), ground.heights());
        assert_eq!(terrain.base_heights(), ground.base_heights());
        near(terrain.ground_height_at(20.0, 12.0), 2.0);
    }

    #[test]
    fn the_slope_of_a_raised_road_is_smooth_no_steeper_than_one_to_two_and_ends_on_the_ground() {
        let ground = ground_of(40, 24, |_, _| 0.0);
        let mut form = resolve_forms(&[input(0, [4.0, 12.0], [36.0, 12.0], 2.0, 0.0)], &ground)[0];
        form.shape.z = 2.0;
        form.end_z = 2.0;
        let patch = patch_of(&ground, &[form]);
        let at = |row: usize| patch.lift(&ground, 40, row).unwrap_or(0.0);
        let mut previous = at(24);
        near(previous, 2.0);
        // ядро — 1,5 клетки (3 точки), откос — 3 × 2 = 6 клеток (12 точек по полклетки)
        for row in 25..=24 + 3 + 12 + 2 {
            let height = at(row);
            assert!(
                height <= previous + 1e-12,
                "откос растёт: {previous} → {height}"
            );
            assert!(
                previous - height <= 0.5 * 0.5 + 1e-9,
                "слишком крутой шаг {previous} → {height}"
            );
            previous = height;
        }
        near(at(24 + 3 + 12), 0.0);
        near(at(24 + 18), 0.0);
    }

    #[test]
    fn a_point_in_one_core_and_the_slope_of_another_belongs_to_the_core_and_the_stronger_slope_wins()
     {
        let ground = ground_of(30, 20, |_, _| 0.0);
        let raised = |id: u32, y: f64, z: f64| {
            let mut form = resolve_forms(&[input(id, [4.0, y], [24.0, y], 1.0, 0.0)], &ground)[0];
            form.shape.z = z;
            form.end_z = z;
            form
        };
        let patch = patch_of(&ground, &[raised(0, 8.0, 1.0), raised(1, 11.0, 3.0)]);
        let at = |y: f64| patch.lift(&ground, 28, (y * 2.0) as usize);
        near(at(8.5).unwrap_or(f64::NAN), 1.0);
        near(at(10.5).unwrap_or(f64::NAN), 3.0);
        let share =
            |gap: f64, rise: f64| 1.0 - smoothstep(gap / (1.0_f64).max(SLOPE_RUN_PER_RISE * rise));
        let (first, second) = (share(0.5, 1.0), share(0.5, 3.0));
        let expected = if first > second {
            first * 1.0
        } else {
            second * 3.0
        };
        near(at(9.5).expect("откос"), expected);
    }

    #[test]
    fn the_correction_goes_with_the_road_and_an_unchanged_road_changes_nothing() {
        let ground = ground_of(40, 24, hill(20.0, 12.0, 8.0, 2.0));
        let road = input(0, [4.0, 12.0], [36.0, 12.0], 2.0, 0.0);
        let mut state = RoadGround::default();
        let mut terrain = ground.clone();
        let patch = state.settle(&[road], &ground).expect("участок");
        assert!(terrain.adjust_for_roads(patch.area.clone(), |c, r| patch.lift(&ground, c, r)));
        assert!(state.settle(&[road], &ground).is_none(), "то же — ничего");
        let patch = state.settle(&[], &ground).expect("дорога исчезла");
        assert!(terrain.adjust_for_roads(patch.area.clone(), |c, r| patch.lift(&ground, c, r)));
        assert_eq!(terrain.heights(), ground.heights());
        assert!(state.is_empty());
    }

    #[test]
    fn a_level_road_does_not_touch_the_ground() {
        let ground = ground_of(40, 24, hill(20.0, 12.0, 8.0, 2.0));
        let mut bridge = input(0, [4.0, 12.0], [36.0, 12.0], 2.0, 0.0);
        bridge.level = true;
        bridge.shape.z = 5.0;
        let terrain = corrected(&ground, &[bridge]);
        assert_eq!(terrain.heights(), ground.heights());
    }

    #[test]
    fn a_curved_road_corrects_the_ground_along_its_arc() {
        let ground = ground_of(40, 30, |x, _| 0.15 * x);
        let arc = input(0, [6.0, 15.0], [30.0, 15.0], 2.0, 90.0);
        let terrain = corrected(&ground, &[arc]);
        let form = resolve_forms(&[arc], &ground)[0];
        let road = Road::from_form(&form);
        for fraction in [0.1, 0.3, 0.5, 0.7, 0.9] {
            let s = road.length() * fraction;
            let point = road.point_at(s);
            let drawn = terrain.height_at(point[0], point[1]);
            assert!(
                (drawn - road.height_at(s)).abs() < 2e-3,
                "на {fraction} длины дуги земля {drawn}, профиль {}",
                road.height_at(s)
            );
        }
    }
}

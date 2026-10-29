//! «Правила игры» → «Лазание», «Движения» → «Руки и ноги на перекладинах»: лестница в сцене, подход, подъём, сход и
//! спуск объекта и точки, куда в каждый миг встают его кисти и ступни. Что лазание помнит между шагами, лежит в мире
//! (`World::climb_state`), а не в свойствах объекта; поза рук и ног зависит только от высоты тела.

use std::f64::consts::PI;

use super::building::Frame;
use super::property;
use super::rules::Selector;
use super::runner::STEP_SECONDS;
use super::step::{face_along, selector_matches, turn_toward};
use super::surface;
use super::value::Rotation;
use super::world::World;

/// Точка сцены: клетки по `x` и `y`, высота.
pub type Point = [f64; 3];

/// За сколько секунд объект переходит на место у конца лестницы.
const APPROACH_SECONDS: f64 = 0.3;

/// Дальше этого от места у конца лестницы по горизонтали лезть не начинают.
const REACH: f64 = 1.0;

/// Место у низа лежит на столько клеток от низа к лезущему, место наверху — на столько от верха от лезущего.
const FOOT_BACK: f64 = 0.4;
const TOP_FORWARD: f64 = 0.5;

/// Насколько рука или нога отходит от лестницы посреди перехвата.
const ARC: f64 = 0.1;

/// Доля круга, за которую одна кисть или ступня переходит на перекладину две выше.
const TURN: f64 = 0.25;

/// На сколько шагов перекладин раньше подъёма тела идут перехваты: нижняя ступня уходит с перекладины, когда та
/// поравнялась с основанием тела, и нога не повисает.
const LEAD: f64 = 0.5;

/// Кисть или ступня в очереди перехватов за круг: правая кисть, левая ступня, левая кисть, правая ступня.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Limb {
    RightHand,
    LeftFoot,
    LeftHand,
    RightFoot,
}

impl Limb {
    /// Четверть круга, в которую она перехватывается.
    fn slot(self) -> f64 {
        f64::from(self as u8)
    }

    /// Правая кисть и левая ступня берут перекладины с чётными номерами, левая кисть и правая ступня — с нечётными: так
    /// кисть держится у плеча, когда нижняя ступня — у основания тела.
    fn parity(self) -> i64 {
        match self {
            Limb::RightHand | Limb::LeftFoot => 0,
            Limb::LeftHand | Limb::RightFoot => 1,
        }
    }
}

fn smoothstep(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp(from: Point, to: Point, along: f64) -> Point {
    std::array::from_fn(|axis| from[axis] + (to[axis] - from[axis]) * along)
}

/// Лестница в сцене: перекладины с места, поворота и увеличения её объекта, по высоте снизу вверх.
#[derive(Debug, Clone, PartialEq)]
pub struct Ladder {
    rungs: Vec<Point>,
    /// Средний шаг перекладин по высоте.
    step: f64,
    /// Отход лестницы по горизонтали на единицу высоты.
    slope: [f64; 2],
    /// Единичное направление по горизонтали от низа лестницы к верху.
    lean: [f64; 2],
    /// Единичное направление от лестницы к лезущему в вертикальной плоскости, куда она наклонена.
    away: Point,
    /// Поручни у верха лестницы.
    grabs: Vec<Point>,
}

impl Ladder {
    /// Лестница объекта `id`; `None`, если у его модели нет перекладин.
    pub fn of(world: &World, id: u32) -> Option<Ladder> {
        let model = world.model_of(id)?;
        let frame = Frame::of(world, id)?;
        let in_scene = |points: &[[f32; 3]]| -> Vec<Point> {
            points
                .iter()
                .map(|point| frame.scene_point(point.map(f64::from)))
                .collect()
        };
        Ladder::through(in_scene(&model.building.rungs))
            .map(|ladder| ladder.with_grabs(in_scene(&model.building.grabs)))
    }

    /// Лестница по перекладинам `rungs` сцены, по высоте снизу вверх; `None`, если их меньше двух или лестница отвесна.
    pub fn through(rungs: Vec<Point>) -> Option<Ladder> {
        let (bottom, top) = (*rungs.first()?, *rungs.last()?);
        let rise = top[2] - bottom[2];
        let run = [top[0] - bottom[0], top[1] - bottom[1]];
        let length = run[0].hypot(run[1]);
        if rise <= 0.0 || length <= 0.0 {
            return None;
        }
        let slant = length.hypot(rise);
        Some(Ladder {
            step: rise / (rungs.len() - 1) as f64,
            slope: run.map(|side| side / rise),
            lean: run.map(|side| side / length),
            away: [
                -run[0] / length * rise / slant,
                -run[1] / length * rise / slant,
                length / slant,
            ],
            rungs,
            grabs: Vec::new(),
        })
    }

    /// Та же лестница с поручнями `grabs` у верха.
    fn with_grabs(self, grabs: Vec<Point>) -> Ladder {
        Ladder { grabs, ..self }
    }

    pub fn rungs(&self) -> &[Point] {
        &self.rungs
    }

    pub fn grabs(&self) -> &[Point] {
        &self.grabs
    }

    /// Единичное направление по горизонтали от лестницы к лезущему.
    pub fn back(&self) -> [f64; 2] {
        self.lean.map(|side| -side)
    }

    /// Единичное направление вдоль лестницы снизу вверх.
    pub fn upward(&self) -> Point {
        let (bottom, top) = (self.rungs[0], self.rungs[self.rungs.len() - 1]);
        let rise: Point = std::array::from_fn(|axis| top[axis] - bottom[axis]);
        let length = rise.iter().map(|side| side * side).sum::<f64>().sqrt();
        rise.map(|side| side / length)
    }

    /// Точка на линии лестницы на высоте `height`.
    fn line(&self, height: f64) -> Point {
        let bottom = self.rungs[0];
        let shift = height - bottom[2];
        [
            bottom[0] + self.slope[0] * shift,
            bottom[1] + self.slope[1] * shift,
            height,
        ]
    }

    /// Низ лестницы: на шаг перекладин ниже нижней.
    fn low_end(&self) -> Point {
        self.line(self.rungs[0][2] - self.step)
    }

    /// Верх лестницы: на шаг перекладин выше верхней.
    fn high_end(&self) -> Point {
        self.line(self.rungs[self.rungs.len() - 1][2] + self.step)
    }

    /// Где держится кисть или ступня, взявшая место `index` в ряду перекладин: перекладина, а выше верхней — верх лестницы;
    /// кисть со сдвигом `beside` от середины лестницы выше верха берёт ближний поручень на высоте места в ряду, но не выше
    /// поручня. Перекладины и верх — со сдвигом `beside`, поручень — без.
    fn hold(&self, index: i64, beside: Option<[f64; 2]>) -> Point {
        let last = self.rungs.len() as i64 - 1;
        let aside = |point: Point| match beside {
            Some(by) => [point[0] + by[0], point[1] + by[1], point[2]],
            None => point,
        };
        if index <= last {
            return aside(self.rungs[self.rung_near(index)]);
        }
        let top = aside(self.high_end());
        if index == last + 1 || beside.is_none() {
            return top;
        }
        let height = self.rungs[0][2] + index as f64 * self.step;
        let apart = |grab: &Point| (grab[0] - top[0]).hypot(grab[1] - top[1]);
        self.grabs
            .iter()
            .min_by(|a, b| apart(a).total_cmp(&apart(b)))
            .map_or(top, |grab| [grab[0], grab[1], height.min(grab[2])])
    }

    /// Перекладина с номером `index`, сдвинутым на чётность `index` внутрь лестницы.
    fn rung_near(&self, index: i64) -> usize {
        let inside = if index < 0 {
            index + 2 * ((1 - index) / 2)
        } else {
            index
        };
        inside as usize
    }
}

/// Лестница с местами у её концов для лезущего объекта: тело идёт по линии, что параллельна лестнице и проходит через
/// место у низа.
struct Course {
    ladder: Ladder,
    low: Point,
    high: Point,
}

impl Course {
    fn new(world: &World, climber: u32, ladder: Ladder) -> Course {
        let stand = |end: Point, side: f64| {
            let at = [
                end[0] + ladder.lean[0] * side,
                end[1] + ladder.lean[1] * side,
            ];
            [at[0], at[1], surface::ground_at(world, climber, at, end[2])]
        };
        Course {
            low: stand(ladder.low_end(), -FOOT_BACK),
            high: stand(ladder.high_end(), TOP_FORWARD),
            ladder,
        }
    }

    /// Середина основания тела на высоте `height`.
    fn body(&self, height: f64) -> Point {
        let shift = height - self.low[2];
        [
            self.low[0] + self.ladder.slope[0] * shift,
            self.low[1] + self.ladder.slope[1] * shift,
            height,
        ]
    }

    /// Высота, с которой тело сходит наверх за время одной перекладины.
    fn line_top(&self) -> f64 {
        (self.high[2] - self.ladder.step).max(self.low[2])
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Stage {
    /// Переход на место у конца лестницы из точки `from` — середины основания тела — с поворотом от `facing` `turned`.
    Approach {
        from: Point,
        turned: f64,
        progress: f64,
    },
    /// Сход наверх или посадка на лестницу сверху: от линии лестницы (0) до места наверху (1).
    Top { reach: f64 },
    /// Тело идёт по линии лестницы на этой высоте.
    Rungs { height: f64 },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Climb {
    ladder: u32,
    /// Поколение объекта-лестницы: удалённая лестница и новый объект на её месте — разные лестницы.
    generation: u32,
    up: bool,
    stage: Stage,
}

/// Что лазание помнит об объекте между шагами: ничего, пока он не лезет.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ClimbState {
    active: Option<Climb>,
}

impl ClimbState {
    pub fn is_climbing(&self) -> bool {
        self.active.is_some()
    }
}

fn course_of(world: &World, climber: u32, climb: &Climb) -> Option<Course> {
    if !world.is_alive(climb.ladder) || world.generation(climb.ladder) != climb.generation {
        return None;
    }
    Some(Course::new(
        world,
        climber,
        Ladder::of(world, climb.ladder)?,
    ))
}

fn place(world: &mut World, id: u32, at: Point) {
    if let Some(size) = world.vec2(id, property::SIZE) {
        world.set_position_exact(id, [at[0] - size[0] / 2.0, at[1] - size[1] / 2.0], at[2]);
    }
}

fn finish(world: &mut World, id: u32, at: Point) -> Option<Climb> {
    place(world, id, at);
    None
}

/// Начинает лезть по просьбе: берёт ближайший конец лестницы, до которого от середины основания объекта меньше клетки по
/// горизонтали и не больше ступеньки по высоте.
fn start(world: &mut World, id: u32, ladders: &Selector) -> Option<Climb> {
    let (position, size) = (
        world.vec2(id, property::POSITION)?,
        world.vec2(id, property::SIZE)?,
    );
    let middle = [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0];
    let base = world.base_z(id);
    let mut nearest: Option<(f64, u32, bool, [f64; 2])> = None;
    for ladder in world
        .ids()
        .filter(|&other| other != id && selector_matches(ladders, world, other))
    {
        let Some(found) = Ladder::of(world, ladder) else {
            continue;
        };
        let course = Course::new(world, id, found);
        for (up, end) in [(true, course.low), (false, course.high)] {
            let distance = (middle[0] - end[0]).hypot(middle[1] - end[1]);
            let reachable =
                distance < REACH && (base - end[2]).abs() <= surface::STEP + surface::TOLERANCE;
            if reachable && nearest.is_none_or(|(best, ..)| distance < best) {
                nearest = Some((distance, ladder, up, course.ladder.lean));
            }
        }
    }
    let (_, ladder, up, _) = nearest?;
    Some(Climb {
        ladder,
        generation: world.generation(ladder),
        up,
        stage: Stage::Approach {
            from: [middle[0], middle[1], base],
            turned: world
                .rotation(id, property::FACING)
                .map_or(0.0, Rotation::angle),
            progress: 0.0,
        },
    })
}

/// Один шаг лазания; `None` — лазание кончилось: объект дошёл или лестницы больше нет.
fn advance(world: &mut World, id: u32, climb: Climb, speed: f64) -> Option<Climb> {
    let course = course_of(world, id, &climb)?;
    let travel = speed.max(0.0) * STEP_SECONDS;
    let mut climb = climb;
    match climb.stage {
        Stage::Approach {
            from,
            turned,
            progress,
        } => {
            let goal = if climb.up { course.low } else { course.high };
            let progress = progress + STEP_SECONDS / APPROACH_SECONDS;
            turn_toward(world, id, course.ladder.lean, turned, smoothstep(progress));
            if progress >= 1.0 {
                place(world, id, goal);
                climb.stage = if climb.up {
                    Stage::Rungs { height: goal[2] }
                } else {
                    Stage::Top { reach: 1.0 }
                };
            } else {
                place(world, id, lerp(from, goal, smoothstep(progress)));
                climb.stage = Stage::Approach {
                    from,
                    turned,
                    progress,
                };
            }
        }
        Stage::Rungs { height } => {
            face_along(world, id, course.ladder.lean);
            let top = course.line_top();
            if climb.up {
                let height = height + travel;
                let reached = height >= top;
                let height = height.min(top);
                place(world, id, course.body(height));
                climb.stage = if reached {
                    Stage::Top { reach: 0.0 }
                } else {
                    Stage::Rungs { height }
                };
            } else {
                if height <= course.low[2] {
                    return finish(world, id, course.low);
                }
                let height = (height - travel).max(course.low[2]);
                place(world, id, course.body(height));
                climb.stage = Stage::Rungs { height };
            }
        }
        Stage::Top { reach } => {
            face_along(world, id, course.ladder.lean);
            let rate = travel / course.ladder.step;
            let top = course.line_top();
            if climb.up {
                if reach >= 1.0 {
                    return finish(world, id, course.high);
                }
                let reach = (reach + rate).min(1.0);
                place(world, id, lerp(course.body(top), course.high, reach));
                climb.stage = Stage::Top { reach };
            } else {
                let reach = reach - rate;
                if reach <= 0.0 {
                    place(world, id, course.body(top));
                    climb.stage = Stage::Rungs { height: top };
                } else {
                    place(world, id, lerp(course.body(top), course.high, reach));
                    climb.stage = Stage::Top { reach };
                }
            }
        }
    }
    Some(climb)
}

/// «Лазание»: одно правило `climb` — каждый объект `for_` с просьбой `climb` начинает лезть по лестнице из `ladders`,
/// если стоит у её конца, а кто уже лезет, делает шаг; просьбу движок снимает в любом случае. Отмечает в `moved` тех, кто
/// сдвинулся, и отдаёт их в `moved_objects`.
pub fn apply(
    world: &mut World,
    for_: &Selector,
    ladders: &Selector,
    moved: &mut [bool],
    mut moved_objects: Option<&mut Vec<u32>>,
) {
    let climbers: Vec<u32> = world
        .ids()
        .filter(|&id| selector_matches(for_, world, id))
        .collect();
    for id in climbers {
        let asked = world.flag(id, property::CLIMB);
        if asked {
            world.set_flag(id, property::CLIMB, false);
        }
        let speed = world.number_like(id, property::CLIMB_SPEED).unwrap_or(0.0);
        let before = (world.vec2(id, property::POSITION), world.base_z(id));
        let next = match world.climb_state(id).active {
            Some(climb) => advance(world, id, climb, speed),
            None if asked && speed > 0.0 => start(world, id, ladders),
            None => continue,
        };
        world.set_flag(id, property::CLIMBING, next.is_some());
        world.set_climb_state(id, ClimbState { active: next });
        if before != (world.vec2(id, property::POSITION), world.base_z(id)) {
            moved[id as usize] = true;
            if let Some(list) = moved_objects.as_deref_mut() {
                list.push(id);
            }
        }
    }
}

/// Хват лезущего объекта в этот миг: лестница, высота тела и доля, на которую кисти и ступни тянутся к перекладинам.
#[derive(Debug, Clone, PartialEq)]
pub struct Grip {
    pub ladder: Ladder,
    pub height: f64,
    /// 1 — всё на перекладинах; 0 — хвата нет: тело берётся с лестницы или сходит на пол.
    pub weight: f64,
    /// То же для кистей: наверху они отпускают лестницу за первую половину схода и берутся за неё за последнюю половину
    /// посадки сверху.
    pub hands_weight: f64,
}

/// Хват объекта `id`; `None`, пока он не лезет по лестнице или ещё подходит к ней.
pub fn grip(world: &World, id: u32) -> Option<Grip> {
    let climb = world.climb_state(id).active?;
    let course = course_of(world, id, &climb)?;
    let (held, hands_held) = match climb.stage {
        Stage::Approach { .. } => return None,
        Stage::Rungs { .. } => (1.0, 1.0),
        Stage::Top { reach } => (1.0 - smoothstep(reach), 1.0 - smoothstep(2.0 * reach)),
    };
    let height = world.base_z(id);
    let grown = smoothstep((height - course.low[2]) / course.ladder.step);
    Some(Grip {
        weight: held * grown,
        hands_weight: hands_held * grown,
        ladder: course.ladder,
        height,
    })
}

impl Grip {
    /// Где держатся левая и правая кисти; `above_base` — высота плеча каждой руки над основанием тела в позе движения `climb`,
    /// `beside` — её сдвиг по горизонтали от середины тела вбок, на ширину плеч.
    pub fn hands(&self, above_base: [f64; 2], beside: [[f64; 2]; 2]) -> [Point; 2] {
        [
            self.limb(Limb::LeftHand, above_base[0], Some(beside[0])),
            self.limb(Limb::RightHand, above_base[1], Some(beside[1])),
        ]
    }

    /// Где стоят левая и правая ступни.
    pub fn feet(&self) -> [Point; 2] {
        [
            self.limb(Limb::LeftFoot, 0.0, None),
            self.limb(Limb::RightFoot, 0.0, None),
        ]
    }

    /// Где держится `limb`, чья высота в позе — `above_base` над основанием (`Ladder::hold`): за круг тело поднимается на
    /// две перекладины, а каждая кисть и ступня за свою четверть круга переходит на перекладину своей чётности через одну
    /// выше по дуге, что отходит от лестницы на `ARC` посередине. Перекладина — ближайшая к высоте `limb` в середине того
    /// времени, что она на ней держится.
    fn limb(&self, limb: Limb, above_base: f64, beside: Option<[f64; 2]>) -> Point {
        let ladder = &self.ladder;
        let (slot, parity) = (limb.slot(), limb.parity());
        let cycle = (self.height + LEAD * ladder.step - ladder.rungs[0][2]) / (2.0 * ladder.step)
            - slot * TURN;
        let turn = cycle.floor();
        let held = |turn: f64| {
            let wish = turn + slot * TURN + (1.0 + TURN) / 2.0 - LEAD / 2.0
                + (above_base - parity as f64 * ladder.step) / (2.0 * ladder.step);
            ladder.hold(parity + 2 * wish.round() as i64, beside)
        };
        let to = held(turn);
        let moving = cycle - turn;
        let from = held(turn - 1.0);
        if moving >= TURN || from == to {
            return to;
        }
        let along = smoothstep(moving / TURN);
        let bow = ARC * (PI * along).sin();
        let on_line = lerp(from, to, along);
        std::array::from_fn(|axis| on_line[axis] + ladder.away[axis] * bow)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Лестница в девять перекладин с шагом 0,4 по высоте: низ на высоте 0,4, верх на 3,6, отход по x на 0,1 за шаг.
    fn ladder() -> Ladder {
        Ladder::through(
            (0..9)
                .map(|k| [0.1 * f64::from(k), 0.0, 0.4 + 0.4 * f64::from(k)])
                .collect(),
        )
        .expect("лестница наклонна")
    }

    fn grip_at(height: f64) -> Grip {
        Grip {
            ladder: ladder(),
            height,
            weight: 1.0,
            hands_weight: 1.0,
        }
    }

    fn distance(a: Point, b: Point) -> f64 {
        (0..3)
            .map(|axis| (a[axis] - b[axis]).powi(2))
            .sum::<f64>()
            .sqrt()
    }

    #[test]
    fn a_ladder_runs_from_a_step_below_the_lowest_rung_to_a_step_above_the_highest() {
        let ladder = ladder();
        assert!((ladder.step - 0.4).abs() < 1e-12);
        let (low, high) = (ladder.low_end(), ladder.high_end());
        assert!(distance(low, [-0.1, 0.0, 0.0]) < 1e-12, "{low:?}");
        assert!(distance(high, [0.9, 0.0, 4.0]) < 1e-12, "{high:?}");
        assert_eq!(ladder.lean, [1.0, 0.0]);
    }

    #[test]
    fn rungs_that_are_fewer_than_two_or_stand_on_one_vertical_or_one_height_are_no_ladder() {
        assert!(Ladder::through(vec![]).is_none());
        assert!(Ladder::through(vec![[0.0, 0.0, 1.0]]).is_none());
        assert!(Ladder::through(vec![[0.0, 0.0, 1.0], [0.0, 0.0, 2.0]]).is_none());
        assert!(Ladder::through(vec![[0.0, 0.0, 1.0], [1.0, 0.0, 1.0]]).is_none());
    }

    #[test]
    fn the_same_body_height_gives_the_same_grip_and_every_moment_has_three_limbs_on_rungs() {
        let ladder = ladder();
        let on_rung = |point: Point| {
            ladder
                .rungs()
                .iter()
                .any(|&rung| distance(point, rung) < 0.02)
        };
        for tick in 0..400 {
            let height = 0.4 + f64::from(tick) * 0.005;
            let grip = grip_at(height);
            let [left_hand, right_hand] = grip.hands([0.4, 0.4], [[0.0; 2]; 2]);
            let [left_foot, right_foot] = grip.feet();
            let held = [left_hand, right_hand, left_foot, right_foot]
                .into_iter()
                .filter(|&point| on_rung(point))
                .count();
            assert!(
                held >= 3,
                "на высоте {height}: на перекладинах {held} из четырёх"
            );
            assert_eq!(
                grip.hands([0.4, 0.4], [[0.0; 2]; 2]),
                grip_at(height).hands([0.4, 0.4], [[0.0; 2]; 2])
            );
        }
    }

    #[test]
    fn limbs_change_rungs_in_turn_a_quarter_of_the_cycle_each_and_a_cycle_climbs_two_rungs() {
        let moving = |height: f64| -> Vec<usize> {
            let grip = grip_at(height);
            let [left_hand, right_hand] = grip.hands([0.4, 0.4], [[0.0; 2]; 2]);
            let [left_foot, right_foot] = grip.feet();
            [right_hand, left_foot, left_hand, right_foot]
                .into_iter()
                .enumerate()
                .filter(|&(_, point)| {
                    grip.ladder
                        .rungs()
                        .iter()
                        .all(|&rung| distance(point, rung) > 0.02)
                })
                .map(|(at, _)| at)
                .collect()
        };
        // Круг в 0,8 по высоте: правая кисть, левая ступня, левая кисть, правая ступня — по четверти, каждая на середине
        // своего перехода; перехваты идут на полшага перекладин раньше тела.
        for quarter in 0..4 {
            let height = 1.1 + 0.2 * quarter as f64;
            assert_eq!(moving(height), vec![quarter], "высота {height}");
        }
    }

    #[test]
    fn a_limb_in_the_middle_of_its_turn_lies_between_two_rungs_bowed_out_of_the_ladder() {
        let grip = grip_at(1.1);
        let [_, right_hand] = grip.hands([0.4, 0.4], [[0.0; 2]; 2]);
        let rungs = grip.ladder.rungs();
        let bowed = rungs.windows(3).any(|three| {
            let middle = lerp(three[0], three[2], 0.5);
            (distance(right_hand, middle) - ARC).abs() < 0.01 && right_hand[0] < middle[0]
        });
        assert!(
            bowed,
            "кисть не на дуге между перекладинами через одну: {right_hand:?}"
        );
    }

    #[test]
    fn limbs_above_the_top_rung_and_below_the_bottom_one_stay_on_the_ends_of_the_ladder() {
        for height in [-3.0, 0.0, 4.5, 8.0] {
            let grip = grip_at(height);
            let points = [grip.hands([0.4, 0.4], [[0.0; 2]; 2]), grip.feet()].concat();
            let top = grip.ladder.high_end();
            for point in points {
                assert!(
                    grip.ladder
                        .rungs()
                        .iter()
                        .chain([&top])
                        .any(|&rung| distance(point, rung) < 0.2),
                    "на высоте {height}: {point:?}"
                );
            }
        }
    }

    #[test]
    fn above_the_top_the_feet_stand_on_the_top_of_the_ladder_and_the_hands_take_the_grab_on_their_side()
     {
        let ladder = ladder().with_grabs(vec![[1.0, -0.4, 4.3], [1.0, 0.4, 4.3]]);
        let grip = Grip {
            ladder,
            height: 4.2,
            weight: 1.0,
            hands_weight: 1.0,
        };
        let top = grip.ladder.high_end();
        for foot in grip.feet() {
            assert!(distance(foot, top) < 1e-9, "ступня {foot:?}, верх {top:?}");
        }
        let [left, right] = grip.hands([0.9, 0.9], [[0.0, -0.2], [0.0, 0.2]]);
        let on_grab = |hand: Point, y: f64| {
            (hand[0] - 1.0).abs() < 1e-9 && (hand[1] - y).abs() < 1e-9 && hand[2] <= 4.3
        };
        assert!(on_grab(left, -0.4), "левая кисть {left:?}");
        assert!(on_grab(right, 0.4), "правая кисть {right:?}");
        let bare = grip_at(4.2).hands([0.9, 0.9], [[0.0, -0.2], [0.0, 0.2]]);
        for (hand, y) in bare.into_iter().zip([-0.2, 0.2]) {
            assert!(
                distance(hand, [top[0], y, top[2]]) < 1e-9,
                "без поручней кисть на верху лестницы: {hand:?}"
            );
        }
    }
}

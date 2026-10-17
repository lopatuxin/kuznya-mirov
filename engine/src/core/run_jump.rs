//! «Правила игры» → «Бег и прыжок»: правило `run_jump` — герой плоской сцены бежит по `hold_left` и
//! `hold_right`, прыгает по `hold_jump`, стоит на твёрдом `solid` и встаёт на площадки `one_way`. Всё,
//! что правило помнит между шагами, лежит в мире (`World::jump_state`), а не в свойствах объекта.

use super::property;
use super::rules::{Rule, RuleSet, Selector};
use super::runner::STEP_SECONDS;
use super::step::selector_matches;
use super::time::seconds_to_steps;
use super::world::World;

const GROUND_ACCEL_SECONDS: f64 = 0.1;
const AIR_ACCEL_SECONDS: f64 = 0.15;
const MEMORY_SECONDS: f64 = 0.1;
const DEFAULT_JUMP_TIME: f64 = 0.4;
const FALL_LIMIT: f64 = 1.5;
const CUT_SPEED: f64 = 1.0 / 3.0;
const HANG_SPEED: f64 = 0.25;
const HANG_GRAVITY: f64 = 0.5;

/// Касание краями — не пересечение: зазор и перекрытие меньше этого числа считаются нулём.
const TOUCH: f64 = 1e-9;

/// Что правило помнит об объекте между шагами.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct JumpState {
    held_before: bool,
    press_age: Option<u32>,
    since_ground: Option<u32>,
    rising: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutlineKind {
    Solid,
    OneWay,
    Other,
}

#[derive(Clone, Copy)]
struct Span {
    lo: f64,
    hi: f64,
}

impl Span {
    fn overlaps(self, other: Span) -> bool {
        self.lo < other.hi - TOUCH && self.hi > other.lo + TOUCH
    }
}

type Body = [Span; 2];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Holder {
    Solid,
    OneWay,
}

struct Obstacles<'a> {
    solid: &'a Option<Selector>,
    one_way: &'a Option<Selector>,
}

impl Obstacles<'_> {
    fn kind_of(&self, world: &World, id: u32) -> Option<Holder> {
        let matches = |selector: &Option<Selector>| {
            selector
                .as_ref()
                .is_some_and(|selector| selector_matches(selector, world, id))
        };
        if matches(self.solid) {
            Some(Holder::Solid)
        } else if matches(self.one_way) {
            Some(Holder::OneWay)
        } else {
            None
        }
    }

    fn around<'w>(
        &'w self,
        world: &'w World,
        hero: u32,
    ) -> impl Iterator<Item = (Body, Holder)> + 'w {
        (0..world.slot_count() as u32)
            .filter(move |&id| id != hero && world.is_alive(id))
            .filter_map(move |id| {
                let body = body_of(world, id)?;
                Some((body, self.kind_of(world, id)?))
            })
    }
}

struct Numbers {
    run_speed: f64,
    jump_height: f64,
    jump_time: f64,
}

impl Numbers {
    fn read(world: &World, id: u32) -> Option<Numbers> {
        let run_speed = world.number_like(id, property::RUN_SPEED)?;
        let jump_height = world.number_like(id, property::JUMP_HEIGHT)?;
        let jump_time = world
            .number_like(id, property::JUMP_TIME)
            .unwrap_or(DEFAULT_JUMP_TIME);
        let fits = run_speed >= 0.0 && jump_height > 0.0 && jump_time > 0.0;
        (fits && run_speed.is_finite() && jump_height.is_finite() && jump_time.is_finite())
            .then_some(Numbers {
                run_speed,
                jump_height,
                jump_time,
            })
    }

    fn jump_speed(&self) -> f64 {
        2.0 * self.jump_height / self.jump_time
    }

    fn gravity(&self) -> f64 {
        2.0 * self.jump_height / (self.jump_time * self.jump_time)
    }
}

fn body_of(world: &World, id: u32) -> Option<Body> {
    let position = world.vec2(id, property::POSITION)?;
    let size = world.vec2(id, property::SIZE)?;
    Some([
        Span {
            lo: position[0],
            hi: position[0] + size[0],
        },
        Span {
            lo: position[1],
            hi: position[1] + size[1],
        },
    ])
}

fn approach(value: f64, target: f64, max_delta: f64) -> f64 {
    if (target - value).abs() <= max_delta {
        target
    } else {
        value + max_delta.copysign(target - value)
    }
}

/// Путь `delta` по оси `axis` до первого прямоугольника, который объект держит: насколько можно
/// пройти и упёрся ли он. `start` — объект в начале шага: твёрдое и площадка, с которыми он тогда
/// пересекался, не держат.
fn travel(
    world: &World,
    hero: u32,
    obstacles: &Obstacles,
    axis: usize,
    start: &Body,
    moving: &Body,
    delta: f64,
) -> (f64, bool) {
    if delta == 0.0 {
        return (0.0, false);
    }
    let mut reach = delta.abs();
    let mut blocked = false;
    for (other, holder) in obstacles.around(world, hero) {
        if start[0].overlaps(other[0]) && start[1].overlaps(other[1]) {
            continue;
        }
        if !moving[1 - axis].overlaps(other[1 - axis]) {
            continue;
        }
        let gap = if delta > 0.0 {
            other[axis].lo - moving[axis].hi
        } else {
            moving[axis].lo - other[axis].hi
        };
        let from_above = start[1].hi <= other[1].lo + TOUCH;
        let one_way_holds = axis == 1 && delta > 0.0 && from_above;
        if gap < -TOUCH || (holder == Holder::OneWay && !one_way_holds) {
            continue;
        }
        if gap <= reach {
            reach = if gap <= TOUCH { 0.0 } else { gap };
            blocked = true;
        }
    }
    (reach.copysign(delta), blocked)
}

fn shifted(body: &Body, axis: usize, by: f64) -> Body {
    let mut moved = *body;
    moved[axis] = Span {
        lo: body[axis].lo + by,
        hi: body[axis].hi + by,
    };
    moved
}

/// Клавиши героя на этом шаге.
struct Keys {
    left: bool,
    right: bool,
    jump: bool,
}

impl Keys {
    fn read(world: &World, hero: u32) -> Keys {
        Keys {
            left: world.flag(hero, property::HOLD_LEFT),
            right: world.flag(hero, property::HOLD_RIGHT),
            jump: world.flag(hero, property::HOLD_JUMP),
        }
    }

    /// Обе стрелки или ни одной — 0.
    fn direction(&self) -> f64 {
        f64::from(i8::from(self.right)) - f64::from(i8::from(self.left))
    }
}

impl JumpState {
    /// Запоминает нажатие прыжка и считает шаги с тех пор, как объект сошёл с земли.
    fn observe(&mut self, jump_held: bool, grounded: bool) {
        let pressed = jump_held && !self.held_before;
        self.held_before = jump_held;
        self.press_age = if pressed {
            Some(0)
        } else {
            self.press_age.map(|age| age.saturating_add(1))
        };
        self.since_ground = if grounded {
            Some(0)
        } else {
            self.since_ground.map(|steps| steps.saturating_add(1))
        };
    }

    /// Прыжок по помнящемуся нажатию, если объект на земле или только что сошёл с неё; на подъёме после
    /// прыжка отпущенная клавиша срезает скорость вверх.
    fn take_off(&mut self, velocity_y: &mut f64, jump_held: bool, grounded: bool, jump_speed: f64) {
        let may_jump = grounded
            || self
                .since_ground
                .is_some_and(|steps| steps <= memory_steps());
        if self.remembers_press() && may_jump {
            self.leap(velocity_y, jump_speed);
        }
        if !self.rising {
            return;
        }
        if *velocity_y >= 0.0 {
            self.rising = false;
        } else if !jump_held {
            *velocity_y = velocity_y.max(-jump_speed * CUT_SPEED);
        }
    }

    /// Нажатие прыжка ещё помнится: с него прошло не больше 0,1 секунды.
    fn remembers_press(&self) -> bool {
        self.press_age.is_some_and(|age| age <= memory_steps())
    }

    /// Отрыв: скорость вверх — скорость отрыва; нажатие и запас после края израсходованы.
    fn leap(&mut self, velocity_y: &mut f64, jump_speed: f64) {
        *velocity_y = -jump_speed;
        self.press_age = None;
        self.since_ground = None;
        self.rising = true;
    }

    /// Касание земли с помнящимся нажатием: прыжок в том же шаге, а не шагом позже.
    fn leap_on_touch(&mut self, velocity_y: &mut f64, jump_speed: f64) -> bool {
        if !self.remembers_press() {
            return false;
        }
        self.leap(velocity_y, jump_speed);
        true
    }
}

fn memory_steps() -> u32 {
    seconds_to_steps(MEMORY_SECONDS) as u32
}

/// Скорость по ширине идёт к цели не быстрее, чем разгон на земле или в воздухе.
fn run_velocity(speed: f64, keys: &Keys, grounded: bool, numbers: &Numbers) -> f64 {
    let accel_seconds = if grounded {
        GROUND_ACCEL_SECONDS
    } else {
        AIR_ACCEL_SECONDS
    };
    approach(
        speed,
        keys.direction() * numbers.run_speed,
        numbers.run_speed / accel_seconds * STEP_SECONDS,
    )
}

/// Тяжесть за шаг: вполовину слабее у верхней точки при зажатом прыжке; падение ограничено.
fn fall_velocity(speed: f64, jump_held: bool, numbers: &Numbers) -> f64 {
    let jump_speed = numbers.jump_speed();
    let hanging = jump_held && speed.abs() < jump_speed * HANG_SPEED;
    let gravity = numbers.gravity() * if hanging { HANG_GRAVITY } else { 1.0 };
    (speed + gravity * STEP_SECONDS).min(jump_speed * FALL_LIMIT)
}

/// Куда сдвинется объект за шаг и в какие прямоугольники он упрётся: сначала по ширине, потом по высоте.
struct Displacement {
    by: [f64; 2],
    hit_wall: bool,
    hit_floor_or_ceiling: bool,
}

fn displace(
    world: &World,
    hero: u32,
    obstacles: &Obstacles,
    start: &Body,
    velocity: [f64; 2],
) -> Displacement {
    let (dx, hit_wall) = travel(
        world,
        hero,
        obstacles,
        0,
        start,
        start,
        velocity[0] * STEP_SECONDS,
    );
    let after_x = shifted(start, 0, dx);
    let (dy, hit_floor_or_ceiling) = travel(
        world,
        hero,
        obstacles,
        1,
        start,
        &after_x,
        velocity[1] * STEP_SECONDS,
    );
    Displacement {
        by: [dx, dy],
        hit_wall,
        hit_floor_or_ceiling,
    }
}

fn run_hero(world: &mut World, hero: u32, obstacles: &Obstacles) -> bool {
    let Some(numbers) = Numbers::read(world, hero) else {
        return false;
    };
    let (Some(position), Some(start)) =
        (world.vec2(hero, property::POSITION), body_of(world, hero))
    else {
        return false;
    };
    let keys = Keys::read(world, hero);
    let grounded = world.flag(hero, property::ON_GROUND);
    let mut velocity = world.vec2(hero, property::VELOCITY).unwrap_or([0.0, 0.0]);
    let mut state = world.jump_state(hero);

    state.observe(keys.jump, grounded);
    velocity[0] = run_velocity(velocity[0], &keys, grounded, &numbers);
    velocity[1] = fall_velocity(velocity[1], keys.jump, &numbers);
    state.take_off(&mut velocity[1], keys.jump, grounded, numbers.jump_speed());

    let falling = velocity[1] > 0.0;
    let moving = displace(world, hero, obstacles, &start, velocity);
    if moving.hit_wall {
        velocity[0] = 0.0;
    }
    if moving.hit_floor_or_ceiling {
        velocity[1] = 0.0;
        state.rising = false;
    }
    let landed = moving.hit_floor_or_ceiling && falling;
    let leapt = landed && state.leap_on_touch(&mut velocity[1], numbers.jump_speed());

    let new_position = [position[0] + moving.by[0], position[1] + moving.by[1]];
    let moved = new_position != position;
    if moved {
        world.set_vec2(hero, property::POSITION, new_position);
    }
    world.set_vec2(hero, property::VELOCITY, velocity);
    world.set_flag(hero, property::ON_GROUND, landed && !leapt);
    world.set_jump_state(hero, state);
    moved
}

/// Один шаг правила `run_jump` по всем объектам, которых берёт `for_`, в порядке номеров. Сдвинутых
/// объектов отмечает в `moved` и, если нужен отчёт шага, складывает в `reported`.
pub fn apply(
    world: &mut World,
    for_: &Selector,
    solid: &Option<Selector>,
    one_way: &Option<Selector>,
    moved: &mut [bool],
    mut reported: Option<&mut Vec<u32>>,
) {
    let obstacles = Obstacles { solid, one_way };
    for id in 0..world.slot_count() as u32 {
        if !world.is_alive(id) || !selector_matches(for_, world, id) {
            continue;
        }
        if run_hero(world, id, &obstacles) {
            moved[id as usize] = true;
            if let Some(list) = reported.as_deref_mut() {
                list.push(id);
            }
        }
    }
}

fn solid_of(rule: &Rule) -> Option<&Selector> {
    match rule {
        Rule::RunJump { solid, .. } => solid.as_ref(),
        _ => None,
    }
}

fn one_way_of(rule: &Rule) -> Option<&Selector> {
    match rule {
        Rule::RunJump { one_way, .. } => one_way.as_ref(),
        _ => None,
    }
}

/// «Редактор», `outlines()`: объекты плоской сцены с `position` и `size`, но без картинки и цвета, и вид
/// каждого — твёрдое любого правила `run_jump`, площадка или прочий. Объект в обоих отборах — твёрдое.
pub fn outlines(world: &World, rules: &RuleSet) -> Vec<(u32, OutlineKind)> {
    if world.three_d() {
        return Vec::new();
    }
    let picked = |id: u32, select: fn(&Rule) -> Option<&Selector>| {
        rules
            .rules
            .iter()
            .filter_map(select)
            .any(|selector| selector_matches(selector, world, id))
    };
    world
        .ids()
        .filter(|&id| {
            world.has_all(id, &[property::POSITION, property::SIZE])
                && world.has_none(id, &[property::IMAGE, property::COLOR])
        })
        .map(|id| {
            let kind = if picked(id, solid_of) {
                OutlineKind::Solid
            } else if picked(id, one_way_of) {
                OutlineKind::OneWay
            } else {
                OutlineKind::Other
            };
            (id, kind)
        })
        .collect()
}

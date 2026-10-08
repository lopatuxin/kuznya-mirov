//! «Ветер и частицы» → «Ветер», «Качание», «Кадры качающегося объекта»: ветер в точке сцены, цель
//! наклона и период пружины объекта, часы движения, пружины и кадры качающихся объектов, изгиб
//! рисунка. Ни видеокарты, ни браузера: `atlas` берёт отсюда наклоны и кадры, `wasm::mod` ведёт
//! часы и зовёт [`Motion::update`] раз в кадр.

use std::f64::consts::TAU;

use super::particles::{Emitter, OpaqueMask, Particles};

/// «Часы движения», требование 18: пружины и кадры качающихся объектов идут шагами по 1/60 секунды,
/// как шаг мира.
const STEPS_PER_SECOND: f64 = 60.0;
const STEP_SECONDS: f64 = 1.0 / STEPS_PER_SECOND;
/// «Часы движения», требование 17: кадр редактора двигает часы не больше чем на эту долю секунды.
const MAX_FRAME_SECONDS: f64 = 0.1;
/// «Часы движения», требование 18: часы, ушедшие вперёд больше чем на столько шагов разом (две
/// секунды), начинают качание заново.
const MAX_JUMP_STEPS: f64 = 2.0 * STEPS_PER_SECOND;
/// «Видео на объекте», требование 5: часы, не сдвинувшиеся столько секунд, стоят. Больше паузы между
/// шагами мира на частом экране, иначе видео дёргалось бы между запуском и остановкой.
const CLOCK_GRACE_SECONDS: f64 = 0.1;

const DAMPING: f64 = 0.3;
const MAX_LEAN_OF_HEIGHT: f64 = 0.7;
const GOLDEN_FRACTION: f64 = 0.618034;

/// «Ветер», требование 6: узор порывов `g` в точке следа `ξ`, от −1 до 1.
fn gust_pattern(trace: f64) -> f64 {
    0.5 * (TAU * trace / 23.0).sin()
        + 0.3 * (TAU * trace / 11.3 + 1.7).sin()
        + 0.2 * (TAU * trace / 6.1 + 4.1).sin()
}

/// «Ветер», требование 6: порыв `G` — во сколько раз ровный ветер сильнее в точке `x` в момент `t`
/// (секунды часов движения), от 0,8 до 1,7. Узор едет по сцене со скоростью ровного ветра `flat_x`.
pub fn gust(flat_x: f64, x: f64, t: f64) -> f64 {
    0.8 + 0.9 * gust_pattern(x - flat_x * t).max(0.0).powi(2)
}

/// «Ветер», требование 6: рябь `T` ветра в точке `x` в момент `t`.
fn ripple(x: f64, t: f64) -> f64 {
    0.12 * (TAU * (t / 1.7 + x / 5.3)).sin() + 0.08 * (TAU * (t / 0.9 - x / 3.7)).sin()
}

/// «Ветер», требование 6: ветер по `x` в точке сцены `x` (клетки) в момент `t` — ровный ветер `flat_x`
/// с порывом, рябью и движением воздуха в безветрие.
pub fn wind_at(flat_x: f64, x: f64, t: f64) -> f64 {
    let air = 0.3 * (TAU * (t / 4.3 + x / 9.1)).sin();
    flat_x * gust(flat_x, x, t) * (1.0 + ripple(x, t)) + air
}

/// «Ветер и частицы» → «Частица», требование 12: ветер в точке частицы `(x, ·)` в момент `t` — по `x`
/// как у `wind_at`, по `y` ровный ветер `flat[1]` с тем же порывом и рябью, без движения воздуха.
pub fn wind_vector(flat: [f64; 2], x: f64, t: f64) -> [f64; 2] {
    [
        wind_at(flat[0], x, t),
        flat[1] * gust(flat[0], x, t) * (1.0 + ripple(x, t)),
    ]
}

/// «Качание», требование 10: на сколько клеток уходит вбок верх рисунка высоты `height` при ветре `u`
/// в точке объекта с гибкостью `sway`.
pub fn lean_target(sway: f64, u: f64, height: f64) -> f64 {
    let pressure = u * u.abs();
    let lean = sway * pressure * (7.0 / 6.0) / (1.0 + pressure.abs() / 6.0);
    let limit = (MAX_LEAN_OF_HEIGHT * height).max(0.0);
    lean.clamp(-limit, limit)
}

/// «Качание», требование 12: период пружины в секундах — выше рисунок и ближе слой, медленнее.
/// `parallax` — слоя объекта, 1 у объекта без него.
pub fn sway_period(height: f64, parallax: f64) -> f64 {
    let depth = parallax.clamp(0.2, 3.0);
    (0.6 + 0.3 * height / depth).clamp(0.6, 4.0)
}

/// «Качание», требование 12: шаг пружины, `Δ` = 1/60 секунды. Возвращает наклон и скорость.
pub fn spring_step(lean: f64, velocity: f64, target: f64, period: f64) -> (f64, f64) {
    let omega = TAU / period;
    let velocity = velocity
        + (omega * omega * (target - lean) - 2.0 * DAMPING * omega * velocity) * STEP_SECONDS;
    (lean + velocity * STEP_SECONDS, velocity)
}

/// «Качание», требование 14: где стоит точка рисунка высотой `height` с наклоном `lean`, поднятая на
/// `above_bottom` над нижним краем, — вбок на `lean · s²` (`s` — доля высоты) и вниз от прежней высоты
/// на столько, чтобы остаться на дуге над своим местом на нижнем крае. Возвращает `(вбок, вниз)`;
/// те же числа считает вершинный шейдер `rect.wgsl`.
pub fn bend(height: f64, lean: f64, above_bottom: f64) -> (f64, f64) {
    let along = above_bottom / height;
    let shift = lean * along * along;
    let reach = (above_bottom * above_bottom - shift * shift)
        .max(0.0)
        .sqrt();
    (shift, shift * shift / (above_bottom + reach).max(1e-6))
}

/// «Кадры качающегося объекта», требование 20: во сколько раз кадры объекта идут быстрее своего
/// `frame_time` при ветре `u` в его точке — от 0,5 до 2.
pub fn frame_rate(u: f64) -> f64 {
    (0.5 + 0.5 * u.abs()).clamp(0.5, 2.0)
}

/// «Кадры качающегося объекта», требования 20–21: кадр объекта номер `id` по его часам `clock`
/// (секунды) у картинки в `frames` кадров по `frame_time` секунд. Объект начинает со своего места
/// цикла: `frac(id · 0,618034)` от его длины.
pub fn swaying_frame(id: u32, clock: f64, frames: u32, frame_time: f64) -> u32 {
    let cycle = f64::from(frames) * frame_time;
    let start = (f64::from(id) * GOLDEN_FRACTION).fract() * cycle;
    let passed = ((start + clock) / frame_time).floor().max(0.0);
    (passed as u64 % u64::from(frames.max(1))) as u32
}

/// «Качание», требование 14: на сколько частей по высоте делится рисунок — изгиб идёт по их границам.
const STRIP_BANDS: usize = 8;
/// Вершин полосы-треугольников: две на каждую из `STRIP_BANDS + 1` границ.
pub const STRIP_VERTICES: usize = (STRIP_BANDS + 1) * 2;

/// «Качание», требование 14: вершины единичного прямоугольника, полоса-треугольников из
/// `STRIP_BANDS` частей по высоте — слева направо и сверху вниз, `y` от 0 до 1.
pub fn strip_units() -> [[f32; 2]; STRIP_VERTICES] {
    std::array::from_fn(|index| [(index % 2) as f32, (index / 2) as f32 / STRIP_BANDS as f32])
}

/// Что движению надо знать о качающемся объекте: «Качание», требования 10–12. `x` — середина
/// нижнего края записанного прямоугольника, `height` — высота нарисованного рисунка, `image` — номер
/// картинки (`None` у заливки цветом). `sway` — не меньше нуля: объект со свойством `sway` любого
/// значения отдаётся всегда. Метка жизни, картинка и высота — отпечаток объекта: сборка мира заново
/// раздаёт номера по порядку и метки у всех 0, и другой объект под тем же номером узнают по отпечатку
/// на первом обновлении после неё ([`Motion::world_rebuilt`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwayObject {
    pub id: u32,
    pub generation: u32,
    pub image: Option<usize>,
    pub sway: f64,
    pub x: f64,
    pub height: f64,
    pub parallax: f64,
}

impl SwayObject {
    fn is_same_object(&self, other: &SwayObject, compare_look: bool) -> bool {
        self.generation == other.generation
            && (!compare_look || (self.image == other.image && self.height == other.height))
    }
}

#[derive(Debug, Clone, Copy)]
struct SwayState {
    object: SwayObject,
    touched: u32,
    lean: f64,
    velocity: f64,
    frame_clock: f64,
}

impl SwayState {
    fn step(&mut self, flat_x: f64, t: f64) {
        let object = self.object;
        let u = wind_at(flat_x, object.x, t);
        let target = lean_target(object.sway, u, object.height);
        let period = sway_period(object.height, object.parallax);
        (self.lean, self.velocity) = spring_step(self.lean, self.velocity, target, period);
        self.frame_clock += frame_rate(u) * STEP_SECONDS;
    }
}

/// «Часы движения» и всё, что идёт по ним: наклоны и кадры качающихся объектов. Часы считают шаги
/// по 1/60 секунды: в партии и повторе — шаги мира (`follow_world`), в редакторе вне партии — время
/// кадров (`advance`).
#[derive(Debug, Default)]
pub struct Motion {
    clock_steps: f64,
    integrated_steps: u64,
    states: Vec<Option<SwayState>>,
    /// Качающиеся объекты, у которых наклона ещё нет, — на время одного `update`.
    fresh: Vec<SwayObject>,
    pass: u32,
    /// Мир собран заново: на ближайшем `update` картинка и высота объекта сверяются с прежними.
    compare_look: bool,
    /// «Ветер и частицы» → «Частицы»: источники и частицы идут по тем же часам.
    particles: Particles,
    /// Часы, какими их видел последний [`Motion::clock_running`], и сколько секунд после их
    /// последнего сдвига они ещё считаются идущими.
    watched_steps: f64,
    running_grace: f64,
}

impl Motion {
    /// Часы, в шагах, — по ним кадры по времени выбирают кадр.
    pub fn clock_steps(&self) -> f64 {
        self.clock_steps
    }

    /// «Видео на объекте», требование 5: идут ли часы — звать раз в кадр, `frame_seconds` — сколько
    /// длился кадр. Идут, если сдвинулись за последние [`CLOCK_GRACE_SECONDS`]; до первого сдвига стоят.
    pub fn clock_running(&mut self, frame_seconds: f64) -> bool {
        if self.clock_steps != self.watched_steps {
            self.watched_steps = self.clock_steps;
            self.running_grace = CLOCK_GRACE_SECONDS;
        } else {
            self.running_grace = (self.running_grace - frame_seconds.max(0.0)).max(0.0);
        }
        self.running_grace > 0.0
    }

    /// Часы одного кадра (требование 17): в партии и повторе — на шаге мира `party_steps`, вне
    /// партии — на `dt_seconds` вперёд.
    pub fn tick(&mut self, party_steps: Option<u64>, dt_seconds: f64) {
        match party_steps {
            Some(steps) => self.follow_world(steps),
            None => self.advance(dt_seconds),
        }
    }

    /// Партия и повтор: часы стоят на шаге мира `steps`. Шаг назад или скачок больше двух секунд
    /// вперёд — качание начинается заново (требование 18).
    fn follow_world(&mut self, steps: u64) {
        let steps = steps as f64;
        if steps < self.clock_steps || steps - self.clock_steps > MAX_JUMP_STEPS {
            self.restart_at(steps);
        } else {
            self.clock_steps = steps;
        }
    }

    /// Редактор вне партии: кадр длился `dt_seconds`, не меньше нуля и не больше 0,1 (требование 17).
    fn advance(&mut self, dt_seconds: f64) {
        let dt = dt_seconds.clamp(0.0, MAX_FRAME_SECONDS);
        if !dt.is_nan() {
            self.clock_steps += dt * STEPS_PER_SECOND;
        }
    }

    /// Забывает все наклоны и кадры: мир собран заново, и каждый качающийся объект начнёт с цели
    /// (требование 16). Часы остаются где были.
    pub fn reset(&mut self) {
        self.restart_at(self.clock_steps);
    }

    /// Мир собран из сцены заново без `reset` (загрузка, показ сцены): на ближайшем `update` другой
    /// объект под прежним номером — с другой картинкой или высотой — узнают и начнут с цели.
    pub fn world_rebuilt(&mut self) {
        self.compare_look = true;
    }

    fn restart_at(&mut self, steps: f64) {
        self.clock_steps = steps;
        self.integrated_steps = steps.floor() as u64;
        self.states.fill(None);
        self.particles.restart(steps);
    }

    /// Доводит частицы до часов — раз в кадр, после `tick`. `emitters` — объекты мира с эффектами
    /// сейчас, `world_exists` — есть ли мир.
    pub fn update_particles(
        &mut self,
        world_exists: bool,
        flat: [f64; 2],
        emitters: impl Iterator<Item = Emitter>,
    ) {
        self.particles
            .update(self.clock_steps, world_exists, flat, emitters);
    }

    /// «Листопад»: непрозрачные точки первых кадров картинок загруженной игры, по номеру картинки.
    pub fn set_opaque_masks(&mut self, masks: Vec<OpaqueMask>) {
        self.particles.set_masks(masks);
    }

    pub fn particles(&self) -> &Particles {
        &self.particles
    }

    /// Доводит наклоны и кадры `objects` до часов шагами по 1/60 секунды; остаток часов ждёт
    /// следующего вызова. `flat_x` — ровный ветер сцены по `x`. Объект, которого нет в `objects`,
    /// забыт; новый — в том числе получивший `sway` впервые, — а также другой под прежним номером (метка
    /// жизни не совпала, а после [`Motion::world_rebuilt`] ещё и картинка или высота) встаёт на цель без
    /// рывка (требование 16). Объект со свойством `sway` держит наклон, пока свойство есть: при `sway`
    /// нуль цель нулевая, и пружина доводит наклон до неё, как до любой другой.
    pub fn update(&mut self, flat_x: f64, objects: impl Iterator<Item = SwayObject>) {
        self.pass = self.pass.wrapping_add(1);
        for object in objects {
            self.touch(object);
        }
        self.compare_look = false;
        let pass = self.pass;
        for slot in &mut self.states {
            if slot.is_some_and(|state| state.touched != pass) {
                *slot = None;
            }
        }
        let due = self.clock_steps.floor() as u64;
        while self.integrated_steps < due {
            self.integrated_steps += 1;
            let t = self.integrated_steps as f64 / STEPS_PER_SECOND;
            for state in self.states.iter_mut().flatten() {
                state.step(flat_x, t);
            }
        }
        let now = self.clock_steps / STEPS_PER_SECOND;
        for object in self.fresh.drain(..) {
            let u = wind_at(flat_x, object.x, now);
            self.states[object.id as usize] = Some(SwayState {
                object,
                touched: pass,
                lean: lean_target(object.sway, u, object.height),
                velocity: 0.0,
                frame_clock: 0.0,
            });
        }
    }

    fn touch(&mut self, object: SwayObject) {
        let index = object.id as usize;
        if self.states.len() <= index {
            self.states.resize(index + 1, None);
        }
        match &mut self.states[index] {
            Some(state) if state.object.is_same_object(&object, self.compare_look) => {
                state.object = object;
                state.touched = self.pass;
            }
            slot => {
                *slot = None;
                self.fresh.push(object);
            }
        }
    }

    fn state(&self, id: u32, generation: u32) -> Option<&SwayState> {
        self.states
            .get(id as usize)?
            .as_ref()
            .filter(|state| state.object.generation == generation)
    }

    /// Наклон верха рисунка объекта в клетках; 0 у объекта, который не качается.
    pub fn lean(&self, id: u32, generation: u32) -> f64 {
        self.state(id, generation).map_or(0.0, |state| state.lean)
    }

    /// Часы кадров качающегося объекта в секундах; `None` у объекта, который не качается, и у того,
    /// у кого `sway` нуль, — его кадры идут по общим часам (требование 22).
    pub fn frame_clock(&self, id: u32, generation: u32) -> Option<f64> {
        self.state(id, generation)
            .filter(|state| state.object.sway > 0.0)
            .map(|state| state.frame_clock)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn grass(id: u32) -> SwayObject {
        SwayObject {
            id,
            generation: 0,
            image: Some(0),
            sway: 0.3,
            x: 10.0,
            height: 5.0,
            parallax: 1.0,
        }
    }

    /// «Ветер», требования 6–7: узор порывов едет по сцене со скоростью ровного ветра.
    #[test]
    fn a_gust_travels_with_the_flat_wind() {
        for t in [0.0, 1.3, 7.7, 40.0] {
            assert!(near(gust(1.5, 13.0, t + 2.0), gust(1.5, 10.0, t), 1e-12));
        }
    }

    #[test]
    fn a_gust_is_between_eight_tenths_and_seventeen_tenths_of_the_flat_wind() {
        for step in 0..2000 {
            let g = gust(1.5, f64::from(step) * 0.37, f64::from(step) * 0.11);
            assert!((0.8..=1.7).contains(&g), "{g}");
        }
    }

    #[test]
    fn without_flat_wind_the_air_barely_moves() {
        for step in 0..2000 {
            let u = wind_at(0.0, f64::from(step) * 0.37, f64::from(step) * 0.11);
            assert!(u.abs() <= 0.3 + 1e-12, "{u}");
        }
    }

    #[test]
    fn the_lean_target_matches_the_worked_examples() {
        assert!(near(lean_target(0.3, 2.0, 5.0), 0.84, 1e-9));
        assert!(near(lean_target(0.3, -1.0, 5.0), -0.3, 1e-9));
        assert!(near(lean_target(0.3, 10.0, 2.0), 1.4, 1e-9));
        assert!(near(lean_target(0.25, 1.0, 5.0), 0.25, 1e-12));
    }

    #[test]
    fn the_period_matches_the_worked_examples_and_is_clamped() {
        assert!(near(sway_period(5.04, 1.4), 1.68, 1e-9));
        assert!(near(sway_period(7.5, 1.0), 2.85, 1e-9));
        assert!(near(sway_period(4.74, 0.6), 2.97, 1e-9));
        assert_eq!(sway_period(0.0, 1.0), 0.6);
        assert_eq!(sway_period(40.0, 1.0), 4.0);
        assert_eq!(sway_period(5.0, 0.0), sway_period(5.0, 0.2));
    }

    fn run_spring(target: impl Fn(f64) -> f64, period: f64, seconds: f64) -> Vec<f64> {
        let (mut lean, mut velocity) = (0.0, 0.0);
        let steps = (seconds * 60.0).round() as u32;
        (1..=steps)
            .map(|step| {
                let t = f64::from(step) / 60.0;
                (lean, velocity) = spring_step(lean, velocity, target(t), period);
                lean
            })
            .collect()
    }

    /// «Качание», требование 13: скачок цели с 0 до 1 — перелёт до 1,35–1,40, затем покой.
    #[test]
    fn a_jump_of_the_target_overshoots_then_settles() {
        let period = 1.68;
        let omega = TAU / period;
        let settle = 5.0 / (DAMPING * omega);
        let path = run_spring(|_| 1.0, period, settle);
        let peak = path.iter().copied().fold(f64::MIN, f64::max);
        assert!((1.35..=1.40).contains(&peak), "{peak}");
        assert!(near(*path.last().unwrap(), 1.0, 0.01));
    }

    /// Требование 12: одинаковый итог при одном шаге `dt` 0,05 и при пяти по 0,01 — часы переносят
    /// остаток, а шаг пружины всегда 1/60.
    #[test]
    fn the_same_time_gives_the_same_lean_however_it_is_sliced() {
        let run = |slices: &[f64]| {
            let mut motion = Motion::default();
            motion.update(2.0, std::iter::once(grass(1)));
            for dt in slices {
                motion.advance(*dt);
                motion.update(2.0, std::iter::once(grass(1)));
            }
            motion.lean(1, 0)
        };
        let whole = run(&[0.025; 41]);
        let sliced = run(&[0.005; 205]);
        assert!(near(whole, sliced, 1e-9), "{whole} против {sliced}");
    }

    /// «Видео на объекте», требование 5: часы идут, пока сдвигаются, и стоят, когда замерли.
    #[test]
    fn the_clock_runs_while_it_moves_and_stands_when_it_stops() {
        let mut motion = Motion::default();
        assert!(!motion.clock_running(0.016), "до первого сдвига часы стоят");
        motion.tick(Some(1), 0.0);
        assert!(motion.clock_running(0.016));
        for _ in 0..15 {
            assert!(motion.clock_running(0.005), "запас ещё не кончился");
        }
        assert!(
            !motion.clock_running(0.1),
            "десятая доля секунды без шагов — часы стоят"
        );
        motion.tick(Some(2), 0.0);
        assert!(motion.clock_running(0.016), "сдвинулись — снова идут");
    }

    /// Мир на частом экране шагает не каждый кадр: пауза между шагами часы не останавливает.
    #[test]
    fn a_pause_between_world_steps_does_not_stop_the_clock() {
        let mut motion = Motion::default();
        for step in 1..=20u64 {
            motion.tick(Some(step), 0.0);
            assert!(motion.clock_running(0.007), "шаг {step}");
            assert!(motion.clock_running(0.007), "кадр без шага после {step}");
        }
    }

    #[test]
    fn the_editor_clock_runs_while_frames_move_it_and_stops_without_them() {
        let mut motion = Motion::default();
        motion.tick(None, 0.016);
        assert!(motion.clock_running(0.016));
        motion.tick(None, 0.0);
        assert!(motion.clock_running(0.05), "запас ещё не кончился");
        motion.tick(None, 0.0);
        assert!(!motion.clock_running(0.1));
    }

    #[test]
    fn the_strip_is_eight_bands_of_two_vertices_a_row_from_top_to_bottom() {
        let strip = strip_units();
        assert_eq!(strip.len(), 18);
        for (index, vertex) in strip.iter().enumerate() {
            assert_eq!(vertex[0], (index % 2) as f32, "{index}");
            assert_eq!(vertex[1], (index / 2) as f32 / 8.0, "{index}");
        }
        assert_eq!((strip[0], strip[1]), ([0.0, 0.0], [1.0, 0.0]));
        assert_eq!((strip[16], strip[17]), ([0.0, 1.0], [1.0, 1.0]));
    }

    #[test]
    fn the_bend_matches_the_worked_example_and_the_bottom_edge_stays() {
        let (shift, drop) = bend(5.0, 1.0, 5.0);
        assert!(near(shift, 1.0, 1e-12));
        assert!(near(drop, 5.0 - 24f64.sqrt(), 1e-12));
        assert!(near(drop, 0.101, 1e-3));
        let (shift, drop) = bend(5.0, 1.0, 2.5);
        assert!(near(shift, 0.25, 1e-12));
        assert!(near(drop, 0.013, 1e-3));
        assert_eq!(bend(5.0, 1.0, 0.0), (0.0, 0.0));
        assert_eq!(bend(5.0, 0.0, 3.0), (0.0, 0.0));
    }

    /// «Кадры качающегося объекта», требование 21: 8 кадров по 0,125 секунды.
    #[test]
    fn the_swaying_frames_start_from_each_objects_own_place() {
        assert_eq!(swaying_frame(1, 0.0, 8, 0.125), 4);
        assert_eq!(swaying_frame(2, 0.0, 8, 0.125), 1);
        assert_eq!(swaying_frame(0, 0.0, 8, 0.125), 0);
        // Ветер в точке 1 — кадры с `frame_time`: за секунду проходит весь цикл.
        assert_eq!(swaying_frame(1, 1.0, 8, 0.125), 4);
        assert_eq!(swaying_frame(1, 0.125, 8, 0.125), 5);
    }

    #[test]
    fn the_frames_run_faster_in_a_gust_but_never_more_than_twice() {
        assert_eq!(frame_rate(0.0), 0.5);
        assert!(near(frame_rate(1.0), 1.0, 1e-12));
        assert!(near(frame_rate(0.3), 0.65, 1e-12));
        assert_eq!(frame_rate(5.0), 2.0);
        assert_eq!(frame_rate(-5.0), 2.0);
    }

    #[test]
    fn a_new_object_starts_on_its_target_without_a_jerk() {
        let mut motion = Motion::default();
        motion.advance(0.05);
        motion.update(1.5, std::iter::once(grass(3)));
        let u = wind_at(1.5, 10.0, motion.clock_steps() / 60.0);
        assert!(near(motion.lean(3, 0), lean_target(0.3, u, 5.0), 1e-12));
        assert_eq!(motion.frame_clock(3, 0), Some(0.0));
        assert_eq!(motion.lean(4, 0), 0.0);
        assert_eq!(motion.frame_clock(4, 0), None);
    }

    #[test]
    fn a_deleted_object_forgets_its_lean_and_a_new_one_in_its_slot_starts_over() {
        let mut motion = Motion::default();
        motion.update(1.5, std::iter::once(grass(2)));
        motion.advance(0.1);
        motion.update(1.5, std::iter::once(grass(2)));
        assert!(motion.frame_clock(2, 0).unwrap() > 0.0);

        motion.update(1.5, std::iter::empty());
        assert_eq!(motion.frame_clock(2, 0), None);

        let mut reborn = grass(2);
        reborn.generation = 1;
        motion.update(1.5, std::iter::once(reborn));
        assert_eq!(motion.frame_clock(2, 0), None, "поколение другое");
        assert_eq!(motion.frame_clock(2, 1), Some(0.0));
    }

    /// Требование 16: мир собран заново, номера раздали по порядку и метки у всех 0 — другой объект
    /// под прежним номером начинает с цели, а тот же объект сохраняет наклон.
    #[test]
    fn another_object_under_a_known_number_starts_over_and_the_same_one_keeps_its_lean() {
        let birch = SwayObject {
            id: 1,
            image: Some(1),
            height: 7.5,
            x: 30.0,
            ..grass(0)
        };
        let mut motion = Motion::default();
        for _ in 0..30 {
            motion.advance(0.1);
            motion.update(1.5, [grass(0), birch].into_iter());
        }
        let (grass_lean, birch_lean) = (motion.lean(0, 0), motion.lean(1, 0));
        let now = motion.clock_steps() / 60.0;
        let birch_target = lean_target(0.3, wind_at(1.5, 30.0, now), 7.5);
        assert!(
            (grass_lean - birch_target).abs() > 1e-3,
            "иначе тест ничего не различает: {grass_lean} и {birch_target}"
        );

        motion.world_rebuilt();
        motion.update(1.5, [grass(0), birch].into_iter());
        assert_eq!(
            (motion.lean(0, 0), motion.lean(1, 0)),
            (grass_lean, birch_lean)
        );

        motion.world_rebuilt();
        motion.update(1.5, std::iter::once(SwayObject { id: 0, ..birch }));
        assert!(
            near(motion.lean(0, 0), birch_target, 1e-12),
            "{}",
            motion.lean(0, 0)
        );
        assert_eq!(motion.frame_clock(0, 0), Some(0.0));
        assert_eq!(motion.frame_clock(1, 0), None, "старый номер забыт");
    }

    /// Кадры партии: часы мира на следующем шаге, затем `update`; возвращает наклон объекта после
    /// каждого кадра.
    fn run_frames(
        motion: &mut Motion,
        step: &mut u64,
        object: SwayObject,
        frames: u32,
    ) -> Vec<f64> {
        (0..frames)
            .map(|_| {
                *step += 1;
                motion.follow_world(*step);
                motion.update(2.0, std::iter::once(object));
                motion.lean(object.id, object.generation)
            })
            .collect()
    }

    /// Ни одного скачка за кадр: у цели порядка 0,6–0,9 пружина за 1/60 секунды уходит на сотые доли.
    fn assert_no_jump(before: f64, leans: &[f64]) {
        let mut previous = before;
        for (frame, lean) in leans.iter().enumerate() {
            assert!(
                (lean - previous).abs() < 0.1,
                "скачок на кадре {frame}: {previous} -> {lean}"
            );
            previous = *lean;
        }
    }

    /// «Крайние случаи»: правило меняет `sway` в партии — 0,3 → 0 → 0,3, и оба перехода идут пружиной.
    #[test]
    fn a_sway_changed_in_a_party_is_followed_by_the_spring_both_ways() {
        let mut motion = Motion::default();
        let mut step = 0;
        let settled = run_frames(&mut motion, &mut step, grass(1), 120);
        let before = *settled.last().unwrap();
        assert!(before.abs() > 0.3, "{before}");

        let straight = SwayObject {
            sway: 0.0,
            ..grass(1)
        };
        let leans = run_frames(&mut motion, &mut step, straight, 600);
        assert!(leans[0] != 0.0, "наклон не пропал за кадр");
        assert_no_jump(before, &leans);
        assert!(leans.last().unwrap().abs() < 1e-3, "выпрямился");
        assert_eq!(motion.frame_clock(1, 0), None, "кадры — по общим часам");

        let leans = run_frames(&mut motion, &mut step, grass(1), 120);
        assert!(leans[0].abs() < 0.05, "растёт из нуля: {}", leans[0]);
        assert_no_jump(0.0, &leans);
        assert!(leans.iter().any(|lean| lean.abs() > 0.3), "дорос до цели");
        assert!(motion.frame_clock(1, 0).is_some());
    }

    /// «Крайние случаи»: у объекта `sway` был нуль с начала — правило ставит 0,3, наклон растёт
    /// пружиной из нуля, а не встаёт на цель за кадр.
    #[test]
    fn an_object_with_zero_sway_from_the_start_grows_its_lean_by_the_spring() {
        let mut motion = Motion::default();
        let mut step = 0;
        let straight = SwayObject {
            sway: 0.0,
            ..grass(1)
        };
        let still = run_frames(&mut motion, &mut step, straight, 30);
        assert!(still.iter().all(|lean| *lean == 0.0));

        let leans = run_frames(&mut motion, &mut step, grass(1), 120);
        assert!(leans[0].abs() < 0.05, "растёт из нуля: {}", leans[0]);
        assert_no_jump(0.0, &leans);
        assert!(leans.iter().any(|lean| lean.abs() > 0.3), "дорос до цели");
    }

    /// Требование 16: правило меняет `image` и `size` — объект тот же, наклон не начинается заново.
    #[test]
    fn a_new_picture_or_height_in_a_party_does_not_restart_the_lean() {
        let mut motion = Motion::default();
        let mut step = 0;
        let settled = run_frames(&mut motion, &mut step, grass(1), 60);
        let before = *settled.last().unwrap();
        let changed = SwayObject {
            image: Some(1),
            height: 7.5,
            ..grass(1)
        };
        let leans = run_frames(&mut motion, &mut step, changed, 1);
        let target = lean_target(0.3, wind_at(2.0, 10.0, motion.clock_steps() / 60.0), 7.5);
        assert!(
            (before - target).abs() > 1e-3,
            "иначе тест ничего не различает: {before} и {target}"
        );
        assert!(
            (leans[0] - before).abs() < (leans[0] - target).abs(),
            "наклон идёт дальше, а не встаёт на цель: {before} -> {} (цель {target})",
            leans[0]
        );
    }

    /// Требование 16: после сборки мира заново другая картинка или высота под тем же номером — другой
    /// объект, он начинает с цели; признак снимается после первого обновления.
    #[test]
    fn after_a_rebuild_another_picture_under_the_same_number_starts_on_its_target() {
        let mut motion = Motion::default();
        let mut step = 0;
        let settled = run_frames(&mut motion, &mut step, grass(1), 60);
        let before = *settled.last().unwrap();
        let other = SwayObject {
            image: Some(1),
            ..grass(1)
        };

        motion.world_rebuilt();
        run_frames(&mut motion, &mut step, grass(1), 1);
        let kept = motion.lean(1, 0);
        assert!(
            (kept - before).abs() < 0.1,
            "тот же объект: {before} -> {kept}"
        );

        motion.world_rebuilt();
        step += 1;
        motion.follow_world(step);
        motion.update(2.0, std::iter::once(other));
        let target = lean_target(0.3, wind_at(2.0, 10.0, motion.clock_steps() / 60.0), 5.0);
        assert!(
            (kept - target).abs() > 1e-3,
            "иначе тест ничего не различает: {kept} и {target}"
        );
        assert!(
            near(motion.lean(1, 0), target, 1e-12),
            "{}",
            motion.lean(1, 0)
        );

        let lagging = *run_frames(&mut motion, &mut step, other, 30)
            .last()
            .unwrap();
        let leans = run_frames(&mut motion, &mut step, grass(1), 1);
        let target = lean_target(0.3, wind_at(2.0, 10.0, motion.clock_steps() / 60.0), 5.0);
        assert!(
            (lagging - target).abs() > 1e-3,
            "иначе тест ничего не различает: {lagging} и {target}"
        );
        assert!(
            (leans[0] - lagging).abs() < (leans[0] - target).abs(),
            "признак снят: картинка снова другая, а наклон идёт дальше"
        );
    }

    #[test]
    fn an_object_that_never_swayed_gets_no_lean_from_a_zero_sway() {
        let mut motion = Motion::default();
        motion.advance(0.1);
        motion.update(
            2.0,
            std::iter::once(SwayObject {
                sway: 0.0,
                ..grass(1)
            }),
        );
        assert_eq!(motion.lean(1, 0), 0.0);
        assert_eq!(motion.frame_clock(1, 0), None);
    }

    #[test]
    fn the_clock_jumping_back_or_far_ahead_starts_every_lean_over() {
        for jump in [10, 400] {
            let mut motion = Motion::default();
            motion.follow_world(100);
            motion.update(1.5, std::iter::once(grass(1)));
            motion.follow_world(106);
            motion.update(1.5, std::iter::once(grass(1)));
            assert!(motion.frame_clock(1, 0).unwrap() > 0.0);

            motion.follow_world(if jump == 10 { 50 } else { 106 + jump });
            motion.update(1.5, std::iter::once(grass(1)));
            let at = motion.clock_steps() / 60.0;
            assert_eq!(motion.frame_clock(1, 0), Some(0.0), "прыжок {jump}");
            assert!(near(
                motion.lean(1, 0),
                lean_target(0.3, wind_at(1.5, 10.0, at), 5.0),
                1e-12
            ));
        }
    }

    #[test]
    fn a_small_step_forward_keeps_going_and_does_not_restart() {
        let mut motion = Motion::default();
        motion.follow_world(10);
        motion.update(0.0, std::iter::once(grass(1)));
        motion.follow_world(70);
        motion.update(0.0, std::iter::once(grass(1)));
        let clock = motion.frame_clock(1, 0).unwrap();
        assert!((0.5..=0.65 + 1e-9).contains(&clock), "{clock}");
    }

    #[test]
    fn reset_forgets_every_lean_but_keeps_the_clock() {
        let mut motion = Motion::default();
        motion.advance(0.1);
        motion.update(1.5, std::iter::once(grass(1)));
        let clock = motion.clock_steps();
        motion.reset();
        motion.update(1.5, std::iter::empty());
        assert_eq!(motion.frame_clock(1, 0), None);
        assert_eq!(motion.clock_steps(), clock);
    }

    #[test]
    fn the_editor_clock_takes_a_frame_of_at_most_a_tenth_of_a_second() {
        let mut motion = Motion::default();
        motion.advance(5.0);
        assert!(near(motion.clock_steps(), 6.0, 1e-12));
        motion.advance(-1.0);
        motion.advance(f64::NAN);
        assert!(near(motion.clock_steps(), 6.0, 1e-12));
    }

    #[test]
    fn a_stopped_clock_does_not_move_the_frames() {
        let mut motion = Motion::default();
        motion.follow_world(30);
        motion.update(1.5, std::iter::once(grass(1)));
        let before = (motion.lean(1, 0), motion.frame_clock(1, 0));
        for _ in 0..10 {
            motion.follow_world(30);
            motion.update(1.5, std::iter::once(grass(1)));
        }
        assert_eq!(before, (motion.lean(1, 0), motion.frame_clock(1, 0)));
    }
}

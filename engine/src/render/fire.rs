//! «Огонь», «Включение и выключение», «Время и случайность»: огни — по паре «номер и поколение
//! объекта», — их нынешняя сила, дыхание, наклон по ветру пружиной, ход узора, разгорание, угасание,
//! прогрев при сборке мира и своя случайность. Ни видеокарты, ни браузера: `Motion` ведёт часы и зовёт
//! [`Fires::update`] раз в кадр, `atlas` берёт отсюда рисунки пламени и ореола. Числа вида огня — в
//! двух местах: высота, дыхание, ветер, разгорание и ореол — здесь, в начале файла; узор, форму языков
//! и цвет полосами считает видеокарта, и их числа — блоком `FLAME_*` в `rect.wgsl`.

use std::f64::consts::TAU;

use crate::core::fire::Settings;
use crate::core::particles::Rgb;
use crate::core::rng::Rng;

use super::particles::Rect;
use super::wind::{spring_step, wind_at};

const STEPS_PER_SECOND: f64 = 60.0;
const STEP_SECONDS: f64 = 1.0 / STEPS_PER_SECOND;
/// Начальное число счётчика случайности огня, одно и то же в каждом движке.
const SEED: u64 = 0x0F1A_3E5E_ED0F_14E5;

/// Высота пламени — доля высоты прямоугольника огня: `FLOOR + REACH · s`, умноженная на дыхание.
const HEIGHT_FLOOR: f64 = 0.3;
const HEIGHT_REACH: f64 = 0.7;

/// Дыхание — от `BREATH_MIN` до `BREATH_MAX`: сумма двух колебаний с этими периодами (секунды) и долями
/// размаха; доли в сумме дают единицу.
const BREATH_MIN: f64 = 0.8;
const BREATH_MAX: f64 = 1.0;
const BREATH_PERIODS: [f64; 2] = [1.3, 0.5];
const BREATH_SHARES: [f64; 2] = [0.6, 0.4];

/// Цель наклона верха — `высота пламени · clamp(LEAN_PER_WIND · ветер, ±LEAN_LIMIT)`; пружина догоняет
/// её за `LEAN_PERIOD` секунд, и наклон не уходит дальше `LEAN_LIMIT` высоты.
const LEAN_PER_WIND: f64 = 0.15;
const LEAN_LIMIT: f64 = 0.5;
const LEAN_PERIOD: f64 = 0.4;

/// Нынешняя сила догоняет `fire`: за `EASE_SECONDS` остаётся `EASE_REMAINS` разницы. Пламя гаснет,
/// когда сила меньше `GONE_BELOW`.
const EASE_SECONDS: f64 = 0.15;
const EASE_REMAINS: f64 = 1.0 / 3.0;
const GONE_BELOW: f64 = 0.01;

/// Ореол: поперечник — `HALO_WIDTH · ширина + HALO_REACH · высота пламени`; яркость набирает полную
/// силу при нынешней силе `HALO_FULL_AT`.
const HALO_WIDTH: f64 = 1.2;
const HALO_REACH: f64 = 2.5;
const HALO_FULL_AT: f64 = 0.1;
/// Рисунок ореола тает от середины к краю как `1 − плавный шаг расстояния` в этой степени: больше —
/// свет плотнее у пламени. Рисунок движок рисует при сборке атласа (`particle_shapes`).
pub(super) const HALO_FALLOFF_POWER: i32 = 2;

/// Ход узора в секунду — от `SCROLL_CALM` при силе нуль до `SCROLL_RAGE` при единице. За единицу хода
/// узор языков поднимается на `FLAME_LICK_RISE` узлов, мелкий — на `FLAME_RAG_RISE`, а высота языка
/// меняется `FLAME_TONGUE_RATE` раз (`rect.wgsl`).
const SCROLL_CALM: f64 = 0.7;
const SCROLL_RAGE: f64 = 2.0;
/// Ход узора делится на целую часть и дробную: шейдер считает по целой часть решётки целыми числами,
/// поэтому узор не рассыпается за часы. Целая часть возвращается к нулю после стольких клеток.
const SCROLL_WRAP: f64 = 1_048_576.0;
/// Узор каждого огня начинает с места в пределах стольких клеток.
const SCROLL_START: f64 = 4096.0;
/// Зерно узора — целое до `2^SEED_BITS`: шейдер подмешивает его в хеш решётки.
const SEED_BITS: u32 = 20;

/// Огонь на этот кадр: объект мира с записанным прямоугольником и тем, что у него записано про огонь.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FireObject {
    pub id: u32,
    pub generation: u32,
    pub rect: Rect,
    pub settings: Settings,
}

/// Пламя: прямоугольник без сдвига слоя глубины, наклон верха в клетках и то, что шейдер берёт у
/// экземпляра, — сила, зерно узора и ход узора на целую и дробную часть.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Flame {
    pub position: [f64; 2],
    pub size: [f64; 2],
    pub lean: f64,
    pub strength: f64,
    pub color: Rgb,
    pub seed: f32,
    pub scroll_whole: f32,
    pub scroll_fraction: f32,
}

/// Ореол: середина без сдвига слоя глубины, поперечник в клетках, цвет и яркость.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Halo {
    pub center: [f64; 2],
    pub diameter: f64,
    pub color: Rgb,
    pub brightness: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Drawing {
    pub flame: Flame,
    pub halo: Halo,
}

/// Высота пламени при нынешней силе `strength` и дыхании `breath` у прямоугольника высотой `height`.
fn flame_height(height: f64, strength: f64, breath: f64) -> f64 {
    height * (HEIGHT_FLOOR + HEIGHT_REACH * strength) * breath
}

/// Дыхание огня в момент `t` (секунды часов движения) с фазами `phases`, от 0,8 до 1.
fn breath(phases: [f64; 2], t: f64) -> f64 {
    let wave: f64 = [0, 1]
        .map(|i| BREATH_SHARES[i] * (TAU * t / BREATH_PERIODS[i] + phases[i]).sin())
        .iter()
        .sum();
    BREATH_MIN + (BREATH_MAX - BREATH_MIN) * (0.5 + 0.5 * wave)
}

/// Цель наклона верха пламени высотой `height` при ветре `wind` в месте огня, в клетках.
fn lean_target(height: f64, wind: f64) -> f64 {
    height * (LEAN_PER_WIND * wind).clamp(-LEAN_LIMIT, LEAN_LIMIT)
}

/// Поперечник ореола у пламени шириной `width` и высотой `flame`.
fn halo_diameter(width: f64, flame: f64) -> f64 {
    HALO_WIDTH * width + HALO_REACH * flame
}

/// Яркость ореола: `fire_glow` в такт дыханию, и гаснет вместе с огнём.
fn halo_brightness(glow: f64, breath: f64, strength: f64) -> f64 {
    glow * breath * (strength / HALO_FULL_AT).min(1.0)
}

/// Клеток в секунду, на которые поднимается основной слой узора при силе `strength`.
fn scroll_speed(strength: f64) -> f64 {
    SCROLL_CALM + (SCROLL_RAGE - SCROLL_CALM) * strength
}

#[derive(Debug)]
struct Fire {
    generation: u32,
    rect: Rect,
    settings: Settings,
    strength: f64,
    breath: f64,
    phases: [f64; 2],
    seed: u32,
    scroll: f64,
    lean: f64,
    velocity: f64,
    touched: u32,
}

impl Fire {
    fn follow(&mut self, object: &FireObject, pass: u32) {
        self.rect = object.rect;
        self.settings = object.settings;
        self.touched = pass;
    }

    fn height(&self) -> f64 {
        flame_height(self.rect.1[1], self.strength, self.breath)
    }

    fn wind_x(&self) -> f64 {
        self.rect.0[0] + self.rect.1[0] / 2.0
    }

    /// Один шаг `Δ` = 1/60 секунды в момент `t` при ровном ветре `flat_x`.
    fn step(&mut self, flat_x: f64, t: f64) {
        let ease = 1.0 - EASE_REMAINS.powf(STEP_SECONDS / EASE_SECONDS);
        self.strength += (self.settings.fire - self.strength) * ease;
        self.breath = breath(self.phases, t);
        self.scroll += scroll_speed(self.strength) * STEP_SECONDS;
        let target = lean_target(self.height(), wind_at(flat_x, self.wind_x(), t));
        (self.lean, self.velocity) = spring_step(self.lean, self.velocity, target, LEAN_PERIOD);
    }

    /// Огонь погас: ему не гореть и сила ушла ниже порога.
    fn is_out(&self) -> bool {
        self.settings.fire <= 0.0 && self.strength < GONE_BELOW
    }

    fn drawing(&self) -> Drawing {
        let ([x, y], [width, height]) = self.rect;
        let flame = self.height();
        let limit = LEAN_LIMIT * flame;
        let lean = self.lean.clamp(-limit, limit);
        let whole = self.scroll.floor();
        Drawing {
            flame: Flame {
                position: [x, y + height - flame],
                size: [width, flame],
                lean,
                strength: self.strength,
                color: self.settings.color,
                seed: self.seed as f32,
                scroll_whole: whole.rem_euclid(SCROLL_WRAP) as f32,
                scroll_fraction: (self.scroll - whole) as f32,
            },
            halo: Halo {
                center: [x + width / 2.0 + lean / 2.0, y + height - flame / 2.0],
                diameter: halo_diameter(width, flame),
                color: self.settings.color,
                brightness: halo_brightness(self.settings.glow, self.breath, self.strength),
            },
        }
    }
}

/// Все огни мира.
#[derive(Debug)]
pub struct Fires {
    rng: Rng,
    slots: Vec<Option<Fire>>,
    /// Объекты, у которых огня ещё нет, а он горит, — на время одного `update`.
    fresh: Vec<FireObject>,
    integrated_steps: u64,
    /// Ближайшая сборка мира застанет огни — они горят с первого кадра.
    warm: bool,
    /// Мир собран из файлов заново без `restart`: на ближайшем `update` огни узнают свои объекты по
    /// прямоугольнику, а не по номеру.
    rebuilt: bool,
    pass: u32,
}

impl Default for Fires {
    fn default() -> Fires {
        Fires {
            rng: Rng::new(SEED),
            slots: Vec::new(),
            fresh: Vec::new(),
            integrated_steps: 0,
            warm: true,
            rebuilt: false,
            pass: 0,
        }
    }
}

impl Fires {
    /// Часы ушли назад или далеко вперёд, партия началась заново — все огни забыты, найденные на
    /// ближайшем кадре горят с первого кадра. Счётчик случайности не трогается.
    pub fn restart(&mut self, steps: f64) {
        self.slots.clear();
        self.integrated_steps = steps.floor() as u64;
        self.warm = true;
        self.rebuilt = false;
    }

    /// Мир собран из файлов заново без `restart` — редактор перечитал сцену: номера объектов розданы
    /// по порядку, и на ближайшем `update` огонь узнаёт свой объект по прямоугольнику.
    pub fn world_rebuilt(&mut self) {
        self.rebuilt = true;
    }

    /// Доводит огни до часов `clock_steps` шагами по 1/60 секунды; остаток часов ждёт следующего
    /// вызова. `flat` — ровный ветер сцены, `objects` — объекты мира с записанным прямоугольником.
    /// Огонь объекта, которого нет среди `objects`, пропадает сразу. Объект с `fire` больше нуля без
    /// огня разгорается с нуля; если мир только что собран (`restart` и первая сборка), то горит с
    /// первого кадра. Огонь, у которого `fire` стал нулём или снят, гаснет сам. `world_exists` — есть ли
    /// мир: прогрев ждёт его.
    pub fn update(
        &mut self,
        clock_steps: f64,
        world_exists: bool,
        flat: [f64; 2],
        objects: impl Iterator<Item = FireObject>,
    ) {
        self.pass = self.pass.wrapping_add(1);
        let pass = self.pass;
        let objects: Vec<FireObject> = objects.collect();
        if std::mem::take(&mut self.rebuilt) {
            self.rematch(&objects);
        }
        for object in objects {
            let index = object.id as usize;
            if self.slots.len() <= index {
                self.slots.resize_with(index + 1, || None);
            }
            match &mut self.slots[index] {
                Some(fire) if fire.generation == object.generation => fire.follow(&object, pass),
                slot => {
                    *slot = None;
                    if object.settings.fire > 0.0 {
                        self.fresh.push(object);
                    }
                }
            }
        }
        for slot in &mut self.slots {
            if slot.as_ref().is_some_and(|fire| fire.touched != pass) {
                *slot = None;
            }
        }

        let due = clock_steps.floor() as u64;
        while self.integrated_steps < due {
            self.integrated_steps += 1;
            let t = self.integrated_steps as f64 / STEPS_PER_SECOND;
            for fire in self.slots.iter_mut().flatten() {
                fire.step(flat[0], t);
            }
        }
        for slot in &mut self.slots {
            if slot.as_ref().is_some_and(Fire::is_out) {
                *slot = None;
            }
        }

        let warm = self.warm && world_exists;
        let now = clock_steps / STEPS_PER_SECOND;
        for index in 0..self.fresh.len() {
            let object = self.fresh[index];
            let fire = self.ignite(&object, warm, flat[0], now, pass);
            self.slots[object.id as usize] = Some(fire);
        }
        self.fresh.clear();
        if world_exists {
            self.warm = false;
        }
    }

    /// Мир собран из файлов заново: номера розданы по порядку, поколения у всех нуль. Огонь переходит к
    /// объекту с тем же прямоугольником — сначала под тем же номером, потом под любым, если у объекта
    /// есть `fire`: удалили объект раньше огня в сцене, номера сдвинулись, а огонь горит дальше без
    /// перезапуска (требование 17), и соседний объект под его прежним номером чужого пламени не получает.
    /// Последним объект с `fire` и другим прямоугольником под тем же номером — его передвинули —
    /// сохраняет огонь; этот проход идёт после поиска по прямоугольнику, иначе новый огонь, вставленный
    /// раньше горящего, забрал бы его пламя.
    fn rematch(&mut self, objects: &[FireObject]) {
        let mut previous = std::mem::take(&mut self.slots);
        let mut found: Vec<Option<Fire>> = objects.iter().map(|_| None).collect();
        for (object, slot) in objects.iter().zip(&mut found) {
            if let Some(old) = previous.get_mut(object.id as usize)
                && old.as_ref().is_some_and(|fire| fire.rect == object.rect)
            {
                *slot = old.take();
            }
        }
        for (object, slot) in objects.iter().zip(&mut found) {
            if slot.is_none()
                && object.settings.fire > 0.0
                && let Some(old) = previous
                    .iter_mut()
                    .find(|old| old.as_ref().is_some_and(|fire| fire.rect == object.rect))
            {
                *slot = old.take();
            }
        }
        for (object, slot) in objects.iter().zip(&mut found) {
            if slot.is_none()
                && object.settings.fire > 0.0
                && let Some(old) = previous.get_mut(object.id as usize)
            {
                *slot = old.take();
            }
        }
        for (object, fire) in objects.iter().zip(found) {
            if let Some(mut fire) = fire {
                fire.generation = object.generation;
                let index = object.id as usize;
                if self.slots.len() <= index {
                    self.slots.resize_with(index + 1, || None);
                }
                self.slots[index] = Some(fire);
            }
        }
    }

    fn ignite(
        &mut self,
        object: &FireObject,
        warm: bool,
        flat_x: f64,
        now: f64,
        pass: u32,
    ) -> Fire {
        let phases = [self.rng.next_unit() * TAU, self.rng.next_unit() * TAU];
        let seed = (self.rng.next_u64() >> (64 - SEED_BITS)) as u32;
        let scroll = self.rng.next_unit() * SCROLL_START;
        let mut fire = Fire {
            generation: object.generation,
            rect: object.rect,
            settings: object.settings,
            strength: if warm { object.settings.fire } else { 0.0 },
            breath: breath(phases, now),
            phases,
            seed,
            scroll,
            lean: 0.0,
            velocity: 0.0,
            touched: pass,
        };
        fire.lean = lean_target(fire.height(), wind_at(flat_x, fire.wind_x(), now));
        fire
    }

    fn fire(&self, id: u32, generation: u32) -> Option<&Fire> {
        self.slots
            .get(id as usize)?
            .as_ref()
            .filter(|fire| fire.generation == generation)
    }

    /// Горит ли огонь у объекта: пока он есть, объект стоит в порядке рисования.
    pub fn is_burning(&self, id: u32, generation: u32) -> bool {
        self.fire(id, generation).is_some()
    }

    /// Нынешняя сила огня объекта; `None` — огня нет.
    pub fn strength(&self, id: u32, generation: u32) -> Option<f64> {
        self.fire(id, generation).map(|fire| fire.strength)
    }

    /// Что рисовать у объекта: ореол и пламя; `None` — огня нет.
    pub fn drawing(&self, id: u32, generation: u32) -> Option<Drawing> {
        self.fire(id, generation).map(Fire::drawing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn settings(fire: f64) -> Settings {
        Settings {
            fire,
            color: [1.0, 0.5, 0.1],
            glow: 0.5,
        }
    }

    fn object(id: u32, fire: f64) -> FireObject {
        FireObject {
            id,
            generation: 0,
            rect: ([10.0 + f64::from(id) * 4.0, 5.0], [2.0, 1.0]),
            settings: settings(fire),
        }
    }

    /// Мир с огнями `objects`, собранный на часах 0: горят с первого кадра.
    fn lit(objects: &[FireObject]) -> (Fires, f64) {
        let mut fires = Fires::default();
        fires.update(0.0, true, [0.0, 0.0], objects.iter().copied());
        (fires, 0.0)
    }

    /// Часы идут на `seconds` секунд вперёд, кадр за кадром по 1/60 секунды.
    fn run(
        fires: &mut Fires,
        clock: &mut f64,
        flat: [f64; 2],
        seconds: f64,
        objects: &[FireObject],
    ) {
        for _ in 0..(seconds * 60.0).round() as u32 {
            *clock += 1.0;
            fires.update(*clock, true, flat, objects.iter().copied());
        }
    }

    #[test]
    fn the_flame_is_as_tall_as_the_rectangle_at_full_strength_on_a_full_breath() {
        assert_eq!(flame_height(2.0, 1.0, 1.0), 2.0);
        assert!(flame_height(2.0, 0.2, 1.0) < 1.0);
        assert!(near(flame_height(2.0, 0.2, 1.0), 0.88, 1e-12));
        assert!(near(flame_height(2.0, 0.0, 1.0), 0.6, 1e-12));
    }

    #[test]
    fn the_breath_stays_between_eight_tenths_and_one_and_uses_the_whole_range() {
        let (mut low, mut high) = (f64::MAX, f64::MIN);
        for step in 0..20_000 {
            let b = breath([0.7, 2.9], f64::from(step) / 60.0);
            assert!((0.8 - 1e-12..=1.0 + 1e-12).contains(&b), "{b}");
            low = low.min(b);
            high = high.max(b);
        }
        assert!(low < 0.83 && high > 0.97, "{low} {high}");
    }

    #[test]
    fn two_fires_with_different_numbers_do_not_breathe_in_step() {
        let (mut fires, mut clock) = lit(&[object(0, 1.0), object(1, 1.0)]);
        let mut differing = 0;
        for _ in 0..600 {
            run(
                &mut fires,
                &mut clock,
                [0.0; 2],
                0.05,
                &[object(0, 1.0), object(1, 1.0)],
            );
            let (a, b) = (
                fires.fire(0, 0).unwrap().breath,
                fires.fire(1, 0).unwrap().breath,
            );
            differing += usize::from((a - b).abs() > 0.01);
        }
        assert!(differing > 300, "{differing}");
        let seed = |id| fires.fire(id, 0).unwrap().seed;
        assert_ne!(seed(0), seed(1), "узоры у огней свои");
    }

    #[test]
    fn a_fire_from_the_scene_burns_at_full_strength_on_the_first_frame() {
        let (fires, _) = lit(&[object(0, 0.5)]);
        assert_eq!(fires.strength(0, 0), Some(0.5));
        assert!(fires.is_burning(0, 0));
    }

    #[test]
    fn a_fire_lit_later_rises_from_nothing_and_is_nearly_full_in_half_a_second() {
        let (mut fires, mut clock) = lit(&[]);
        let objects = [object(0, 0.8)];
        fires.update(clock, true, [0.0; 2], objects.iter().copied());
        assert_eq!(fires.strength(0, 0), Some(0.0), "начинает с нуля");
        let start = fires.drawing(0, 0).unwrap();
        assert_eq!(start.flame.strength, 0.0);
        assert_eq!(start.halo.brightness, 0.0, "ореол начинает тёмным");
        run(&mut fires, &mut clock, [0.0; 2], 0.5, &objects);
        let strength = fires.strength(0, 0).unwrap();
        assert!(strength > 0.95 * 0.8 && strength < 0.8, "{strength}");
    }

    #[test]
    fn the_strength_loses_two_thirds_of_the_gap_in_a_fifth_of_a_second_or_so() {
        let (mut fires, mut clock) = lit(&[object(0, 1.0)]);
        let dimmed = [object(0, 0.4)];
        run(&mut fires, &mut clock, [0.0; 2], 0.15, &dimmed);
        let strength = fires.strength(0, 0).unwrap();
        assert!(near(strength, 0.4 + 0.6 / 3.0, 1e-9), "{strength}");
    }

    #[test]
    fn a_fire_set_to_zero_is_gone_within_a_second_and_a_deleted_object_at_once() {
        let (mut fires, mut clock) = lit(&[object(0, 1.0), object(1, 1.0)]);
        let out = [object(0, 0.0)];
        run(&mut fires, &mut clock, [0.0; 2], 0.2, &out);
        assert!(fires.is_burning(0, 0), "сила уходит плавно");
        assert!(!fires.is_burning(1, 0), "удалённый объект гаснет сразу");
        run(&mut fires, &mut clock, [0.0; 2], 0.8, &out);
        assert!(!fires.is_burning(0, 0));
        assert_eq!(fires.slots.iter().flatten().count(), 0);
    }

    #[test]
    fn a_fire_whose_property_is_taken_off_fades_the_same_way_and_can_be_lit_again() {
        let (mut fires, mut clock) = lit(&[object(0, 1.0)]);
        let bare = [object(0, 0.0)];
        run(&mut fires, &mut clock, [0.0; 2], 0.1, &bare);
        let fading = fires.strength(0, 0).unwrap();
        assert!(fading > 0.01 && fading < 1.0, "{fading}");
        run(&mut fires, &mut clock, [0.0; 2], 0.1, &[object(0, 1.0)]);
        assert!(fires.strength(0, 0).unwrap() > fading, "снова разгорается");
    }

    #[test]
    fn another_object_under_the_same_number_does_not_inherit_the_fire() {
        let (mut fires, clock) = lit(&[object(0, 1.0)]);
        let mut newcomer = object(0, 1.0);
        newcomer.generation = 1;
        fires.update(clock, true, [0.0; 2], [newcomer].into_iter());
        assert!(!fires.is_burning(0, 0));
        assert_eq!(fires.strength(0, 1), Some(0.0), "новый разгорается с нуля");
    }

    /// Требование 17: редактор перечитал сцену, из которой удалили объект раньше огня, — горн под новым
    /// номером горит дальше, сосед под его прежним номером чужого пламени не получает, а огонь,
    /// которого до перечитывания не было, разгорается с нуля.
    #[test]
    fn after_a_reread_the_fire_follows_its_rectangle_to_a_new_number() {
        let wall = object(0, 0.0);
        let mut forge = object(1, 1.0);
        let (mut fires, mut clock) = lit(&[wall, forge]);
        run(&mut fires, &mut clock, [0.0; 2], 0.5, &[wall, forge]);
        forge.id = 0;
        let mut neighbour = object(2, 0.0);
        neighbour.id = 1;
        let mut torch = object(3, 0.6);
        torch.id = 2;
        fires.world_rebuilt();
        fires.update(clock, true, [0.0; 2], [forge, neighbour, torch].into_iter());
        assert_eq!(fires.strength(0, 0), Some(1.0));
        assert!(!fires.is_burning(1, 0));
        assert_eq!(fires.strength(2, 0), Some(0.0));
    }

    /// Требование 17: новый огонь вставили в сцену раньше горящего — горящий сдвинулся на номер дальше
    /// и горит дальше, а новый разгорается с нуля, а не забирает его пламя.
    #[test]
    fn after_a_reread_a_fire_inserted_before_a_burning_one_rises_from_nothing() {
        let mut forge = object(0, 1.0);
        let (mut fires, mut clock) = lit(&[forge]);
        run(&mut fires, &mut clock, [0.0; 2], 0.5, &[forge]);
        let mut torch = object(5, 0.6);
        torch.id = 0;
        forge.id = 1;
        fires.world_rebuilt();
        fires.update(clock, true, [0.0; 2], [torch, forge].into_iter());
        assert_eq!(fires.strength(0, 0), Some(0.0));
        assert_eq!(fires.strength(1, 0), Some(1.0));
    }

    /// Требование 18: удалённый огонь гаснет сразу, даже если после перечитывания объект без огня под
    /// другим номером стоит в его прямоугольнике.
    #[test]
    fn after_a_reread_an_object_without_fire_in_the_same_rectangle_does_not_take_a_deleted_fire() {
        let wall = object(0, 0.0);
        let forge = object(1, 1.0);
        let mut plate = forge;
        plate.id = 2;
        plate.settings = settings(0.0);
        let (mut fires, clock) = lit(&[wall, forge, plate]);
        plate.id = 0;
        fires.world_rebuilt();
        fires.update(clock, true, [0.0; 2], [plate].into_iter());
        assert!(!fires.is_burning(0, 0));
        assert_eq!(fires.slots.iter().flatten().count(), 0);
    }

    /// Требование 17: огонь, который передвинули в редакторе, после перечитывания горит дальше.
    #[test]
    fn after_a_reread_a_moved_fire_keeps_burning() {
        let mut forge = object(0, 1.0);
        let (mut fires, clock) = lit(&[forge]);
        forge.rect.0 = [30.0, 6.0];
        fires.world_rebuilt();
        fires.update(clock, true, [0.0; 2], [forge].into_iter());
        assert_eq!(fires.strength(0, 0), Some(1.0));
    }

    #[test]
    fn after_a_restart_the_fires_found_burn_at_once_again() {
        let (mut fires, mut clock) = lit(&[]);
        run(&mut fires, &mut clock, [0.0; 2], 0.1, &[object(0, 0.7)]);
        assert!(fires.strength(0, 0).unwrap() < 0.7);
        fires.restart(clock);
        assert!(!fires.is_burning(0, 0));
        fires.update(clock, true, [0.0; 2], [object(0, 0.7)].into_iter());
        assert_eq!(fires.strength(0, 0), Some(0.7));
    }

    #[test]
    fn a_world_that_does_not_exist_yet_keeps_the_warm_up_waiting() {
        let mut fires = Fires::default();
        fires.update(0.0, false, [0.0; 2], std::iter::empty());
        fires.update(0.0, true, [0.0; 2], [object(0, 0.7)].into_iter());
        assert_eq!(fires.strength(0, 0), Some(0.7));
    }

    #[test]
    fn with_the_clock_standing_nothing_changes() {
        let (mut fires, mut clock) = lit(&[object(0, 1.0)]);
        run(&mut fires, &mut clock, [2.0, 0.0], 1.0, &[object(0, 1.0)]);
        let before = fires.drawing(0, 0).unwrap();
        for _ in 0..30 {
            fires.update(clock, true, [2.0, 0.0], [object(0, 0.2)].into_iter());
        }
        assert_eq!(fires.drawing(0, 0).unwrap(), before);
    }

    #[test]
    fn a_wind_of_one_and_a_half_leans_the_flame_about_a_quarter_of_its_height_and_never_past_half()
    {
        let objects = [object(0, 1.0)];
        let (mut fires, mut clock) = lit(&objects);
        run(&mut fires, &mut clock, [1.5, 0.0], 1.0, &objects);
        let flame = fires.drawing(0, 0).unwrap().flame;
        let share = flame.lean / flame.size[1];
        assert!((0.15..0.4).contains(&share), "{share}");
        for _ in 0..600 {
            run(&mut fires, &mut clock, [6.0, 0.0], 0.05, &objects);
            let flame = fires.drawing(0, 0).unwrap().flame;
            assert!(flame.lean.abs() <= 0.5 * flame.size[1] + 1e-9);
        }
    }

    #[test]
    fn a_wind_the_other_way_leans_the_other_way_and_a_wind_down_does_not_lean_it() {
        let objects = [object(0, 1.0)];
        let (mut left, mut clock) = lit(&objects);
        run(&mut left, &mut clock, [-1.5, 0.0], 1.0, &objects);
        assert!(left.drawing(0, 0).unwrap().flame.lean < -0.1);

        let (mut down, mut clock) = lit(&objects);
        run(&mut down, &mut clock, [0.0, 3.0], 1.0, &objects);
        let flame = down.drawing(0, 0).unwrap().flame;
        assert!(flame.lean.abs() < 0.07 * flame.size[1], "{}", flame.lean);
    }

    #[test]
    fn editing_the_colour_or_the_glow_does_not_restart_the_fire() {
        let (mut fires, mut clock) = lit(&[object(0, 0.6)]);
        run(&mut fires, &mut clock, [0.0; 2], 0.3, &[object(0, 0.6)]);
        let seed = fires.fire(0, 0).unwrap().seed;
        let mut edited = object(0, 0.6);
        edited.settings.color = [0.0, 0.0, 1.0];
        edited.settings.glow = 0.9;
        run(&mut fires, &mut clock, [0.0; 2], 0.05, &[edited]);
        assert_eq!(fires.strength(0, 0), Some(0.6));
        assert_eq!(fires.fire(0, 0).unwrap().seed, seed);
        let drawing = fires.drawing(0, 0).unwrap();
        assert_eq!(drawing.flame.color, [0.0, 0.0, 1.0]);
        assert_eq!(drawing.halo.color, [0.0, 0.0, 1.0]);
    }

    #[test]
    fn the_halo_widens_with_the_flame_and_its_brightness_follows_the_glow() {
        assert!(halo_diameter(2.0, 1.0) > halo_diameter(2.0, 0.5));
        assert!(near(halo_diameter(2.0, 1.0), 1.2 * 2.0 + 2.5, 1e-12));
        assert_eq!(halo_brightness(0.0, 0.9, 1.0), 0.0);
        assert!(near(halo_brightness(1.0, 0.9, 1.0), 0.9, 1e-12));
        let half = halo_brightness(0.5, 0.9, 1.0);
        assert!(near(half, halo_brightness(1.0, 0.9, 1.0) / 2.0, 1e-12));
        assert!(
            near(halo_brightness(1.0, 1.0, 0.05), 0.5, 1e-12),
            "гаснет вместе с огнём"
        );
        assert_eq!(halo_brightness(1.0, 1.0, 0.0), 0.0);
        assert_eq!(halo_brightness(1.0, 1.0, 0.5), 1.0);
    }

    #[test]
    fn the_drawing_stands_on_the_bottom_edge_and_the_halo_sits_in_the_middle_of_the_flame() {
        let objects = [object(0, 1.0)];
        let (mut fires, mut clock) = lit(&objects);
        run(&mut fires, &mut clock, [0.0; 2], 0.5, &objects);
        let Drawing { flame, halo } = fires.drawing(0, 0).unwrap();
        let ([x, y], [width, height]) = objects[0].rect;
        assert_eq!(flame.size[0], width);
        assert!(near(flame.position[1] + flame.size[1], y + height, 1e-12));
        assert_eq!(flame.position[0], x);
        let middle = flame.position[1] + flame.size[1] / 2.0;
        assert!(near(halo.center[1], middle, 1e-12));
        assert!(near(
            halo.center[0],
            x + width / 2.0 + flame.lean / 2.0,
            1e-12
        ));
    }

    #[test]
    fn the_pattern_climbs_faster_when_the_fire_is_stronger_and_its_whole_part_wraps() {
        assert!(scroll_speed(1.0) > scroll_speed(0.2));
        let mut fire = lit(&[object(0, 1.0)]).0.slots[0].take().unwrap();
        fire.scroll = SCROLL_WRAP + 5.25;
        let flame = fire.drawing().flame;
        assert_eq!((flame.scroll_whole, flame.scroll_fraction), (5.0, 0.25));
        assert!(fire.seed < (1 << SEED_BITS));
    }

    #[test]
    fn the_randomness_is_the_fires_own_and_the_same_in_every_engine() {
        let seeds = || {
            let (fires, _) = lit(&[object(0, 1.0), object(1, 1.0)]);
            (0..2)
                .map(|id| fires.fire(id, 0).unwrap().seed)
                .collect::<Vec<_>>()
        };
        assert_eq!(seeds(), seeds());
    }
}

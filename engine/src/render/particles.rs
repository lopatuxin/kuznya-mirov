//! «Ветер и частицы» → «Частицы», «Источник», «Время и случайность»: источники с копилками вылета,
//! частицы, их полёт шагами по 1/60 секунды со сносом ветром, прогрев источников, своя случайность
//! и рисунки частиц. Ни видеокарты, ни браузера: `Motion` ведёт часы и зовёт [`Particles::update`] раз
//! в кадр, `atlas` берёт отсюда рисунки.

use std::f64::consts::TAU;

use crate::core::particles::{ParticleKind, ParticleLook, ParticleTable};
use crate::core::rng::Rng;
use crate::data::load::ImageDecl;

use super::wind::wind_vector;

const STEPS_PER_SECOND: f64 = 60.0;
const STEP_SECONDS: f64 = 1.0 / STEPS_PER_SECOND;
/// «Частица», требование 11: снос догоняет ветер в точке частицы примерно за это время.
const WIND_CATCH_SECONDS: f64 = 0.5;
/// «Частица», требование 10: ритм виляния — секунд на размах, от и до.
const WOBBLE_PERIOD_SECONDS: (f64, f64) = (1.2, 2.0);
/// «Время и случайность»: начальное число счётчика случайности частиц, одно и то же в каждом движке.
const SEED: u64 = 0x31F0_0D5E_ED5A_11E5;

/// Источник на этот кадр: объект мира со свойством `particles` — «Источник», требования 4–7.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter<'a> {
    pub id: u32,
    pub generation: u32,
    pub kind: &'a str,
    /// Записанный прямоугольник — левый верхний угол и размер; `None` — у объекта нет `position` или
    /// `size`, и он ничего не выпускает.
    pub rect: Option<([f64; 2], [f64; 2])>,
    pub layer: i32,
    pub parallax: f64,
}

/// Рисунок одной частицы: середина без сдвига слоя глубины, ширина в клетках, поворот по часовой
/// стрелке в градусах, просвечивание.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub look: ParticleLook,
    pub frame: u32,
    pub center: [f64; 2],
    pub width: f64,
    pub angle: f64,
    pub opacity: f64,
    pub parallax: f64,
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    kind: u32,
    layer: i32,
    parallax: f64,
    age: f64,
    life: f64,
    size: f64,
    velocity: [f64; 2],
    drift: [f64; 2],
    position: [f64; 2],
    spin: f64,
    angle: f64,
    frame_roll: u32,
    wobble_period: f64,
    wobble_phase: f64,
}

#[derive(Debug)]
struct Source {
    id: u32,
    generation: u32,
    kind: u32,
    pool: f64,
    rect: Option<([f64; 2], [f64; 2])>,
    layer: i32,
    parallax: f64,
    touched: u32,
    particles: Vec<Particle>,
}

impl Source {
    fn new(emitter: &Emitter, kind: u32, pass: u32) -> Source {
        let mut source = Source {
            id: emitter.id,
            generation: emitter.generation,
            kind,
            pool: 0.0,
            rect: None,
            layer: 0,
            parallax: 1.0,
            touched: pass,
            particles: Vec::new(),
        };
        source.follow(emitter, pass);
        source
    }

    fn follow(&mut self, emitter: &Emitter, pass: u32) {
        self.rect = emitter.rect;
        self.layer = emitter.layer;
        self.parallax = emitter.parallax;
        self.touched = pass;
    }
}

/// Виды игры на этот кадр: таблица и место каждого видного имени в ней.
#[derive(Clone, Copy)]
struct Kinds<'a> {
    table: &'a ParticleTable,
    resolved: &'a [Option<usize>],
}

impl<'a> Kinds<'a> {
    fn get(self, slot: u32) -> Option<&'a ParticleKind> {
        self.table.kind_at((*self.resolved.get(slot as usize)?)?)
    }
}

/// Все источники и частицы мира. Частицы источника лежат у него, частицы ушедшего источника — в
/// `orphans`: они доживают.
#[derive(Debug)]
pub struct Particles {
    rng: Rng,
    names: Vec<String>,
    table: ParticleTable,
    resolved: Vec<Option<usize>>,
    sources: Vec<Source>,
    orphans: Vec<Particle>,
    integrated_steps: u64,
    /// «Источник», требование 8: ближайшая сборка мира застанет источники — они прогреются.
    warm: bool,
    pass: u32,
}

impl Default for Particles {
    fn default() -> Particles {
        Particles {
            rng: Rng::new(SEED),
            names: Vec::new(),
            table: ParticleTable::default(),
            resolved: Vec::new(),
            sources: Vec::new(),
            orphans: Vec::new(),
            integrated_steps: 0,
            warm: true,
            pass: 0,
        }
    }
}

impl Particles {
    /// «Часы и случайность», требование 17: часы ушли назад или далеко вперёд, партия началась заново —
    /// все частицы исчезают, источники прогреются на ближайшем кадре. Счётчик случайности не трогается.
    pub fn restart(&mut self, steps: f64) {
        self.sources.clear();
        self.orphans.clear();
        self.integrated_steps = steps.floor() as u64;
        self.warm = true;
    }

    /// Живых частиц — у источников и у ушедших.
    pub fn count(&self) -> usize {
        self.orphans.len()
            + self
                .sources
                .iter()
                .map(|source| source.particles.len())
                .sum::<usize>()
    }

    fn kinds(&self) -> Kinds<'_> {
        Kinds {
            table: &self.table,
            resolved: &self.resolved,
        }
    }

    fn intern(&mut self, name: &str) -> u32 {
        let slot = self
            .names
            .iter()
            .position(|known| known == name)
            .unwrap_or_else(|| {
                self.names.push(name.to_string());
                self.names.len() - 1
            });
        slot as u32
    }

    /// Доводит источники и частицы до часов `clock_steps` шагами по 1/60 секунды; остаток часов ждёт
    /// следующего вызова. `flat` — ровный ветер сцены, `table` — виды сейчас: правка вида меняет и уже
    /// вылетевшие частицы, вида больше нет — его частицы исчезают. Источник, которого нет среди
    /// `emitters`, больше не выпускает, его частицы доживают; новый встаёт на часы без вылета —
    /// копилка у него случайная, — а если мир только что собран (`restart` и первая сборка), то
    /// прогретым: будто простоял на месте верхнюю границу `lifetime` своего вида. `world_exists` —
    /// есть ли мир: прогрев ждёт его.
    pub fn update<'e>(
        &mut self,
        clock_steps: f64,
        world_exists: bool,
        flat: [f64; 2],
        emitters: impl Iterator<Item = Emitter<'e>>,
        table: &ParticleTable,
    ) {
        self.table = table.clone();
        self.pass = self.pass.wrapping_add(1);
        let pass = self.pass;
        let mut fresh = Vec::new();
        for emitter in emitters {
            let kind = self.intern(emitter.kind);
            match self.sources.iter_mut().find(|source| {
                source.id == emitter.id
                    && source.generation == emitter.generation
                    && source.kind == kind
            }) {
                Some(source) => source.follow(&emitter, pass),
                None => fresh.push(Source::new(&emitter, kind, pass)),
            }
        }
        self.resolved.clear();
        self.resolved
            .extend(self.names.iter().map(|name| self.table.index_of(name)));
        self.retire_missing_sources(pass);

        let due = clock_steps.floor() as u64;
        while self.integrated_steps < due {
            self.integrated_steps += 1;
            self.step(flat, self.integrated_steps as f64 / STEPS_PER_SECOND);
        }

        let warm = self.warm && world_exists;
        for mut source in fresh {
            source.pool = self.rng.next_unit();
            if warm {
                let kinds = Kinds {
                    table: &self.table,
                    resolved: &self.resolved,
                };
                warm_up(&mut source, &mut self.rng, kinds, flat, due as i64);
            }
            self.sources.push(source);
        }
        if world_exists {
            self.warm = false;
        }
    }

    fn retire_missing_sources(&mut self, pass: u32) {
        let Particles {
            sources, orphans, ..
        } = self;
        sources.retain_mut(|source| {
            if source.touched == pass {
                return true;
            }
            orphans.append(&mut source.particles);
            false
        });
    }

    /// Один шаг мира в момент `t` (секунды часов): частицы летят, источники выпускают новые.
    fn step(&mut self, flat: [f64; 2], t: f64) {
        let Particles {
            rng,
            table,
            resolved,
            sources,
            orphans,
            ..
        } = self;
        let kinds = Kinds {
            table: &*table,
            resolved: resolved.as_slice(),
        };
        for source in sources.iter_mut() {
            step_source(source, rng, kinds, flat, t);
        }
        step_particles(orphans, kinds, flat, t);
    }

    /// Рисунки частиц источника: от старших к младшим, чтобы молодые лежали сверху.
    pub fn live_sprites<'a>(
        &'a self,
        id: u32,
        generation: u32,
        images: &'a [ImageDecl],
    ) -> impl Iterator<Item = Sprite> + 'a {
        let kinds = self.kinds();
        self.sources
            .iter()
            .find(move |source| source.id == id && source.generation == generation)
            .into_iter()
            .flat_map(|source| source.particles.iter())
            .filter_map(move |particle| sprite_of(particle, kinds, images))
    }

    /// Рисунки частиц ушедших источников и источников без записанного прямоугольника (в порядке
    /// рисования их нет) со `layer` источника на момент вылета.
    pub fn orphan_sprites<'a>(
        &'a self,
        images: &'a [ImageDecl],
    ) -> impl Iterator<Item = (i32, Sprite)> + 'a {
        let kinds = self.kinds();
        let unplaced = self
            .sources
            .iter()
            .filter(|source| source.rect.is_none())
            .flat_map(|source| source.particles.iter());
        self.orphans
            .iter()
            .chain(unplaced)
            .filter_map(move |particle| Some((particle.layer, sprite_of(particle, kinds, images)?)))
    }
}

/// «Источник», требование 8: источник простоял на месте `lifetime.to` секунд при ветре тех мгновений
/// — шаги `due − n + 1 ..= due` часов.
fn warm_up(source: &mut Source, rng: &mut Rng, kinds: Kinds, flat: [f64; 2], due: i64) {
    let Some(kind) = kinds.get(source.kind) else {
        return;
    };
    let steps = (kind.lifetime.to * STEPS_PER_SECOND).ceil() as i64;
    for step in (due - steps + 1)..=due {
        step_source(source, rng, kinds, flat, step as f64 / STEPS_PER_SECOND);
    }
}

fn step_source(source: &mut Source, rng: &mut Rng, kinds: Kinds, flat: [f64; 2], t: f64) {
    step_particles(&mut source.particles, kinds, flat, t);
    let (Some(kind), Some(rect)) = (kinds.get(source.kind), source.rect) else {
        return;
    };
    source.pool += kind.rate * STEP_SECONDS;
    while source.pool >= 1.0 {
        source.pool -= 1.0;
        source.particles.push(emit(
            rng,
            kind,
            source.kind,
            rect,
            source.layer,
            source.parallax,
        ));
    }
}

fn step_particles(particles: &mut Vec<Particle>, kinds: Kinds, flat: [f64; 2], t: f64) {
    particles.retain_mut(|particle| {
        let Some(kind) = kinds.get(particle.kind) else {
            return false;
        };
        let wind = wind_vector(flat, particle.position[0], t);
        advance(particle, kind, wind)
    });
}

/// «Частица», требование 10: что частица берёт наугад при вылете, в одном и том же порядке при любом
/// виде — счётчик случайности не зависит от данных вида.
fn emit(
    rng: &mut Rng,
    kind: &ParticleKind,
    slot: u32,
    rect: ([f64; 2], [f64; 2]),
    layer: i32,
    parallax: f64,
) -> Particle {
    let (corner, size) = rect;
    let x = corner[0] + rng.next_unit() * size[0];
    let y = corner[1] + rng.next_unit() * size[1];
    let life = kind.lifetime.at(rng.next_unit());
    let width = kind.size.at(rng.next_unit());
    let speed = kind.speed.at(rng.next_unit());
    let spin_roll = rng.next_unit();
    let spin = kind.spin.map_or(0.0, |span| span.at(spin_roll));
    let direction = kind.direction + (rng.next_unit() * 2.0 - 1.0) * kind.spread;
    let angle = rng.next_unit() * 360.0;
    let frame_roll = (rng.next_u64() >> 32) as u32;
    let wobble_period = WOBBLE_PERIOD_SECONDS.0
        + rng.next_unit() * (WOBBLE_PERIOD_SECONDS.1 - WOBBLE_PERIOD_SECONDS.0);
    let wobble_phase = rng.next_unit() * TAU;
    let radians = direction.to_radians();
    Particle {
        kind: slot,
        layer,
        parallax,
        age: 0.0,
        life,
        size: width,
        velocity: [speed * radians.sin(), -speed * radians.cos()],
        drift: [0.0, 0.0],
        position: [x, y],
        spin,
        angle: if kind.spin.is_some() { angle } else { 0.0 },
        frame_roll,
        wobble_period,
        wobble_phase,
    }
}

/// «Частица», требование 11: один шаг `Δ` = 1/60 секунды при ветре `wind` в точке частицы. Возвращает
/// `false`, когда частица дожила.
fn advance(particle: &mut Particle, kind: &ParticleKind, wind: [f64; 2]) -> bool {
    particle.age += STEP_SECONDS;
    if particle.age >= particle.life {
        return false;
    }
    particle.velocity[1] += kind.gravity * STEP_SECONDS;
    let catch = STEP_SECONDS / WIND_CATCH_SECONDS;
    for (axis, wind) in wind.into_iter().enumerate() {
        particle.drift[axis] += (kind.wind * wind - particle.drift[axis]) * catch;
        particle.position[axis] += (particle.velocity[axis] + particle.drift[axis]) * STEP_SECONDS;
    }
    true
}

/// «Частица», требование 13: просвечивание на доле жизни `fraction` — точки через равные доли, между
/// ними по прямой.
pub fn opacity_at(points: &[f64], fraction: f64) -> f64 {
    match points {
        [] => 1.0,
        [only] => *only,
        _ => {
            let scaled = fraction.clamp(0.0, 1.0) * (points.len() - 1) as f64;
            let index = (scaled.floor() as usize).min(points.len() - 2);
            let part = scaled - index as f64;
            points[index] + (points[index + 1] - points[index]) * part
        }
    }
}

/// «Частица», требование 13: кадр картинки — первый у картинки без кадров, взятый при вылете у картинки
/// с кадрами без `frame_time`, по времени от вылета частицы у картинки с `frame_time`; кадр больше
/// числа кадров картинки берётся по кругу.
fn frame_of(particle: &Particle, decl: &ImageDecl) -> u32 {
    if decl.frames <= 1 {
        return 0;
    }
    if decl.animated && decl.frame_seconds > 0.0 {
        let passed = (particle.age / decl.frame_seconds).floor() as u64;
        return (passed % u64::from(decl.frames)) as u32;
    }
    particle.frame_roll % decl.frames
}

fn sprite_of(particle: &Particle, kinds: Kinds, images: &[ImageDecl]) -> Option<Sprite> {
    let kind = kinds.get(particle.kind)?;
    let frame = match kind.look {
        ParticleLook::Image(image) => frame_of(particle, images.get(image)?),
        ParticleLook::Shape(shape) => particle.frame_roll % shape.frames(),
    };
    let fraction = particle.age / particle.life;
    let sway =
        kind.wobble * (TAU * particle.age / particle.wobble_period + particle.wobble_phase).sin();
    Some(Sprite {
        look: kind.look,
        frame,
        center: [particle.position[0] + sway, particle.position[1]],
        width: particle.size * (1.0 + (kind.grow - 1.0) * fraction),
        angle: particle.angle + particle.spin * particle.age,
        opacity: opacity_at(&kind.opacity, fraction),
        parallax: particle.parallax,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::particles::{ParticleShape, Span};
    use crate::core::screens::Anchor;

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn smoke() -> ParticleKind {
        ParticleKind {
            look: ParticleLook::Image(0),
            rate: 6.0,
            lifetime: Span { from: 4.0, to: 6.0 },
            size: Span { from: 0.6, to: 0.9 },
            grow: 3.0,
            speed: Span { from: 0.6, to: 1.0 },
            direction: 0.0,
            spread: 20.0,
            gravity: 0.0,
            opacity: vec![0.0, 0.7, 0.0],
            spin: Some(Span {
                from: -20.0,
                to: 20.0,
            }),
            wobble: 0.0,
            wind: 1.0,
        }
    }

    fn table_of(kind: ParticleKind) -> ParticleTable {
        ParticleTable::new(vec![("smoke".to_string(), Some(kind))])
    }

    fn decl(frames: u32, frame_seconds: f64) -> ImageDecl {
        ImageDecl {
            name: "puff".to_string(),
            path: "puff.png".to_string(),
            frames,
            frame_steps: 1,
            animated: frame_seconds > 0.0,
            columns: None,
            size: None,
            anchor: Anchor::Center,
            offset: [0.0, 0.0],
            frame_by: None,
            frame_by_name: None,
            smooth: false,
            glow: false,
            frame_seconds,
        }
    }

    fn chimney(id: u32) -> Emitter<'static> {
        Emitter {
            id,
            generation: 0,
            kind: "smoke",
            rect: Some(([10.0, 10.0], [2.0, 1.0])),
            layer: 0,
            parallax: 1.0,
        }
    }

    /// Кадры мира: часы идут на шаг, `update` зовётся на каждом.
    fn run(
        particles: &mut Particles,
        table: &ParticleTable,
        emitters: &[Emitter],
        from: u64,
        steps: u64,
    ) {
        for step in from..from + steps {
            particles.update(
                (step + 1) as f64,
                true,
                [0.0, 0.0],
                emitters.iter().copied(),
                table,
            );
        }
    }

    #[test]
    fn the_random_numbers_of_a_particle_do_not_depend_on_the_kind_having_a_spin() {
        let rect = ([3.0, 4.0], [2.0, 1.0]);
        let with_spin = smoke();
        let mut without_spin = smoke();
        without_spin.spin = None;
        let mut rng_with = Rng::new(7);
        let mut rng_without = Rng::new(7);
        for _ in 0..5 {
            let a = emit(&mut rng_with, &with_spin, 0, rect, 0, 1.0);
            let b = emit(&mut rng_without, &without_spin, 0, rect, 0, 1.0);
            assert_eq!(a.position, b.position);
            assert_eq!(a.life, b.life);
            assert_eq!(a.size, b.size);
            assert_eq!(a.velocity, b.velocity);
            assert_eq!(a.frame_roll, b.frame_roll);
            assert_eq!(a.wobble_period, b.wobble_period);
            assert_eq!(a.wobble_phase, b.wobble_phase);
        }
        assert_eq!(rng_with.next_u64(), rng_without.next_u64());
    }

    #[test]
    fn a_particle_flies_where_its_direction_points() {
        let mut rng = Rng::new(1);
        let mut kind = smoke();
        kind.spread = 0.0;
        kind.speed = Span::single(2.0);
        kind.direction = 90.0;
        let particle = emit(&mut rng, &kind, 0, ([0.0, 0.0], [1.0, 1.0]), 0, 1.0);
        assert!(
            near(particle.velocity[0], 2.0, 1e-9),
            "{:?}",
            particle.velocity
        );
        assert!(
            near(particle.velocity[1], 0.0, 1e-9),
            "{:?}",
            particle.velocity
        );
        kind.direction = 0.0;
        let particle = emit(&mut rng, &kind, 0, ([0.0, 0.0], [1.0, 1.0]), 0, 1.0);
        assert!(
            near(particle.velocity[0], 0.0, 1e-9),
            "{:?}",
            particle.velocity
        );
        assert!(
            near(particle.velocity[1], -2.0, 1e-9),
            "{:?}",
            particle.velocity
        );
    }

    #[test]
    fn a_particle_takes_its_values_between_the_ends_and_the_spread_around_the_direction() {
        let mut rng = Rng::new(2);
        let kind = smoke();
        for _ in 0..200 {
            let particle = emit(&mut rng, &kind, 0, ([10.0, 10.0], [2.0, 1.0]), 0, 1.0);
            assert!((4.0..=6.0).contains(&particle.life));
            assert!((0.6..=0.9).contains(&particle.size));
            assert!((10.0..=12.0).contains(&particle.position[0]));
            assert!((10.0..=11.0).contains(&particle.position[1]));
            let speed = particle.velocity[0].hypot(particle.velocity[1]);
            assert!((0.6 - 1e-9..=1.0 + 1e-9).contains(&speed), "{speed}");
            let degrees = particle.velocity[0]
                .atan2(-particle.velocity[1])
                .to_degrees();
            assert!(degrees.abs() <= 20.0 + 1e-9, "{degrees}");
        }
    }

    #[test]
    fn the_start_angle_is_random_only_for_a_kind_with_spin() {
        let mut rng = Rng::new(3);
        let mut kind = smoke();
        let spun: Vec<f64> = (0..20)
            .map(|_| emit(&mut rng, &kind, 0, ([0.0; 2], [1.0; 2]), 0, 1.0).angle)
            .collect();
        assert!(spun.iter().any(|angle| *angle > 1.0), "{spun:?}");
        kind.spin = None;
        for _ in 0..20 {
            let particle = emit(&mut rng, &kind, 0, ([0.0; 2], [1.0; 2]), 0, 1.0);
            assert_eq!((particle.angle, particle.spin), (0.0, 0.0));
        }
    }

    /// Требование 11: доля 1, ветер в точке всё время 2 по `x`.
    #[test]
    fn the_drift_catches_the_wind_in_half_a_second() {
        let mut kind = smoke();
        kind.speed = Span::single(0.0);
        let mut rng = Rng::new(4);
        let mut particle = emit(&mut rng, &kind, 0, ([0.0; 2], [1.0; 2]), 0, 1.0);
        particle.life = 100.0;
        let drift_after = |steps: usize, particle: &mut Particle| {
            for _ in 0..steps {
                assert!(advance(particle, &kind, [2.0, 0.0]));
            }
            particle.drift[0]
        };
        assert!(near(drift_after(30, &mut particle), 1.277, 1e-3));
        assert!(near(drift_after(60, &mut particle), 1.905, 1e-3));
    }

    #[test]
    fn a_share_of_the_wind_scales_the_drift_and_gravity_pulls_down() {
        let mut kind = smoke();
        kind.speed = Span::single(0.0);
        kind.wind = 0.5;
        kind.gravity = 3.0;
        let mut rng = Rng::new(5);
        let mut particle = emit(&mut rng, &kind, 0, ([0.0; 2], [1.0; 2]), 0, 1.0);
        particle.life = 100.0;
        let start = particle.position;
        for _ in 0..60 {
            advance(&mut particle, &kind, [2.0, 0.0]);
        }
        assert!(near(particle.velocity[1], 3.0, 1e-9));
        assert!(particle.position[1] > start[1] + 1.0);
        assert!(particle.drift[0] < 1.0 && particle.drift[0] > 0.5);
    }

    #[test]
    fn a_particle_that_reached_its_life_disappears() {
        let kind = smoke();
        let mut rng = Rng::new(6);
        let mut particle = emit(&mut rng, &kind, 0, ([0.0; 2], [1.0; 2]), 0, 1.0);
        particle.life = 0.105;
        let alive = (0..10)
            .take_while(|_| advance(&mut particle, &kind, [0.0; 2]))
            .count();
        assert_eq!(alive, 6);
    }

    #[test]
    fn the_opacity_goes_through_the_points_by_straight_lines() {
        let points = [0.0, 0.7, 0.0];
        assert!(near(opacity_at(&points, 0.25), 0.35, 1e-12));
        assert!(near(opacity_at(&points, 0.5), 0.7, 1e-12));
        assert!(near(opacity_at(&points, 0.9), 0.14, 1e-12));
        assert_eq!(opacity_at(&[0.4], 0.9), 0.4);
        assert_eq!(opacity_at(&points, 1.0), 0.0);
    }

    #[test]
    fn a_source_emits_its_rate_per_second() {
        let mut kind = smoke();
        kind.lifetime = Span::single(1000.0);
        let table = table_of(kind);
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], std::iter::empty(), &table);
        run(&mut particles, &table, &[chimney(0)], 0, 600);
        let emitted = particles.count();
        assert!((59..=61).contains(&emitted), "{emitted}");
    }

    /// Требование 8: источник, застанный первой сборкой, на первом кадре уже выпустил то, что успел
    /// бы за верхнюю границу `lifetime`.
    #[test]
    fn a_source_found_on_the_first_assembly_is_warmed_up() {
        let table = table_of(smoke());
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        let live = particles.count();
        assert!((25..=35).contains(&live), "{live}");
    }

    #[test]
    fn a_source_that_appears_later_starts_from_nothing() {
        let table = table_of(smoke());
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], std::iter::empty(), &table);
        assert_eq!(particles.count(), 0);
        particles.update(120.0, true, [0.0; 2], [chimney(3)].into_iter(), &table);
        assert_eq!(
            particles.count(),
            0,
            "новый источник — ноль на первом кадре"
        );
    }

    #[test]
    fn the_warm_up_waits_for_a_world() {
        let table = table_of(smoke());
        let mut particles = Particles::default();
        particles.update(0.0, false, [0.0; 2], std::iter::empty(), &table);
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        assert!(particles.count() >= 25);
    }

    #[test]
    fn a_restart_clears_everything_and_warms_the_sources_again() {
        let table = table_of(smoke());
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        particles.update(
            30.0,
            true,
            [0.0; 2],
            [chimney(0), chimney(5)].into_iter(),
            &table,
        );
        assert!(particles.count() > 0);
        particles.restart(10.0);
        assert_eq!(particles.count(), 0);
        particles.update(
            10.0,
            true,
            [0.0; 2],
            [chimney(0), chimney(5)].into_iter(),
            &table,
        );
        let live = particles.count();
        assert!((50..=70).contains(&live), "{live}");
    }

    #[test]
    fn a_stopped_clock_moves_nothing() {
        let table = table_of(smoke());
        let mut particles = Particles::default();
        particles.update(60.0, true, [1.5, 0.0], [chimney(0)].into_iter(), &table);
        let before: Vec<Sprite> = particles.live_sprites(0, 0, &[decl(1, 0.0)]).collect();
        for _ in 0..5 {
            particles.update(60.0, true, [1.5, 0.0], [chimney(0)].into_iter(), &table);
        }
        let after: Vec<Sprite> = particles.live_sprites(0, 0, &[decl(1, 0.0)]).collect();
        assert_eq!(before, after);
    }

    #[test]
    fn a_vanished_source_stops_emitting_and_its_particles_live_out() {
        let table = table_of(smoke());
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        let live = particles.count();
        particles.update(1.0, true, [0.0; 2], std::iter::empty(), &table);
        assert!(particles.count() > 0 && particles.count() <= live);
        assert_eq!(particles.live_sprites(0, 0, &[decl(1, 0.0)]).count(), 0);
        let orphans = particles.orphan_sprites(&[decl(1, 0.0)]).count();
        assert_eq!(orphans, particles.count());
        particles.update(60.0 * 8.0, true, [0.0; 2], std::iter::empty(), &table);
        assert_eq!(particles.count(), 0, "дожили и исчезли");
    }

    #[test]
    fn a_source_that_changed_its_kind_is_a_new_source() {
        let mut other = smoke();
        other.rate = 6.0;
        let table = ParticleTable::new(vec![
            ("smoke".to_string(), Some(smoke())),
            ("steam".to_string(), Some(other)),
        ]);
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        let mut steam = chimney(0);
        steam.kind = "steam";
        particles.update(1.0, true, [0.0; 2], [steam].into_iter(), &table);
        let images = [decl(1, 0.0)];
        assert_eq!(particles.live_sprites(0, 0, &images).count(), 0);
        assert!(particles.orphan_sprites(&images).count() > 20);
    }

    #[test]
    fn an_edited_kind_changes_the_particles_in_flight_but_not_their_taken_values() {
        let mut particles = Particles::default();
        let table = table_of(smoke());
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        let images = [decl(1, 0.0)];
        let before: Vec<Sprite> = particles.live_sprites(0, 0, &images).collect();
        let mut heavy = smoke();
        heavy.grow = 1.0;
        heavy.opacity = vec![0.5];
        particles.update(
            0.0,
            true,
            [0.0; 2],
            [chimney(0)].into_iter(),
            &table_of(heavy),
        );
        let after: Vec<Sprite> = particles.live_sprites(0, 0, &images).collect();
        assert_eq!(before.len(), after.len());
        assert!(after.iter().all(|sprite| sprite.opacity == 0.5));
        assert!(
            after
                .iter()
                .all(|sprite| (0.6..=0.9).contains(&sprite.width))
        );
        assert!(before.iter().any(|sprite| sprite.width > 0.95));
    }

    #[test]
    fn the_wobble_sways_the_sprite_sideways_within_its_amplitude() {
        let mut kind = smoke();
        kind.speed = Span::single(0.0);
        kind.spread = 0.0;
        kind.wind = 0.0;
        kind.wobble = 0.5;
        let mut source = chimney(0);
        source.rect = Some(([10.0, 10.0], [0.0, 0.0]));
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [source].into_iter(), &table_of(kind));
        let sprites: Vec<Sprite> = particles.live_sprites(0, 0, &[decl(1, 0.0)]).collect();
        assert!(sprites.len() > 20);
        assert!(
            sprites
                .iter()
                .all(|sprite| (9.5 - 1e-9..=10.5 + 1e-9).contains(&sprite.center[0]))
        );
        assert!(sprites.iter().all(|sprite| sprite.center[1] == 10.0));
        let (low, high) = sprites
            .iter()
            .fold((f64::MAX, f64::MIN), |(low, high), sprite| {
                (low.min(sprite.center[0]), high.max(sprite.center[0]))
            });
        assert!(
            high - low > 0.3,
            "у каждой частицы свой ритм и фаза: {low}..{high}"
        );
    }

    #[test]
    fn a_new_gravity_pulls_the_particles_in_flight_and_their_taken_life_stays() {
        let mut calm = smoke();
        calm.speed = Span::single(0.0);
        calm.wind = 0.0;
        let mut particles = Particles::default();
        particles.update(
            0.0,
            true,
            [0.0; 2],
            [chimney(0)].into_iter(),
            &table_of(calm.clone()),
        );
        let youngest = *particles.sources[0].particles.last().expect("есть частицы");
        assert_eq!(youngest.velocity[1], 0.0);
        let mut heavy = calm;
        heavy.gravity = 10.0;
        particles.update(
            30.0,
            true,
            [0.0; 2],
            [chimney(0)].into_iter(),
            &table_of(heavy),
        );
        let same = particles.sources[0]
            .particles
            .iter()
            .find(|particle| particle.life == youngest.life)
            .expect("жива: живёт не меньше четырёх секунд");
        assert!(near(same.velocity[1], 5.0, 1e-6), "{:?}", same.velocity);
        assert!(same.position[1] > youngest.position[1] + 1.0);
    }

    #[test]
    fn a_kind_that_is_gone_takes_its_particles_with_it() {
        let mut particles = Particles::default();
        particles.update(
            0.0,
            true,
            [0.0; 2],
            [chimney(0)].into_iter(),
            &table_of(smoke()),
        );
        assert!(particles.count() > 0);
        particles.update(
            1.0,
            true,
            [0.0; 2],
            [chimney(0)].into_iter(),
            &ParticleTable::default(),
        );
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn a_source_without_a_rectangle_emits_nothing() {
        let mut particles = Particles::default();
        particles.update(0.0, false, [0.0; 2], std::iter::empty(), &table_of(smoke()));
        let mut source = chimney(0);
        source.rect = None;
        particles.update(
            600.0,
            true,
            [0.0; 2],
            [source].into_iter(),
            &table_of(smoke()),
        );
        particles.update(
            1200.0,
            true,
            [0.0; 2],
            [source].into_iter(),
            &table_of(smoke()),
        );
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn the_frames_follow_the_picture_of_the_kind() {
        let mut rng = Rng::new(9);
        let mut particle = emit(&mut rng, &smoke(), 0, ([0.0; 2], [1.0; 2]), 0, 1.0);
        assert_eq!(frame_of(&particle, &decl(1, 0.0)), 0);
        particle.age = 5.0;
        let rolled = frame_of(&particle, &decl(3, 0.0));
        assert!(rolled < 3);
        particle.age = 0.0;
        assert_eq!(
            frame_of(&particle, &decl(3, 0.0)),
            rolled,
            "кадр держится всю жизнь"
        );
        particle.age = 0.0;
        assert_eq!(
            frame_of(&particle, &decl(4, 0.25)),
            0,
            "от вылета — с первого кадра"
        );
        particle.age = 0.3;
        assert_eq!(frame_of(&particle, &decl(4, 0.25)), 1);
        particle.age = 1.1;
        assert_eq!(frame_of(&particle, &decl(4, 0.25)), 0, "по кругу");
        particle.frame_roll = 7;
        particle.age = 0.0;
        assert_eq!(
            frame_of(&particle, &decl(3, 0.0)),
            1,
            "больше числа кадров — по кругу"
        );
    }

    #[test]
    fn a_leaf_takes_one_of_its_four_frames_and_holds_it_all_its_life() {
        let mut kind = smoke();
        kind.look = ParticleLook::Shape(ParticleShape::Leaf);
        let table = table_of(kind.clone());
        let kinds = Kinds {
            table: &table,
            resolved: &[Some(0)],
        };
        let mut rng = Rng::new(11);
        let mut seen = Vec::new();
        for _ in 0..40 {
            let mut particle = emit(&mut rng, &kind, 0, ([0.0; 2], [1.0; 2]), 0, 1.0);
            let first = sprite_of(&particle, kinds, &[]).expect("вид есть").frame;
            assert!(first < 4);
            while advance(&mut particle, &kind, [0.0; 2]) {
                let sprite = sprite_of(&particle, kinds, &[]).expect("вид есть");
                assert_eq!(sprite.frame, first, "кадр держится всю жизнь");
            }
            seen.push(first);
        }
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen, [0, 1, 2, 3]);
    }

    #[test]
    fn the_sprite_grows_turns_and_wobbles_with_its_age() {
        let mut kind = smoke();
        kind.wobble = 0.5;
        kind.spin = Some(Span::single(30.0));
        let table = table_of(kind);
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
        let images = [decl(1, 0.0)];
        let young = particles
            .live_sprites(0, 0, &images)
            .last()
            .expect("есть частицы");
        assert!(young.width < 1.0);
        let oldest = particles
            .live_sprites(0, 0, &images)
            .next()
            .expect("есть частицы");
        assert!(oldest.width > 1.5, "выросла до {}", oldest.width);
        assert!(oldest.angle.abs() > 0.0);
    }

    #[test]
    fn the_random_counter_of_the_particles_is_the_same_in_every_engine() {
        let draw = || {
            let table = table_of(smoke());
            let mut particles = Particles::default();
            particles.update(0.0, true, [0.0; 2], [chimney(0)].into_iter(), &table);
            particles
                .live_sprites(0, 0, &[decl(1, 0.0)])
                .map(|sprite| sprite.center)
                .collect::<Vec<_>>()
        };
        assert_eq!(draw(), draw());
    }
}

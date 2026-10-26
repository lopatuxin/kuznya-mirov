//! «Ветер и частицы» → «Частицы», «Дым и искры», «Листопад», «Пух», «Бабочки», «Включение и
//! выключение», «Время и случайность»: источники — пары «объект и эффект» — с копилками вылета,
//! частицы, их полёт шагами по 1/60 секунды со сносом ветром, посадка листа, блуждание пуха, курс и
//! посадки бабочки, прогрев источников и своя случайность. Ни видеокарты, ни браузера: `Motion` ведёт
//! часы и зовёт [`Particles::update`] раз в кадр, `atlas` берёт отсюда рисунки. Тот же `Particles`
//! ведёт и частицы экрана интерфейса — «На экране»: источник там панель, а клетка — `cell_pixels`.

use std::f64::consts::TAU;

use crate::core::particles::{
    BUTTERFLY_COLORS, BUTTERFLY_EDGE, BUTTERFLY_FADE_SECONDS, BUTTERFLY_FLAP_HZ,
    BUTTERFLY_LANDING_CHANCE, BUTTERFLY_LANDING_SECONDS, BUTTERFLY_MAX_LANDINGS,
    BUTTERFLY_SIT_FLAP_HZ, BUTTERFLY_SIT_SECONDS, BUTTERFLY_TAKEOFF_SECONDS,
    BUTTERFLY_TEN_CELLS_SECONDS, Effect, FLUFF_FADE_IN_SECONDS, FLUFF_FADE_OUT_SECONDS, Kind,
    LEAF_ACCELERATION_SECONDS, LEAF_AIR_SECONDS, LEAF_BRIGHTNESS, LEAF_LIE_SECONDS,
    LEAF_MELT_SECONDS, ParticleShape, Rgb, SMOKE_LAUNCH_FACTOR, SMOKE_SLOWDOWN, Settings,
    butterfly_mean_life, spark_color,
};
use crate::core::rng::Rng;

use super::wind::wind_vector;

const STEPS_PER_SECOND: f64 = 60.0;
const STEP_SECONDS: f64 = 1.0 / STEPS_PER_SECOND;
/// Снос догоняет ветер в точке частицы примерно за это время.
const WIND_CATCH_SECONDS: f64 = 0.5;
/// «Бабочки»: размах виляния пути вверх-вниз в клетках, секунд на размах и подскок за каждый взмах.
const BUTTERFLY_WEAVE: f64 = 0.35;
const BUTTERFLY_WEAVE_PERIOD: (f64, f64) = (1.4, 2.6);
const BUTTERFLY_HOP: f64 = 0.05;
/// «Бабочки»: скорость вперёд качается вокруг ровного хода на эту долю, секунд на размах: чуть больше
/// единицы — бабочка то и дело отлетает назад, но в среднем идёт вперёд ровным ходом.
const BUTTERFLY_JITTER: f64 = 1.3;
const BUTTERFLY_JITTER_PERIOD: (f64, f64) = (0.8, 1.6);
/// «Бабочки»: наклон к горизонту не больше стольких градусов и догоняет путь за это время.
const BUTTERFLY_TILT_LIMIT: f64 = 30.0;
const BUTTERFLY_TILT_SECONDS: f64 = 0.15;
/// «Бабочки»: на сколько клеток вперёд от места, где решила сесть, она садится.
const BUTTERFLY_LANDING_REACH: f64 = 1.2;
/// «Бабочки»: места пролёта, где она садится, — доли ширины от входа: первая посадка в начале пути,
/// вторая в конце.
const BUTTERFLY_FIRST_LANDING: (f64, f64) = (0.12, 0.3);
const BUTTERFLY_SECOND_LANDING: (f64, f64) = (0.55, 0.3);
/// «Бабочки»: после взлёта она летит по курсу ещё столько секунд сверх самого взлёта, прежде чем следующая
/// посадка может начаться.
const BUTTERFLY_CRUISE_SECONDS: f64 = 0.5;
/// Чисел, которые вылет берёт сверх общих из отдельного потока: поток засеян общим числом кадра, так что
/// счётчик случайности частиц тратится на вылет так же, как до пуха и бабочек, и прежние источники
/// выпускают прежние частицы.
const EXTRA_ROLLS: usize = 8 + 5 * BUTTERFLY_MAX_LANDINGS;
/// «Слои глубины»: во сколько раз `parallax` объекта уменьшает или увеличивает размеры и скорости
/// частиц — от и до, как у качания.
const DEPTH_SCALE: (f64, f64) = (0.2, 3.0);
/// «Листопад»: точка картинки непрозрачна, если её непрозрачность от половины и выше.
const OPAQUE_ALPHA: u8 = 128;
/// «Время и случайность»: начальное число счётчика случайности частиц, одно и то же в каждом движке.
const SEED: u64 = 0x31F0_0D5E_ED5A_11E5;
/// «На экране»: случайность частиц экрана — своя, отдельная от мира.
const SCREEN_SEED: u64 = 0x5C4E_E11E_0F5C_7EA3;
/// «На экране»: часы окна, ушедшие вперёд больше чем на столько шагов (две секунды) — вкладка стояла
/// скрытой, — не догоняются шагами: частицы начинаются заново и источники прогреваются.
const MAX_CATCH_UP_STEPS: f64 = 2.0 * STEPS_PER_SECOND;
const WHITE: Rgb = [1.0, 1.0, 1.0];

/// Левый верхний угол и размер.
pub type Rect = ([f64; 2], [f64; 2]);

#[derive(Debug, Clone, Copy, PartialEq)]
struct Run {
    /// Номер первой точки подряд: строка кадра на его ширину плюс столбец.
    start: u32,
    len: u32,
    /// Сколько непрозрачных точек лежит до этой полосы.
    before: u32,
}

/// «Листопад»: непрозрачные точки первого кадра картинки, полосами подряд по строкам — список
/// собирается один раз при загрузке картинок.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OpaqueMask {
    width: u32,
    height: u32,
    runs: Vec<Run>,
    count: u32,
}

impl OpaqueMask {
    /// Первый кадр — левый верхний угол ленты `pixels` (RGBA, `strip_width` точек в строке),
    /// `width` на `height` точек.
    pub fn of_first_frame(pixels: &[u8], strip_width: u32, width: u32, height: u32) -> OpaqueMask {
        let mut runs = Vec::new();
        let mut count = 0u32;
        for y in 0..height {
            let mut open: Option<u32> = None;
            for x in 0..=width {
                let alpha_at = ((y * strip_width + x) * 4 + 3) as usize;
                let opaque = x < width
                    && pixels
                        .get(alpha_at)
                        .is_some_and(|alpha| *alpha >= OPAQUE_ALPHA);
                match (opaque, open) {
                    (true, None) => open = Some(x),
                    (false, Some(from)) => {
                        runs.push(Run {
                            start: y * width + from,
                            len: x - from,
                            before: count,
                        });
                        count += x - from;
                        open = None;
                    }
                    _ => {}
                }
            }
        }
        OpaqueMask {
            width,
            height,
            runs,
            count,
        }
    }

    fn count(&self) -> u32 {
        self.count
    }

    /// Доля непрозрачных точек в кадре.
    fn fraction(&self) -> f64 {
        let all = u64::from(self.width) * u64::from(self.height);
        if all == 0 {
            return 0.0;
        }
        f64::from(self.count) / all as f64
    }

    /// Непрозрачная точка номер `index` (`index < count`) — столбец и строка кадра.
    fn nth(&self, index: u32) -> (u32, u32) {
        let run = self.runs[self.runs.partition_point(|run| run.before <= index) - 1];
        let at = run.start + (index - run.before);
        (at % self.width, at / self.width)
    }
}

/// Где нарисован первый кадр картинки объекта: точка кадра в долях ширины `u` и высоты `v` лежит в
/// сцене в `origin + u · across + v · down` — с учётом `size`, `anchor`, `offset`, `flip_x` и поворота.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Picture {
    pub image: usize,
    pub area: f64,
    pub origin: [f64; 2],
    pub across: [f64; 2],
    pub down: [f64; 2],
}

impl Picture {
    fn point(&self, u: f64, v: f64) -> [f64; 2] {
        [0, 1].map(|axis| self.origin[axis] + u * self.across[axis] + v * self.down[axis])
    }
}

/// Объект мира на этот кадр: что у него включено и где он стоит — «Частицы», «Источник».
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter {
    pub id: u32,
    pub generation: u32,
    /// Записанный прямоугольник; `None` — у объекта нет `position` или `size`, и он ничего не
    /// выпускает.
    pub rect: Option<Rect>,
    pub layer: i32,
    pub parallax: f64,
    pub settings: Settings,
    /// Откуда рвутся листья; `None` — из любой точки прямоугольника.
    pub picture: Option<Picture>,
}

/// Рисунок одной частицы: середина без сдвига слоя глубины, ширина в клетках, поворот по часовой
/// стрелке в градусах, цвет и просвечивание.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sprite {
    pub shape: ParticleShape,
    pub frame: u32,
    pub center: [f64; 2],
    pub width: f64,
    pub angle: f64,
    /// Рисунок отражён слева направо до поворота: бабочка, летящая влево, смотрит влево, а не висит
    /// вверх ногами, как после поворота на полоборота.
    pub flip_x: bool,
    pub opacity: f64,
    pub color: Rgb,
    pub parallax: f64,
}

/// Что делает бабочка: летит, опускается на место посадки, сидит, взлетает.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
enum Stage {
    #[default]
    Fly,
    Descend,
    Sit,
    Takeoff,
}

/// Посадка бабочки, решённая при вылете: с какого места пути она начинается, куда бабочка садится и
/// сколько сидит.
#[derive(Debug, Clone, Copy, Default)]
struct Stop {
    active: bool,
    at: f64,
    target: [f64; 2],
    sit: f64,
}

/// Курс бабочки. Путь — ровный ход по `x` с вилянием вверх-вниз; всё, что нужно, решено при вылете, так
/// что полёт не берёт чисел из счётчика случайности.
#[derive(Debug, Clone, Copy, Default)]
struct Flight {
    /// `1` — влетела слева и летит вправо, `−1` — наоборот.
    dir: f64,
    speed: f64,
    base_y: f64,
    weave_amplitude: f64,
    weave_period: f64,
    weave_phase: f64,
    jitter_period: f64,
    jitter_phase: f64,
    flap_hz: f64,
    sit_flap_hz: f64,
    /// За этим `x` бабочка начинает таять.
    exit_x: f64,
    path_x: f64,
    /// Секунд в полёте: на посадке время пути стоит.
    fly_time: f64,
    flap_phase: f64,
    /// Куда ветер сдвинул бабочку с её пути.
    wind_offset: [f64; 2],
    /// Наклон носа к горизонту, градусов, вниз положительный.
    tilt: f64,
    stage: Stage,
    stage_age: f64,
    stage_length: f64,
    from: [f64; 2],
    next_stop: usize,
    stops: [Stop; BUTTERFLY_MAX_LANDINGS],
}

impl Flight {
    /// Где бабочка на своём пути: ровный ход, виляние вверх-вниз, подскок за взмах и снос ветром.
    fn point(&self) -> [f64; 2] {
        let weave = self.weave_amplitude
            * (TAU * self.fly_time / self.weave_period + self.weave_phase).sin();
        let hop = -BUTTERFLY_HOP * 0.5 * (1.0 - (TAU * self.flap_phase).cos());
        [
            self.path_x + self.wind_offset[0],
            self.base_y + weave + hop + self.wind_offset[1],
        ]
    }

    /// Дальний от входа край прямоугольника.
    fn far_edge(&self) -> f64 {
        self.exit_x - self.dir * BUTTERFLY_EDGE
    }

    /// Бабочка дошла до места, откуда начинается следующая посадка.
    fn stop_reached(&self) -> bool {
        self.stops
            .get(self.next_stop)
            .is_some_and(|stop| self.dir * (self.path_x - stop.at) >= 0.0)
    }

    /// Взлёт с посадки: следующая посадка не раньше, чем бабочка пролетит взлёт и ещё немного по курсу, и всегда
    /// впереди. Не вмещается до дальнего края — следующей посадки не будет.
    fn clear_the_way(&mut self) {
        let dir = self.dir;
        let earliest =
            self.path_x + dir * self.speed * (BUTTERFLY_TAKEOFF_SECONDS + BUTTERFLY_CRUISE_SECONDS);
        let far = self.far_edge();
        let Some(stop) = self.stops.get_mut(self.next_stop) else {
            return;
        };
        let shift = dir * (dir * (earliest - stop.at)).max(0.0);
        stop.at += shift;
        stop.target[0] += shift;
        if dir * (stop.at - far) > 0.0 {
            stop.active = false;
        }
    }

    /// Начало спуска на следующее место посадки из точки `from`: место впереди бабочки по курсу и внутри
    /// прямоугольника, а если впереди места нет — бабочка не садится.
    fn start_descent(&mut self, from: [f64; 2]) {
        let dir = self.dir;
        let far = self.far_edge();
        let stop = &mut self.stops[self.next_stop];
        if dir * (from[0] - stop.target[0]) > 0.0 {
            stop.target[0] = from[0];
        }
        if dir * (stop.target[0] - far) > 0.0 {
            stop.active = false;
            return;
        }
        self.stage = Stage::Descend;
        self.stage_age = 0.0;
        self.stage_length = BUTTERFLY_LANDING_SECONDS;
        self.from = from;
    }
}

#[derive(Debug, Clone, Copy)]
struct Particle {
    layer: i32,
    parallax: f64,
    age: f64,
    life: f64,
    /// Лист, пушинка и бабочка: с какого возраста они тают.
    fade_from: f64,
    size: f64,
    velocity: [f64; 2],
    drift: [f64; 2],
    position: [f64; 2],
    spin: f64,
    angle: f64,
    frame_roll: u32,
    wobble_period: f64,
    wobble_phase: f64,
    /// Пух: ритм и фаза блуждания вверх-вниз.
    wander_period: f64,
    wander_phase: f64,
    /// Лист и пушинка: установившаяся скорость падения.
    fall: f64,
    brightness: f64,
    /// Лист: нижний край объекта, на который он ложится.
    ground: f64,
    lying: bool,
    flight: Flight,
}

#[derive(Debug)]
struct Source {
    id: u32,
    generation: u32,
    kind: Kind,
    pool: f64,
    rect: Option<Rect>,
    layer: i32,
    parallax: f64,
    picture: Option<Picture>,
    touched: u32,
    particles: Vec<Particle>,
}

impl Source {
    fn new(emitter: &Emitter, effect: Effect, pass: u32) -> Source {
        let mut source = Source {
            id: emitter.id,
            generation: emitter.generation,
            kind: emitter.settings.kind(effect),
            pool: 0.0,
            rect: None,
            layer: 0,
            parallax: 1.0,
            picture: None,
            touched: pass,
            particles: Vec::new(),
        };
        source.follow(emitter, pass);
        source
    }

    fn follow(&mut self, emitter: &Emitter, pass: u32) {
        self.kind = emitter.settings.kind(self.kind.effect);
        self.rect = emitter.rect;
        self.layer = emitter.layer;
        self.parallax = emitter.parallax;
        self.picture = emitter.picture;
        self.touched = pass;
    }

    /// Площадь, с которой рвутся листья: непрозрачная часть картинки, у заливки — весь прямоугольник.
    fn leaf_area(&self, rect: Rect, masks: &[OpaqueMask]) -> f64 {
        match self
            .picture
            .and_then(|picture| Some((picture, masks.get(picture.image)?)))
        {
            Some((picture, mask)) => mask.fraction() * picture.area,
            None => rect.1[0] * rect.1[1],
        }
    }
}

/// Частицы источника, которого больше нет: они доживают с теми параметрами, что были у него в конце.
#[derive(Debug)]
struct Retired {
    kind: Kind,
    particles: Vec<Particle>,
}

#[derive(Clone, Copy)]
struct Surroundings<'a> {
    flat: [f64; 2],
    masks: &'a [OpaqueMask],
}

/// Все источники и частицы мира.
#[derive(Debug)]
pub struct Particles {
    rng: Rng,
    masks: Vec<OpaqueMask>,
    sources: Vec<Source>,
    retired: Vec<Retired>,
    integrated_steps: u64,
    /// Ближайшая сборка мира застанет источники — они прогреются.
    warm: bool,
    pass: u32,
}

impl Default for Particles {
    fn default() -> Particles {
        Particles {
            rng: Rng::new(SEED),
            masks: Vec::new(),
            sources: Vec::new(),
            retired: Vec::new(),
            integrated_steps: 0,
            warm: true,
            pass: 0,
        }
    }
}

impl Particles {
    /// Непрозрачные точки первых кадров картинок игры, по номеру картинки.
    pub(crate) fn set_masks(&mut self, masks: Vec<OpaqueMask>) {
        self.masks = masks;
    }

    /// Часы ушли назад или далеко вперёд, партия началась заново — все частицы исчезают, источники
    /// прогреются на ближайшем кадре. Счётчик случайности не трогается.
    pub fn restart(&mut self, steps: f64) {
        self.sources.clear();
        self.retired.clear();
        self.integrated_steps = steps.floor() as u64;
        self.warm = true;
    }

    /// Живых частиц — у источников и у ушедших.
    pub fn count(&self) -> usize {
        self.retired
            .iter()
            .map(|group| group.particles.len())
            .chain(self.sources.iter().map(|source| source.particles.len()))
            .sum()
    }

    /// Доводит источники и частицы до часов `clock_steps` шагами по 1/60 секунды; остаток часов ждёт
    /// следующего вызова. `flat` — ровный ветер сцены. Параметры эффекта читаются из `emitters` на
    /// каждом кадре: правка настройки меняет и уже вылетевшие частицы. Эффект, которого нет среди
    /// `emitters`, больше не выпускает, его частицы доживают; новый встаёт на часы без вылета —
    /// копилка у него случайная, — а если мир только что собран (`restart` и первая сборка), то
    /// прогретым: будто простоял на месте всё, что живёт его частица. `world_exists` — есть ли мир:
    /// прогрев ждёт его.
    pub fn update(
        &mut self,
        clock_steps: f64,
        world_exists: bool,
        flat: [f64; 2],
        emitters: impl Iterator<Item = Emitter>,
    ) {
        self.pass = self.pass.wrapping_add(1);
        let pass = self.pass;
        let mut fresh = Vec::new();
        for emitter in emitters {
            for effect in Effect::ALL {
                if !emitter.settings.runs(effect) {
                    continue;
                }
                match self.sources.iter_mut().find(|source| {
                    source.id == emitter.id
                        && source.generation == emitter.generation
                        && source.kind.effect == effect
                }) {
                    Some(source) => source.follow(&emitter, pass),
                    None => fresh.push(Source::new(&emitter, effect, pass)),
                }
            }
        }
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
                let around = Surroundings {
                    flat,
                    masks: &self.masks,
                };
                warm_up(&mut source, &mut self.rng, around, due as i64);
            }
            self.sources.push(source);
        }
        if world_exists {
            self.warm = false;
        }
    }

    fn retire_missing_sources(&mut self, pass: u32) {
        let Particles {
            sources, retired, ..
        } = self;
        sources.retain_mut(|source| {
            if source.touched == pass {
                return true;
            }
            if !source.particles.is_empty() {
                retired.push(Retired {
                    kind: source.kind,
                    particles: std::mem::take(&mut source.particles),
                });
            }
            false
        });
    }

    /// Один шаг мира в момент `t` (секунды часов): частицы летят, источники выпускают новые.
    fn step(&mut self, flat: [f64; 2], t: f64) {
        let Particles {
            rng,
            masks,
            sources,
            retired,
            ..
        } = self;
        let around = Surroundings { flat, masks };
        for source in sources.iter_mut() {
            step_source(source, rng, around, t);
        }
        for group in retired.iter_mut() {
            step_particles(&mut group.particles, &group.kind, flat, t);
        }
        retired.retain(|group| !group.particles.is_empty());
    }

    /// Рисунки частиц объекта: у каждого эффекта от старших к младшим, чтобы молодые лежали сверху.
    pub fn live_sprites(&self, id: u32, generation: u32) -> impl Iterator<Item = Sprite> + '_ {
        self.sources
            .iter()
            .filter(move |source| source.id == id && source.generation == generation)
            .flat_map(|source| {
                source
                    .particles
                    .iter()
                    .map(move |particle| sprite_of(particle, &source.kind))
            })
    }

    /// Рисунки частиц ушедших источников и источников без записанного прямоугольника (в порядке
    /// рисования их нет) со `layer` источника на момент вылета.
    pub fn orphan_sprites(&self) -> impl Iterator<Item = (i32, Sprite)> + '_ {
        let unplaced = self
            .sources
            .iter()
            .filter(|source| source.rect.is_none())
            .map(|source| (&source.kind, source.particles.as_slice()));
        self.retired
            .iter()
            .map(|group| (&group.kind, group.particles.as_slice()))
            .chain(unplaced)
            .flat_map(|(kind, particles)| {
                particles
                    .iter()
                    .map(move |particle| (particle.layer, sprite_of(particle, kind)))
            })
    }
}

/// «Ветер и частицы» → «На экране»: частицы экрана интерфейса. Те же источники и частицы, что в мире, но
/// свои часы — время окна, — своя случайность и своя судьба: экран открылся — прежние частицы
/// исчезают, а источники нового экрана прогреты, как объекты сцены на первом кадре.
#[derive(Debug)]
pub struct ScreenParticles {
    particles: Particles,
    /// Какое по счёту открытие экрана показывали в прошлый кадр и какими были часы окна в шагах.
    shown: Option<(u64, f64)>,
}

impl Default for ScreenParticles {
    fn default() -> ScreenParticles {
        ScreenParticles {
            particles: Particles {
                rng: Rng::new(SCREEN_SEED),
                ..Particles::default()
            },
            shown: None,
        }
    }
}

impl ScreenParticles {
    /// Непрозрачные точки первых кадров картинок игры, по номеру картинки.
    pub(crate) fn set_masks(&mut self, masks: Vec<OpaqueMask>) {
        self.particles.set_masks(masks);
    }

    /// Доводит частицы экрана до часов окна `window_steps` (шаги по 1/60 секунды). `opened` — сколько раз
    /// открывался экран: он вырос или часы ушли назад (новая партия в редакторе) — частицы начинаются
    /// заново. `wind` — ровный ветер экрана, `emitters` — панели-источники экрана этого кадра.
    pub fn update(
        &mut self,
        opened: u64,
        window_steps: f64,
        wind: [f64; 2],
        emitters: impl Iterator<Item = Emitter>,
    ) {
        let continues = self.shown.is_some_and(|(seen, steps)| {
            seen == opened && (0.0..=MAX_CATCH_UP_STEPS).contains(&(window_steps - steps))
        });
        if !continues {
            self.particles.restart(window_steps);
        }
        self.shown = Some((opened, window_steps));
        self.particles.update(window_steps, true, wind, emitters);
    }

    pub fn particles(&self) -> &Particles {
        &self.particles
    }

    /// Игра загружена заново: ближайший кадр начинает частицы экрана с нуля и прогревает источники.
    pub(crate) fn forget(&mut self) {
        self.shown = None;
    }
}

/// Источник простоял на месте столько, сколько живёт его частица, при ветре тех мгновений — шаги
/// `due − n + 1 ..= due` часов.
fn warm_up(source: &mut Source, rng: &mut Rng, around: Surroundings, due: i64) {
    let width = source.rect.map_or(0.0, |rect| rect.1[0]);
    let steps = (source.kind.warm_seconds(width) * STEPS_PER_SECOND).ceil() as i64;
    for step in (due - steps + 1)..=due {
        step_source(source, rng, around, step as f64 / STEPS_PER_SECOND);
    }
}

fn step_source(source: &mut Source, rng: &mut Rng, around: Surroundings, t: f64) {
    step_particles(&mut source.particles, &source.kind, around.flat, t);
    let Some(rect) = source.rect else {
        return;
    };
    source.pool += emission_rate(source, rect, around, t) * STEP_SECONDS;
    while source.pool >= 1.0 {
        source.pool -= 1.0;
        let particle = emit(rng, source, rect, around.masks);
        source.particles.push(particle);
    }
}

/// Частота вылета в момент `t`. Листья: на клетку площади непрозрачной части, и чаще в порыв — во
/// столько раз, во сколько `1 + сила ветра` в точке объекта. Пух и бабочки: нужное число частиц
/// одновременно, делённое на среднюю жизнь, — по площади прямоугольника и по его ширине.
fn emission_rate(source: &Source, rect: Rect, around: Surroundings, t: f64) -> f64 {
    match source.kind.effect {
        Effect::Leaves => {
            let [wind_x, wind_y] = wind_vector(around.flat, rect.0[0] + rect.1[0] / 2.0, t);
            source.kind.rate * source.leaf_area(rect, around.masks) * (1.0 + wind_x.hypot(wind_y))
        }
        Effect::Fluff => source.kind.rate * rect.1[0] * rect.1[1] / source.kind.lifetime.at(0.5),
        Effect::Butterflies => source.kind.rate * rect.1[0] / butterfly_mean_life(rect.1[0]),
        Effect::Smoke | Effect::Sparks => source.kind.rate,
    }
}

fn step_particles(particles: &mut Vec<Particle>, kind: &Kind, flat: [f64; 2], t: f64) {
    particles.retain_mut(|particle| {
        let wind = wind_vector(flat, particle.position[0], t);
        advance(particle, kind, wind)
    });
}

fn depth_scale(parallax: f64) -> f64 {
    parallax.clamp(DEPTH_SCALE.0, DEPTH_SCALE.1)
}

/// Что частица берёт наугад при вылете, в одном и том же порядке при любом эффекте — счётчик
/// случайности не зависит от данных.
fn emit(rng: &mut Rng, source: &Source, rect: Rect, masks: &[OpaqueMask]) -> Particle {
    let kind = &source.kind;
    let place = [rng.next_unit(), rng.next_unit()];
    let life_roll = rng.next_unit();
    let width_roll = rng.next_unit();
    let speed_roll = rng.next_unit();
    let spin_roll = rng.next_unit();
    let direction_roll = rng.next_unit();
    let angle_roll = rng.next_unit();
    let frame_bits = rng.next_u64();
    let frame_roll = (frame_bits >> 32) as u32;
    let wobble_roll = rng.next_unit();
    let phase_roll = rng.next_unit();
    let brightness_roll = rng.next_unit();
    let mut extra = Rng::new(frame_bits);
    let more: [f64; EXTRA_ROLLS] = std::array::from_fn(|_| extra.next_unit());

    let scale = depth_scale(source.parallax);
    let direction = (kind.direction + (direction_roll * 2.0 - 1.0) * kind.spread).to_radians();
    let heading = [direction.sin(), -direction.cos()];
    let (life, speed) = match kind.effect {
        Effect::Smoke => {
            let life = kind.lifetime.at(life_roll);
            (life, SMOKE_LAUNCH_FACTOR * kind.rise / life)
        }
        Effect::Sparks => {
            let speed = kind.speed.at(speed_roll);
            (kind.lifetime.at(life_roll) * speed / kind.gravity, speed)
        }
        Effect::Leaves | Effect::Fluff => (kind.lifetime.at(life_roll), 0.0),
        Effect::Butterflies => (f64::INFINITY, 0.0),
    };
    let is_leaf = kind.effect == Effect::Leaves;
    let falls = matches!(kind.effect, Effect::Leaves | Effect::Fluff);
    let mut particle = Particle {
        layer: source.layer,
        parallax: source.parallax,
        age: 0.0,
        life,
        fade_from: match kind.effect {
            Effect::Leaves => LEAF_AIR_SECONDS,
            Effect::Fluff => life - FLUFF_FADE_OUT_SECONDS,
            Effect::Smoke | Effect::Sparks | Effect::Butterflies => life,
        },
        size: kind.size.at(width_roll) * scale,
        velocity: heading.map(|axis| axis * speed),
        drift: [0.0, 0.0],
        position: place_of(source, rect, masks, place),
        spin: kind.spin.at(spin_roll),
        angle: match kind.effect {
            Effect::Sparks | Effect::Butterflies => 0.0,
            Effect::Smoke | Effect::Leaves | Effect::Fluff => angle_roll * 360.0,
        },
        frame_roll,
        wobble_period: kind.rhythm.at(wobble_roll),
        wobble_phase: phase_roll * TAU,
        wander_period: kind.rhythm.at(more[1]),
        wander_phase: more[2] * TAU,
        fall: if falls {
            kind.speed.at(speed_roll) * scale
        } else {
            0.0
        },
        brightness: LEAF_BRIGHTNESS.at(brightness_roll),
        ground: rect.0[1] + rect.1[1],
        lying: false,
        flight: Flight::default(),
    };
    if is_leaf && particle.position[1] >= particle.ground {
        settle(&mut particle, kind);
    }
    if kind.effect == Effect::Butterflies {
        particle.flight = butterfly_flight(rect, place[1], speed_roll, &more);
        particle.position = particle.flight.point();
    }
    particle
}

/// Курс бабочки, решённый при вылете: из-за левого или правого края на случайной высоте к
/// противоположному, с посадками, которые случатся в пути.
fn butterfly_flight(
    rect: Rect,
    height_roll: f64,
    speed_roll: f64,
    more: &[f64; EXTRA_ROLLS],
) -> Flight {
    let ([left, top], [width, height]) = rect;
    let right = left + width;
    let dir = if more[0] < 0.5 { 1.0 } else { -1.0 };
    let amplitude = BUTTERFLY_WEAVE.min(height * 0.25);
    let across = |fraction: f64| {
        if dir > 0.0 {
            left + width * fraction
        } else {
            right - width * fraction
        }
    };
    let stops = std::array::from_fn(|index| {
        let roll = |offset: usize| more[8 + 5 * index + offset];
        let (first, spread) = if index == 0 {
            BUTTERFLY_FIRST_LANDING
        } else {
            BUTTERFLY_SECOND_LANDING
        };
        let at = across(first + spread * roll(1));
        Stop {
            active: roll(0) < BUTTERFLY_LANDING_CHANCE,
            at,
            target: [
                (at + dir * roll(2) * BUTTERFLY_LANDING_REACH).clamp(left, right),
                top + height * (2.0 / 3.0 + roll(3) / 3.0),
            ],
            sit: BUTTERFLY_SIT_SECONDS.at(roll(4)),
        }
    });
    let between = |range: (f64, f64), roll: f64| range.0 + (range.1 - range.0) * roll;
    Flight {
        dir,
        speed: 10.0 / BUTTERFLY_TEN_CELLS_SECONDS.at(speed_roll),
        base_y: top
            + amplitude
            + BUTTERFLY_HOP
            + height_roll * (height - 2.0 * amplitude - BUTTERFLY_HOP).max(0.0),
        weave_amplitude: amplitude,
        weave_period: between(BUTTERFLY_WEAVE_PERIOD, more[3]),
        weave_phase: more[2] * TAU,
        jitter_period: between(BUTTERFLY_JITTER_PERIOD, more[4]),
        jitter_phase: more[5] * TAU,
        flap_hz: BUTTERFLY_FLAP_HZ.at(more[6]),
        sit_flap_hz: BUTTERFLY_SIT_FLAP_HZ.at(more[7]),
        exit_x: across(1.0) + dir * BUTTERFLY_EDGE,
        path_x: across(0.0) - dir * BUTTERFLY_EDGE,
        flap_phase: more[6],
        stops,
        ..Flight::default()
    }
}

/// Где частица вылетает: листья — из случайной непрозрачной точки картинки, всё остальное — из
/// случайной точки прямоугольника.
fn place_of(source: &Source, rect: Rect, masks: &[OpaqueMask], roll: [f64; 2]) -> [f64; 2] {
    let (corner, size) = rect;
    if source.kind.effect == Effect::Leaves
        && let Some(picture) = &source.picture
        && let Some(mask) = masks.get(picture.image)
        && mask.count() > 0
    {
        let pick = roll[0] * f64::from(mask.count());
        let (x, y) = mask.nth((pick.floor() as u32).min(mask.count() - 1));
        return picture.point(
            (f64::from(x) + pick.fract()) / f64::from(mask.width),
            (f64::from(y) + roll[1]) / f64::from(mask.height),
        );
    }
    [corner[0] + roll[0] * size[0], corner[1] + roll[1] * size[1]]
}

/// Лист лёг: не сносится, не качается и не кувыркается, лежит `LEAF_LIE_SECONDS` и тает в последнюю
/// секунду. Лист, который уже тает в воздухе, лежит до конца своего таяния.
fn settle(particle: &mut Particle, kind: &Kind) {
    particle.position[0] += wobble_offset(particle, kind);
    particle.angle += particle.spin * particle.age;
    particle.spin = 0.0;
    particle.velocity = [0.0, 0.0];
    particle.drift = [0.0, 0.0];
    particle.lying = true;
    if particle.age < particle.fade_from {
        particle.fade_from = particle.age + LEAF_LIE_SECONDS - LEAF_MELT_SECONDS;
        particle.life = particle.age + LEAF_LIE_SECONDS;
    }
}

/// Скорость клуба на доле жизни `fraction` в долях начальной: к концу жизни падает втрое.
fn slowdown(fraction: f64) -> f64 {
    1.0 - (1.0 - 1.0 / SMOKE_SLOWDOWN) * fraction
}

/// Один шаг `Δ` = 1/60 секунды при ветре `wind` в точке частицы. Возвращает `false`, когда частица
/// дожила.
fn advance(particle: &mut Particle, kind: &Kind, wind: [f64; 2]) -> bool {
    let before = particle.age / particle.life;
    particle.age += STEP_SECONDS;
    if particle.age >= particle.life {
        return false;
    }
    if particle.lying {
        return true;
    }
    match kind.effect {
        Effect::Smoke => {
            let ratio = slowdown(particle.age / particle.life) / slowdown(before);
            particle.velocity = particle.velocity.map(|axis| axis * ratio);
        }
        Effect::Sparks => particle.velocity[1] += kind.gravity * STEP_SECONDS,
        Effect::Leaves | Effect::Fluff => {
            let accelerated =
                particle.velocity[1] + particle.fall / LEAF_ACCELERATION_SECONDS * STEP_SECONDS;
            particle.velocity[1] = accelerated.min(particle.fall);
        }
        Effect::Butterflies => {
            fly_butterfly(particle, kind, wind);
            return true;
        }
    }
    let catch = STEP_SECONDS / WIND_CATCH_SECONDS;
    for (axis, wind) in wind.into_iter().enumerate() {
        particle.drift[axis] += (kind.wind * wind - particle.drift[axis]) * catch;
        particle.position[axis] += (particle.velocity[axis] + particle.drift[axis]) * STEP_SECONDS;
    }
    if kind.effect == Effect::Leaves && particle.position[1] >= particle.ground {
        particle.position[1] = particle.ground;
        settle(particle, kind);
    }
    true
}

/// Плавный переход от 0 к 1 за долю пути `unit` от 0 до 1.
fn ease(unit: f64) -> f64 {
    let unit = unit.clamp(0.0, 1.0);
    unit * unit * (3.0 - 2.0 * unit)
}

fn lerp(from: [f64; 2], to: [f64; 2], unit: f64) -> [f64; 2] {
    [0, 1].map(|axis| from[axis] + (to[axis] - from[axis]) * unit)
}

/// Один шаг бабочки при ветре `wind` в её точке: ровный ход по пути с вилянием, спуск на место
/// посадки, посадка, взлёт на тот же путь. Ветер сносит бабочку слабо и только в полёте. Дойдя до
/// края, за которым она улетает, бабочка начинает таять.
fn fly_butterfly(particle: &mut Particle, kind: &Kind, wind: [f64; 2]) {
    let catch = STEP_SECONDS / WIND_CATCH_SECONDS;
    for (axis, wind) in wind.into_iter().enumerate() {
        particle.drift[axis] += (kind.wind * wind - particle.drift[axis]) * catch;
    }
    let drift = particle.drift;
    let before = particle.position;
    let flight = &mut particle.flight;
    flight.stage_age += STEP_SECONDS;
    match flight.stage {
        Stage::Fly | Stage::Takeoff => {
            flight.fly_time += STEP_SECONDS;
            flight.flap_phase += flight.flap_hz * STEP_SECONDS;
            let ahead = 1.0
                + BUTTERFLY_JITTER
                    * (TAU * flight.fly_time / flight.jitter_period + flight.jitter_phase).sin();
            flight.path_x += flight.dir * flight.speed * ahead * STEP_SECONDS;
            for (offset, drift) in flight.wind_offset.iter_mut().zip(drift) {
                *offset += drift * STEP_SECONDS;
            }
            let path = flight.point();
            if flight.stage == Stage::Takeoff {
                let unit = flight.stage_age / flight.stage_length;
                particle.position = lerp(flight.from, path, ease(unit));
                if unit >= 1.0 {
                    flight.stage = Stage::Fly;
                }
            } else {
                particle.position = path;
            }
        }
        Stage::Descend => {
            flight.flap_phase += flight.flap_hz * STEP_SECONDS;
            let unit = flight.stage_age / flight.stage_length;
            let stop = flight.stops[flight.next_stop];
            particle.position = lerp(flight.from, stop.target, ease(unit));
            if unit >= 1.0 {
                flight.stage = Stage::Sit;
                flight.stage_age = 0.0;
                flight.stage_length = stop.sit;
                flight.wind_offset = [0.0, 0.0];
            }
        }
        Stage::Sit => {
            flight.flap_phase += flight.sit_flap_hz * STEP_SECONDS;
            let stop = flight.stops[flight.next_stop];
            particle.position = stop.target;
            if flight.stage_age >= flight.stage_length {
                flight.stage = Stage::Takeoff;
                flight.stage_age = 0.0;
                flight.stage_length = BUTTERFLY_TAKEOFF_SECONDS;
                flight.from = stop.target;
                flight.path_x = stop.target[0];
                flight.next_stop += 1;
                flight.clear_the_way();
            }
        }
    }
    let slope = (particle.position[1] - before[1]) / STEP_SECONDS / flight.speed.max(0.1);
    let tilt = slope
        .atan()
        .to_degrees()
        .clamp(-BUTTERFLY_TILT_LIMIT, BUTTERFLY_TILT_LIMIT);
    flight.tilt += (tilt - flight.tilt) * (STEP_SECONDS / BUTTERFLY_TILT_SECONDS).min(1.0);
    particle.velocity = [0, 1].map(|axis| (particle.position[axis] - before[axis]) / STEP_SECONDS);

    if !matches!(flight.stage, Stage::Fly | Stage::Takeoff) {
        return;
    }
    while flight.next_stop < flight.stops.len() && !flight.stops[flight.next_stop].active {
        flight.next_stop += 1;
    }
    if flight.dir * (flight.path_x - flight.exit_x) >= 0.0 {
        if particle.fade_from.is_infinite() {
            particle.fade_from = particle.age;
            particle.life = particle.age + BUTTERFLY_FADE_SECONDS;
        }
    } else if flight.stage == Stage::Fly && flight.stop_reached() {
        flight.start_descent(particle.position);
    }
}

/// Просвечивание на доле жизни `fraction` — точки через равные доли, между ними по прямой.
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

/// На сколько клеток лист ушёл вбок от места, где сорвался: при рождении ровно на нуль, дальше размах
/// растёт и падает вместе с `parallax` объекта.
fn wobble_offset(particle: &Particle, kind: &Kind) -> f64 {
    let swing = (TAU * particle.age / particle.wobble_period + particle.wobble_phase).sin()
        - particle.wobble_phase.sin();
    kind.wobble * depth_scale(particle.parallax) * swing
}

/// На сколько клеток пушинка ушла вверх-вниз от места, где родилась: при рождении ровно на нуль.
fn bob_offset(particle: &Particle, kind: &Kind) -> f64 {
    let swing = (TAU * particle.age / particle.wander_period + particle.wander_phase).sin()
        - particle.wander_phase.sin();
    kind.wander * depth_scale(particle.parallax) * swing
}

fn leaf_opacity(particle: &Particle) -> f64 {
    let melting = particle.life - particle.fade_from;
    1.0 - ((particle.age - particle.fade_from) / melting).clamp(0.0, 1.0)
}

/// Пушинка и бабочка проявляются за `fade_in` секунд и тают от `fade_from` до конца жизни.
fn envelope(particle: &Particle, fade_in: f64) -> f64 {
    let rising = (particle.age / fade_in).min(1.0);
    if particle.age < particle.fade_from {
        return rising;
    }
    rising * leaf_opacity(particle)
}

/// Кадр взмаха крыльев бабочки: за один взмах кадры идут по кругу.
fn flap_frame(flight: &Flight) -> u32 {
    (flight.flap_phase.rem_euclid(1.0) * f64::from(ParticleShape::Butterfly.frames())) as u32
}

/// Угол бабочки: нарисована она носом вправо, летящая влево отражается (`Sprite::flip_x`), и наклон
/// носа к горизонту у отражённой — с обратным знаком.
fn butterfly_angle(flight: &Flight) -> f64 {
    if flight.dir >= 0.0 {
        flight.tilt
    } else {
        -flight.tilt
    }
}

fn sprite_of(particle: &Particle, kind: &Kind) -> Sprite {
    let fraction = particle.age / particle.life;
    let shape = kind.shape();
    let (opacity, color) = match kind.effect {
        Effect::Smoke => (
            opacity_at(&kind.opacity, fraction),
            kind.tint.unwrap_or(WHITE),
        ),
        Effect::Sparks => (opacity_at(&kind.opacity, fraction), spark_color(fraction)),
        Effect::Leaves => (
            leaf_opacity(particle),
            kind.tint.map_or(WHITE, |tint| {
                tint.map(|channel| channel * particle.brightness as f32)
            }),
        ),
        Effect::Fluff => (
            envelope(particle, FLUFF_FADE_IN_SECONDS) * kind.opacity[0],
            kind.tint.unwrap_or(WHITE),
        ),
        Effect::Butterflies => (
            envelope(particle, BUTTERFLY_FADE_SECONDS),
            kind.tint
                .unwrap_or(BUTTERFLY_COLORS[particle.frame_roll as usize % BUTTERFLY_COLORS.len()]),
        ),
    };
    let sway = if particle.lying {
        0.0
    } else {
        wobble_offset(particle, kind)
    };
    let (frame, angle) = match kind.effect {
        Effect::Butterflies => (
            flap_frame(&particle.flight),
            butterfly_angle(&particle.flight),
        ),
        _ => (
            particle.frame_roll % shape.frames(),
            particle.angle + particle.spin * particle.age,
        ),
    };
    Sprite {
        shape,
        frame,
        center: [
            particle.position[0] + sway,
            particle.position[1] + bob_offset(particle, kind),
        ],
        width: particle.size * (1.0 + (kind.grow - 1.0) * fraction),
        angle,
        flip_x: kind.effect == Effect::Butterflies && particle.flight.dir < 0.0,
        opacity,
        color,
        parallax: particle.parallax,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::particles::LEAF_BRIGHTNESS;

    fn near(a: f64, b: f64, tolerance: f64) -> bool {
        (a - b).abs() <= tolerance
    }

    fn base() -> Settings {
        Settings {
            smoke_color: [0.5, 0.5, 0.5],
            ..Settings::default()
        }
    }

    fn smoking() -> Settings {
        Settings {
            smoke: 0.5,
            ..base()
        }
    }

    fn sparking() -> Settings {
        Settings {
            sparks: 0.5,
            ..base()
        }
    }

    fn leafing() -> Settings {
        Settings {
            leaf_fall: 0.3,
            ..base()
        }
    }

    const CHIMNEY: Rect = ([10.0, 10.0], [2.0, 1.0]);
    const CROWN: Rect = ([20.0, 5.0], [8.0, 4.0]);

    fn emitter(id: u32, settings: Settings) -> Emitter {
        Emitter {
            id,
            generation: 0,
            rect: Some(CHIMNEY),
            layer: 0,
            parallax: 1.0,
            settings,
            picture: None,
        }
    }

    fn crown(settings: Settings, picture: Option<Picture>) -> Emitter {
        Emitter {
            rect: Some(CROWN),
            picture,
            ..emitter(0, settings)
        }
    }

    fn source_of(settings: Settings, effect: Effect) -> Source {
        Source::new(&emitter(0, settings), effect, 0)
    }

    fn leaf_source(settings: Settings, picture: Option<Picture>) -> Source {
        Source::new(&crown(settings, picture), Effect::Leaves, 0)
    }

    fn launch(source: &Source, rng: &mut Rng) -> Particle {
        emit(rng, source, source.rect.expect("есть прямоугольник"), &[])
    }

    fn fly(particle: &mut Particle, kind: &Kind, wind: [f64; 2]) -> usize {
        let mut steps = 0;
        while advance(particle, kind, wind) {
            steps += 1;
        }
        steps
    }

    /// Мир уже был, когда появляются источники: ничего не прогрето.
    fn calm_world() -> Particles {
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], std::iter::empty());
        particles
    }

    fn run(particles: &mut Particles, emitters: &[Emitter], from: u64, steps: u64) {
        for step in from..from + steps {
            particles.update(
                (step + 1) as f64,
                true,
                [0.0, 0.0],
                emitters.iter().copied(),
            );
        }
    }

    fn sprites(particles: &Particles) -> Vec<Sprite> {
        particles.live_sprites(0, 0).collect()
    }

    fn half_mask() -> OpaqueMask {
        let (width, height) = (8u32, 4u32);
        let mut pixels = vec![0u8; (width * height * 4) as usize];
        for y in 0..height {
            for x in width / 2..width {
                pixels[((y * width + x) * 4 + 3) as usize] = 255;
            }
        }
        OpaqueMask::of_first_frame(&pixels, width, width, height)
    }

    fn picture(flip: bool) -> Picture {
        let (origin, across) = if flip {
            ([28.0, 5.0], [-8.0, 0.0])
        } else {
            ([20.0, 5.0], [8.0, 0.0])
        };
        Picture {
            image: 0,
            area: 32.0,
            origin,
            across,
            down: [0.0, 4.0],
        }
    }

    #[test]
    fn the_opaque_points_are_half_alpha_and_up_and_lie_in_the_first_frame_only() {
        let mask = half_mask();
        assert_eq!(mask.count(), 16);
        assert_eq!(mask.fraction(), 0.5);
        let mut seen: Vec<(u32, u32)> = (0..mask.count()).map(|i| mask.nth(i)).collect();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), 16);
        assert!(seen.iter().all(|(x, y)| (4..8).contains(x) && *y < 4));

        let mut pixels = vec![0u8; 16 * 4 * 4];
        let alpha = |x: u32, y: u32| ((y * 16 + x) * 4 + 3) as usize;
        pixels[alpha(0, 0)] = 127;
        pixels[alpha(1, 0)] = 128;
        pixels[alpha(12, 2)] = 255;
        let first = OpaqueMask::of_first_frame(&pixels, 16, 8, 4);
        assert_eq!((first.count(), first.nth(0)), (1, (1, 0)));
    }

    #[test]
    fn a_leaf_is_torn_only_from_the_opaque_points_of_the_picture_and_a_mirror_swaps_the_sides() {
        let masks = [half_mask()];
        for (flip, sides) in [(false, 24.0..=28.0), (true, 20.0..=24.0)] {
            let source = leaf_source(leafing(), Some(picture(flip)));
            let mut rng = Rng::new(21);
            for _ in 0..200 {
                let leaf = emit(&mut rng, &source, CROWN, &masks);
                let [x, y] = sprite_of(&leaf, &source.kind).center;
                assert!(sides.contains(&x), "{flip}: {leaf:?}");
                assert!((5.0..=9.0).contains(&y));
            }
        }
    }

    #[test]
    fn a_leaf_of_an_object_without_a_picture_is_torn_from_anywhere_in_the_rectangle() {
        let source = leaf_source(leafing(), None);
        let mut rng = Rng::new(22);
        let xs: Vec<f64> = (0..200)
            .map(|_| sprite_of(&emit(&mut rng, &source, CROWN, &[]), &source.kind).center[0])
            .collect();
        assert!(xs.iter().all(|x| (20.0..=28.0).contains(x)));
        assert!(xs.iter().any(|x| *x < 22.0) && xs.iter().any(|x| *x > 26.0));
    }

    #[test]
    fn a_picture_without_a_single_opaque_point_drops_no_leaves() {
        let empty = OpaqueMask::of_first_frame(&[0u8; 8 * 4 * 4], 8, 8, 4);
        let mut particles = Particles::default();
        particles.set_masks(vec![empty]);
        let around = crown(leafing(), Some(picture(false)));
        particles.update(0.0, true, [0.0; 2], [around].into_iter());
        run(&mut particles, &[around], 0, 600);
        assert_eq!(particles.count(), 0);
    }

    #[test]
    fn the_smoke_of_calm_air_melts_at_its_height() {
        for height in [4.0, 2.0] {
            let source = source_of(
                Settings {
                    smoke_height: height,
                    ..smoking()
                },
                Effect::Smoke,
            );
            let mut rng = Rng::new(1);
            for _ in 0..50 {
                let mut puff = launch(&source, &mut rng);
                let start = puff.position[1];
                fly(&mut puff, &source.kind, [0.0, 0.0]);
                let rise = start - puff.position[1];
                assert!(
                    (height - 0.5..=height + 0.5).contains(&rise),
                    "высота {height}: поднялся на {rise}"
                );
            }
        }
    }

    #[test]
    fn the_smoke_slows_down_to_a_third_as_it_rises() {
        let source = source_of(smoking(), Effect::Smoke);
        let mut puff = launch(&source, &mut Rng::new(2));
        let before = puff.velocity[1].abs();
        for _ in 0..(puff.life * 60.0) as usize - 2 {
            advance(&mut puff, &source.kind, [0.0, 0.0]);
        }
        assert!(near(puff.velocity[1].abs() / before, 1.0 / 3.0, 0.02));
    }

    #[test]
    fn the_wind_carries_the_smoke_sideways_within_a_second() {
        let source = source_of(smoking(), Effect::Smoke);
        let shift = |wind_x: f64| {
            let mut puff = launch(&source, &mut Rng::new(3));
            let start = puff.position[0];
            for _ in 0..60 {
                advance(&mut puff, &source.kind, [wind_x, 0.0]);
            }
            puff.position[0] - start
        };
        let carried = shift(1.0) - shift(0.0);
        assert!((0.4..0.7).contains(&carried), "{carried}");
    }

    #[test]
    fn a_puff_is_wider_at_the_end_pales_to_nothing_and_has_the_colour_of_the_smoke() {
        let source = source_of(smoking(), Effect::Smoke);
        let mut puff = launch(&source, &mut Rng::new(4));
        let young = sprite_of(&puff, &source.kind);
        assert!((0.45..=0.6).contains(&young.width));
        assert!(near(young.opacity, 0.5, 1e-9));
        puff.age = puff.life * 0.999;
        let old = sprite_of(&puff, &source.kind);
        assert!(near(old.width / young.width, 3.5, 0.05));
        assert!(old.opacity < 0.01);
        assert_eq!(old.shape, ParticleShape::Smoke);
        assert_eq!(old.color, [0.5, 0.5, 0.5]);
    }

    #[test]
    fn the_smoke_comes_twelve_puffs_a_second_at_half_density_and_none_at_zero() {
        let mut particles = calm_world();
        run(&mut particles, &[emitter(0, smoking())], 0, 180);
        let emitted = particles.count();
        assert!((35..=37).contains(&emitted), "{emitted}");

        let mut none = calm_world();
        let off = Settings {
            smoke: 0.0,
            ..smoking()
        };
        run(&mut none, &[emitter(0, off)], 0, 600);
        assert_eq!(none.count(), 0);
    }

    #[test]
    fn the_fastest_spark_flung_straight_up_rises_exactly_the_reach() {
        let source = source_of(
            Settings {
                sparks_spread: 0.0,
                ..sparking()
            },
            Effect::Sparks,
        );
        let mut rng = Rng::new(6);
        let speed = |spark: &Particle| spark.velocity[1].abs();
        let mut fastest = (0..300)
            .map(|_| launch(&source, &mut rng))
            .max_by(|a, b| speed(a).total_cmp(&speed(b)))
            .expect("есть искры");
        fastest.life = 100.0;
        let start = fastest.position[1];
        let mut top = start;
        for _ in 0..200 {
            advance(&mut fastest, &source.kind, [0.0, 0.0]);
            top = top.min(fastest.position[1]);
        }
        assert!((1.4..=1.55).contains(&(start - top)), "{}", start - top);
    }

    #[test]
    fn a_spark_goes_out_after_eight_to_fourteen_tenths_of_its_time_to_the_top() {
        let source = source_of(sparking(), Effect::Sparks);
        let mut rng = Rng::new(7);
        for _ in 0..200 {
            let spark = launch(&source, &mut rng);
            let speed = spark.velocity[0].hypot(spark.velocity[1]);
            let share = spark.life / (speed / source.kind.gravity);
            assert!((0.8 - 1e-9..=1.4 + 1e-9).contains(&share), "{share}");
        }
    }

    #[test]
    fn a_spark_flies_where_the_direction_points_around_the_circle() {
        let direction_of = |direction: f64| {
            let source = source_of(
                Settings {
                    sparks_direction: direction,
                    sparks_spread: 0.0,
                    ..sparking()
                },
                Effect::Sparks,
            );
            let spark = launch(&source, &mut Rng::new(8));
            spark.velocity[0].atan2(-spark.velocity[1]).to_degrees()
        };
        assert!(near(direction_of(0.0), 0.0, 1e-9));
        assert!(near(direction_of(90.0), 90.0, 1e-9));
        assert!(near(direction_of(450.0), 90.0, 1e-9));
        assert!(near(direction_of(-90.0), -90.0, 1e-9));
    }

    #[test]
    fn a_spark_cools_from_yellow_to_red_and_glows() {
        let source = source_of(sparking(), Effect::Sparks);
        let mut spark = launch(&source, &mut Rng::new(9));
        let hot = sprite_of(&spark, &source.kind);
        spark.age = spark.life * 0.9;
        let cool = sprite_of(&spark, &source.kind);
        assert_eq!(hot.shape, ParticleShape::Spark);
        assert!(cool.color[1] / cool.color[0] < hot.color[1] / hot.color[0] - 0.3);
        assert!(cool.opacity < hot.opacity);
        assert_eq!(hot.angle, 0.0);
    }

    #[test]
    fn the_wind_carries_a_spark_less_than_a_puff_of_smoke() {
        let drift_after_a_second = |settings: Settings, effect: Effect| {
            let source = source_of(settings, effect);
            let mut particle = launch(&source, &mut Rng::new(10));
            particle.life = 100.0;
            for _ in 0..60 {
                advance(&mut particle, &source.kind, [2.0, 0.0]);
            }
            particle.drift[0]
        };
        let puff = drift_after_a_second(smoking(), Effect::Smoke);
        let spark = drift_after_a_second(sparking(), Effect::Sparks);
        assert!(spark > 0.0 && spark < puff / 3.0, "{spark} {puff}");
    }

    #[test]
    fn a_leaf_lands_on_the_bottom_edge_lies_two_seconds_and_goes() {
        let source = leaf_source(leafing(), None);
        let ground = CROWN.0[1] + CROWN.1[1];
        let mut rng = Rng::new(11);
        for _ in 0..10 {
            let mut leaf = emit(&mut rng, &source, CROWN, &[]);
            let mut guard = 0;
            while !leaf.lying {
                assert!(advance(&mut leaf, &source.kind, [0.0, 0.0]));
                guard += 1;
                assert!(guard < 60 * 30, "лист не долетел");
            }
            assert_eq!(leaf.position[1], ground);
            let landed = leaf.position;
            for _ in 0..60 {
                assert!(advance(&mut leaf, &source.kind, [3.0, 0.0]));
            }
            assert_eq!(leaf.position, landed, "лежит и не сносится");
            assert!(near(sprite_of(&leaf, &source.kind).opacity, 1.0, 1e-9));
            for _ in 0..30 {
                advance(&mut leaf, &source.kind, [3.0, 0.0]);
            }
            let melting = sprite_of(&leaf, &source.kind).opacity;
            assert!(near(melting, 0.5, 0.02), "{melting}");
            let left = fly(&mut leaf, &source.kind, [3.0, 0.0]);
            assert!((28..=31).contains(&left), "{left}");
        }
    }

    #[test]
    fn a_leaf_falls_swaying_and_tumbling_until_it_lands() {
        let source = leaf_source(leafing(), None);
        let mut leaf = emit(&mut Rng::new(12), &source, CROWN, &[]);
        leaf.position[1] = 5.0;
        let first = sprite_of(&leaf, &source.kind);
        assert_eq!(first.center, leaf.position, "при рождении без сдвига");
        let mut sways = Vec::new();
        for _ in 0..180 {
            advance(&mut leaf, &source.kind, [0.0, 0.0]);
            sways.push(sprite_of(&leaf, &source.kind).center[0] - first.center[0]);
        }
        let later = sprite_of(&leaf, &source.kind);
        assert!(later.center[1] > first.center[1] + 0.5, "падает");
        assert!(later.angle != first.angle, "кувыркается");
        let (low, high) = sways
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), s| (l.min(*s), h.max(*s)));
        assert!(
            high - low > 0.9 && low >= -1.0 - 1e-9 && high <= 1.0 + 1e-9,
            "{low}..{high}"
        );
    }

    #[test]
    fn a_leaf_lands_where_it_is_drawn() {
        let source = leaf_source(leafing(), None);
        let mut rng = Rng::new(16);
        for _ in 0..10 {
            let mut leaf = emit(&mut rng, &source, CROWN, &[]);
            let mut last = sprite_of(&leaf, &source.kind).center;
            while !leaf.lying {
                assert!(advance(&mut leaf, &source.kind, [0.0, 0.0]));
                let drawn = sprite_of(&leaf, &source.kind).center;
                if leaf.lying {
                    assert!(near(drawn[0], last[0], 0.1), "{drawn:?} против {last:?}");
                    assert_eq!(drawn[0], leaf.position[0]);
                }
                last = drawn;
            }
        }
    }

    #[test]
    fn a_leaf_a_strong_updraft_holds_melts_after_thirty_seconds_where_it_is() {
        let tall = Emitter {
            rect: Some(([0.0, 0.0], [2.0, 40.0])),
            ..emitter(0, leafing())
        };
        let source = Source::new(&tall, Effect::Leaves, 0);
        let mut leaf = emit(&mut Rng::new(13), &source, tall.rect.unwrap(), &[]);
        leaf.position[1] = 20.0;
        let mut seconds = 0.0;
        let mut half = None;
        while advance(&mut leaf, &source.kind, [0.0, -6.0]) {
            seconds += 1.0 / 60.0;
            if half.is_none() && seconds >= 30.5 {
                half = Some(sprite_of(&leaf, &source.kind).opacity);
            }
            assert!(!leaf.lying);
        }
        assert!(near(seconds, 31.0, 0.1), "{seconds}");
        assert!(near(half.expect("дожил до 30,5"), 0.5, 0.05));
    }

    #[test]
    fn a_leaf_torn_below_the_line_lies_where_it_was_torn() {
        let low = crown(
            leafing(),
            Some(Picture {
                origin: [20.0, 9.5],
                ..picture(false)
            }),
        );
        let source = Source::new(&low, Effect::Leaves, 0);
        let masks = [OpaqueMask::of_first_frame(&[255u8; 8 * 4 * 4], 8, 8, 4)];
        let leaf = emit(&mut Rng::new(14), &source, CROWN, &masks);
        assert!(leaf.lying);
        let [x, y] = sprite_of(&leaf, &source.kind).center;
        assert!(y > 9.0);
        assert!((20.0..=28.0).contains(&x), "{x}");
        assert!(near(leaf.fade_from, leaf.life - 1.0, 1e-9));
        assert!(near(leaf.life, 2.0, 1e-9));
    }

    #[test]
    fn a_leaf_is_narrower_in_proportion_to_the_parallax_of_its_object() {
        let width_at = |parallax: f64| {
            let layer = Emitter {
                parallax,
                ..emitter(0, leafing())
            };
            let source = Source::new(&layer, Effect::Leaves, 0);
            let leaf = emit(&mut Rng::new(15), &source, CHIMNEY, &[]);
            (sprite_of(&leaf, &source.kind).width, leaf.fall)
        };
        let (plain, plain_fall) = width_at(1.0);
        let (far, far_fall) = width_at(0.5);
        assert!(near(far, plain * 0.5, 1e-12));
        assert!(near(far_fall, plain_fall * 0.5, 1e-12));
        assert!(near(width_at(0.01).0, plain * 0.2, 1e-12), "зажат снизу");
        assert!(near(width_at(9.0).0, plain * 3.0, 1e-12), "зажат сверху");
    }

    #[test]
    fn the_parallax_scales_the_smoke_and_sparks_widths_but_not_their_height() {
        let layer = Emitter {
            parallax: 0.5,
            ..emitter(
                0,
                Settings {
                    sparks: 0.5,
                    ..smoking()
                },
            )
        };
        let smoke = Source::new(&layer, Effect::Smoke, 0);
        let mut puff = emit(&mut Rng::new(16), &smoke, CHIMNEY, &[]);
        assert!((0.225..=0.3).contains(&puff.size), "{}", puff.size);
        let start = puff.position[1];
        fly(&mut puff, &smoke.kind, [0.0, 0.0]);
        assert!((3.5..=4.5).contains(&(start - puff.position[1])));
        let sparks = Source::new(&layer, Effect::Sparks, 0);
        let spark = emit(&mut Rng::new(16), &sparks, CHIMNEY, &[]);
        assert!((0.06..=0.1).contains(&spark.size));
    }

    #[test]
    fn a_leaf_colour_turns_the_leaf_white_and_tints_each_leaf_with_its_own_brightness() {
        let tint = [0.8, 0.2, 0.1];
        let coloured = Settings {
            leaf_color: Some(tint),
            ..leafing()
        };
        let source = leaf_source(coloured, None);
        let mut rng = Rng::new(17);
        let mut seen = Vec::new();
        for _ in 0..50 {
            let leaf = emit(&mut rng, &source, CROWN, &[]);
            let sprite = sprite_of(&leaf, &source.kind);
            assert_eq!(sprite.shape, ParticleShape::WhiteLeaf);
            let brightness = f64::from(sprite.color[0] / tint[0]);
            assert!(
                (LEAF_BRIGHTNESS.from - 1e-6..=LEAF_BRIGHTNESS.to + 1e-6).contains(&brightness)
            );
            assert!(near(f64::from(sprite.color[1] / tint[1]), brightness, 1e-5));
            seen.push(brightness);
        }
        assert!(seen.iter().any(|b| *b < 0.95) && seen.iter().any(|b| *b > 1.0));

        let autumn = leaf_source(leafing(), None);
        let leaf = emit(&mut rng, &autumn, CROWN, &[]);
        let sprite = sprite_of(&leaf, &autumn.kind);
        assert_eq!((sprite.shape, sprite.color), (ParticleShape::Leaf, WHITE));
    }

    #[test]
    fn the_leaves_come_oftener_in_a_gust() {
        let tall = Emitter {
            rect: Some(([0.0, 0.0], [2.0, 30.0])),
            ..emitter(
                0,
                Settings {
                    leaf_fall: 1.0,
                    ..base()
                },
            )
        };
        let torn_in = |wind: f64| {
            let mut particles = calm_world();
            for step in 0..300u64 {
                particles.update((step + 1) as f64, true, [wind, 0.0], [tall].into_iter());
            }
            particles.count()
        };
        let (calm, windy) = (torn_in(0.0), torn_in(3.0));
        assert!(calm > 15, "{calm}");
        assert!(windy > calm * 2, "{calm} {windy}");
    }

    #[test]
    fn the_leaf_rate_is_a_tenth_of_the_density_per_cell_of_the_opaque_part() {
        let around = crown(
            Settings {
                leaf_fall: 1.0,
                ..base()
            },
            Some(picture(false)),
        );
        let mut particles = calm_world();
        particles.set_masks(vec![half_mask()]);
        particles.update(1.0, true, [0.0, 0.0], [around].into_iter());
        let source = &particles.sources[0];
        let area = source.leaf_area(CROWN, &particles.masks);
        assert!(near(area, 16.0, 1e-9), "половина от 32 клеток");
        assert!(near(source.kind.rate * area, 1.6, 1e-9));
    }

    #[test]
    fn an_object_with_smoke_and_sparks_emits_both() {
        let mut particles = calm_world();
        let both = emitter(
            0,
            Settings {
                smoke: 0.5,
                sparks: 0.5,
                ..base()
            },
        );
        run(&mut particles, &[both], 0, 120);
        let shapes: Vec<ParticleShape> = sprites(&particles).iter().map(|s| s.shape).collect();
        assert!(shapes.contains(&ParticleShape::Smoke));
        assert!(shapes.contains(&ParticleShape::Spark));
    }

    #[test]
    fn a_source_found_on_the_first_assembly_is_warmed_up_and_one_that_appears_later_is_not() {
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        let smoke = particles.count();
        assert!((25..=70).contains(&smoke), "{smoke}");

        let mut leaves = Particles::default();
        leaves.update(0.0, true, [0.0; 2], [crown(leafing(), None)].into_iter());
        assert!(leaves.count() > 3, "{}", leaves.count());

        let mut later = calm_world();
        later.update(120.0, true, [0.0; 2], [emitter(3, smoking())].into_iter());
        assert_eq!(later.count(), 0, "новый источник — ноль на первом кадре");
    }

    #[test]
    fn the_warm_up_waits_for_a_world() {
        let mut particles = Particles::default();
        particles.update(0.0, false, [0.0; 2], std::iter::empty());
        particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        assert!(particles.count() >= 25);
    }

    #[test]
    fn a_restart_clears_everything_and_warms_the_sources_again() {
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        particles.update(30.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        assert!(particles.count() > 0);
        particles.restart(10.0);
        assert_eq!(particles.count(), 0);
        particles.update(10.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        assert!(particles.count() >= 25);
    }

    #[test]
    fn a_stopped_clock_moves_nothing() {
        let mut particles = Particles::default();
        let around = [emitter(0, smoking())];
        particles.update(60.0, true, [1.5, 0.0], around.into_iter());
        let before = sprites(&particles);
        for _ in 0..5 {
            particles.update(60.0, true, [1.5, 0.0], around.into_iter());
        }
        assert_eq!(before, sprites(&particles));
    }

    #[test]
    fn an_effect_taken_away_stops_and_its_particles_live_out() {
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        let live = particles.count();
        particles.update(1.0, true, [0.0; 2], std::iter::empty());
        assert!(particles.count() > 0 && particles.count() <= live);
        assert_eq!(sprites(&particles).len(), 0);
        assert_eq!(particles.orphan_sprites().count(), particles.count());
        particles.update(60.0 * 8.0, true, [0.0; 2], std::iter::empty());
        assert_eq!(particles.count(), 0, "дожили и исчезли");
    }

    #[test]
    fn the_density_set_to_zero_and_back_starts_the_effect_from_nothing() {
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        let off = Settings {
            smoke: 0.0,
            ..smoking()
        };
        particles.update(1.0, true, [0.0; 2], [emitter(0, off)].into_iter());
        assert_eq!(sprites(&particles).len(), 0);
        particles.update(2.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        assert_eq!(sprites(&particles).len(), 0, "как новый");
    }

    #[test]
    fn a_setting_changed_on_the_go_changes_the_particles_in_flight_but_does_not_restart_them() {
        let mut particles = Particles::default();
        particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
        let before = sprites(&particles);
        let denser = Settings {
            smoke: 1.0,
            smoke_color: [1.0, 0.0, 0.0],
            ..smoking()
        };
        particles.update(0.0, true, [0.0; 2], [emitter(0, denser)].into_iter());
        let after = sprites(&particles);
        assert_eq!(before.len(), after.len(), "не перезапущен");
        assert!(after.iter().all(|s| s.color == [1.0, 0.0, 0.0]));
        assert!(before.iter().all(|s| s.color == [0.5, 0.5, 0.5]));
        assert_eq!(
            before.iter().map(|s| s.center).collect::<Vec<_>>(),
            after.iter().map(|s| s.center).collect::<Vec<_>>()
        );
        let middle = |sprites: &[Sprite]| {
            sprites
                .iter()
                .find(|s| s.width > 0.9)
                .map(|s| s.opacity)
                .expect("есть клуб в возрасте")
        };
        assert!(middle(&after) > middle(&before));
    }

    #[test]
    fn a_source_without_a_rectangle_emits_nothing_and_its_particles_are_drawn_apart() {
        let mut particles = calm_world();
        let mut placed = emitter(0, smoking());
        run(&mut particles, &[placed], 0, 120);
        let live = particles.count();
        assert!(live > 0);
        placed.rect = None;
        run(&mut particles, &[placed], 120, 60);
        assert!(particles.count() <= live);
        assert_eq!(sprites(&particles).len(), particles.count());
        assert_eq!(particles.orphan_sprites().count(), particles.count());
    }

    #[test]
    fn the_random_counter_of_the_particles_is_the_same_in_every_engine() {
        let draw = || {
            let mut particles = Particles::default();
            particles.update(0.0, true, [0.0; 2], [emitter(0, smoking())].into_iter());
            sprites(&particles)
                .into_iter()
                .map(|sprite| sprite.center)
                .collect::<Vec<_>>()
        };
        assert_eq!(draw(), draw());
    }

    #[test]
    fn every_effect_takes_the_same_numbers_from_the_random_counter() {
        let left = |settings: Settings, effect: Effect| {
            let source = source_of(settings, effect);
            let mut rng = Rng::new(31);
            launch(&source, &mut rng);
            rng.next_u64()
        };
        let smoke = left(smoking(), Effect::Smoke);
        assert_eq!(smoke, left(sparking(), Effect::Sparks));
        assert_eq!(smoke, left(leafing(), Effect::Leaves));
        assert_eq!(smoke, left(fluffing(), Effect::Fluff));
        assert_eq!(smoke, left(fluttering(), Effect::Butterflies));
    }

    fn fluffing() -> Settings {
        Settings {
            fluff: 1.0,
            ..base()
        }
    }

    fn fluttering() -> Settings {
        Settings {
            butterflies: 1.0,
            ..base()
        }
    }

    const MEADOW: Rect = ([0.0, 0.0], [10.0, 3.0]);

    fn meadow(settings: Settings, rect: Rect) -> Emitter {
        Emitter {
            rect: Some(rect),
            ..emitter(0, settings)
        }
    }

    /// Пушинка, выпущенная над лугом.
    fn fluff_of(seed: u64) -> (Particle, Kind) {
        let source = Source::new(&meadow(fluffing(), MEADOW), Effect::Fluff, 0);
        (emit(&mut Rng::new(seed), &source, MEADOW, &[]), source.kind)
    }

    /// Бабочка, выпущенная над прямоугольником `rect`.
    fn butterfly_of(seed: u64, rect: Rect) -> (Particle, Kind) {
        let source = Source::new(&meadow(fluttering(), rect), Effect::Butterflies, 0);
        (emit(&mut Rng::new(seed), &source, rect, &[]), source.kind)
    }

    /// Ход частицы до конца жизни: секунды, место и действие на каждом шаге.
    fn follow(particle: &mut Particle, kind: &Kind, wind: [f64; 2]) -> Vec<(f64, [f64; 2], Stage)> {
        let mut track = Vec::new();
        while advance(particle, kind, wind) {
            track.push((particle.age, particle.position, particle.flight.stage));
            assert!(track.len() < 60 * 120, "бабочка не улетает");
        }
        track
    }

    fn sitting(track: &[(f64, [f64; 2], Stage)]) -> Vec<&(f64, [f64; 2], Stage)> {
        track
            .iter()
            .filter(|(_, _, stage)| *stage == Stage::Sit)
            .collect()
    }

    /// Посадок за пролёт: сколько раз ход входил в посадку.
    fn landings(track: &[(f64, [f64; 2], Stage)]) -> usize {
        track
            .windows(2)
            .filter(|pair| pair[0].2 != Stage::Sit && pair[1].2 == Stage::Sit)
            .count()
    }

    #[test]
    fn a_fluff_is_born_in_the_rectangle_fades_in_for_half_a_second_and_melts_in_the_last_second() {
        let source = Source::new(&meadow(fluffing(), MEADOW), Effect::Fluff, 0);
        let mut rng = Rng::new(40);
        for _ in 0..100 {
            let mut puff = emit(&mut rng, &source, MEADOW, &[]);
            let [x, y] = puff.position;
            assert!(
                (0.0..=10.0).contains(&x) && (0.0..=3.0).contains(&y),
                "{x} {y}"
            );
            assert!((6.0..=10.0).contains(&puff.life), "{}", puff.life);
            assert!(near(puff.fade_from, puff.life - 1.0, 1e-9));
            assert_eq!(
                sprite_of(&puff, &source.kind).opacity,
                0.0,
                "в нуле, пока не проявилась"
            );
            puff.age = 0.25;
            let half_way = sprite_of(&puff, &source.kind).opacity;
            puff.age = 0.5;
            let seen = sprite_of(&puff, &source.kind).opacity;
            assert!(
                near(half_way * 2.0, seen, 1e-9) && seen > 0.0,
                "{half_way} {seen}"
            );
            puff.age = puff.life - 0.5;
            assert!(near(
                sprite_of(&puff, &source.kind).opacity,
                seen / 2.0,
                1e-9
            ));
            puff.age = puff.life - 1e-6;
            assert!(sprite_of(&puff, &source.kind).opacity < 1e-4);
        }
    }

    #[test]
    fn a_fluff_glows_and_is_the_new_built_in_picture() {
        let (puff, kind) = fluff_of(41);
        let sprite = sprite_of(&puff, &kind);
        assert_eq!(sprite.shape, ParticleShape::Fluff);
        assert_eq!(sprite.color, [1.0, 0.96, 0.86], "белый с тёплым отливом");
        let tinted = Settings {
            fluff_color: [0.2, 0.4, 0.9],
            ..fluffing()
        };
        let kind = tinted.kind(Effect::Fluff);
        assert_eq!(sprite_of(&puff, &kind).color, [0.2, 0.4, 0.9]);
    }

    #[test]
    fn a_fluff_hangs_in_calm_air_barely_sinking_and_turning() {
        let (mut puff, kind) = fluff_of(42);
        puff.age = 1.0;
        let start = sprite_of(&puff, &kind);
        for _ in 0..(5.0 * 60.0) as usize {
            assert!(advance(&mut puff, &kind, [0.0, 0.0]));
        }
        let later = sprite_of(&puff, &kind);
        assert!(
            (later.center[1] - start.center[1]).abs() < 0.8,
            "висит: {} → {}",
            start.center[1],
            later.center[1]
        );
        assert!(later.angle != start.angle, "кружится");
        let (low, high) = (0..300).fold((f64::MAX, f64::MIN), |(low, high), _| {
            advance(&mut puff, &kind, [0.0, 0.0]);
            let y = sprite_of(&puff, &kind).center[1];
            (low.min(y), high.max(y))
        });
        assert!(high - low > 0.05, "блуждает вверх-вниз: {low}..{high}");
    }

    #[test]
    fn a_fluff_in_a_steady_wind_of_one_cell_a_second_flies_at_least_eight_tenths_of_it() {
        let (mut puff, kind) = fluff_of(43);
        puff.life = 1000.0;
        puff.fade_from = 1000.0;
        for _ in 0..(6.0 * 60.0) as usize {
            advance(&mut puff, &kind, [1.0, 0.0]);
        }
        assert!(puff.drift[0] >= 0.8, "{}", puff.drift[0]);
        let start = puff.position[0];
        for _ in 0..60 {
            advance(&mut puff, &kind, [1.0, 0.0]);
        }
        assert!(
            puff.position[0] - start >= 0.8,
            "{}",
            puff.position[0] - start
        );
    }

    #[test]
    fn the_wind_carries_a_fluff_more_than_any_other_particle() {
        let share = |settings: Settings, effect: Effect| settings.kind(effect).wind;
        let fluff = share(fluffing(), Effect::Fluff);
        for (settings, effect) in [
            (smoking(), Effect::Smoke),
            (sparking(), Effect::Sparks),
            (leafing(), Effect::Leaves),
            (fluttering(), Effect::Butterflies),
        ] {
            assert!(fluff > share(settings, effect), "{effect:?}");
        }
    }

    #[test]
    fn a_fluff_at_full_density_holds_three_to_a_cell_of_area_on_average() {
        let mut particles = calm_world();
        let around = meadow(fluffing(), MEADOW);
        let mut counts = Vec::new();
        for step in 0..60 * 40u64 {
            particles.update((step + 1) as f64, true, [0.0, 0.0], [around].into_iter());
            if step > 60 * 12 && step % 30 == 0 {
                counts.push(particles.count() as f64);
            }
        }
        let average = counts.iter().sum::<f64>() / counts.len() as f64;
        assert!(
            (78.0..=102.0).contains(&average),
            "30 клеток × 3 = 90: {average}"
        );
    }

    #[test]
    fn a_tenth_of_the_density_is_a_third_of_a_fluff_to_a_cell() {
        let mut particles = calm_world();
        let around = meadow(
            Settings {
                fluff: 0.1,
                ..base()
            },
            MEADOW,
        );
        let mut counts = Vec::new();
        for step in 0..60 * 120u64 {
            particles.update((step + 1) as f64, true, [0.0, 0.0], [around].into_iter());
            if step > 60 * 12 && step % 30 == 0 {
                counts.push(particles.count() as f64);
            }
        }
        let average = counts.iter().sum::<f64>() / counts.len() as f64;
        assert!(
            (7.5..=10.5).contains(&average),
            "30 клеток × 0,3 = 9: {average}"
        );
    }

    #[test]
    fn a_butterfly_flies_in_from_beyond_one_edge_on_any_height_and_out_beyond_the_other() {
        let mut seen = [0, 0];
        let mut heights = Vec::new();
        for seed in 0..120 {
            let (mut fly, kind) = butterfly_of(seed, MEADOW);
            let dir = fly.flight.dir;
            let start = fly.position;
            seen[(dir > 0.0) as usize] += 1;
            heights.push(start[1]);
            assert!((0.0..=3.0).contains(&start[1]), "{start:?}");
            if dir > 0.0 {
                assert!(start[0] < 0.0, "влетает слева: {start:?}");
            } else {
                assert!(start[0] > 10.0, "влетает справа: {start:?}");
            }
            let track = follow(&mut fly, &kind, [0.0, 0.0]);
            let end = track.last().expect("полёт").1;
            if dir > 0.0 {
                assert!(end[0] > 10.0, "улетает направо: {end:?}");
            } else {
                assert!(end[0] < 0.0, "улетает налево: {end:?}");
            }
        }
        assert!(seen[0] > 30 && seen[1] > 30, "с обеих сторон: {seen:?}");
        let (low, high) = heights
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(high - low > 1.0, "высоты разные: {low}..{high}");
    }

    #[test]
    fn a_butterfly_fades_in_for_half_a_second_and_melts_for_half_a_second_beyond_the_edge() {
        let (mut fly, kind) = butterfly_of(3, MEADOW);
        assert_eq!(sprite_of(&fly, &kind).opacity, 0.0);
        let mut fade_in = None;
        let mut melt = None;
        while advance(&mut fly, &kind, [0.0, 0.0]) {
            let opacity = sprite_of(&fly, &kind).opacity;
            if fade_in.is_none() && opacity >= 1.0 {
                fade_in = Some(fly.age);
            }
            if melt.is_none() && fly.fade_from.is_finite() {
                melt = Some(fly.age);
            }
        }
        assert!(near(fade_in.expect("проявилась"), 0.5, 0.02));
        let melt_start = melt.expect("растаяла");
        assert!(
            near(fly.age - melt_start, 0.5, 0.02),
            "{} {melt_start}",
            fly.age
        );
        let end = fly.position[0];
        assert!(
            end > 10.0 + 0.2 || end < -0.2,
            "тает уже за краем прямоугольника: {end}"
        );
    }

    #[test]
    fn ten_cells_of_width_take_six_to_twelve_seconds_of_flight() {
        let mut slowest: f64 = 0.0;
        let mut fastest = f64::MAX;
        for seed in 0..80 {
            let (mut fly, kind) = butterfly_of(seed, MEADOW);
            for stop in &mut fly.flight.stops {
                stop.active = false;
            }
            let dir = fly.flight.dir;
            let track = follow(&mut fly, &kind, [0.0, 0.0]);
            let inside = |x: f64| if dir > 0.0 { x >= 0.0 } else { x <= 10.0 };
            let past = |x: f64| if dir > 0.0 { x >= 10.0 } else { x <= 0.0 };
            let entered = track
                .iter()
                .find(|(_, at, _)| inside(at[0]))
                .expect("вошла");
            let left = track.iter().find(|(_, at, _)| past(at[0])).expect("вышла");
            let crossing = left.0 - entered.0;
            slowest = slowest.max(crossing);
            fastest = fastest.min(crossing);
            assert!((5.4..=12.6).contains(&crossing), "{crossing}");
        }
        assert!(fastest < 7.5 && slowest > 10.5, "{fastest}..{slowest}");
    }

    #[test]
    fn the_wings_beat_eight_to_twelve_times_a_second_each_butterfly_in_its_own_rhythm() {
        let mut rates = Vec::new();
        for seed in 0..40 {
            let (mut fly, kind) = butterfly_of(seed, MEADOW);
            for stop in &mut fly.flight.stops {
                stop.active = false;
            }
            fly.age = 1.0;
            let mut frames = Vec::new();
            for _ in 0..60 {
                advance(&mut fly, &kind, [0.0, 0.0]);
                frames.push(sprite_of(&fly, &kind).frame);
            }
            let beats = frames
                .windows(2)
                .filter(|pair| pair[0] == 3 && pair[1] == 0)
                .count();
            rates.push(fly.flight.flap_hz);
            assert!((8.0..=12.0).contains(&fly.flight.flap_hz));
            assert!(
                (beats as f64 - fly.flight.flap_hz).abs() <= 1.0,
                "{beats} взмахов при {}",
                fly.flight.flap_hz
            );
            assert!(frames.iter().all(|frame| *frame < 4));
        }
        let (low, high) = rates
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), r| (l.min(*r), h.max(*r)));
        assert!(high - low > 1.5, "ритмы разные: {low}..{high}");
    }

    #[test]
    fn the_path_zigzags_up_and_down_with_a_hop_on_every_beat() {
        let (mut fly, kind) = butterfly_of(5, ([0.0, 0.0], [10.0, 4.0]));
        for stop in &mut fly.flight.stops {
            stop.active = false;
        }
        let track = follow(&mut fly, &kind, [0.0, 0.0]);
        let ys: Vec<f64> = track.iter().map(|(_, at, _)| at[1]).collect();
        let (low, high) = ys
            .iter()
            .fold((f64::MAX, f64::MIN), |(l, h), y| (l.min(*y), h.max(*y)));
        assert!(high - low > 0.3, "виляет вверх-вниз: {low}..{high}");
        let turns = ys
            .windows(3)
            .filter(|w| (w[1] - w[0]) * (w[2] - w[1]) < 0.0)
            .count();
        assert!(turns > 40, "подскоки на каждый взмах: {turns}");
        let xs: Vec<f64> = track.iter().map(|(_, at, _)| at[0]).collect();
        let back = xs
            .windows(2)
            .filter(|w| (w[1] - w[0]) * fly.flight.dir < 0.0)
            .count();
        assert!(back > 0, "чуть вперёд-назад");
        assert!(near(xs[xs.len() - 1] - xs[0], fly.flight.dir * 11.0, 2.5));
    }

    #[test]
    fn a_landing_goes_to_the_lower_third_sits_one_to_three_seconds_and_takes_off_on_the_same_course()
     {
        let (mut fly, kind) = butterfly_of(6, MEADOW);
        fly.flight.stops[0].active = true;
        fly.flight.stops[1].active = false;
        let dir = fly.flight.dir;
        let track = follow(&mut fly, &kind, [0.0, 0.0]);
        let sat = sitting(&track);
        assert!(!sat.is_empty(), "села");
        assert_eq!(landings(&track), 1);
        let seconds = sat.len() as f64 / 60.0;
        assert!((1.0..=3.05).contains(&seconds), "{seconds}");
        for (_, at, _) in &sat {
            assert!((2.0..=3.0).contains(&at[1]), "нижняя треть: {at:?}");
            assert!((0.0..=10.0).contains(&at[0]));
            assert_eq!(*at, sat[0].1, "сидит на месте");
        }
        let end = track.last().expect("полёт").1;
        assert!((end[0] - sat[0].1[0]) * dir > 0.0, "взлетела тем же курсом");
        assert!(end[0] > 10.0 || end[0] < 0.0, "и улетела за край");
    }

    #[test]
    fn after_a_landing_the_butterfly_flies_on_before_the_next_one_and_the_next_one_lies_ahead() {
        for width in [2.0, 3.0, 5.0, 6.0, 8.0, 9.0, 10.0, 14.0] {
            for seed in 0..300 {
                let (mut fly, kind) = butterfly_of(seed, ([0.0, 0.0], [width, 3.0]));
                for stop in &mut fly.flight.stops {
                    stop.active = true;
                }
                let dir = fly.flight.dir;
                let track = follow(&mut fly, &kind, [0.0, 0.0]);
                let sits: Vec<f64> = track
                    .windows(2)
                    .filter(|pair| pair[0].2 != Stage::Sit && pair[1].2 == Stage::Sit)
                    .map(|pair| pair[1].1[0])
                    .collect();
                for pair in sits.windows(2) {
                    assert!(
                        (pair[1] - pair[0]) * dir > 0.0,
                        "ширина {width}, зерно {seed}: вторая посадка не впереди первой: {sits:?}"
                    );
                }
                let mut left_the_seat = None;
                for pair in track.windows(2) {
                    if pair[0].2 == Stage::Sit && pair[1].2 != Stage::Sit {
                        left_the_seat = Some(pair[1].0);
                    }
                    if pair[0].2 != Stage::Descend
                        && pair[1].2 == Stage::Descend
                        && let Some(took_off) = left_the_seat.take()
                    {
                        assert!(
                            pair[1].0 - took_off >= BUTTERFLY_TAKEOFF_SECONDS - 1e-6,
                            "ширина {width}, зерно {seed}: села снова через {} с после взлёта",
                            pair[1].0 - took_off
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_wings_fold_and_unfold_once_or_twice_a_second_on_a_landing() {
        let (mut fly, kind) = butterfly_of(7, MEADOW);
        fly.flight.stops[0].active = true;
        fly.flight.stops[1].active = false;
        fly.flight.stops[0].sit = 3.0;
        let mut frames = Vec::new();
        while advance(&mut fly, &kind, [0.0, 0.0]) {
            if fly.flight.stage == Stage::Sit {
                frames.push(sprite_of(&fly, &kind).frame);
            }
        }
        let cycles = frames
            .windows(2)
            .filter(|pair| pair[0] == 3 && pair[1] == 0)
            .count();
        assert!(
            (3..=6).contains(&cycles),
            "за три секунды 3–6 взмахов: {cycles}"
        );
        assert!((1.0..=2.0).contains(&fly.flight.sit_flap_hz));
    }

    #[test]
    fn a_butterfly_lands_once_on_average_and_never_more_than_twice() {
        let mut total = 0;
        let flights = 300;
        for seed in 0..flights {
            let (mut fly, kind) = butterfly_of(seed, MEADOW);
            let track = follow(&mut fly, &kind, [0.0, 0.0]);
            let count = landings(&track);
            assert!(count <= 2, "{count}");
            total += count;
        }
        let average = total as f64 / flights as f64;
        assert!((0.85..=1.15).contains(&average), "{average}");
    }

    #[test]
    fn the_wind_pushes_a_butterfly_weakly_and_does_not_move_it_on_a_landing() {
        let (_, kind) = butterfly_of(8, MEADOW);
        assert!(kind.wind <= 0.15, "{}", kind.wind);
        let (mut fly, kind) = butterfly_of(8, MEADOW);
        for stop in &mut fly.flight.stops {
            stop.active = false;
        }
        let (mut calm, calm_kind) = butterfly_of(8, MEADOW);
        for stop in &mut calm.flight.stops {
            stop.active = false;
        }
        let windy = follow(&mut fly, &kind, [3.0, 0.0]);
        let still = follow(&mut calm, &calm_kind, [0.0, 0.0]);
        let shift = windy.last().unwrap().1[0] - still.last().unwrap().1[0];
        let seconds = windy.last().unwrap().0;
        assert!(
            shift.abs() <= 0.15 * 3.0 * seconds + 0.5,
            "ветер в 3 клетки унёс на {shift} за {seconds} с"
        );

        let (mut sitter, kind) = butterfly_of(9, MEADOW);
        sitter.flight.stops[0].active = true;
        sitter.flight.stops[1].active = false;
        let track = follow(&mut sitter, &kind, [5.0, 0.0]);
        let sat = sitting(&track);
        assert!(!sat.is_empty());
        assert!(
            sat.iter().all(|(_, at, _)| *at == sat[0].1),
            "ветер не сдвигает севшую"
        );
    }

    #[test]
    fn a_butterfly_looks_where_it_flies() {
        for seed in 0..40 {
            let (mut fly, kind) = butterfly_of(seed, MEADOW);
            let dir = fly.flight.dir;
            for _ in 0..90 {
                advance(&mut fly, &kind, [0.0, 0.0]);
            }
            let sprite = sprite_of(&fly, &kind);
            let angle = sprite.angle.rem_euclid(360.0);
            assert!(
                angle <= 31.0 || angle >= 329.0,
                "нос по курсу, тельцем вниз: {angle}"
            );
            assert_eq!(sprite.flip_x, dir < 0.0, "влево — отражена, вправо — нет");
        }
    }

    #[test]
    fn a_rectangle_narrower_than_a_cell_still_sees_butterflies_come_and_go_and_sit_low() {
        let narrow: Rect = ([4.0, 1.0], [0.6, 2.0]);
        for seed in 0..40 {
            let (mut fly, kind) = butterfly_of(seed, narrow);
            fly.flight.stops[0].active = true;
            let dir = fly.flight.dir;
            assert!(fly.position[0] * dir < (4.0 + if dir > 0.0 { 0.0 } else { 0.6 }) * dir);
            let track = follow(&mut fly, &kind, [0.0, 0.0]);
            let end = track.last().expect("полёт").1;
            assert!(
                (end[0] - 4.3) * dir > 0.3,
                "улетела за противоположный край: {end:?}"
            );
            for (_, at, _) in sitting(&track) {
                assert!((1.0 + 2.0 * 2.0 / 3.0..=3.0).contains(&at[1]), "{at:?}");
                assert!((4.0..=4.6).contains(&at[0]), "{at:?}");
            }
        }
    }

    #[test]
    fn the_butterflies_over_a_meadow_keep_a_third_of_the_width_at_full_and_a_tenth_of_it_at_a_tenth()
     {
        for (amount, expected) in [(1.0, 10.0 / 3.0), (0.1, 1.0 / 3.0)] {
            let around = meadow(
                Settings {
                    butterflies: amount,
                    ..base()
                },
                MEADOW,
            );
            let mut particles = calm_world();
            let mut counts = Vec::new();
            for step in 0..60 * 400u64 {
                particles.update((step + 1) as f64, true, [0.0, 0.0], [around].into_iter());
                if step > 60 * 30 && step % 30 == 0 {
                    counts.push(particles.count() as f64);
                }
            }
            let average = counts.iter().sum::<f64>() / counts.len() as f64;
            assert!(
                near(average, expected, expected * 0.2),
                "при {amount}: ждали {expected}, в среднем {average}"
            );
        }
    }

    #[test]
    fn butterflies_widen_with_the_rectangle() {
        let count = |width: f64| {
            let around = meadow(fluttering(), ([0.0, 0.0], [width, 3.0]));
            let mut particles = calm_world();
            let mut sum = 0.0;
            let mut n = 0.0;
            for step in 0..60 * 300u64 {
                particles.update((step + 1) as f64, true, [0.0, 0.0], [around].into_iter());
                if step > 60 * 40 && step % 30 == 0 {
                    sum += particles.count() as f64;
                    n += 1.0;
                }
            }
            sum / n
        };
        let (narrow, wide) = (count(6.0), count(30.0));
        assert!(near(narrow, 2.0, 0.5), "{narrow}");
        assert!(near(wide, 10.0, 2.0), "{wide}");
    }

    #[test]
    fn a_butterfly_source_found_on_the_first_assembly_is_warmed_up_with_butterflies_on_the_way() {
        let mut particles = Particles::default();
        particles.update(
            0.0,
            true,
            [0.0; 2],
            [meadow(fluttering(), MEADOW)].into_iter(),
        );
        assert!(particles.count() >= 1, "{}", particles.count());
        let mut fluff = Particles::default();
        fluff.update(
            0.0,
            true,
            [0.0; 2],
            [meadow(fluffing(), MEADOW)].into_iter(),
        );
        assert!(fluff.count() > 60, "{}", fluff.count());
    }

    #[test]
    fn a_butterfly_set_to_zero_flies_out_and_a_fluff_taken_away_melts() {
        let mut particles = Particles::default();
        let on = meadow(fluttering(), MEADOW);
        particles.update(0.0, true, [0.0; 2], [on].into_iter());
        let flying = particles.count();
        assert!(flying > 0);
        let off = meadow(
            Settings {
                butterflies: 0.0,
                ..fluttering()
            },
            MEADOW,
        );
        particles.update(1.0, true, [0.0; 2], [off].into_iter());
        assert_eq!(particles.live_sprites(0, 0).count(), 0);
        assert!(particles.count() > 0 && particles.count() <= flying);
        particles.update(60.0 * 40.0, true, [0.0; 2], [off].into_iter());
        assert_eq!(particles.count(), 0, "дожили и улетели");
    }

    #[test]
    fn the_screen_particles_start_over_when_the_screen_opens_again_and_follow_the_window_clock() {
        let mut screen = ScreenParticles::default();
        let around = meadow(fluffing(), MEADOW);
        screen.update(1, 600.0, [0.0; 2], [around].into_iter());
        let warm = screen.particles().count();
        assert!(warm > 60, "источник прогрет на первом кадре: {warm}");
        screen.update(1, 606.0, [0.0; 2], [around].into_iter());
        assert!(screen.particles().count().abs_diff(warm) < 30);
        screen.update(2, 612.0, [0.0; 2], std::iter::empty());
        assert_eq!(screen.particles().count(), 0, "экран сменился — частиц нет");
        screen.update(3, 618.0, [0.0; 2], [around].into_iter());
        assert!(screen.particles().count() > 60, "снова прогрето");
        screen.update(3, 10.0, [0.0; 2], [around].into_iter());
        assert!(
            screen.particles().count() > 60,
            "часы ушли назад — всё заново и прогрето"
        );
        screen.update(3, 60.0 * 600.0, [0.0; 2], [around].into_iter());
        assert!(
            screen.particles().count() > 60,
            "вкладка стояла — не догоняется шагами"
        );
        screen.update(3, 60.0 * 600.0 + 1.0, [0.0; 2], std::iter::empty());
        screen.forget();
        screen.update(3, 60.0 * 600.0 + 2.0, [0.0; 2], [around].into_iter());
        assert!(
            screen.particles().count() > 60,
            "игра загружена заново — всё с нуля и прогрето"
        );
    }

    #[test]
    fn the_screen_has_a_random_counter_of_its_own() {
        let world = |seed_of: fn() -> Particles| {
            let mut particles = seed_of();
            particles.update(
                0.0,
                true,
                [0.0; 2],
                [meadow(fluffing(), MEADOW)].into_iter(),
            );
            sprites(&particles)
                .into_iter()
                .map(|s| s.center)
                .collect::<Vec<_>>()
        };
        let in_the_world = world(Particles::default);
        let mut screen = ScreenParticles::default();
        screen.update(0, 0.0, [0.0; 2], [meadow(fluffing(), MEADOW)].into_iter());
        let on_the_screen: Vec<[f64; 2]> = screen
            .particles()
            .live_sprites(0, 0)
            .map(|s| s.center)
            .collect();
        assert_ne!(in_the_world, on_the_screen);
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
    fn the_depth_scale_is_clamped_like_the_sway() {
        assert_eq!(depth_scale(0.0), 0.2);
        assert_eq!(depth_scale(1.0), 1.0);
        assert_eq!(depth_scale(7.0), 3.0);
    }
}

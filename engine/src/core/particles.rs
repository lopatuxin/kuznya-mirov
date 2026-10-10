//! «Ветер и частицы» → «Частицы», «Дым и искры», «Листопад»: девять свойств объекта и то, что движок
//! из них считает про каждый эффект, — частоту, жизнь, размер, скорость, тяжесть, долю ветра,
//! просвечивание и цвет частиц.

use super::property;
use super::world::World;

/// «Число или пара»: каждая частица берёт своё значение наугад между краями; число — оба края равны.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub from: f64,
    pub to: f64,
}

impl Span {
    pub const fn single(value: f64) -> Span {
        Span {
            from: value,
            to: value,
        }
    }

    /// Значение на доле `unit` отрезка, `unit` от 0 до 1.
    pub fn at(self, unit: f64) -> f64 {
        self.from + (self.to - self.from) * unit
    }
}

/// Встроенный рисунок частиц: движок рисует его сам при сборке атласа, в список картинок игры он не
/// входит. Дым, искра и белый лист окрашиваются цветом прямоугольника; осенний лист рисуется своими
/// цветами.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleShape {
    Smoke,
    Spark,
    Leaf,
    WhiteLeaf,
    Halo,
}

impl ParticleShape {
    /// Кадров в рисунке: частица листа берёт кадр наугад при вылете и держит его всю жизнь.
    pub const fn frames(self) -> u32 {
        match self {
            ParticleShape::Leaf | ParticleShape::WhiteLeaf => 4,
            ParticleShape::Smoke | ParticleShape::Spark | ParticleShape::Halo => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Effect {
    Smoke,
    Sparks,
    Leaves,
}

impl Effect {
    pub const ALL: [Effect; 3] = [Effect::Smoke, Effect::Sparks, Effect::Leaves];
}

pub type Rgb = [f32; 3];

/// Серый нынешнего дыма: рисунок клуба белый, этот цвет делает его серым.
const DEFAULT_SMOKE_COLOR: Rgb = [166.0 / 255.0, 166.0 / 255.0, 172.0 / 255.0];
const DEFAULT_SMOKE_HEIGHT: f64 = 4.0;
const DEFAULT_SPARKS_REACH: f64 = 1.5;
const DEFAULT_SPARKS_SPREAD: f64 = 30.0;

const SMOKE_RATE: (f64, f64) = (2.0, 20.0);
const SMOKE_LIFETIME: Span = Span { from: 4.0, to: 5.5 };
const SMOKE_SIZE: Span = Span {
    from: 0.45,
    to: 0.6,
};
const SMOKE_GROW: f64 = 3.5;
const SMOKE_SPREAD: f64 = 12.0;
/// Начальная скорость клуба — во столько раз больше `smoke_height / жизнь`: к концу жизни она падает
/// втрое, и клуб поднимается ровно на `smoke_height`.
pub const SMOKE_LAUNCH_FACTOR: f64 = 1.5;
/// Во сколько раз скорость клуба к концу жизни меньше начальной.
pub const SMOKE_SLOWDOWN: f64 = 3.0;
const SMOKE_SPIN: Span = Span {
    from: -20.0,
    to: 20.0,
};
const SMOKE_WIND: f64 = 1.0;

const SPARKS_RATE: (f64, f64) = (2.0, 20.0);
const SPARK_GRAVITY: f64 = 2.5;
const SPARK_SPEED_SHARE: Span = Span { from: 0.7, to: 1.0 };
const SPARK_RISE_SHARE: Span = Span { from: 0.8, to: 1.4 };
const SPARK_SIZE: Span = Span {
    from: 0.12,
    to: 0.2,
};
const SPARK_WIND: f64 = 0.15;
const SPARK_HOT: Rgb = [1.0, 0.85, 0.25];
const SPARK_COOL: Rgb = [0.95, 0.15, 0.08];

const LEAF_RATE_PER_CELL: f64 = 0.1;
/// Секунд в воздухе, после которых лист тает там, где он есть.
pub const LEAF_AIR_SECONDS: f64 = 30.0;
/// Секунд, за которые лист тает.
pub const LEAF_MELT_SECONDS: f64 = 1.0;
/// Секунд, которые лист лежит на земле, считая таяние.
pub const LEAF_LIE_SECONDS: f64 = 2.0;
/// Секунд, за которые лист набирает установившуюся скорость падения.
pub const LEAF_ACCELERATION_SECONDS: f64 = 1.0;
const LEAF_SIZE: Span = Span {
    from: 0.18,
    to: 0.28,
};
const LEAF_FALL_SPEED: Span = Span { from: 0.4, to: 0.7 };
const LEAF_WOBBLE: f64 = 0.5;
const LEAF_SPIN: Span = Span {
    from: -120.0,
    to: 120.0,
};
const LEAF_WIND: f64 = 0.8;
/// Яркость цвета листьев, на которую каждый лист множит `leaf_color`.
pub const LEAF_BRIGHTNESS: Span = Span {
    from: 0.8,
    to: 1.15,
};

/// Настройки частиц объекта, прочитанные из его свойств. Значения вне допустимых отрезков
/// подтягиваются к ним: рисование данные не отвергает.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Settings {
    pub smoke: f64,
    pub smoke_height: f64,
    pub smoke_color: Rgb,
    pub sparks: f64,
    pub sparks_reach: f64,
    pub sparks_direction: f64,
    pub sparks_spread: f64,
    pub leaf_fall: f64,
    /// `None` — осенние листья вперемешку.
    pub leaf_color: Option<Rgb>,
}

impl Settings {
    pub fn read(world: &World, id: u32) -> Settings {
        let number = |prop| world.number_like(id, prop).filter(|n| !n.is_nan());
        let color = |prop| world.color(id, prop).map(|[r, g, b, _]| [r, g, b]);
        let positive = |prop, default: f64| {
            number(prop)
                .filter(|n| n.is_finite() && *n > 0.0)
                .unwrap_or(default)
        };
        Settings {
            smoke: number(property::SMOKE).map_or(0.0, |n| n.clamp(0.0, 1.0)),
            smoke_height: positive(property::SMOKE_HEIGHT, DEFAULT_SMOKE_HEIGHT),
            smoke_color: color(property::SMOKE_COLOR).unwrap_or(DEFAULT_SMOKE_COLOR),
            sparks: number(property::SPARKS).map_or(0.0, |n| n.clamp(0.0, 1.0)),
            sparks_reach: positive(property::SPARKS_REACH, DEFAULT_SPARKS_REACH),
            sparks_direction: number(property::SPARKS_DIRECTION)
                .filter(|n| n.is_finite())
                .unwrap_or(0.0),
            sparks_spread: number(property::SPARKS_SPREAD)
                .map_or(DEFAULT_SPARKS_SPREAD, |n| n.clamp(0.0, 180.0)),
            leaf_fall: number(property::LEAF_FALL).map_or(0.0, |n| n.clamp(0.0, 1.0)),
            leaf_color: color(property::LEAF_COLOR),
        }
    }

    /// Главное свойство эффекта: пока оно больше нуля, эффект идёт.
    fn amount(&self, effect: Effect) -> f64 {
        match effect {
            Effect::Smoke => self.smoke,
            Effect::Sparks => self.sparks,
            Effect::Leaves => self.leaf_fall,
        }
    }

    pub fn runs(&self, effect: Effect) -> bool {
        self.amount(effect) > 0.0
    }

    pub fn any_runs(&self) -> bool {
        Effect::ALL.into_iter().any(|effect| self.runs(effect))
    }

    /// Внутренние параметры частиц эффекта `effect`.
    pub fn kind(&self, effect: Effect) -> Kind {
        match effect {
            Effect::Smoke => self.smoke_kind(),
            Effect::Sparks => self.sparks_kind(),
            Effect::Leaves => self.leaves_kind(),
        }
    }

    fn smoke_kind(&self) -> Kind {
        let density = 0.25 + 0.5 * self.smoke;
        Kind {
            effect: Effect::Smoke,
            rate: SMOKE_RATE.0 + SMOKE_RATE.1 * self.smoke,
            lifetime: SMOKE_LIFETIME,
            size: SMOKE_SIZE,
            grow: SMOKE_GROW,
            speed: Span::single(0.0),
            rise: self.smoke_height,
            direction: 0.0,
            spread: SMOKE_SPREAD,
            gravity: 0.0,
            wind: SMOKE_WIND,
            opacity: [density, 0.6 * density, 0.0],
            spin: SMOKE_SPIN,
            wobble: 0.0,
            tint: Some(self.smoke_color),
        }
    }

    fn sparks_kind(&self) -> Kind {
        let fastest = (2.0 * SPARK_GRAVITY * self.sparks_reach).sqrt();
        Kind {
            effect: Effect::Sparks,
            rate: SPARKS_RATE.0 + SPARKS_RATE.1 * self.sparks,
            lifetime: SPARK_RISE_SHARE,
            size: SPARK_SIZE,
            grow: 1.0,
            speed: Span {
                from: SPARK_SPEED_SHARE.from * fastest,
                to: SPARK_SPEED_SHARE.to * fastest,
            },
            rise: 0.0,
            direction: self.sparks_direction,
            spread: self.sparks_spread,
            gravity: SPARK_GRAVITY,
            wind: SPARK_WIND,
            opacity: [1.0, 1.0, 0.0],
            spin: Span::single(0.0),
            wobble: 0.0,
            tint: None,
        }
    }

    fn leaves_kind(&self) -> Kind {
        Kind {
            effect: Effect::Leaves,
            rate: LEAF_RATE_PER_CELL * self.leaf_fall,
            lifetime: Span::single(LEAF_AIR_SECONDS + LEAF_MELT_SECONDS),
            size: LEAF_SIZE,
            grow: 1.0,
            speed: LEAF_FALL_SPEED,
            rise: 0.0,
            direction: 0.0,
            spread: 0.0,
            gravity: 0.0,
            wind: LEAF_WIND,
            opacity: [1.0, 1.0, 1.0],
            spin: LEAF_SPIN,
            wobble: LEAF_WOBBLE,
            tint: self.leaf_color,
        }
    }
}

/// Внутренние параметры частиц одного эффекта.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kind {
    pub effect: Effect,
    /// Частиц в секунду; у листьев — в секунду на клетку площади непрозрачной части.
    pub rate: f64,
    /// Секунды жизни; у искр — доли времени подъёма до верхней точки.
    pub lifetime: Span,
    /// Ширина в клетках до поправки на `parallax`.
    pub size: Span,
    pub grow: f64,
    /// Скорость вылета; у листьев — установившаяся скорость падения; у дыма скорость выводится из
    /// `rise`.
    pub speed: Span,
    /// Дым: на сколько клеток поднимается клуб.
    pub rise: f64,
    pub direction: f64,
    pub spread: f64,
    pub gravity: f64,
    /// Доля ветра в точке частицы, которую она подхватывает.
    pub wind: f64,
    /// Просвечивание через равные доли жизни.
    pub opacity: [f64; 3],
    pub spin: Span,
    /// Размах качания листа вбок в клетках до поправки на `parallax`.
    pub wobble: f64,
    /// Цвет, которым окрашен рисунок; `None` — у искры цвет идёт по жизни, у листа осенние цвета.
    pub tint: Option<Rgb>,
}

impl Kind {
    pub fn shape(&self) -> ParticleShape {
        match (self.effect, self.tint) {
            (Effect::Smoke, _) => ParticleShape::Smoke,
            (Effect::Sparks, _) => ParticleShape::Spark,
            (Effect::Leaves, Some(_)) => ParticleShape::WhiteLeaf,
            (Effect::Leaves, None) => ParticleShape::Leaf,
        }
    }

    /// Сколько секунд источник должен простоять на месте, чтобы выпустить всё, что успеет жить:
    /// столько он прогревается.
    pub fn warm_seconds(&self) -> f64 {
        match self.effect {
            Effect::Sparks => self.lifetime.to * self.speed.to / self.gravity,
            Effect::Smoke | Effect::Leaves => self.lifetime.to,
        }
    }
}

/// Цвет искры на доле жизни `fraction`: от жёлтого к красному.
pub fn spark_color(fraction: f64) -> Rgb {
    let fraction = fraction.clamp(0.0, 1.0) as f32;
    [0, 1, 2].map(|i| SPARK_HOT[i] + (SPARK_COOL[i] - SPARK_HOT[i]) * fraction)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::PropertyTable;
    use crate::core::world::World;

    fn world_with(set: impl FnOnce(&mut World)) -> World {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        assert_eq!(world.create(), 0);
        set(&mut world);
        world
    }

    fn defaults() -> Settings {
        Settings::read(&world_with(|_| {}), 0)
    }

    #[test]
    fn an_object_without_the_properties_has_the_defaults_and_no_effect() {
        let settings = defaults();
        assert_eq!(settings.smoke_height, 4.0);
        assert_eq!(settings.sparks_reach, 1.5);
        assert_eq!(settings.sparks_direction, 0.0);
        assert_eq!(settings.sparks_spread, 30.0);
        assert_eq!(settings.leaf_color, None);
        assert!(!settings.any_runs());
    }

    #[test]
    fn an_effect_runs_while_its_main_property_is_above_zero_and_settings_alone_do_nothing() {
        let world = world_with(|world| {
            world.set_number(0, property::SPARKS_REACH, 3.0);
            world.set_number(0, property::SMOKE, 0.0);
            world.set_number(0, property::LEAF_FALL, 0.3);
        });
        let settings = Settings::read(&world, 0);
        assert_eq!(settings.sparks_reach, 3.0);
        assert!(!settings.runs(Effect::Smoke));
        assert!(!settings.runs(Effect::Sparks));
        assert!(settings.runs(Effect::Leaves));
    }

    #[test]
    fn values_outside_the_ranges_are_pulled_in_and_nonsense_gives_the_default() {
        let world = world_with(|world| {
            world.set_number(0, property::SMOKE, 7.0);
            world.set_number(0, property::SPARKS, -1.0);
            world.set_number(0, property::SPARKS_SPREAD, 400.0);
            world.set_number(0, property::SMOKE_HEIGHT, -2.0);
            world.set_number(0, property::SPARKS_REACH, f64::NAN);
        });
        let settings = Settings::read(&world, 0);
        assert_eq!(settings.smoke, 1.0);
        assert_eq!(settings.sparks, 0.0);
        assert_eq!(settings.sparks_spread, 180.0);
        assert_eq!(settings.smoke_height, 4.0);
        assert_eq!(settings.sparks_reach, 1.5);
    }

    #[test]
    fn the_smoke_comes_at_two_plus_twenty_times_the_density_puffs_a_second() {
        let kind = |smoke| {
            Settings {
                smoke,
                ..defaults()
            }
            .kind(Effect::Smoke)
        };
        assert_eq!(kind(0.5).rate, 12.0);
        assert_eq!(kind(0.0).rate, 2.0);
        assert_eq!(kind(1.0).rate, 22.0);
    }

    #[test]
    fn the_smoke_fades_from_its_density_through_six_tenths_of_it_to_nothing() {
        let kind = Settings {
            smoke: 0.5,
            ..defaults()
        }
        .kind(Effect::Smoke);
        let [start, middle, end] = kind.opacity;
        assert!((start - 0.5).abs() < 1e-12);
        assert!((middle - 0.3).abs() < 1e-12);
        assert_eq!(end, 0.0);
    }

    #[test]
    fn the_fastest_spark_rises_exactly_the_reach() {
        let kind = Settings {
            sparks: 0.5,
            sparks_reach: 1.5,
            ..defaults()
        }
        .kind(Effect::Sparks);
        let apex = kind.speed.to * kind.speed.to / (2.0 * kind.gravity);
        assert!((apex - 1.5).abs() < 1e-12, "{apex}");
        assert!((kind.speed.from / kind.speed.to - 0.7).abs() < 1e-12);
    }

    #[test]
    fn the_shape_follows_the_effect_and_a_leaf_colour_turns_the_leaf_white() {
        let plain = defaults();
        assert_eq!(plain.kind(Effect::Smoke).shape(), ParticleShape::Smoke);
        assert_eq!(plain.kind(Effect::Sparks).shape(), ParticleShape::Spark);
        assert_eq!(plain.kind(Effect::Leaves).shape(), ParticleShape::Leaf);
        let coloured = Settings {
            leaf_color: Some([1.0, 0.0, 0.0]),
            ..plain
        };
        assert_eq!(
            coloured.kind(Effect::Leaves).shape(),
            ParticleShape::WhiteLeaf
        );
    }

    #[test]
    fn a_spark_goes_from_yellow_to_red() {
        let [r0, g0, _] = spark_color(0.0);
        let [r1, g1, _] = spark_color(1.0);
        assert!(g0 / r0 > g1 / r1 + 0.5);
        assert_eq!(spark_color(-3.0), spark_color(0.0));
    }
}

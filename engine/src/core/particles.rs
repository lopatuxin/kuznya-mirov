//! «Ветер и частицы» → «Частицы»: виды частиц из `particles.json` — то, что движок знает о виде,
//! когда источник выпускает частицу и когда её рисует.

use std::sync::Arc;

use super::value::ImageId;

/// «Число или пара» из таблицы видов: каждая частица берёт своё значение наугад между краями; число
/// — оба края равны.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Span {
    pub from: f64,
    pub to: f64,
}

impl Span {
    pub fn single(value: f64) -> Span {
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
/// входит.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleShape {
    Dot,
    Smoke,
    Spark,
    Leaf,
}

impl ParticleShape {
    /// Значения ключа `shape` в том порядке, в котором их перечисляет ошибка.
    pub(crate) const NAMES: [(&'static str, ParticleShape); 4] = [
        ("smoke", ParticleShape::Smoke),
        ("spark", ParticleShape::Spark),
        ("leaf", ParticleShape::Leaf),
        ("dot", ParticleShape::Dot),
    ];

    pub(crate) fn from_name(name: &str) -> Option<ParticleShape> {
        Self::NAMES
            .iter()
            .find(|(known, _)| *known == name)
            .map(|(_, shape)| *shape)
    }

    /// Кадров в рисунке: частица листа берёт кадр наугад при вылете и держит его всю жизнь.
    pub const fn frames(self) -> u32 {
        match self {
            ParticleShape::Leaf => 4,
            ParticleShape::Dot | ParticleShape::Smoke | ParticleShape::Spark => 1,
        }
    }
}

/// Чем рисуются частицы вида: картинкой игры (`image`) или встроенным рисунком (`shape`; без обоих
/// ключей — мягкая точка).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParticleLook {
    Image(ImageId),
    Shape(ParticleShape),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParticleKind {
    pub look: ParticleLook,
    pub rate: f64,
    pub lifetime: Span,
    pub size: Span,
    pub grow: f64,
    pub speed: Span,
    pub direction: f64,
    pub spread: f64,
    pub gravity: f64,
    /// Точки просвечивания через равные доли жизни; одна точка — на всю жизнь. Не пусто.
    pub opacity: Vec<f64>,
    /// `None` — ключа `spin` в виде нет: частица не повёрнута.
    pub spin: Option<Span>,
    pub wobble: f64,
    pub wind: f64,
}

/// Виды игры; копия дешёвая — таблицу делят свойства, мир и живая игра. Вид с ошибкой остаётся в
/// таблице без описания: его имя не должно рождать второй ошибки «такого вида нет».
#[derive(Debug, Clone, Default)]
pub struct ParticleTable {
    kinds: Arc<Vec<(String, Option<ParticleKind>)>>,
}

impl ParticleTable {
    pub fn new(kinds: Vec<(String, Option<ParticleKind>)>) -> ParticleTable {
        ParticleTable {
            kinds: Arc::new(kinds),
        }
    }

    pub fn has(&self, name: &str) -> bool {
        self.kinds.iter().any(|(kind_name, _)| kind_name == name)
    }

    pub fn find(&self, name: &str) -> Option<&ParticleKind> {
        self.index_of(name).and_then(|index| self.kind_at(index))
    }

    /// Место вида в таблице: по нему вид берут без поиска по имени, пока таблица не поменялась.
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.kinds
            .iter()
            .position(|(kind_name, _)| kind_name == name)
    }

    pub fn kind_at(&self, index: usize) -> Option<&ParticleKind> {
        self.kinds.get(index)?.1.as_ref()
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.kinds.iter().map(|(name, _)| name.as_str())
    }

    /// Виды без ошибок, в порядке разбора.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &ParticleKind)> {
        self.kinds
            .iter()
            .filter_map(|(name, kind)| Some((name.as_str(), kind.as_ref()?)))
    }
}

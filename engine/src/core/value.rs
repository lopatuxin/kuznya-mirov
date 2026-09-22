pub type Vec2 = [f64; 2];

/// Index into `files.images`, in declaration order — «Картинки»: an object's `image`
/// property, a spawn template's `image` field and a panel/button's `image` field all resolve
/// their name to one of these at load time, the same way a track name resolves to a `MusicId`.
pub type ImageId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    Vec2,
    Number,
    Time,
    Timer,
    Flag,
    Color,
    Layer,
    Text,
    Grid,
    Keys,
    Image,
    Rotation,
    FollowMouse,
}

impl PropKind {
    pub fn label(self) -> &'static str {
        match self {
            PropKind::Vec2 => "пара чисел",
            PropKind::Number => "число",
            PropKind::Time => "время",
            PropKind::Timer => "таймер",
            PropKind::Flag => "признак",
            PropKind::Color => "цвет",
            PropKind::Layer => "слой",
            PropKind::Text => "строка",
            PropKind::Grid => "grid",
            PropKind::Keys => "keys",
            PropKind::Image => "картинка",
            PropKind::Rotation => "поворот",
            PropKind::FollowMouse => "слежение за мышью",
        }
    }
}

/// «Свойства» → `follow_mouse`: `"x"`, `"y"` or `"xy"` — which axes of the object's midpoint
/// track the world cursor.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FollowAxis {
    X,
    Y,
    Xy,
}

impl FollowAxis {
    pub fn parse(s: &str) -> Option<FollowAxis> {
        Some(match s {
            "x" => FollowAxis::X,
            "y" => FollowAxis::Y,
            "xy" => FollowAxis::Xy,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            FollowAxis::X => "x",
            FollowAxis::Y => "y",
            FollowAxis::Xy => "xy",
        }
    }

    pub fn affects_x(self) -> bool {
        matches!(self, FollowAxis::X | FollowAxis::Xy)
    }

    pub fn affects_y(self) -> bool {
        matches!(self, FollowAxis::Y | FollowAxis::Xy)
    }
}

/// «Свойства» → `rotation`: degrees clockwise, one of the four values a quarter turn can land
/// on — 90×quarters, never anything else.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rotation(u16);

impl Rotation {
    pub const ALLOWED: [u16; 4] = [0, 90, 180, 270];

    /// Accepts only an exact match against one of the four allowed values — «Свойства»: «другое
    /// значение в данных — ошибка проверки», «запись не из 0, 90, 180, 270 — ошибка кода». No
    /// wraparound (`-90`, `360`) and no rounding: those are a different value, not this one.
    pub fn from_degrees_exact(degrees: f64) -> Option<Rotation> {
        Self::ALLOWED
            .iter()
            .find(|&&d| d as f64 == degrees)
            .map(|&d| Rotation(d))
    }

    pub fn from_quarters(quarters: i32) -> Rotation {
        Rotation((quarters.rem_euclid(4) * 90) as u16)
    }

    pub fn degrees(self) -> u16 {
        self.0
    }

    pub fn quarters(self) -> u8 {
        (self.0 / 90) as u8
    }

    /// «turn»: rotates by `dir` quarters (`1` clockwise, `-1` counter-clockwise).
    pub fn turned(self, dir: i32) -> Rotation {
        Rotation::from_quarters(self.quarters() as i32 + dir)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Flag(bool),
    Number(f64),
    Time(i64),
    Timer(i64),
    Vec2(Vec2),
    Color([f32; 4]),
    Layer(i32),
    Text(String),
    Image(ImageId),
    Rotation(Rotation),
    FollowMouse(FollowAxis),
}

impl Value {
    pub fn kind(&self) -> PropKind {
        match self {
            Value::Flag(_) => PropKind::Flag,
            Value::Number(_) => PropKind::Number,
            Value::Time(_) => PropKind::Time,
            Value::Timer(_) => PropKind::Timer,
            Value::Vec2(_) => PropKind::Vec2,
            Value::Color(_) => PropKind::Color,
            Value::Layer(_) => PropKind::Layer,
            Value::Text(_) => PropKind::Text,
            Value::Image(_) => PropKind::Image,
            Value::Rotation(_) => PropKind::Rotation,
            Value::FollowMouse(_) => PropKind::FollowMouse,
        }
    }

    pub fn as_number_like(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Time(t) | Value::Timer(t) => Some(*t as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSpec {
    pub interval_steps: i64,
}

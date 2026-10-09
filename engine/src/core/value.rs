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
    /// «Ветер и частицы» → «Облака»: список имён картинок игры (`cloud_images`), пустой допустим.
    ImageList,
    Rotation,
    FollowMouse,
    /// «Трёхмерная сцена»: `shape` — имя простой фигуры; в плоской сцене недоступно.
    Shape,
    /// «Мышь в мире», требование 19: `on_click` — список записей `[свойство, значение]`, как
    /// `keys`' `press`, но без таблицы кодов и без `release`. Как `Grid`/`Keys`: не заводится
    /// простым значением и не входит в generic `Value`.
    OnClick,
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
            PropKind::ImageList => "список картинок",
            PropKind::Rotation => "поворот",
            PropKind::FollowMouse => "слежение за мышью",
            PropKind::Shape => "фигура",
            PropKind::OnClick => "on_click",
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

/// «Свойства» → `rotation`: degrees clockwise. Плоская сцена держит только четыре значения (вид
/// поворачивается четвертями, `from_degrees_exact`); трёхмерная — любой конечный угол
/// (`from_degrees`), которым поворачивается и место на земле.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rotation(f64);

impl Rotation {
    pub const ALLOWED: [u16; 4] = [0, 90, 180, 270];

    /// Accepts only an exact match against one of the four allowed values — «Свойства»: «другое
    /// значение в данных — ошибка проверки», «запись не из 0, 90, 180, 270 — ошибка кода». No
    /// wraparound (`-90`, `360`) and no rounding: those are a different value, not this one.
    pub fn from_degrees_exact(degrees: f64) -> Option<Rotation> {
        Self::ALLOWED
            .iter()
            .find(|&&d| d as f64 == degrees)
            .map(|&d| Rotation(d as f64))
    }

    /// «Трёхмерная сцена»: любое конечное число градусов, без приведения к кругу — `360` остаётся
    /// `360`, а значит и печатается в файле как есть.
    pub fn from_degrees(degrees: f64) -> Option<Rotation> {
        degrees.is_finite().then_some(Rotation(degrees))
    }

    pub fn from_quarters(quarters: i32) -> Rotation {
        Rotation((quarters.rem_euclid(4) * 90) as f64)
    }

    /// Ближайшая четверть оборота в градусах — 0, 90, 180 или 270, то, что хранит плоская сцена.
    pub fn degrees(self) -> u16 {
        self.quarters() as u16 * 90
    }

    /// Угол как есть, в градусах: в трёхмерной сцене — любое конечное число.
    pub fn angle(self) -> f64 {
        self.0
    }

    /// Четверти оборота по часовой (0–3): кратное четверти значение — точно, остальное
    /// округляется до ближайшей четверти (плоская сцена другого не хранит).
    pub fn quarters(self) -> u8 {
        ((self.0 / 90.0).round() as i64).rem_euclid(4) as u8
    }

    /// «turn»: rotates by `dir` quarters (`1` clockwise, `-1` counter-clockwise).
    pub fn turned(self, dir: i32) -> Rotation {
        Rotation((self.0 + 90.0 * dir as f64).rem_euclid(360.0))
    }

    /// «Исполнение игры» → «Повторяемость», требование 17: синус и косинус угла — из `libm`, одни и
    /// те же в браузере и на компьютере; кратный 90 поворот — точные 0 и ±1. Возвращает `(sin, cos)`.
    pub fn sin_cos(self) -> (f64, f64) {
        let d = self.0.rem_euclid(360.0);
        if d % 90.0 == 0.0 {
            return match (d / 90.0) as u8 {
                0 => (0.0, 1.0),
                1 => (1.0, 0.0),
                2 => (0.0, -1.0),
                _ => (-1.0, 0.0),
            };
        }
        let radians = d.to_radians();
        (libm::sin(radians), libm::cos(radians))
    }
}

/// «Трёхмерная сцена», «Объект в объёме»: простая фигура — заполняет прямоугольник объекта и
/// `height` над ним.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Box,
    Cylinder,
    Capsule,
    Sphere,
}

impl Shape {
    pub const ALL: [Shape; 4] = [Shape::Box, Shape::Cylinder, Shape::Capsule, Shape::Sphere];

    pub fn parse(name: &str) -> Option<Shape> {
        Self::ALL.into_iter().find(|shape| shape.as_str() == name)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Shape::Box => "box",
            Shape::Cylinder => "cylinder",
            Shape::Capsule => "capsule",
            Shape::Sphere => "sphere",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Flag(bool),
    Number(f64),
    Time(i64),
    Timer(i64),
    Vec2(Vec2),
    /// «Рельеф»: `position` или `walk_to` трёхмерной сцены с третьим числом — высотой основания.
    Vec3([f64; 3]),
    Color([f32; 4]),
    Layer(i32),
    Text(String),
    Image(ImageId),
    ImageList(Vec<ImageId>),
    Rotation(Rotation),
    FollowMouse(FollowAxis),
    Shape(Shape),
}

impl Value {
    pub fn kind(&self) -> PropKind {
        match self {
            Value::Flag(_) => PropKind::Flag,
            Value::Number(_) => PropKind::Number,
            Value::Time(_) => PropKind::Time,
            Value::Timer(_) => PropKind::Timer,
            Value::Vec2(_) | Value::Vec3(_) => PropKind::Vec2,
            Value::Color(_) => PropKind::Color,
            Value::Layer(_) => PropKind::Layer,
            Value::Text(_) => PropKind::Text,
            Value::Image(_) => PropKind::Image,
            Value::ImageList(_) => PropKind::ImageList,
            Value::Rotation(_) => PropKind::Rotation,
            Value::FollowMouse(_) => PropKind::FollowMouse,
            Value::Shape(_) => PropKind::Shape,
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

/// `#rrggbb` or `#rrggbbaa` → RGBA in `0..=1`. Only ASCII hex digits count: a non-Latin
/// character or a sign (`"#+f+f+f"`) after `#` is `None`, not a panic or a valid color.
pub fn parse_color(s: &str) -> Option<[f32; 4]> {
    let hex = s.strip_prefix('#')?;
    if !matches!(hex.len(), 6 | 8) || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let component = |i: usize| {
        u8::from_str_radix(&hex[i..i + 2], 16)
            .ok()
            .map(|v| v as f32 / 255.0)
    };
    let alpha = if hex.len() == 8 { component(6)? } else { 1.0 };
    Some([component(0)?, component(2)?, component(4)?, alpha])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_color_reads_both_forms() {
        assert_eq!(parse_color("#ff0000"), Some([1.0, 0.0, 0.0, 1.0]));
        assert_eq!(parse_color("#00ff0000"), Some([0.0, 1.0, 0.0, 0.0]));
        assert_eq!(parse_color("#FFFFFF"), Some([1.0, 1.0, 1.0, 1.0]));
    }

    #[test]
    fn parse_color_rejects_signs_non_latin_and_wrong_lengths() {
        for bad in [
            "#+f+f+f",
            "#-f-f-f",
            "#aжжжb",
            "#жжж",
            "ff0000",
            "#ff00",
            "#ff00000",
            "#gg0000",
        ] {
            assert_eq!(parse_color(bad), None, "{bad}");
        }
    }
}

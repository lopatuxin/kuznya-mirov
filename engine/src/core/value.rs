pub type Vec2 = [f64; 2];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropKind {
    Vec2,
    Number,
    Time,
    Flag,
    Color,
    Layer,
    Text,
    Grid,
    Keys,
}

impl PropKind {
    pub fn label(self) -> &'static str {
        match self {
            PropKind::Vec2 => "пара чисел",
            PropKind::Number => "число",
            PropKind::Time => "время",
            PropKind::Flag => "признак",
            PropKind::Color => "цвет",
            PropKind::Layer => "слой",
            PropKind::Text => "строка",
            PropKind::Grid => "grid",
            PropKind::Keys => "keys",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Flag(bool),
    Number(f64),
    Time(i64),
    Vec2(Vec2),
    Color([f32; 4]),
    Layer(i32),
    Text(String),
}

impl Value {
    pub fn kind(&self) -> PropKind {
        match self {
            Value::Flag(_) => PropKind::Flag,
            Value::Number(_) => PropKind::Number,
            Value::Time(_) => PropKind::Time,
            Value::Vec2(_) => PropKind::Vec2,
            Value::Color(_) => PropKind::Color,
            Value::Layer(_) => PropKind::Layer,
            Value::Text(_) => PropKind::Text,
        }
    }

    pub fn as_number_like(&self) -> Option<f64> {
        match self {
            Value::Number(n) => Some(*n),
            Value::Time(t) => Some(*t as f64),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GridSpec {
    pub interval_steps: i64,
}

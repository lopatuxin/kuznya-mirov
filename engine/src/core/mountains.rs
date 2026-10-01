//! «Лепка рельефа» → «Горы»: штамп — карта высот одной горы от 0 до 1, гора — штамп, растянутый на
//! повёрнутый прямоугольник земли. Итоговая высота точки сетки — `heights` плюс наибольшая из гор.

use std::sync::Arc;

use super::value::{Rotation, Vec2};

/// Карта высот горы: строки сверху вниз, первая строка — северный край, первое число — западный.
#[derive(Debug, Clone, PartialEq)]
pub struct Stamp {
    columns: usize,
    rows: usize,
    heights: Vec<f64>,
}

impl Stamp {
    /// Штамп из строк высот. Строки одной длины и точек не меньше двух на две — это уже проверила
    /// загрузка.
    pub fn new(rows: &[Vec<f64>]) -> Stamp {
        Stamp {
            columns: rows.first().map_or(0, Vec::len),
            rows: rows.len(),
            heights: rows.iter().flatten().copied().collect(),
        }
    }

    /// Высота в доле прямоугольника `(u, v)` от 0 до 1: билинейно между четырьмя соседними точками.
    fn sample(&self, u: f64, v: f64) -> f64 {
        let (x, y) = (u * (self.columns - 1) as f64, v * (self.rows - 1) as f64);
        let column = (x.floor() as usize).min(self.columns - 2);
        let row = (y.floor() as usize).min(self.rows - 2);
        let (fx, fy) = (x - column as f64, y - row as f64);
        let at = |dc: usize, dr: usize| self.heights[(row + dr) * self.columns + column + dc];
        let top = at(0, 0) * (1.0 - fx) + at(1, 0) * fx;
        let bottom = at(0, 1) * (1.0 - fx) + at(1, 1) * fx;
        top * (1.0 - fy) + bottom * fy
    }
}

/// Таблица `files.stamps` загруженной игры по порядку объявления; `None` — штамп с ошибкой, она уже
/// записана, и горы с этим штампом молча выпадают, не плодя вторую ошибку.
#[derive(Debug, Clone, Default)]
pub struct StampTable {
    entries: Vec<(String, Option<Arc<Stamp>>)>,
}

impl StampTable {
    pub fn new(entries: Vec<(String, Option<Arc<Stamp>>)>) -> StampTable {
        StampTable { entries }
    }

    /// Номер штампа по имени и сам штамп; `None` — имя не объявлено.
    pub fn find(&self, name: &str) -> Option<(usize, Option<&Arc<Stamp>>)> {
        self.entries
            .iter()
            .position(|(declared, _)| declared == name)
            .map(|index| (index, self.entries[index].1.as_ref()))
    }
}

/// Гора: штамп `stamp` (номер в таблице), растянутый на прямоугольник `size` с серединой `position` и
/// повёрнутый на `rotation` вокруг середины так же, как объект; 1 штампа становится `height` клеток.
#[derive(Debug, Clone, PartialEq)]
pub struct Mountain {
    pub stamp: usize,
    pub shape: Arc<Stamp>,
    pub position: Vec2,
    pub size: Vec2,
    pub height: f64,
    pub rotation: Rotation,
}

impl Mountain {
    /// Высота горы в месте сцены: вне прямоугольника — 0.
    pub fn height_at(&self, point: Vec2) -> f64 {
        let (sin, cos) = self.rotation.sin_cos();
        let (dx, dy) = (point[0] - self.position[0], point[1] - self.position[1]);
        let along = cos * dx + sin * dy;
        let across = -sin * dx + cos * dy;
        let (u, v) = (along / self.size[0] + 0.5, across / self.size[1] + 0.5);
        if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
            return 0.0;
        }
        self.shape.sample(u, v) * self.height
    }

    /// Прямоугольник по осям, охватывающий повёрнутый: `[левый верхний, правый нижний]`.
    pub fn bounds(&self) -> [Vec2; 2] {
        let (sin, cos) = self.rotation.sin_cos();
        let half = [
            (cos.abs() * self.size[0] + sin.abs() * self.size[1]) / 2.0,
            (sin.abs() * self.size[0] + cos.abs() * self.size[1]) / 2.0,
        ];
        [
            [self.position[0] - half[0], self.position[1] - half[1]],
            [self.position[0] + half[0], self.position[1] + half[1]],
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wedge() -> Arc<Stamp> {
        Arc::new(Stamp::new(&[vec![0.0, 0.5], vec![0.5, 1.0]]))
    }

    fn mountain(position: Vec2, size: Vec2, degrees: f64) -> Mountain {
        Mountain {
            stamp: 0,
            shape: wedge(),
            position,
            size,
            height: 10.0,
            rotation: Rotation::from_degrees(degrees).expect("конечный угол"),
        }
    }

    #[test]
    fn a_stamp_is_read_between_its_four_nearest_points() {
        let stamp = Stamp::new(&[vec![0.0, 0.5], vec![0.5, 1.0]]);
        assert_eq!(stamp.sample(0.0, 0.0), 0.0);
        assert_eq!(stamp.sample(1.0, 1.0), 1.0);
        assert_eq!(stamp.sample(1.0, 0.0), 0.5);
        assert!((stamp.sample(0.5, 0.5) - 0.5).abs() < 1e-12);
        assert!((stamp.sample(0.25, 0.0) - 0.125).abs() < 1e-12);
    }

    #[test]
    fn a_mountain_is_zero_outside_its_rectangle_and_scales_inside() {
        let mountain = mountain([10.0, 10.0], [4.0, 2.0], 0.0);
        assert_eq!(mountain.height_at([20.0, 10.0]), 0.0);
        assert_eq!(mountain.height_at([10.0, 12.5]), 0.0);
        assert_eq!(mountain.height_at([12.0, 11.0]), 10.0, "нижний правый угол");
        assert_eq!(mountain.height_at([8.0, 9.0]), 0.0, "верхний левый угол");
        assert_eq!(mountain.height_at([10.0, 10.0]), 5.0);
    }

    #[test]
    fn a_mountain_turned_by_ninety_degrees_turns_clockwise_like_an_object() {
        let turned = mountain([10.0, 10.0], [4.0, 2.0], 90.0);
        assert_eq!(
            turned.height_at([11.0, 8.0]),
            0.0,
            "верхний левый угол штампа"
        );
        assert_eq!(
            turned.height_at([9.0, 12.0]),
            10.0,
            "нижний правый угол штампа"
        );
        assert_eq!(
            turned.height_at([12.0, 10.0]),
            0.0,
            "вне повёрнутого прямоугольника"
        );
    }

    #[test]
    fn the_bounds_of_a_turned_mountain_cover_its_corners() {
        let [low, high] = mountain([10.0, 10.0], [4.0, 2.0], 90.0).bounds();
        assert!((low[0] - 9.0).abs() < 1e-12 && (high[0] - 11.0).abs() < 1e-12);
        assert!((low[1] - 8.0).abs() < 1e-12 && (high[1] - 12.0).abs() < 1e-12);
    }
}

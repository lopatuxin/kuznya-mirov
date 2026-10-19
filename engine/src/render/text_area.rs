//! Где надпись стоит в окне и чем её обрезает окно. Чистая геометрия без видеокарты: `Renderer` берёт отсюда
//! высоту строки, верх текста и пределы обрезки для `glyphon`.

/// Высота строки одиночной надписи в размерах шрифта.
pub const SINGLE_LINE_HEIGHT: f32 = 1.2;
/// Шаг строк надписи с переносом в размерах шрифта.
pub const WRAPPED_LINE_HEIGHT: f32 = 1.25;

/// Высота строки надписи с шрифтом `font_size`.
pub fn line_height(font_size: f32, wrap: bool) -> f32 {
    font_size
        * if wrap {
            WRAPPED_LINE_HEIGHT
        } else {
            SINGLE_LINE_HEIGHT
        }
}

/// Место надписи: верх первой строки и пределы обрезки.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TextPlace {
    pub top: f32,
    /// `[слева, сверху, справа, снизу]` в точках окна: пересечение прямоугольника надписи с окном.
    pub bounds: [i32; 4],
}

/// Место надписи в прямоугольнике `rect` (`[x, y, ширина, высота]`) окна размером `window`. Без `wrap` строка
/// стоит посередине по высоте, с `wrap` — сверху. Надпись ограничена прямоугольником и окном, но её место
/// от обрезки не зависит: вышедшая за край остаётся на своём месте и режется краем окна. Надпись, что
/// целиком вне окна, не рисуется — `None`.
pub fn place(rect: [f32; 4], line_height: f32, wrap: bool, window: [f32; 2]) -> Option<TextPlace> {
    let bounds = [
        rect[0].max(0.0) as i32,
        rect[1].max(0.0) as i32,
        (rect[0] + rect[2]).min(window[0]) as i32,
        (rect[1] + rect[3]).min(window[1]) as i32,
    ];
    if bounds[2] <= bounds[0] || bounds[3] <= bounds[1] {
        return None;
    }
    let top = if wrap {
        rect[1]
    } else {
        rect[1] + (rect[3] - line_height) / 2.0
    };
    Some(TextPlace { top, bounds })
}

#[cfg(test)]
mod tests {
    use super::*;

    const WINDOW: [f32; 2] = [800.0, 600.0];

    #[test]
    fn a_label_inside_the_window_is_clipped_by_its_own_rectangle() {
        let place = place([100.0, 50.0, 200.0, 24.0], 19.2, false, WINDOW).expect("видна");
        assert_eq!(place.bounds, [100, 50, 300, 74]);
        assert!((place.top - (50.0 + (24.0 - 19.2) / 2.0)).abs() < 1e-5);
    }

    #[test]
    fn a_label_past_the_edge_keeps_its_place_and_is_cut_by_the_edge() {
        let place = place([-120.0, 50.0, 200.0, 24.0], 19.2, false, WINDOW).expect("видна часть");
        assert_eq!(place.bounds, [0, 50, 80, 74], "обрезана краем окна");
        let past_right =
            super::place([760.0, 50.0, 200.0, 24.0], 19.2, false, WINDOW).expect("видна часть");
        assert_eq!(past_right.bounds, [760, 50, 800, 74]);
        let below =
            super::place([100.0, 590.0, 200.0, 24.0], 19.2, false, WINDOW).expect("видна часть");
        assert_eq!(below.bounds, [100, 590, 300, 600]);
    }

    #[test]
    fn a_label_wholly_outside_the_window_is_not_drawn() {
        for rect in [
            [-300.0, 50.0, 200.0, 24.0],
            [-200.0, 50.0, 200.0, 24.0],
            [800.0, 50.0, 200.0, 24.0],
            [100.0, -30.0, 200.0, 24.0],
            [100.0, 600.0, 200.0, 24.0],
            [-5000.0, 9000.0, 40.0, 20.0],
        ] {
            assert_eq!(place(rect, 19.2, false, WINDOW), None, "{rect:?}");
        }
    }

    #[test]
    fn the_top_of_a_label_does_not_move_with_the_cut() {
        let inside = place([10.0, -10.0, 200.0, 40.0], 20.0, false, WINDOW).expect("видна часть");
        let shifted =
            place([10.0, -10.0, 200.0, 40.0], 20.0, false, [800.0, 25.0]).expect("видна часть");
        assert_eq!(inside.top, shifted.top);
        assert_eq!(inside.top, 0.0, "посередине: -10 + (40 - 20) / 2");
    }

    #[test]
    fn a_wrapped_label_starts_at_the_top_of_its_rectangle() {
        let place = place([10.0, 40.0, 200.0, 100.0], 20.0, true, WINDOW).expect("видна");
        assert_eq!(place.top, 40.0);
        assert_eq!(line_height(16.0, true), 20.0);
        assert_eq!(line_height(16.0, false), 19.2);
    }
}

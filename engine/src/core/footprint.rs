//! «Трёхмерная сцена» → `rotation`, «Исполнение игры» → «Столкновения»: место объекта на земле.
//! В плоской сцене — всегда прямоугольник по осям. В трёхмерной `rotation` поворачивает
//! прямоугольник вокруг его середины вместе с видом; поворот, кратный 90, даёт снова прямоугольник по
//! осям (точно), любой другой — повёрнутый. Пересечение — по разделяющим осям.

use super::grid::Rect;
use super::value::{Rotation, Vec2};

/// Допуск на «касание не мешает» для повёрнутых прямоугольников: погрешность синуса и косинуса не
/// должна делать стоящий впритык объект пересекающим соседа на долю клетки.
const TOUCH_EPS: f64 = 1e-9;

/// Повёрнутый прямоугольник: середина, полуразмеры вдоль своих осей, синус и косинус угла. Ось
/// `u = (cos, sin)` — направление вдоль ширины, `v = (−sin, cos)` — вдоль высоты; при положительном
/// угле, если смотреть сверху (y растёт к игроку), поворот идёт по часовой стрелке.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Oriented {
    pub center: Vec2,
    pub half: Vec2,
    pub sin: f64,
    pub cos: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Footprint {
    Aligned(Rect),
    Oriented(Oriented),
}

impl Footprint {
    /// Место объекта в плоской сцене: `rotation` только вид, прямоугольник не поворачивается.
    pub fn flat(position: Vec2, size: Vec2) -> Footprint {
        Footprint::Aligned(Rect {
            x: position[0],
            y: position[1],
            w: size[0],
            h: size[1],
        })
    }

    /// Место объекта в трёхмерной сцене: прямоугольник `position`/`size`, повёрнутый вокруг
    /// своей середины на `rotation`.
    pub fn rotated(position: Vec2, size: Vec2, rotation: Option<Rotation>) -> Footprint {
        let (sin, cos) = rotation.map_or((0.0, 1.0), Rotation::sin_cos);
        let center = [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0];
        if sin == 0.0 {
            return Footprint::flat(position, size);
        }
        if cos == 0.0 {
            return Footprint::Aligned(Rect {
                x: center[0] - size[1] / 2.0,
                y: center[1] - size[0] / 2.0,
                w: size[1],
                h: size[0],
            });
        }
        Footprint::Oriented(Oriented {
            center,
            half: [size[0] / 2.0, size[1] / 2.0],
            sin,
            cos,
        })
    }

    pub fn is_oriented(&self) -> bool {
        matches!(self, Footprint::Oriented(_))
    }

    pub fn center(&self) -> Vec2 {
        match self {
            Footprint::Aligned(r) => [r.x + r.w / 2.0, r.y + r.h / 2.0],
            Footprint::Oriented(o) => o.center,
        }
    }

    pub fn corners(&self) -> [Vec2; 4] {
        match self {
            Footprint::Aligned(r) => [
                [r.x, r.y],
                [r.x + r.w, r.y],
                [r.x + r.w, r.y + r.h],
                [r.x, r.y + r.h],
            ],
            Footprint::Oriented(o) => {
                let u = [o.cos * o.half[0], o.sin * o.half[0]];
                let v = [-o.sin * o.half[1], o.cos * o.half[1]];
                let c = o.center;
                [
                    [c[0] - u[0] - v[0], c[1] - u[1] - v[1]],
                    [c[0] + u[0] - v[0], c[1] + u[1] - v[1]],
                    [c[0] + u[0] + v[0], c[1] + u[1] + v[1]],
                    [c[0] - u[0] + v[0], c[1] - u[1] + v[1]],
                ]
            }
        }
    }

    /// Прямоугольник по осям, охватывающий это место целиком.
    pub fn aabb(&self) -> Rect {
        match self {
            Footprint::Aligned(r) => *r,
            Footprint::Oriented(o) => {
                let hx = o.cos.abs() * o.half[0] + o.sin.abs() * o.half[1];
                let hy = o.sin.abs() * o.half[0] + o.cos.abs() * o.half[1];
                Rect {
                    x: o.center[0] - hx,
                    y: o.center[1] - hy,
                    w: 2.0 * hx,
                    h: 2.0 * hy,
                }
            }
        }
    }

    fn axes(&self) -> [Vec2; 2] {
        match self {
            Footprint::Aligned(_) => [[1.0, 0.0], [0.0, 1.0]],
            Footprint::Oriented(o) => [[o.cos, o.sin], [-o.sin, o.cos]],
        }
    }

    /// Точка внутри: левый и верхний края входят, правый и нижний — нет (как у щелчка по плоскому
    /// объекту); у повёрнутого — по его собственным осям.
    pub fn contains(&self, point: Vec2) -> bool {
        match self {
            Footprint::Aligned(r) => {
                point[0] >= r.x && point[1] >= r.y && point[0] < r.x + r.w && point[1] < r.y + r.h
            }
            Footprint::Oriented(o) => {
                let (dx, dy) = (point[0] - o.center[0], point[1] - o.center[1]);
                let u = o.cos * dx + o.sin * dy;
                let v = -o.sin * dx + o.cos * dy;
                u >= -o.half[0] && u < o.half[0] && v >= -o.half[1] && v < o.half[1]
            }
        }
    }

    /// Строгое пересечение: касание краем — не пересечение. Пара без поворота — прежний
    /// `Rect::overlap`.
    pub fn overlaps(&self, other: &Footprint) -> bool {
        if let (Footprint::Aligned(a), Footprint::Aligned(b)) = (self, other) {
            return a.overlap(b).is_some();
        }
        self.penetrations(other).is_some()
    }

    /// Между прямоугольниками есть зазор: по одной из осей обоих проекции не пересекаются;
    /// касание зазором не считается.
    pub fn is_apart_from(&self, other: &Footprint) -> bool {
        let (own, foreign) = (self.axes(), other.axes());
        let (corners_a, corners_b) = (self.corners(), other.corners());
        [own[0], own[1], foreign[0], foreign[1]]
            .into_iter()
            .any(|axis| {
                let (min_a, max_a) = project(&corners_a, axis);
                let (min_b, max_b) = project(&corners_b, axis);
                max_a < min_b || max_b < min_a
            })
    }

    /// Куда и насколько выталкивать `self` из `other`, чтобы они лишь касались: ось, поперёк
    /// стороны которой выталкивать меньше всего, среди сторон обоих прямоугольников; направление —
    /// со стороны середины `self`. `None`, когда они не пересекаются. Единичная нормаль смотрит
    /// от `other` к `self`.
    pub fn push_out(&self, other: &Footprint) -> Option<(Vec2, f64)> {
        let candidates = self.penetrations(other)?;
        candidates
            .into_iter()
            .fold(None, |best: Option<(Vec2, f64)>, candidate| match best {
                Some(b) if b.1 <= candidate.1 => Some(b),
                _ => Some(candidate),
            })
    }

    /// По каждой из четырёх осей — единичная нормаль от `other` к `self` и глубина, на которую
    /// нужно вытолкнуть `self` по ней; `None`, если хоть по одной оси прямоугольники разошлись.
    fn penetrations(&self, other: &Footprint) -> Option<[(Vec2, f64); 4]> {
        let mut result = [([0.0, 0.0], 0.0); 4];
        let (own, foreign) = (self.axes(), other.axes());
        let axes = [own[0], own[1], foreign[0], foreign[1]];
        let (corners_a, corners_b) = (self.corners(), other.corners());
        let (ca, cb) = (self.center(), other.center());
        for (slot, axis) in result.iter_mut().zip(axes) {
            let (min_a, max_a) = project(&corners_a, axis);
            let (min_b, max_b) = project(&corners_b, axis);
            let overlap = max_a.min(max_b) - min_a.max(min_b);
            if overlap <= TOUCH_EPS {
                return None;
            }
            let toward_self = (ca[0] - cb[0]) * axis[0] + (ca[1] - cb[1]) * axis[1] >= 0.0;
            *slot = if toward_self {
                (axis, max_b - min_a)
            } else {
                ([-axis[0], -axis[1]], max_a - min_b)
            };
        }
        Some(result)
    }
}

fn project(corners: &[Vec2; 4], axis: Vec2) -> (f64, f64) {
    corners
        .iter()
        .map(|c| c[0] * axis[0] + c[1] * axis[1])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
            (lo.min(d), hi.max(d))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rotation(degrees: f64) -> Option<Rotation> {
        Rotation::from_degrees(degrees)
    }

    #[test]
    fn a_quarter_turn_stays_an_exact_axis_aligned_rectangle_around_its_middle() {
        let footprint = Footprint::rotated([2.0, 1.0], [4.0, 1.0], rotation(90.0));
        assert_eq!(
            footprint,
            Footprint::Aligned(Rect {
                x: 3.5,
                y: -0.5,
                w: 1.0,
                h: 4.0
            })
        );
        let full_turn = Footprint::rotated([2.0, 1.0], [4.0, 1.0], rotation(-360.0));
        assert_eq!(full_turn, Footprint::flat([2.0, 1.0], [4.0, 1.0]));
        let minus_ninety = Footprint::rotated([2.0, 1.0], [4.0, 1.0], rotation(-90.0));
        let two_seventy = Footprint::rotated([2.0, 1.0], [4.0, 1.0], rotation(270.0));
        assert_eq!(minus_ninety, two_seventy);
    }

    #[test]
    fn a_forty_five_degree_square_is_a_diamond_with_the_expected_bounding_box() {
        let footprint = Footprint::rotated([0.0, 0.0], [2.0, 2.0], rotation(45.0));
        assert!(footprint.is_oriented());
        let bbox = footprint.aabb();
        let half_diagonal = 2.0_f64.sqrt();
        assert!((bbox.w - 2.0 * half_diagonal).abs() < 1e-12, "{bbox:?}");
        assert!((bbox.x - (1.0 - half_diagonal)).abs() < 1e-12, "{bbox:?}");
    }

    #[test]
    fn touching_edges_are_not_an_overlap_but_a_sliver_is() {
        let wall = Footprint::flat([0.0, 0.0], [1.0, 10.0]);
        // The diamond's left corner sits at x = position.x + 1 - sqrt(2); the wall's right side is x = 1.
        let touching = Footprint::rotated([2.0_f64.sqrt(), 4.0], [2.0, 2.0], rotation(45.0));
        let sliver = Footprint::rotated([2.0_f64.sqrt() - 0.001, 4.0], [2.0, 2.0], rotation(45.0));
        assert!(!touching.overlaps(&wall));
        assert!(sliver.overlaps(&wall));
    }

    #[test]
    fn push_out_picks_the_side_with_the_least_penetration_toward_the_bouncers_middle() {
        let wall = Footprint::flat([0.0, 0.0], [1.0, 10.0]);
        // A diamond sunk 0.3 into the wall's right side: its own middle is to the right.
        let sunk = 2.0_f64.sqrt() - 0.3;
        let diamond = Footprint::rotated([sunk, 4.0], [2.0, 2.0], rotation(45.0));
        let (normal, depth) = diamond.push_out(&wall).expect("overlaps");
        assert_eq!(normal, [1.0, 0.0]);
        assert!((depth - 0.3).abs() < 1e-9, "{depth}");
    }

    #[test]
    fn an_oriented_wall_pushes_along_its_own_normal() {
        let wall = Footprint::rotated([0.0, 0.0], [10.0, 1.0], rotation(30.0));
        let (sin, cos) = (0.5_f64, 30.0_f64.to_radians().cos());
        let center = wall.center();
        // A small square just above the wall's centre along the wall's normal (−sin, cos)... on
        // the far side from the origin: push it out along that normal.
        let probe_center = [center[0] - sin * 0.6, center[1] + cos * 0.6];
        let probe = Footprint::flat([probe_center[0] - 0.25, probe_center[1] - 0.25], [0.5, 0.5]);
        let (normal, depth) = probe.push_out(&wall).expect("overlaps");
        assert!(
            (normal[0] + sin).abs() < 1e-9 && (normal[1] - cos).abs() < 1e-9,
            "{normal:?}"
        );
        assert!(depth > 0.0 && depth < 0.5 + 0.6, "{depth}");
    }
}

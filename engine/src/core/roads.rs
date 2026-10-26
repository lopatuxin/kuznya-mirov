//! «Правила игры» → «Езда по дорогам»: сеть из дорог — полос вдоль оси-прямой или оси-дуги с профилем
//! высот, самый быстрый путь по ней и движение машины по своей стороне со скруглёнными углами. Модуль не
//! знает о `World`: дороги ему приносят готовыми (`RoadForm`), как `pathfind` приносит препятствия.

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};
use std::f64::consts::{FRAC_PI_2, PI, TAU};
use std::rc::Rc;

use super::value::Vec2;

/// Дороги связаны, если их высоты в точке связи расходятся не больше чем на это.
pub const MAX_HEIGHT_GAP: f64 = 0.4;
/// Доля ширины дороги, на которую своя сторона сдвинута от оси.
pub const SIDE_SHIFT_SHARE: f64 = 0.25;
/// Поворот на стыке больше этого — разворот: машина идёт по прямой от одной стороны к другой.
pub const MAX_JOINT_TURN_DEGREES: f64 = 150.0;
/// Доля ширины дороги, дальше которой от излома дуга скругления не начинается.
pub const ARC_WIDTH_SHARE: f64 = 0.5;
/// Изгиб оси строго меньше 180°: записанный больше берётся этим числом, ближайшим допустимым.
pub const MAX_BEND_DEGREES: f64 = 179.9;
/// Поворот оси на одном отрезке ломаной, которой рисуется ось в редакторе.
pub const AXIS_SAMPLE_DEGREES: f64 = 2.0;
/// Поворот на одном отрезке ломаной, по которой идёт сторона машины на дуге: отрезок уходит от дуги не дальше
/// радиус × 0,00004.
const PATH_SAMPLE_DEGREES: f64 = 1.0;

/// Точка лежит на оси, если до неё не дальше этого: пересечения дуг считаются в числах с запасом.
const AXIS_EPS: f64 = 1e-7;
const EPS: f64 = 1e-9;
/// Не больше стольких шагов вдоль луча при поиске полотна дороги; и сколько раз отрезок с пересечением делится пополам.
const RAY_STEPS_MAX: usize = 20_000;
const RAY_BISECTIONS: usize = 50;

pub(super) fn sub(a: Vec2, b: Vec2) -> Vec2 {
    [a[0] - b[0], a[1] - b[1]]
}

pub(super) fn add_scaled(a: Vec2, direction: Vec2, k: f64) -> Vec2 {
    [a[0] + direction[0] * k, a[1] + direction[1] * k]
}

pub(super) fn dot(a: Vec2, b: Vec2) -> f64 {
    a[0] * b[0] + a[1] * b[1]
}

pub(super) fn cross(a: Vec2, b: Vec2) -> f64 {
    a[0] * b[1] - a[1] * b[0]
}

pub(super) fn length(a: Vec2) -> f64 {
    dot(a, a).sqrt()
}

pub(super) fn distance(a: Vec2, b: Vec2) -> f64 {
    length(sub(a, b))
}

pub(super) fn unit(a: Vec2) -> Vec2 {
    let len = length(a);
    if len < EPS {
        [1.0, 0.0]
    } else {
        [a[0] / len, a[1] / len]
    }
}

/// Угол, приведённый к (−π, π].
fn wrap_pi(angle: f64) -> f64 {
    let wrapped = (angle + PI).rem_euclid(TAU) - PI;
    if wrapped <= -PI { PI } else { wrapped }
}

fn lerp(from: f64, to: f64, share: f64) -> f64 {
    from + (to - from) * share
}

/// Ближайшая к `p` точка отрезка `a`–`b` и её доля пути от `a` к `b`.
fn closest_on_segment(p: Vec2, a: Vec2, b: Vec2) -> (Vec2, f64) {
    let along = sub(b, a);
    let len_sq = dot(along, along);
    if len_sq < EPS * EPS {
        return (a, 0.0);
    }
    let share = (dot(sub(p, a), along) / len_sq).clamp(0.0, 1.0);
    (add_scaled(a, along, share), share)
}

/// Изгиб, который можно дать дороге с хордой `chord` и шириной `width`: ближайший к `bend` из тех, при
/// которых изгиб меньше 180° и радиус дуги не меньше ширины. Не число — прямая.
fn clamp_bend(bend: f64, chord: f64, width: f64) -> f64 {
    if bend.is_nan() {
        return 0.0;
    }
    let by_radius = if width > EPS && chord > EPS {
        2.0 * libm::asin((chord / (2.0 * width)).min(1.0)).to_degrees()
    } else {
        MAX_BEND_DEGREES
    };
    let limit = MAX_BEND_DEGREES.min(by_radius);
    bend.clamp(-limit, limit)
}

/// Дорога, как её видит сеть: прямоугольник `position`/`size`, повёрнутый вокруг середины, — хорда оси и ширина — и
/// высота основания. `sin_cos` — синус и косинус `rotation`, как их считает `Rotation::sin_cos`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadShape {
    pub id: u32,
    pub position: Vec2,
    pub size: Vec2,
    pub sin_cos: (f64, f64),
    pub z: f64,
}

/// Дорога целиком: хорда и высота её начала (`shape`), изгиб оси в градусах (`bend`) и высота конца оси
/// (`end_z`). Ровная дорога — `end_z` равна `shape.z`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoadForm {
    pub shape: RoadShape,
    pub bend: f64,
    pub end_z: f64,
}

impl RoadForm {
    /// Прямая ровная дорога на высоте `shape.z`.
    pub fn straight(shape: RoadShape) -> RoadForm {
        RoadForm {
            shape,
            bend: 0.0,
            end_z: shape.z,
        }
    }
}

/// Дорога в расчётах: ось — прямая или дуга окружности, профиль — прямая между высотами концов по длине оси.
#[derive(Debug, Clone, Copy)]
pub(super) struct Road {
    pub(super) id: u32,
    /// Середина и направление хорды: у прямой — самой оси.
    center: Vec2,
    dir: Vec2,
    half_chord: f64,
    pub(super) half_width: f64,
    /// Высоты начала и конца оси.
    pub(super) z: [f64; 2],
    /// Длина оси; у прямой равна хорде.
    length: f64,
    /// Поворот оси на единицу длины со знаком (1 / радиус): положительный поворачивает в сторону роста угла
    /// направления, как положительный `rotation`; у прямой ноль.
    curvature: f64,
    /// Угол направления оси в начале; только у дуги.
    heading: f64,
}

/// Ближайшая точка оси: расстояние от начала оси, сама точка и расстояние до неё.
#[derive(Debug, Clone, Copy)]
pub(super) struct Closest {
    pub(super) s: f64,
    pub(super) point: Vec2,
    pub(super) distance: f64,
}

impl Road {
    /// Прямая ровная дорога по прямоугольнику.
    #[cfg(test)]
    pub(super) fn from_shape(shape: &RoadShape) -> Road {
        Road::from_form(&RoadForm::straight(*shape))
    }

    /// Хорда идёт вдоль длинной стороны; у квадрата — вдоль ширины `size`. Изгиб берётся ближайшим допустимым.
    pub(super) fn from_form(form: &RoadForm) -> Road {
        let shape = &form.shape;
        let (sin, cos) = shape.sin_cos;
        let center = [
            shape.position[0] + shape.size[0] / 2.0,
            shape.position[1] + shape.size[1] / 2.0,
        ];
        let (dir, long, short) = if shape.size[1] > shape.size[0] {
            ([-sin, cos], shape.size[1], shape.size[0])
        } else {
            ([cos, sin], shape.size[0], shape.size[1])
        };
        let bend = if long < EPS {
            0.0
        } else {
            clamp_bend(form.bend, long, short).to_radians()
        };
        let (length, curvature, heading) = if bend.abs() < 1e-12 {
            (long, 0.0, 0.0)
        } else {
            let radius = long / (2.0 * libm::sin(bend.abs() / 2.0));
            let length = radius * bend.abs();
            (
                length,
                bend / length,
                libm::atan2(dir[1], dir[0]) - bend / 2.0,
            )
        };
        Road {
            id: shape.id,
            center,
            dir,
            half_chord: long / 2.0,
            half_width: short / 2.0,
            z: [shape.z, form.end_z],
            length,
            curvature,
            heading,
        }
    }

    pub(super) fn length(&self) -> f64 {
        self.length
    }

    pub(super) fn width(&self) -> f64 {
        self.half_width * 2.0
    }

    pub(super) fn is_straight(&self) -> bool {
        self.curvature == 0.0
    }

    pub(super) fn is_level(&self) -> bool {
        self.z[0] == self.z[1]
    }

    /// Поворот оси на единицу длины со знаком.
    pub(super) fn curvature(&self) -> f64 {
        self.curvature
    }

    /// Уклон профиля: подъём на единицу длины оси от начала к концу.
    pub(super) fn slope(&self) -> f64 {
        if self.length < EPS {
            0.0
        } else {
            (self.z[1] - self.z[0]) / self.length
        }
    }

    /// Высота профиля на расстоянии `s` от начала оси; за концами она равна высоте конца: так же ложится земля у
    /// концов дороги.
    pub(super) fn height_at(&self, s: f64) -> f64 {
        if self.length < EPS {
            self.z[0]
        } else {
            lerp(self.z[0], self.z[1], (s / self.length).clamp(0.0, 1.0))
        }
    }

    fn chord_start(&self) -> Vec2 {
        add_scaled(self.center, self.dir, -self.half_chord)
    }

    /// Угол направления оси на расстоянии `s` от начала; только у дуги.
    fn heading_at(&self, s: f64) -> f64 {
        self.heading + self.curvature * s.clamp(0.0, self.length)
    }

    /// Единичная касательная оси на расстоянии `s` от начала; за концами — касательная конца.
    pub(super) fn tangent_at(&self, s: f64) -> Vec2 {
        if self.is_straight() {
            self.dir
        } else {
            let (sin, cos) = libm::sincos(self.heading_at(s));
            [cos, sin]
        }
    }

    /// Центр дуги; только у дуги.
    pub(super) fn arc_center(&self) -> Vec2 {
        let (sin, cos) = libm::sincos(self.heading);
        add_scaled(self.chord_start(), [-sin, cos], 1.0 / self.curvature)
    }

    pub(super) fn radius(&self) -> f64 {
        1.0 / self.curvature.abs()
    }

    fn curve_point(&self, s: f64) -> Vec2 {
        if self.is_straight() {
            return add_scaled(self.center, self.dir, s - self.half_chord);
        }
        let start = self.chord_start();
        let (sin0, cos0) = libm::sincos(self.heading);
        let (sin, cos) = libm::sincos(self.heading + self.curvature * s);
        [
            start[0] + (sin - sin0) / self.curvature,
            start[1] - (cos - cos0) / self.curvature,
        ]
    }

    /// Точка оси на расстоянии `s` от начала; за концами ось продолжена по касательной конца.
    pub(super) fn point_at(&self, s: f64) -> Vec2 {
        if self.is_straight() {
            return self.curve_point(s);
        }
        let inner = s.clamp(0.0, self.length);
        let on_curve = self.curve_point(inner);
        if inner == s {
            on_curve
        } else {
            add_scaled(on_curve, self.tangent_at(inner), s - inner)
        }
    }

    pub(super) fn end(&self, which: usize) -> Vec2 {
        self.point_at(if which == 0 { 0.0 } else { self.length })
    }

    #[cfg(test)]
    pub(super) fn axis(&self) -> [Vec2; 2] {
        [self.end(0), self.end(1)]
    }

    /// Лежит ли точка в полосе дороги, края включая: ось дуги, ширина в обе стороны, торцы поперёк оси.
    pub(super) fn contains(&self, p: Vec2) -> bool {
        if self.is_straight() {
            let local = sub(p, self.center);
            return dot(local, self.dir).abs() <= self.half_chord + EPS
                && cross(self.dir, local).abs() <= self.half_width + EPS;
        }
        let spoke = sub(p, self.arc_center());
        if (length(spoke) - self.radius()).abs() > self.half_width + EPS {
            return false;
        }
        let s = self.arc_s(spoke);
        s >= -EPS && s <= self.length + EPS
    }

    /// Расстояние по оси до точки дуги с лучом `spoke` от центра; вне дуги может выйти за концы.
    fn arc_s(&self, spoke: Vec2) -> f64 {
        let radial = libm::atan2(spoke[1], spoke[0]);
        let heading = if self.curvature > 0.0 {
            radial + FRAC_PI_2
        } else {
            radial - FRAC_PI_2
        };
        wrap_pi(heading - self.heading) / self.curvature
    }

    /// Ближайшая к `p` точка оси.
    pub(super) fn closest(&self, p: Vec2) -> Closest {
        if self.is_straight() {
            let (point, share) = closest_on_segment(p, self.end(0), self.end(1));
            return Closest {
                s: share * self.length,
                point,
                distance: distance(p, point),
            };
        }
        let center = self.arc_center();
        let spoke = sub(p, center);
        let at_end = |s: f64| {
            let point = self.point_at(s);
            Closest {
                s,
                point,
                distance: distance(p, point),
            }
        };
        if length(spoke) < EPS {
            return at_end(0.0);
        }
        let s = self.arc_s(spoke);
        if (0.0..=self.length).contains(&s) {
            let radius = self.radius();
            Closest {
                s,
                point: add_scaled(center, unit(spoke), radius),
                distance: (length(spoke) - radius).abs(),
            }
        } else {
            let (first, last) = (at_end(0.0), at_end(self.length));
            if first.distance <= last.distance {
                first
            } else {
                last
            }
        }
    }

    /// Ближайшая к `p` точка части оси от `low` до `high`.
    pub(super) fn closest_between(&self, p: Vec2, low: f64, high: f64) -> Closest {
        let free = self.closest(p);
        let at = |s: f64| {
            let point = self.point_at(s);
            Closest {
                s,
                point,
                distance: distance(p, point),
            }
        };
        if self.is_straight() {
            return at(free.s.clamp(low, high));
        }
        [at(free.s.clamp(low, high)), at(low), at(high)]
            .into_iter()
            .min_by(|a, b| a.distance.total_cmp(&b.distance))
            .unwrap_or(free)
    }

    /// Расстояние по оси до точки возле оси; у точек за концом оси — продолжено по касательной конца, как
    /// `point_at`, и выходит за `0..length`.
    pub(super) fn along(&self, p: Vec2) -> f64 {
        if self.is_straight() {
            return dot(sub(p, self.end(0)), self.dir);
        }
        let near = self.closest(p);
        if near.s <= 0.0 {
            dot(sub(p, self.end(0)), self.tangent_at(0.0))
        } else if near.s >= self.length {
            self.length + dot(sub(p, self.end(1)), self.tangent_at(self.length))
        } else {
            near.s
        }
    }

    /// Охватывающий прямоугольник полосы по осям с запасом.
    pub(super) fn bounds(&self) -> [Vec2; 2] {
        if self.is_straight() {
            let reach = |axis: usize| {
                self.dir[axis].abs() * self.half_chord
                    + self.dir[1 - axis].abs() * self.half_width
                    + EPS
            };
            let (rx, ry) = (reach(0), reach(1));
            return [
                [self.center[0] - rx, self.center[1] - ry],
                [self.center[0] + rx, self.center[1] + ry],
            ];
        }
        let mut low = [f64::INFINITY; 2];
        let mut high = [f64::NEG_INFINITY; 2];
        let mut take = |point: Vec2| {
            for axis in 0..2 {
                low[axis] = low[axis].min(point[axis]);
                high[axis] = high[axis].max(point[axis]);
            }
        };
        take(self.end(0));
        take(self.end(1));
        let (first, last) = (
            self.heading
                .min(self.heading + self.curvature * self.length),
            self.heading
                .max(self.heading + self.curvature * self.length),
        );
        let quarter = FRAC_PI_2;
        let (from, to) = (
            (first / quarter).ceil() as i64,
            (last / quarter).floor() as i64,
        );
        for turn in from..=to {
            take(self.curve_point((turn as f64 * quarter - self.heading) / self.curvature));
        }
        let margin = self.half_width + EPS;
        [
            [low[0] - margin, low[1] - margin],
            [high[0] + margin, high[1] + margin],
        ]
    }

    /// Расстояние вдоль луча `origin + t · direction` до полотна дороги: полосы шириной дороги вдоль оси на высоте её
    /// профиля. `None`, если луч не идёт вниз или не встретил полосу.
    pub(super) fn ray_hit(&self, origin: [f64; 3], direction: [f64; 3]) -> Option<f64> {
        if direction[2] > -EPS {
            return None;
        }
        let [low, high] = self.bounds();
        let (mut from, mut to) = (0.0_f64, f64::INFINITY);
        for axis in 0..2 {
            if direction[axis].abs() < EPS {
                if origin[axis] < low[axis] || origin[axis] > high[axis] {
                    return None;
                }
                continue;
            }
            let ends = [
                (low[axis] - origin[axis]) / direction[axis],
                (high[axis] - origin[axis]) / direction[axis],
            ];
            from = from.max(ends[0].min(ends[1]));
            to = to.min(ends[0].max(ends[1]));
        }
        let (top, bottom) = (
            self.z[0].max(self.z[1]) + AXIS_EPS,
            self.z[0].min(self.z[1]) - AXIS_EPS,
        );
        from = from.max((top - origin[2]) / direction[2]);
        to = to.min((bottom - origin[2]) / direction[2]);
        if from > to {
            return None;
        }
        let place = |t: f64| [origin[0] + t * direction[0], origin[1] + t * direction[1]];
        let gap = |t: f64| {
            let point = place(t);
            origin[2] + t * direction[2] - self.height_at(self.closest(point).s)
        };
        let plan = direction[0].hypot(direction[1]);
        let steps = (((to - from) * plan / (self.half_width / 2.0).max(0.05)).ceil() as usize)
            .clamp(1, RAY_STEPS_MAX);
        let mut above: Option<f64> = None;
        for step in 0..=steps {
            let t = from + (to - from) * step as f64 / steps as f64;
            let here = gap(t);
            if here <= 0.0 && self.contains(place(t)) {
                let (mut near, mut far) = (above.unwrap_or(t), t);
                for _ in 0..RAY_BISECTIONS {
                    let middle = (near + far) / 2.0;
                    if gap(middle) > 0.0 {
                        near = middle;
                    } else {
                        far = middle;
                    }
                }
                if self.contains(place(far)) {
                    return Some(far);
                }
            }
            above = (here > 0.0).then_some(t);
        }
        None
    }

    /// Охват полосы: наибольшая из длины и ширины.
    fn reach(&self) -> f64 {
        self.length.max(self.width())
    }

    /// Точки оси через равные доли длины, поворот между соседними не больше `AXIS_SAMPLE_DEGREES`, с высотой.
    pub(super) fn axis_samples(&self) -> Vec<[f64; 3]> {
        let turn = (self.curvature * self.length).abs().to_degrees();
        let parts = (turn / AXIS_SAMPLE_DEGREES).ceil().max(1.0) as usize;
        (0..=parts)
            .map(|k| {
                let s = self.length * k as f64 / parts as f64;
                let point = self.point_at(s);
                [point[0], point[1], self.height_at(s)]
            })
            .collect()
    }
}

/// Точки пересечения осей двух дорог.
fn axes_cross(first: &Road, second: &Road) -> Vec<Vec2> {
    if first.is_straight() && second.is_straight() {
        return straight_cross(first, second).into_iter().collect();
    }
    let candidates = match (first.is_straight(), second.is_straight()) {
        (false, true) => line_circle(second, first),
        (true, false) => line_circle(first, second),
        _ => circle_circle(first, second),
    };
    candidates
        .into_iter()
        .filter(|&point| {
            first.closest(point).distance <= AXIS_EPS && second.closest(point).distance <= AXIS_EPS
        })
        .collect()
}

fn straight_cross(first: &Road, second: &Road) -> Option<Vec2> {
    let (a1, a2) = (first.end(0), second.end(0));
    let r = sub(first.end(1), a1);
    let s = sub(second.end(1), a2);
    let denominator = cross(r, s);
    if denominator.abs() < EPS * EPS {
        return None;
    }
    let between = sub(a2, a1);
    let along_first = cross(between, s) / denominator;
    let along_second = cross(between, r) / denominator;
    let within = |share: f64, len: f64| share * len >= -EPS && (share - 1.0) * len <= EPS;
    (within(along_first, length(r)) && within(along_second, length(s)))
        .then(|| add_scaled(a1, r, along_first))
}

/// Точки пересечения целых носителей осей двух дорог — прямых и окружностей дуг.
pub(super) fn carriers_cross(first: &Road, second: &Road) -> Vec<Vec2> {
    match (first.is_straight(), second.is_straight()) {
        (true, true) => {
            let denominator = cross(first.dir, second.dir);
            if denominator.abs() < EPS * EPS {
                return Vec::new();
            }
            let along = cross(sub(second.end(0), first.end(0)), second.dir) / denominator;
            vec![add_scaled(first.end(0), first.dir, along)]
        }
        (false, true) => line_circle(second, first),
        (true, false) => line_circle(first, second),
        (false, false) => circle_circle(first, second),
    }
}

/// Пересечения прямой `line` (целой) с окружностью дуги `arc` (целой).
fn line_circle(line: &Road, arc: &Road) -> Vec<Vec2> {
    let origin = line.end(0);
    let center = arc.arc_center();
    let spoke = sub(origin, center);
    let half = dot(line.dir, spoke);
    let discriminant = half * half - (dot(spoke, spoke) - arc.radius().powi(2));
    if discriminant < 0.0 {
        return Vec::new();
    }
    let root = discriminant.sqrt();
    let mut points = vec![add_scaled(origin, line.dir, -half - root)];
    if root > 0.0 {
        points.push(add_scaled(origin, line.dir, -half + root));
    }
    points
}

/// Пересечения двух окружностей дуг.
fn circle_circle(first: &Road, second: &Road) -> Vec<Vec2> {
    let (c1, c2) = (first.arc_center(), second.arc_center());
    let (r1, r2) = (first.radius(), second.radius());
    let between = sub(c2, c1);
    let gap = length(between);
    if gap < EPS || gap > r1 + r2 + EPS || gap < (r1 - r2).abs() - EPS {
        return Vec::new();
    }
    let along = (gap * gap + r1 * r1 - r2 * r2) / (2.0 * gap);
    let height = (r1 * r1 - along * along).max(0.0).sqrt();
    let axis = [between[0] / gap, between[1] / gap];
    let middle = add_scaled(c1, axis, along);
    let mut points = vec![add_scaled(middle, [-axis[1], axis[0]], height)];
    if height > 0.0 {
        points.push(add_scaled(middle, [-axis[1], axis[0]], -height));
    }
    points
}

/// Связь двух дорог: точка на оси каждой. Концом `end_of_a` называется тот конец оси дороги `a`,
/// что лежит внутри полосы `b`; у пересечения осей его нет.
#[derive(Debug, Clone, Copy)]
pub(super) struct Link {
    pub(super) a: usize,
    pub(super) b: usize,
    pub(super) end_of_a: Option<usize>,
    pub(super) point_a: Vec2,
    pub(super) point_b: Vec2,
}

/// Кусок сети между узлами `a` и `b`: часть оси дороги `road` от `span[0]` до `span[1]` либо, без `span`, короткий
/// отрезок связи от конца оси дороги `road` до оси другой. `z` — высоты в узлах.
#[derive(Debug, Clone, Copy)]
struct Piece {
    a: usize,
    b: usize,
    road: usize,
    length: f64,
    span: Option<[f64; 2]>,
    z: [f64; 2],
}

/// Пары дорог, чьи полосы могут пересекаться, — через сетку, а не каждая с каждой.
fn candidate_pairs(roads: &[Road]) -> Vec<(usize, usize)> {
    let cell = (roads.iter().map(|road| road.reach() * 2.0).sum::<f64>()
        / roads.len().max(1) as f64)
        .max(1.0);
    let mut cells: BTreeMap<(i64, i64), Vec<usize>> = BTreeMap::new();
    for (index, road) in roads.iter().enumerate() {
        let [low, high] = road.bounds();
        let (x0, y0) = (
            (low[0] / cell).floor() as i64,
            (low[1] / cell).floor() as i64,
        );
        let (x1, y1) = (
            (high[0] / cell).floor() as i64,
            (high[1] / cell).floor() as i64,
        );
        for x in x0..=x1 {
            for y in y0..=y1 {
                cells.entry((x, y)).or_default().push(index);
            }
        }
    }
    let mut pairs = Vec::new();
    for members in cells.values() {
        for (k, &first) in members.iter().enumerate() {
            for &second in &members[k + 1..] {
                pairs.push((first, second));
            }
        }
    }
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

fn links_between(roads: &[Road], a: usize, b: usize, links: &mut Vec<Link>) {
    for point in axes_cross(&roads[a], &roads[b]) {
        let gap = (roads[a].height_at(roads[a].closest(point).s)
            - roads[b].height_at(roads[b].closest(point).s))
        .abs();
        if gap <= MAX_HEIGHT_GAP + EPS {
            links.push(Link {
                a,
                b,
                end_of_a: None,
                point_a: point,
                point_b: point,
            });
        }
    }
    for (carried, carrier) in [(a, b), (b, a)] {
        for end in 0..2 {
            let point = roads[carried].end(end);
            if !roads[carrier].contains(point) {
                continue;
            }
            let near = roads[carrier].closest(point);
            let gap = (roads[carried].z[end] - roads[carrier].height_at(near.s)).abs();
            if gap <= MAX_HEIGHT_GAP + EPS {
                links.push(Link {
                    a: carried,
                    b: carrier,
                    end_of_a: Some(end),
                    point_a: point,
                    point_b: near.point,
                });
            }
        }
    }
}

pub(super) fn find_links(roads: &[Road]) -> Vec<Link> {
    let mut links = Vec::new();
    for (a, b) in candidate_pairs(roads) {
        links_between(roads, a, b, &mut links);
    }
    links
}

/// Узлы сети и куски между ними: узлы — концы осей и точки связей, куски — части осей между
/// соседними узлами и короткие куски связей «конец в полосе другой дороги».
fn cut_into_pieces(roads: &[Road], links: &[Link]) -> (Vec<Vec2>, Vec<Piece>) {
    let mut nodes: Vec<Vec2> = Vec::new();
    let mut cuts: Vec<Vec<(f64, usize)>> = Vec::with_capacity(roads.len());
    let mut ends: Vec<[usize; 2]> = Vec::with_capacity(roads.len());
    for road in roads {
        let first = nodes.len();
        nodes.push(road.end(0));
        nodes.push(road.end(1));
        ends.push([first, first + 1]);
        cuts.push(vec![(0.0, first), (road.length(), first + 1)]);
    }
    let mut pieces = Vec::new();
    for link in links {
        let along_b = roads[link.b].closest(link.point_b).s;
        match link.end_of_a {
            None => {
                let node = nodes.len();
                nodes.push(link.point_a);
                cuts[link.a].push((roads[link.a].closest(link.point_a).s, node));
                cuts[link.b].push((along_b, node));
            }
            Some(end) if distance(link.point_a, link.point_b) <= EPS => {
                cuts[link.b].push((along_b, ends[link.a][end]));
            }
            Some(end) => {
                let node = nodes.len();
                nodes.push(link.point_b);
                cuts[link.b].push((along_b, node));
                pieces.push(Piece {
                    a: ends[link.a][end],
                    b: node,
                    road: link.a,
                    length: distance(link.point_a, link.point_b),
                    span: None,
                    z: [roads[link.a].z[end], roads[link.b].height_at(along_b)],
                });
            }
        }
    }
    for (road, road_cuts) in cuts.iter_mut().enumerate() {
        road_cuts.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        for pair in road_cuts.windows(2) {
            if pair[0].1 != pair[1].1 {
                pieces.push(Piece {
                    a: pair[0].1,
                    b: pair[1].1,
                    road,
                    length: pair[1].0 - pair[0].0,
                    span: Some([pair[0].0, pair[1].0]),
                    z: [
                        roads[road].height_at(pair[0].0),
                        roads[road].height_at(pair[1].0),
                    ],
                });
            }
        }
    }
    (nodes, pieces)
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Side {
    Right,
    Left,
    Middle,
}

impl Side {
    /// Куда от оси сдвинута сторона: `1` — вправо по ходу, `-1` — влево, `0` — по оси.
    fn share(self) -> f64 {
        match self {
            Side::Right => 1.0,
            Side::Left => -1.0,
            Side::Middle => 0.0,
        }
    }
}

/// Высота и уклон по ходу у одной точки части пути: уклон — подъём на единицу пути в сторону хода.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Rise {
    z: f64,
    slope: f64,
}

/// Дуга части пути: центр, радиус и поворот от начала к концу со знаком (против часовой в осях сцены — плюс).
#[derive(Debug, Clone, Copy, PartialEq)]
struct LegArc {
    center: Vec2,
    radius: f64,
    sweep: f64,
}

/// Часть пути по дороге: от `from` до `to` вдоль одной дороги, предельная скорость на ней `cap`. Часть по дуге
/// оси идёт по дуге; высота и уклон — по профилю дороги.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leg {
    pub from: Vec2,
    pub to: Vec2,
    pub road: usize,
    pub width: f64,
    pub cap: Option<f64>,
    arc: Option<LegArc>,
    rise: [Rise; 2],
    /// Расстояния по оси дороги до начала и конца части; нет у короткой части связи.
    span: Option<[f64; 2]>,
}

impl Leg {
    /// Прямая ровная часть между точками.
    #[cfg(test)]
    fn straight(from: Vec2, to: Vec2, road: usize, width: f64, cap: Option<f64>) -> Leg {
        Leg {
            from,
            to,
            road,
            width,
            cap,
            arc: None,
            rise: [Rise { z: 0.0, slope: 0.0 }; 2],
            span: None,
        }
    }

    /// Длина части: по дуге, если она идёт по дуге.
    pub fn length(&self) -> f64 {
        match self.arc {
            Some(arc) => arc.radius * arc.sweep.abs(),
            None => distance(self.from, self.to),
        }
    }

    /// Направление хода у начала и у конца части.
    fn directions(&self) -> (Vec2, Vec2) {
        match self.arc {
            None => {
                let dir = unit(sub(self.to, self.from));
                (dir, dir)
            }
            Some(arc) => {
                let tangent = |point: Vec2| {
                    let spoke = sub(point, arc.center);
                    let along = arc.sweep.signum() / arc.radius;
                    [-spoke[1] * along, spoke[0] * along]
                };
                (tangent(self.from), tangent(self.to))
            }
        }
    }

    /// Сдвиг своей стороны от оси в сторону правой руки, клетки.
    fn shift(&self, side: Side) -> f64 {
        side.share() * SIDE_SHIFT_SHARE * self.width
    }

    /// Точки своей стороны от начала части до конца: у прямой две, у дуги — вершины ломаной на концентрической дуге
    /// не реже чем через `AXIS_SAMPLE_DEGREES`. С высотой и уклоном каждой.
    fn side_curve(&self, side: Side) -> Vec<(Vec2, Rise)> {
        let shift = self.shift(side);
        let rise_at = |share: f64| Rise {
            z: lerp(self.rise[0].z, self.rise[1].z, share),
            slope: lerp(self.rise[0].slope, self.rise[1].slope, share),
        };
        match self.arc {
            None => {
                let dir = unit(sub(self.to, self.from));
                let right = [-dir[1], dir[0]];
                vec![
                    (add_scaled(self.from, right, shift), rise_at(0.0)),
                    (add_scaled(self.to, right, shift), rise_at(1.0)),
                ]
            }
            Some(arc) => {
                let radius = arc.radius - shift * arc.sweep.signum();
                let spoke = sub(self.from, arc.center);
                let parts = (arc.sweep.abs().to_degrees() / PATH_SAMPLE_DEGREES)
                    .ceil()
                    .max(1.0) as usize;
                (0..=parts)
                    .map(|k| {
                        let share = k as f64 / parts as f64;
                        let (sin, cos) = libm::sincos(arc.sweep * share);
                        let turned = [
                            spoke[0] * cos - spoke[1] * sin,
                            spoke[0] * sin + spoke[1] * cos,
                        ];
                        let point = add_scaled(arc.center, turned, radius / arc.radius);
                        (point, rise_at(share))
                    })
                    .collect()
            }
        }
    }
}

/// Путь по осям: въезд, части по дорогам и съезд.
#[derive(Debug, Clone, PartialEq)]
pub struct Route {
    pub entry: Vec2,
    pub legs: Vec<Leg>,
    pub exit: Vec2,
}

/// Длина и время пути по осям — то, что узнаёт код вызовом `road_route`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measure {
    pub length: f64,
    pub time: f64,
}

#[derive(Debug, Clone, Copy)]
struct Entry {
    piece: usize,
    share: f64,
    point: Vec2,
}

#[derive(Debug, Clone, Copy)]
enum Via {
    Unreached,
    Entry,
    Piece { piece: usize, from: usize },
}

struct Search {
    time: Vec<f64>,
    via: Vec<Via>,
}

#[derive(Debug, Clone, Copy)]
struct Exit {
    piece: usize,
    point: Vec2,
    gap: f64,
    time: f64,
    /// Узел, с которого съезжают на кусок; `None` — съезд на том же куске, что и въезд.
    from_node: Option<usize>,
}

#[derive(PartialEq)]
struct Waiting {
    time: f64,
    node: usize,
}

impl Eq for Waiting {}

impl Ord for Waiting {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .time
            .total_cmp(&self.time)
            .then(other.node.cmp(&self.node))
    }
}

impl PartialOrd for Waiting {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Ось дороги для редактора: точки с высотой от начала до конца и изгиб в градусах.
#[derive(Debug, Clone, PartialEq)]
pub struct AxisView {
    pub points: Vec<[f64; 3]>,
    pub bend: f64,
}

/// Что сеть рассказывает редактору о выбранной дороге.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadLinks {
    pub axis: AxisView,
    /// Середина оси: здесь ручка изгиба.
    pub handle: [f64; 3],
    pub links: Vec<([f64; 3], u32)>,
    pub ends: Vec<[f64; 3]>,
    pub neighbours: Vec<(u32, AxisView)>,
}

/// Сеть дорог: узлы, куски и связи. Собирается заново, когда меняются дороги, а предельные скорости
/// дорог — только время на их кусках.
#[derive(Debug, Default)]
pub struct RoadNetwork {
    forms: Vec<RoadForm>,
    speeds: Vec<Option<f64>>,
    roads: Vec<Road>,
    nodes: Vec<Vec2>,
    pieces: Vec<Piece>,
    around: Vec<Vec<usize>>,
    links: Vec<Link>,
    version: u64,
    fresh_forms: Vec<RoadForm>,
    fresh_speeds: Vec<Option<f64>>,
}

impl RoadNetwork {
    /// Номер состояния сети: растёт, когда сеть пересобрана или сменилась скорость на дороге.
    pub fn version(&self) -> u64 {
        self.version
    }

    pub fn is_empty(&self) -> bool {
        self.roads.is_empty()
    }

    /// Приводит сеть к дорогам `forms` (по возрастанию номера) и их скоростям `speeds`; `true`, если
    /// что-то изменилось.
    pub fn sync(&mut self, forms: &[RoadForm], speeds: &[Option<f64>]) -> bool {
        if self.forms != forms {
            self.rebuild(forms);
            self.speeds = speeds.to_vec();
            self.version += 1;
            return true;
        }
        if self.speeds != speeds {
            self.speeds = speeds.to_vec();
            self.version += 1;
            return true;
        }
        false
    }

    /// То же, что `sync`, для дорог, которые `fill` кладёт в переданные буферы: буферы живут между
    /// вызовами, и шаг без перемен ничего не выделяет.
    pub fn sync_with(
        &mut self,
        fill: impl FnOnce(&mut Vec<RoadForm>, &mut Vec<Option<f64>>),
    ) -> bool {
        let mut forms = std::mem::take(&mut self.fresh_forms);
        let mut speeds = std::mem::take(&mut self.fresh_speeds);
        fill(&mut forms, &mut speeds);
        let changed = self.sync(&forms, &speeds);
        self.fresh_forms = forms;
        self.fresh_speeds = speeds;
        changed
    }

    pub fn clear(&mut self) {
        self.rebuild(&[]);
        self.speeds.clear();
        self.version += 1;
    }

    fn rebuild(&mut self, forms: &[RoadForm]) {
        self.forms = forms.to_vec();
        self.roads = forms.iter().map(Road::from_form).collect();
        self.links = find_links(&self.roads);
        (self.nodes, self.pieces) = cut_into_pieces(&self.roads, &self.links);
        self.around = vec![Vec::new(); self.nodes.len()];
        for (index, piece) in self.pieces.iter().enumerate() {
            self.around[piece.a].push(index);
            self.around[piece.b].push(index);
        }
    }

    fn cap(&self, road: usize) -> Option<f64> {
        self.speeds.get(road).copied().flatten()
    }

    fn is_open(&self, road: usize) -> bool {
        self.cap(road).is_none_or(|cap| cap > 0.0)
    }

    /// Скорость машины со скоростью `speed` на дороге; `None` — дорога закрыта.
    fn travel_speed(&self, road: usize, speed: f64) -> Option<f64> {
        self.is_open(road)
            .then(|| self.cap(road).map_or(speed, |cap| cap.min(speed)))
    }

    fn piece_time(&self, piece: &Piece, speed: f64) -> Option<f64> {
        Some(piece.length / self.travel_speed(piece.road, speed)?)
    }

    /// Ближайшая к `p` точка куска, её доля пути по куску от узла `a` к `b` и высота в ней.
    fn piece_closest(&self, piece: &Piece, p: Vec2) -> (Vec2, f64, f64) {
        let (point, share) = match piece.span {
            Some([low, high]) => {
                let found = self.roads[piece.road].closest_between(p, low, high);
                let share = if high - low < EPS {
                    0.0
                } else {
                    (found.s - low) / (high - low)
                };
                (found.point, share)
            }
            None => closest_on_segment(p, self.nodes[piece.a], self.nodes[piece.b]),
        };
        (point, share, lerp(piece.z[0], piece.z[1], share))
    }

    /// Въезд: ближайшая к `start` точка осей открытых дорог; с `z` — среди дорог не выше и не ниже
    /// ступеньки в этой точке, а если таких нет, среди всех.
    fn find_entry(&self, start: Vec2, z: Option<f64>) -> Option<Entry> {
        let nearest = |near_z: Option<f64>| {
            let mut best: Option<(f64, Entry)> = None;
            for (index, piece) in self.pieces.iter().enumerate() {
                if !self.is_open(piece.road) {
                    continue;
                }
                let (point, share, height) = self.piece_closest(piece, start);
                if near_z.is_some_and(|z| (height - z).abs() > MAX_HEIGHT_GAP + EPS) {
                    continue;
                }
                let gap = distance(start, point);
                if best.is_none_or(|(known, _)| gap < known - 1e-12) {
                    best = Some((
                        gap,
                        Entry {
                            piece: index,
                            share,
                            point,
                        },
                    ));
                }
            }
            best.map(|(_, entry)| entry)
        };
        z.and_then(|z| nearest(Some(z))).or_else(|| nearest(None))
    }

    fn search(&self, entry: &Entry, speed: f64) -> Search {
        let mut time = vec![f64::INFINITY; self.nodes.len()];
        let mut via = vec![Via::Unreached; self.nodes.len()];
        let mut queue = BinaryHeap::new();
        let first = self.pieces[entry.piece];
        if let Some(whole) = self.piece_time(&first, speed) {
            for (node, share) in [(first.a, entry.share), (first.b, 1.0 - entry.share)] {
                time[node] = whole * share;
                via[node] = Via::Entry;
                queue.push(Waiting {
                    time: time[node],
                    node,
                });
            }
        }
        while let Some(Waiting {
            time: reached,
            node,
        }) = queue.pop()
        {
            if reached > time[node] {
                continue;
            }
            for &index in &self.around[node] {
                let piece = self.pieces[index];
                let Some(cost) = self.piece_time(&piece, speed) else {
                    continue;
                };
                let next = if piece.a == node { piece.b } else { piece.a };
                if reached + cost < time[next] {
                    time[next] = reached + cost;
                    via[next] = Via::Piece {
                        piece: index,
                        from: node,
                    };
                    queue.push(Waiting {
                        time: time[next],
                        node: next,
                    });
                }
            }
        }
        Search { time, via }
    }

    /// Съезд: ближайшая к `goal` точка осей среди тех, куда от въезда можно доехать; при равных —
    /// та, до которой быстрее.
    fn find_exit(&self, entry: &Entry, search: &Search, goal: Vec2, speed: f64) -> Option<Exit> {
        let mut best: Option<Exit> = None;
        for (index, piece) in self.pieces.iter().enumerate() {
            let Some(whole) = self.piece_time(piece, speed) else {
                continue;
            };
            let (point, share, _) = self.piece_closest(piece, goal);
            let mut arrival: Option<(f64, Option<usize>)> = None;
            let mut consider = |time: f64, from_node: Option<usize>| {
                if arrival.is_none_or(|(known, _)| time < known) {
                    arrival = Some((time, from_node));
                }
            };
            if index == entry.piece {
                consider((share - entry.share).abs() * whole, None);
            }
            consider(search.time[piece.a] + whole * share, Some(piece.a));
            consider(search.time[piece.b] + whole * (1.0 - share), Some(piece.b));
            let Some((time, from_node)) = arrival.filter(|(time, _)| time.is_finite()) else {
                continue;
            };
            let candidate = Exit {
                piece: index,
                point,
                gap: distance(goal, point),
                time,
                from_node,
            };
            let better = best.is_none_or(|known| {
                candidate.gap < known.gap - EPS
                    || (candidate.gap <= known.gap + EPS && candidate.time < known.time - EPS)
            });
            if better {
                best = Some(candidate);
            }
        }
        best
    }

    /// Часть пути по `piece` от точки `from` до точки `to`, обе лежат на куске.
    fn leg(&self, piece: &Piece, from: Vec2, to: Vec2) -> Leg {
        let road = &self.roads[piece.road];
        let cap = self.cap(piece.road);
        let width = road.width();
        let Some([low, high]) = piece.span else {
            let node = self.nodes[piece.a];
            let span = distance(node, self.nodes[piece.b]);
            let share_of = |point: Vec2| {
                if span < EPS {
                    0.0
                } else {
                    distance(node, point) / span
                }
            };
            let outward = if distance(node, road.end(1)) <= distance(node, road.end(0)) {
                1.0
            } else {
                -1.0
            };
            let away_from_end = if share_of(from) <= share_of(to) {
                1.0
            } else {
                -1.0
            };
            let slope = road.slope() * outward * away_from_end;
            return Leg {
                from,
                to,
                road: piece.road,
                width,
                cap,
                arc: None,
                rise: [share_of(from), share_of(to)].map(|share| Rise {
                    z: lerp(piece.z[0], piece.z[1], share),
                    slope,
                }),
                span: None,
            };
        };
        let s_from = road.closest_between(from, low, high).s;
        let s_to = road.closest_between(to, low, high).s;
        road_leg(road, piece.road, cap, [s_from, s_to], [from, to])
    }

    /// Части пути от въезда до съезда: куски, которыми едет машина, без нулевых и со слитыми
    /// продолжениями одной дороги.
    fn legs(&self, entry: &Entry, search: &Search, exit: &Exit) -> Vec<Leg> {
        let mut raw: Vec<Leg> = Vec::new();
        let first_piece = self.pieces[entry.piece];
        match exit.from_node {
            None => raw.push(self.leg(&first_piece, entry.point, exit.point)),
            Some(last) => {
                let mut chain = Vec::new();
                let mut node = last;
                while let Via::Piece { piece, from } = search.via[node] {
                    chain.push((piece, from, node));
                    node = from;
                }
                raw.push(self.leg(&first_piece, entry.point, self.nodes[node]));
                for &(piece, from, to) in chain.iter().rev() {
                    let piece = self.pieces[piece];
                    raw.push(self.leg(&piece, self.nodes[from], self.nodes[to]));
                }
                let last_piece = self.pieces[exit.piece];
                raw.push(self.leg(&last_piece, self.nodes[last], exit.point));
            }
        }
        let mut legs: Vec<Leg> = Vec::new();
        for leg in raw.into_iter().filter(|leg| leg.length() > EPS) {
            if let Some(previous) = legs.last_mut()
                && let Some(merged) = self.merged(previous, &leg)
            {
                *previous = merged;
            } else {
                legs.push(leg);
            }
        }
        legs
    }

    /// Две части подряд по одной дороге в одну сторону — одна часть.
    fn merged(&self, previous: &Leg, next: &Leg) -> Option<Leg> {
        if previous.road != next.road {
            return None;
        }
        match (previous.span, next.span, previous.arc, next.arc) {
            (Some(first), Some(second), _, _) => {
                let same_way = (first[1] - first[0]) * (second[1] - second[0]) > 0.0
                    && (first[1] - second[0]).abs() <= AXIS_EPS;
                same_way.then(|| {
                    road_leg(
                        &self.roads[previous.road],
                        previous.road,
                        previous.cap,
                        [first[0], second[1]],
                        [previous.from, next.to],
                    )
                })
            }
            (_, _, None, None) => {
                let straight_on = cross(
                    unit(sub(previous.to, previous.from)),
                    unit(sub(next.to, next.from)),
                )
                .abs()
                    < EPS
                    && dot(sub(previous.to, previous.from), sub(next.to, next.from)) > 0.0;
                straight_on.then(|| Leg {
                    to: next.to,
                    rise: [previous.rise[0], next.rise[1]],
                    span: None,
                    ..*previous
                })
            }
            _ => None,
        }
    }

    /// Самый быстрый путь по сети от `start` к `goal` для машины со скоростью `speed`; `None` —
    /// открытых дорог нет, и машина едет по прямой. `z` — высота машины: дороги её высоты предпочтены.
    pub fn route(&self, start: Vec2, goal: Vec2, speed: f64, z: Option<f64>) -> Option<Route> {
        if speed.is_nan() || speed <= 0.0 {
            return None;
        }
        let entry = self.find_entry(start, z)?;
        let search = self.search(&entry, speed);
        let exit = self.find_exit(&entry, &search, goal, speed)?;
        Some(Route {
            entry: entry.point,
            legs: self.legs(&entry, &search, &exit),
            exit: exit.point,
        })
    }

    /// Длина и время пути по осям из `from` в `to` со скоростью `speed`, прямые куски до въезда и
    /// после съезда вместе; `None` без скорости.
    pub fn measure(&self, from: Vec2, to: Vec2, speed: f64) -> Option<Measure> {
        if speed.is_nan() || speed <= 0.0 {
            return None;
        }
        let Some(route) = self.route(from, to, speed, None) else {
            let straight = distance(from, to);
            return Some(Measure {
                length: straight,
                time: straight / speed,
            });
        };
        let ends = distance(from, route.entry) + distance(route.exit, to);
        let mut length = ends;
        let mut time = ends / speed;
        for leg in &route.legs {
            length += leg.length();
            time += leg.length() / leg.cap.map_or(speed, |cap| cap.min(speed));
        }
        Some(Measure { length, time })
    }

    fn index_of(&self, id: u32) -> Option<usize> {
        self.roads.binary_search_by_key(&id, |road| road.id).ok()
    }

    fn axis_view(&self, index: usize) -> AxisView {
        let road = &self.roads[index];
        AxisView {
            points: road.axis_samples(),
            bend: (road.curvature() * road.length()).to_degrees(),
        }
    }

    /// Связи дороги `id`; `None`, если такой дороги в сети нет.
    pub fn links_of(&self, id: u32) -> Option<RoadLinks> {
        let index = self.index_of(id)?;
        let road = &self.roads[index];
        let with_height =
            |road: &Road, point: Vec2| [point[0], point[1], road.height_at(road.closest(point).s)];
        let mut links: Vec<(Vec2, u32)> = Vec::new();
        let mut neighbours: Vec<(u32, AxisView)> = Vec::new();
        for link in &self.links {
            let (point, other) = if link.a == index {
                (link.point_a, link.b)
            } else if link.b == index {
                (link.point_b, link.a)
            } else {
                continue;
            };
            let other_road = &self.roads[other];
            if links
                .iter()
                .any(|(known, id)| *id == other_road.id && distance(*known, point) <= EPS)
            {
                continue;
            }
            links.push((point, other_road.id));
            if !neighbours.iter().any(|(known, _)| *known == other_road.id) {
                neighbours.push((other_road.id, self.axis_view(other)));
            }
        }
        let ends = (0..2)
            .map(|end| road.end(end))
            .filter(|&end| links.iter().all(|(point, _)| distance(*point, end) > EPS))
            .map(|end| with_height(road, end))
            .collect();
        let middle = road.length() / 2.0;
        Some(RoadLinks {
            axis: self.axis_view(index),
            handle: with_height(road, road.point_at(middle)),
            links: links
                .into_iter()
                .map(|(point, id)| (with_height(road, point), id))
                .collect(),
            ends,
            neighbours,
        })
    }
}

/// Часть пути по оси дороги `road` от расстояния `span[0]` до `span[1]` между точками `points`.
fn road_leg(road: &Road, index: usize, cap: Option<f64>, span: [f64; 2], points: [Vec2; 2]) -> Leg {
    let forward = if span[1] >= span[0] { 1.0 } else { -1.0 };
    let slope = road.slope() * forward;
    let arc = (!road.is_straight()).then(|| LegArc {
        center: road.arc_center(),
        radius: road.radius(),
        sweep: road.curvature() * (span[1] - span[0]),
    });
    Leg {
        from: points[0],
        to: points[1],
        road: index,
        width: road.width(),
        cap,
        arc,
        rise: span.map(|s| Rise {
            z: road.height_at(s),
            slope,
        }),
        span: Some(span),
    }
}

/// Высота и наклон машины на дороге в её точке.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lift {
    /// Высота дороги в точке машины.
    pub z: f64,
    /// Подъём на единицу пути в сторону хода: тангенс наклона вдоль длины машины, на подъёме больше нуля.
    pub slope: f64,
}

/// Высота и уклон примитива пути; нет у кусков вне дороги.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Climb {
    z: [f64; 2],
    slope: f64,
}

/// Часть пути машины: прямая или дуга окружности. `cap` — предельная скорость на ней.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Primitive {
    Line {
        from: Vec2,
        to: Vec2,
        cap: Option<f64>,
        climb: Option<Climb>,
    },
    Arc {
        center: Vec2,
        start: Vec2,
        radius: f64,
        /// Поворот по дуге в радианах со знаком: куда повернут путь.
        sweep: f64,
        cap: Option<f64>,
        climb: Option<Climb>,
    },
}

impl Primitive {
    fn length(&self) -> f64 {
        match self {
            Primitive::Line { from, to, .. } => distance(*from, *to),
            Primitive::Arc { radius, sweep, .. } => radius * sweep.abs(),
        }
    }

    fn cap(&self) -> Option<f64> {
        match self {
            Primitive::Line { cap, .. } | Primitive::Arc { cap, .. } => *cap,
        }
    }

    fn climb(&self) -> Option<Climb> {
        match self {
            Primitive::Line { climb, .. } | Primitive::Arc { climb, .. } => *climb,
        }
    }

    fn with_climb(self, new: Option<Climb>) -> Primitive {
        match self {
            Primitive::Line { from, to, cap, .. } => Primitive::Line {
                from,
                to,
                cap,
                climb: new,
            },
            Primitive::Arc {
                center,
                start,
                radius,
                sweep,
                cap,
                ..
            } => Primitive::Arc {
                center,
                start,
                radius,
                sweep,
                cap,
                climb: new,
            },
        }
    }

    fn point_at(&self, travelled: f64) -> Vec2 {
        match self {
            Primitive::Line { from, to, .. } => {
                let len = distance(*from, *to);
                if len < EPS {
                    *to
                } else {
                    add_scaled(*from, sub(*to, *from), travelled / len)
                }
            }
            Primitive::Arc {
                center,
                start,
                radius,
                sweep,
                ..
            } => {
                let angle = sweep.signum() * travelled / radius;
                let (sin, cos) = (libm::sin(angle), libm::cos(angle));
                let spoke = sub(*start, *center);
                [
                    center[0] + spoke[0] * cos - spoke[1] * sin,
                    center[1] + spoke[0] * sin + spoke[1] * cos,
                ]
            }
        }
    }
}

fn min_cap(a: Option<f64>, b: Option<f64>) -> Option<f64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// Точка, где линии сторон двух частей продолжаются до пересечения; `None` — на стыке машина идёт
/// по прямой: разворот, параллельные линии или пересечение дальше ширины дороги от узла. Линии идут по
/// касательным сторон в узле.
fn joint_point(previous: Vec2, first: &Leg, second: &Leg, side: Side) -> Option<Vec2> {
    let before = first.side_curve(side);
    let after = second.side_curve(side);
    let (a1, d1) = (before[before.len() - 1].0, first.directions().1);
    let (a2, d2) = (after[0].0, second.directions().0);
    let b2 = after[after.len() - 1].0;
    let turn = libm::atan2(cross(d1, d2).abs(), dot(d1, d2));
    if turn > MAX_JOINT_TURN_DEGREES.to_radians() || cross(d1, d2).abs() < EPS {
        return None;
    }
    let along_first = cross(sub(a2, a1), d2) / cross(d1, d2);
    let point = add_scaled(a1, d1, along_first);
    let reach = first.width.max(second.width);
    (distance(point, first.to) <= reach
        && dot(sub(point, previous), d1) >= -EPS
        && dot(sub(b2, point), d2) >= -EPS)
        .then_some(point)
}

/// Ломаная пути: вершины, ширина дороги у каждой, предельная скорость и высота с уклоном на отрезке, что к ней
/// идёт.
struct Polyline {
    points: Vec<Vec2>,
    widths: Vec<f64>,
    caps: Vec<Option<f64>>,
    /// Высота и уклон при подходе к вершине по отрезку, что к ней идёт.
    rises: Vec<Option<Rise>>,
    /// Высота в вершине в начале отрезка, что от неё уходит: на стыке двух дорог, чьи высоты разошлись, отрезок
    /// начинается на высоте следующей дороги.
    starts: Vec<Option<f64>>,
}

impl Polyline {
    fn push(&mut self, point: Vec2, width: f64, cap: Option<f64>, rise: Option<Rise>) {
        let last = self.points[self.points.len() - 1];
        if distance(last, point) <= EPS {
            let known = self.rises.len() - 1;
            self.rises[known] = self.rises[known].or(rise);
            if let Some(rise) = rise {
                self.starts[known] = Some(rise.z);
            }
            return;
        }
        self.points.push(point);
        self.widths.push(width);
        self.caps.push(cap);
        self.rises.push(rise);
        self.starts.push(rise.map(|rise| rise.z));
    }

    fn last(&self) -> Vec2 {
        self.points[self.points.len() - 1]
    }
}

fn polyline_of(start: Vec2, goal: Vec2, legs: &[Leg], side: Side) -> Polyline {
    let mut line = Polyline {
        points: vec![start],
        widths: vec![0.0],
        caps: vec![None],
        rises: vec![None],
        starts: vec![None],
    };
    let mut joined = false;
    for (index, leg) in legs.iter().enumerate() {
        let curve = leg.side_curve(side);
        let (first, interior, last) =
            (curve[0], &curve[1..curve.len() - 1], curve[curve.len() - 1]);
        if !joined {
            let cap = if index == 0 { None } else { leg.cap };
            line.push(first.0, leg.width, cap, Some(first.1));
        }
        for &(point, rise) in interior {
            line.push(point, leg.width, leg.cap, Some(rise));
        }
        joined = false;
        let Some(next) = legs.get(index + 1) else {
            line.push(last.0, leg.width, leg.cap, Some(last.1));
            continue;
        };
        match joint_point(line.last(), leg, next, side) {
            Some(point) => {
                let rise = Rise {
                    z: (last.1.z + next.rise[0].z) / 2.0,
                    slope: last.1.slope,
                };
                line.push(point, leg.width.min(next.width), leg.cap, Some(rise));
                joined = true;
            }
            None => {
                line.push(last.0, leg.width, leg.cap, Some(last.1));
                let begin = next.side_curve(side)[0];
                line.push(
                    begin.0,
                    next.width,
                    min_cap(leg.cap, next.cap),
                    Some(begin.1),
                );
                joined = true;
            }
        }
    }
    line.push(goal, 0.0, None, None);
    line
}

/// Дуга, скругляющая излом `corner` между отрезками с направлениями `before` и `after`; касается обоих
/// на расстоянии `reach` от излома.
fn fillet(corner: Vec2, before: Vec2, after: Vec2, reach: f64, cap: Option<f64>) -> Primitive {
    let turn = libm::atan2(cross(before, after).abs(), dot(before, after));
    let side = cross(before, after).signum();
    let radius = reach / libm::tan(turn / 2.0);
    let start = add_scaled(corner, before, -reach);
    let center = add_scaled(start, [-before[1], before[0]], side * radius);
    Primitive::Arc {
        center,
        start,
        radius,
        sweep: side * turn,
        cap,
        climb: None,
    }
}

/// Высота и уклон на отрезке ломаной от вершины `index` к следующей, в доле `share` его длины.
struct Segments<'a>(&'a Polyline);

impl Segments<'_> {
    /// Высота отрезка, что идёт от вершины `index` к следующей, на его доле `share`; `None` вне дороги.
    fn z(&self, index: usize, share: f64) -> Option<f64> {
        let (from, to) = (self.0.starts[index]?, self.0.rises[index + 1]?);
        Some(lerp(from, to.z, share))
    }

    fn slope(&self, index: usize) -> Option<f64> {
        self.0.starts[index]?;
        self.0.rises[index + 1].map(|rise| rise.slope)
    }

    fn climb(&self, index: usize, shares: [f64; 2]) -> Option<Climb> {
        Some(Climb {
            z: [self.z(index, shares[0])?, self.z(index, shares[1])?],
            slope: self.slope(index)?,
        })
    }
}

fn round_corners(line: &Polyline) -> Vec<Primitive> {
    let points = &line.points;
    let segments = Segments(line);
    let mut primitives = Vec::new();
    let mut from = points[0];
    let mut from_share = 0.0;
    for (before_index, corner) in points.windows(3).enumerate() {
        let index = before_index + 1;
        let before = unit(sub(corner[1], corner[0]));
        let after = unit(sub(corner[2], corner[1]));
        let turn = libm::atan2(cross(before, after).abs(), dot(before, after));
        let (length_in, length_out) = (
            distance(corner[1], corner[0]),
            distance(corner[2], corner[1]),
        );
        let reach = (line.widths[index] * ARC_WIDTH_SHARE)
            .min(length_in / 2.0)
            .min(length_out / 2.0);
        if turn < EPS || turn > std::f64::consts::PI - EPS || reach < EPS {
            primitives.push(Primitive::Line {
                from,
                to: corner[1],
                cap: line.caps[index],
                climb: segments.climb(before_index, [from_share, 1.0]),
            });
            from = corner[1];
            from_share = 0.0;
            continue;
        }
        let share_in = 1.0 - reach / length_in;
        let share_out = reach / length_out;
        primitives.push(Primitive::Line {
            from,
            to: add_scaled(corner[1], before, -reach),
            cap: line.caps[index],
            climb: segments.climb(before_index, [from_share, share_in]),
        });
        let round = fillet(
            corner[1],
            before,
            after,
            reach,
            min_cap(line.caps[index], line.caps[index + 1]),
        );
        let arc_climb = match (
            segments.z(before_index, share_in),
            segments.z(index, share_out),
            segments.slope(before_index),
            segments.slope(index),
        ) {
            (Some(z0), Some(z1), Some(a), Some(b)) => Some(Climb {
                z: [z0, z1],
                slope: (a + b) / 2.0,
            }),
            _ => None,
        };
        primitives.push(round.with_climb(arc_climb));
        from = add_scaled(corner[1], after, reach);
        from_share = share_out;
    }
    let last = points.len() - 2;
    primitives.push(Primitive::Line {
        from,
        to: points[points.len() - 1],
        cap: line.caps[points.len() - 1],
        climb: segments.climb(last, [from_share, 1.0]),
    });
    primitives
}

/// Путь машины: прямые и дуги от её середины до точки назначения.
#[derive(Debug, Clone, PartialEq)]
pub struct Path {
    primitives: Vec<Primitive>,
    goal: Vec2,
}

impl Path {
    /// Путь по `route` со своей стороной `side`; без пути по сети — прямая от `start` к `goal`.
    pub fn new(start: Vec2, goal: Vec2, route: Option<&Route>, side: Side) -> Path {
        let legs = route.map_or(&[][..], |route| &route.legs);
        let primitives = if legs.is_empty() {
            vec![Primitive::Line {
                from: start,
                to: goal,
                cap: None,
                climb: None,
            }]
        } else {
            round_corners(&polyline_of(start, goal, legs, side))
        };
        Path { primitives, goal }
    }

    /// Длина пути с дугами.
    pub fn length(&self) -> f64 {
        self.primitives.iter().map(Primitive::length).sum()
    }
}

/// Где машина на пути: номер части и сколько по ней проехано.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
struct Cursor {
    index: usize,
    travelled: f64,
}

impl Cursor {
    /// Проезжает путь за `seconds` со скоростью `speed`; остаток времени на границе частей идёт на
    /// следующую со скоростью следующей. Возвращает, доехала ли машина.
    fn advance(&mut self, path: &Path, speed: f64, seconds: f64) -> bool {
        let mut left = seconds;
        while let Some(primitive) = path.primitives.get(self.index) {
            let pace = primitive.cap().map_or(speed, |cap| cap.min(speed));
            let reach = pace * left;
            let remaining = primitive.length() - self.travelled;
            if reach < remaining {
                self.travelled += reach;
                return false;
            }
            left -= remaining / pace;
            self.index += 1;
            self.travelled = 0.0;
        }
        true
    }

    fn position(&self, path: &Path) -> Vec2 {
        path.primitives
            .get(self.index)
            .map_or(path.goal, |primitive| primitive.point_at(self.travelled))
    }

    /// Высота и наклон на дороге; `None` вне дороги. Доехавшая машина стоит на конце последней части пути.
    fn lift(&self, path: &Path) -> Option<Lift> {
        let (primitive, travelled) = match path.primitives.get(self.index) {
            Some(primitive) => (primitive, self.travelled),
            None => {
                let last = path.primitives.last()?;
                (last, last.length())
            }
        };
        let climb = primitive.climb()?;
        let len = primitive.length();
        let share = if len < EPS {
            1.0
        } else {
            (travelled / len).clamp(0.0, 1.0)
        };
        Some(Lift {
            z: lerp(climb.z[0], climb.z[1], share),
            slope: climb.slope,
        })
    }
}

#[derive(Debug)]
struct CarPath {
    target: Vec2,
    position: Vec2,
    version: u64,
    side: Side,
    path: Path,
    cursor: Cursor,
}

/// Машина, какой её видит езда: что нужно, чтобы решить, нужен ли новый путь, и куда она едет.
#[derive(Debug, Clone, Copy)]
pub struct Driver {
    pub position: Vec2,
    pub size: Vec2,
    pub z: f64,
    pub target: Vec2,
    pub speed: f64,
    pub side: Side,
}

/// Куда машина попала за шаг: новое `position`, приехала ли и, если она на дороге, высота и наклон.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moved {
    pub position: Vec2,
    pub arrived: bool,
    pub lift: Option<Lift>,
}

/// Сеть дорог и пути машин — всё, что езда держит между шагами. Сеть одна на игру: ею пользуются и правило
/// `drive`, и вызов кода `road_route`, поэтому у неё два хозяина (`network_handle`); обращаются к ней по
/// очереди, в одном потоке и не вложенно.
#[derive(Debug, Default)]
pub struct RoadState {
    network: Rc<RefCell<RoadNetwork>>,
    paths: HashMap<u32, CarPath>,
}

impl RoadState {
    pub fn new() -> Self {
        RoadState::default()
    }

    /// Сеть игры для исполнителя кода.
    pub fn network_handle(&self) -> Rc<RefCell<RoadNetwork>> {
        Rc::clone(&self.network)
    }

    /// Сверяет сеть с дорогами мира, которые `fill` кладёт в переданные списки; `true`, если сеть
    /// изменилась.
    pub fn sync_with(&self, fill: impl FnOnce(&mut Vec<RoadForm>, &mut Vec<Option<f64>>)) -> bool {
        self.network.borrow_mut().sync_with(fill)
    }

    /// Связи дороги `id` по нынешней сети.
    pub fn links_of(&self, id: u32) -> Option<RoadLinks> {
        self.network.borrow().links_of(id)
    }

    /// Забывает сеть и пути: мир собран заново.
    pub fn clear(&mut self) {
        self.network.borrow_mut().clear();
        self.paths.clear();
    }

    /// Убирает пути машин, которых нет среди `keep`.
    pub fn prune(&mut self, keep: &HashSet<u32>) {
        self.paths.retain(|id, _| keep.contains(id));
    }

    pub fn path_count(&self) -> usize {
        self.paths.len()
    }

    fn plan(&self, driver: &Driver) -> CarPath {
        let center = [
            driver.position[0] + driver.size[0] / 2.0,
            driver.position[1] + driver.size[1] / 2.0,
        ];
        let network = self.network.borrow();
        let route = network.route(center, driver.target, driver.speed, Some(driver.z));
        CarPath {
            target: driver.target,
            position: driver.position,
            version: network.version(),
            side: driver.side,
            path: Path::new(center, driver.target, route.as_ref(), driver.side),
            cursor: Cursor::default(),
        }
    }

    /// Один шаг машины `id`: за `seconds` по пути, найденному заново от того места, где она стоит,
    /// если цель, сеть, сторона или место машины не те, с которыми путь был найден. Приехавшей путь
    /// не запоминается.
    pub fn drive(&mut self, id: u32, driver: &Driver, seconds: f64) -> Moved {
        let version = self.network.borrow().version();
        let mut car = match self.paths.remove(&id) {
            Some(known)
                if known.target == driver.target
                    && known.position == driver.position
                    && known.version == version
                    && known.side == driver.side =>
            {
                known
            }
            _ => self.plan(driver),
        };
        let arrived = car.cursor.advance(&car.path, driver.speed, seconds);
        let center = car.cursor.position(&car.path);
        let lift = car.cursor.lift(&car.path);
        car.position = [
            center[0] - driver.size[0] / 2.0,
            center[1] - driver.size[1] / 2.0,
        ];
        let position = car.position;
        if !arrived {
            self.paths.insert(id, car);
        }
        Moved {
            position,
            arrived,
            lift,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STEP: f64 = 1.0 / 60.0;

    fn shape(id: u32, position: Vec2, size: Vec2) -> RoadShape {
        RoadShape {
            id,
            position,
            size,
            sin_cos: (0.0, 1.0),
            z: 0.0,
        }
    }

    fn crossing() -> Vec<RoadShape> {
        vec![
            shape(0, [0.0, 9.5], [20.0, 1.0]),
            shape(1, [9.5, 0.0], [1.0, 20.0]),
        ]
    }

    fn straight_forms(shapes: &[RoadShape]) -> Vec<RoadForm> {
        shapes.iter().copied().map(RoadForm::straight).collect()
    }

    fn network(shapes: &[RoadShape], speeds: &[Option<f64>]) -> RoadNetwork {
        let mut network = RoadNetwork::default();
        network.sync(&straight_forms(shapes), speeds);
        network
    }

    fn network_of(forms: &[RoadForm], speeds: &[Option<f64>]) -> RoadNetwork {
        let mut network = RoadNetwork::default();
        network.sync(forms, speeds);
        network
    }

    fn near(actual: f64, expected: f64, eps: f64) {
        assert!(
            (actual - expected).abs() <= eps,
            "ожидалось {expected} ± {eps}, вышло {actual}"
        );
    }

    fn near_point(actual: Vec2, expected: Vec2) {
        near(actual[0], expected[0], 1e-9);
        near(actual[1], expected[1], 1e-9);
    }

    fn steps_to_arrive(path: &Path, speed: f64) -> usize {
        let mut cursor = Cursor::default();
        (1..=100_000)
            .find(|_| cursor.advance(path, speed, STEP))
            .expect("машина доехала")
    }

    fn corner_path(side: Side, speeds: &[Option<f64>]) -> Path {
        let network = network(&crossing(), speeds);
        let route = network.route([2.0, 10.0], [10.0, 2.0], 4.0, None);
        Path::new([2.0, 10.0], [10.0, 2.0], route.as_ref(), side)
    }

    /// Дорога по концам оси, ширине, изгибу и высотам концов.
    fn bent(id: u32, from: Vec2, to: Vec2, width: f64, bend: f64, z: [f64; 2]) -> RoadForm {
        let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
        let chord = dx.hypot(dy);
        let middle = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
        RoadForm {
            shape: RoadShape {
                id,
                position: [middle[0] - chord / 2.0, middle[1] - width / 2.0],
                size: [chord, width],
                sin_cos: (dy / chord, dx / chord),
                z: z[0],
            },
            bend,
            end_z: z[1],
        }
    }

    #[test]
    fn axes_run_along_the_long_side_and_a_square_along_its_width() {
        let long = Road::from_shape(&shape(0, [0.0, 9.5], [20.0, 1.0]));
        assert_eq!(long.axis(), [[0.0, 10.0], [20.0, 10.0]]);
        let tall = Road::from_shape(&shape(1, [9.5, 0.0], [1.0, 20.0]));
        assert_eq!(tall.axis(), [[10.0, 0.0], [10.0, 20.0]]);
        let square = Road::from_shape(&shape(2, [4.0, 4.0], [2.0, 2.0]));
        assert_eq!(square.axis(), [[4.0, 5.0], [6.0, 5.0]]);
    }

    #[test]
    fn a_rotated_road_has_a_rotated_axis() {
        let turned = RoadShape {
            sin_cos: (1.0, 0.0),
            ..shape(0, [0.0, 9.5], [20.0, 1.0])
        };
        let axis = Road::from_shape(&turned).axis();
        near_point(axis[0], [10.0, 0.0]);
        near_point(axis[1], [10.0, 20.0]);
    }

    #[test]
    fn crossing_axes_link_at_the_crossing_point() {
        let network = network(&crossing(), &[None, None]);
        let links = network.links_of(0).expect("дорога");
        assert_eq!(links.links, vec![([10.0, 10.0, 0.0], 1)]);
        assert_eq!(links.neighbours.len(), 1);
        assert_eq!(links.neighbours[0].0, 1);
        assert_eq!(links.ends, vec![[0.0, 10.0, 0.0], [20.0, 10.0, 0.0]]);
        assert_eq!(links.axis.points, vec![[0.0, 10.0, 0.0], [20.0, 10.0, 0.0]]);
        assert_eq!(links.handle, [10.0, 10.0, 0.0]);
    }

    #[test]
    fn an_end_inside_another_rectangle_links_with_a_zero_length_piece() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [20.0, 1.0]),
            shape(1, [20.0, 9.6], [8.0, 0.8]),
        ];
        let network = network(&shapes, &[None, None]);
        let links = network.links_of(1).expect("дорога");
        assert_eq!(links.links, vec![([20.0, 10.0, 0.0], 0)]);
        assert_eq!(links.ends, vec![[28.0, 10.0, 0.0]]);
        let route = network
            .route([2.0, 10.0], [27.0, 10.0], 4.0, None)
            .expect("путь");
        assert_eq!(route.legs.len(), 2);
        assert_eq!(route.legs[1].road, 1);
        near_point(route.legs[1].to, [27.0, 10.0]);
    }

    #[test]
    fn a_side_road_ending_beside_the_axis_links_through_a_short_piece() {
        let shapes = vec![
            shape(0, [0.0, 9.0], [20.0, 2.0]),
            shape(1, [10.0, 0.0], [1.0, 9.5]),
        ];
        let network = network(&shapes, &[None, None]);
        let links = network.links_of(1).expect("дорога");
        assert_eq!(links.links.len(), 1);
        near_point([links.links[0].0[0], links.links[0].0[1]], [10.5, 9.5]);
        let route = network
            .route([2.0, 10.0], [10.5, 1.0], 4.0, None)
            .expect("путь");
        assert!(route.legs.iter().any(|leg| leg.road == 1));
        near_point(route.exit, [10.5, 1.0]);
    }

    #[test]
    fn roads_on_different_heights_are_not_linked() {
        let mut shapes = crossing();
        shapes[1].z = 1.2;
        let network = network(&shapes, &[None, None]);
        assert!(network.links_of(0).expect("дорога").links.is_empty());
        shapes[1].z = 0.4;
        let network = self::network(&shapes, &[None, None]);
        assert_eq!(network.links_of(0).expect("дорога").links.len(), 1);
    }

    #[test]
    fn a_square_on_a_junction_is_a_node_for_every_road_ending_in_it() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [9.5, 1.0]),
            shape(1, [9.5, 9.5], [1.0, 1.0]),
            shape(2, [10.5, 9.5], [9.5, 1.0]),
            shape(3, [9.5, 0.0], [1.0, 9.5]),
        ];
        let network = network(&shapes, &[None; 4]);
        let route = network
            .route([1.0, 10.0], [10.0, 1.0], 4.0, None)
            .expect("путь");
        let measure = network
            .measure([1.0, 10.0], [10.0, 1.0], 4.0)
            .expect("мера");
        near(measure.length, 18.0, 1e-9);
        near_point(route.exit, [10.0, 1.0]);
    }

    #[test]
    fn the_example_route_is_sixteen_cells_in_four_seconds() {
        let network = network(&crossing(), &[None, None]);
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 16.0, 1e-9);
        near(measure.time, 4.0, 1e-9);
    }

    #[test]
    fn a_slow_road_changes_the_time_but_not_the_length() {
        let network = network(&crossing(), &[None, Some(2.0)]);
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 16.0, 1e-9);
        near(measure.time, 6.0, 1e-9);
    }

    #[test]
    fn a_closed_road_is_out_of_the_network() {
        let network = network(&crossing(), &[None, Some(0.0)]);
        let route = network
            .route([2.0, 10.0], [10.0, 2.0], 4.0, None)
            .expect("путь");
        near_point(route.exit, [10.0, 10.0]);
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 8.0 + 8.0, 1e-9);
    }

    #[test]
    fn with_every_road_closed_the_car_goes_straight() {
        let network = network(&crossing(), &[Some(0.0), Some(0.0)]);
        assert!(network.route([2.0, 10.0], [10.0, 2.0], 4.0, None).is_none());
        let measure = network
            .measure([2.0, 10.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, (128.0_f64).sqrt(), 1e-9);
    }

    #[test]
    fn the_faster_of_two_ways_wins_even_when_it_is_longer() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [10.0, 1.0]),
            shape(1, [10.0, 9.5], [10.0, 1.0]),
            shape(2, [0.0, 0.0], [1.0, 10.0]),
            shape(3, [0.0, 0.0], [20.0, 1.0]),
            shape(4, [19.0, 0.0], [1.0, 10.0]),
        ];
        let fast_around = network(&shapes, &[Some(1.0), Some(1.0), None, None, None]);
        let route = fast_around
            .route([0.5, 10.0], [19.5, 10.0], 5.0, None)
            .expect("путь");
        assert!(route.legs.iter().any(|leg| leg.road == 3));
        let slow_around = network(&shapes, &[None, None, Some(1.0), Some(1.0), Some(1.0)]);
        let route = slow_around
            .route([0.5, 10.0], [19.5, 10.0], 5.0, None)
            .expect("путь");
        assert!(route.legs.iter().all(|leg| leg.road <= 1));
    }

    #[test]
    fn the_exit_is_the_nearest_reachable_point_to_the_goal() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [10.0, 1.0]),
            shape(1, [12.0, 4.5], [10.0, 1.0]),
        ];
        let network = network(&shapes, &[None, None]);
        let route = network
            .route([2.0, 10.0], [12.0, 5.0], 4.0, None)
            .expect("путь");
        near_point(route.exit, [10.0, 10.0]);
    }

    #[test]
    fn the_entry_prefers_the_road_on_the_height_of_the_car() {
        let mut shapes = crossing();
        shapes[1].z = 1.2;
        let network = network(&shapes, &[None, None]);
        let low = network
            .route([10.0, 9.0], [3.0, 10.0], 4.0, Some(0.0))
            .expect("путь");
        near_point(low.entry, [10.0, 10.0]);
        let high = network
            .route([10.0, 9.0], [10.0, 3.0], 4.0, Some(1.2))
            .expect("путь");
        near_point(high.entry, [10.0, 9.0]);
    }

    #[test]
    fn the_middle_path_is_fifteen_point_seven_nine_cells_and_237_steps() {
        let path = corner_path(Side::Middle, &[None, None]);
        near(path.length(), 15.0 + std::f64::consts::FRAC_PI_4, 1e-9);
        near(path.length(), 15.79, 0.01);
        near(steps_to_arrive(&path, 4.0) as f64, 237.0, 3.0);
    }

    #[test]
    fn a_slow_second_road_slows_the_arc_and_the_rest_of_the_path() {
        let path = corner_path(Side::Middle, &[None, Some(2.0)]);
        near(steps_to_arrive(&path, 4.0) as f64, 361.0, 4.0);
    }

    #[test]
    fn the_right_side_runs_a_quarter_width_from_the_axis_with_a_wide_left_turn() {
        let path = corner_path(Side::Right, &[None, None]);
        let arcs: Vec<Primitive> = path
            .primitives
            .iter()
            .copied()
            .filter(|primitive| matches!(primitive, Primitive::Arc { .. }))
            .collect();
        let corner = arcs
            .iter()
            .find(|arc| matches!(arc, Primitive::Arc { radius, .. } if (radius - 0.5).abs() < 1e-9))
            .expect("дуга на перекрёстке");
        let Primitive::Arc { start, .. } = corner else {
            panic!("дуга");
        };
        near_point(*start, [9.75, 10.25]);
        near_point(corner.point_at(corner.length()), [10.25, 9.75]);
        let mut cursor = Cursor::default();
        let mut straight_part = f64::NEG_INFINITY;
        while !cursor.advance(&path, 4.0, STEP) {
            let at = cursor.position(&path);
            if (3.0..9.0).contains(&at[0]) {
                straight_part = straight_part.max((at[1] - 10.25).abs());
            }
        }
        near(straight_part, 0.0, 1e-9);
    }

    #[test]
    fn a_right_turn_passes_nearer_to_the_near_corner_than_a_left_turn() {
        let network = network(&crossing(), &[None, None]);
        let nearest_to_corner = |goal: Vec2| {
            let route = network.route([2.0, 10.0], goal, 4.0, None);
            let path = Path::new([2.0, 10.0], goal, route.as_ref(), Side::Right);
            let mut cursor = Cursor::default();
            let mut closest = f64::INFINITY;
            while !cursor.advance(&path, 4.0, STEP) {
                closest = closest.min(distance(cursor.position(&path), [9.5, 10.5]));
            }
            closest
        };
        assert!(nearest_to_corner([10.0, 18.0]) < nearest_to_corner([10.0, 2.0]));
    }

    #[test]
    fn an_arc_touches_both_segments_no_farther_than_the_reach_from_the_corner() {
        let arc = fillet([10.25, 10.25], [1.0, 0.0], [0.0, -1.0], 0.5, None);
        let Primitive::Arc { start, radius, .. } = arc else {
            panic!("дуга");
        };
        near_point(start, [9.75, 10.25]);
        near(radius, 0.5, 1e-9);
        near_point(arc.point_at(arc.length()), [10.25, 9.75]);
        for step in 0..=10 {
            let point = arc.point_at(arc.length() * step as f64 / 10.0);
            assert!(distance(point, [10.25, 10.25]) <= 0.5 + 1e-9);
        }
    }

    #[test]
    fn a_uturn_goes_straight_across_from_one_side_to_the_other() {
        let first = Leg::straight([0.0, 10.0], [10.0, 10.0], 0, 1.0, None);
        let back = Leg::straight([10.0, 10.0], [0.0, 10.0], 0, 1.0, None);
        assert!(joint_point([0.0, 10.25], &first, &back, Side::Right).is_none());
        let line = polyline_of([0.0, 10.0], [0.0, 10.0], &[first, back], Side::Right);
        assert!(line.points.contains(&[10.0, 10.25]));
        assert!(line.points.contains(&[10.0, 9.75]));
    }

    #[test]
    fn a_car_far_from_the_roads_first_drives_straight_to_the_nearest_one() {
        let network = network(&crossing(), &[None, None]);
        let route = network
            .route([2.0, 14.0], [10.0, 2.0], 4.0, None)
            .expect("путь");
        near_point(route.entry, [2.0, 10.0]);
        let measure = network
            .measure([2.0, 14.0], [10.0, 2.0], 4.0)
            .expect("мера");
        near(measure.length, 4.0 + 16.0, 1e-9);
    }

    #[test]
    fn a_goal_far_from_the_roads_ends_with_a_straight_piece() {
        let network = network(&crossing(), &[None, None]);
        let measure = network
            .measure([2.0, 10.0], [10.0, -6.0], 4.0)
            .expect("мера");
        near(measure.length, 8.0 + 10.0 + 6.0, 1e-9);
    }

    #[test]
    fn a_goal_on_a_cut_off_network_goes_to_the_nearest_point_of_its_own_piece() {
        let shapes = vec![
            shape(0, [0.0, 9.5], [10.0, 1.0]),
            shape(1, [30.0, 9.5], [10.0, 1.0]),
        ];
        let network = network(&shapes, &[None, None]);
        let route = network
            .route([2.0, 10.0], [29.0, 10.0], 4.0, None)
            .expect("путь");
        near_point(route.exit, [10.0, 10.0]);
    }

    #[test]
    fn driving_on_the_example_arrives_exactly_and_forgets_the_path() {
        let mut state = RoadState::new();
        state
            .network
            .borrow_mut()
            .sync(&straight_forms(&crossing()), &[None, None]);
        let mut driver = Driver {
            position: [1.9, 9.7],
            size: [0.2, 0.6],
            z: 0.0,
            target: [10.0, 2.0],
            speed: 4.0,
            side: Side::Middle,
        };
        let mut arrived_after = None;
        for step in 1..=400 {
            let moved = state.drive(0, &driver, STEP);
            driver.position = moved.position;
            if moved.arrived {
                arrived_after = Some(step);
                break;
            }
        }
        near(arrived_after.expect("доехала") as f64, 237.0, 3.0);
        near_point(
            [
                driver.position[0] + driver.size[0] / 2.0,
                driver.position[1] + driver.size[1] / 2.0,
            ],
            [10.0, 2.0],
        );
        assert_eq!(state.path_count(), 0);
    }

    #[test]
    fn a_moved_car_gets_a_new_path_from_where_it_stands() {
        let mut state = RoadState::new();
        state
            .network
            .borrow_mut()
            .sync(&straight_forms(&crossing()), &[None, None]);
        let mut driver = Driver {
            position: [1.9, 9.7],
            size: [0.2, 0.6],
            z: 0.0,
            target: [10.0, 2.0],
            speed: 4.0,
            side: Side::Middle,
        };
        state.drive(0, &driver, STEP);
        driver.position = [14.9, 9.7];
        let again = state.drive(0, &driver, STEP);
        near_point(
            [again.position[0] + 0.1, again.position[1] + 0.3],
            [15.0 - 4.0 / 60.0, 10.0],
        );
    }

    #[test]
    fn many_roads_find_their_links_through_the_grid() {
        let mut shapes = Vec::new();
        for row in 0..30 {
            shapes.push(shape(row, [-5.0, row as f64 * 3.0 + 1.0], [70.0, 1.0]));
        }
        for column in 0..20 {
            shapes.push(shape(
                30 + column,
                [column as f64 * 3.0, -5.0],
                [1.0, 100.0],
            ));
        }
        let speeds = vec![None; shapes.len()];
        let network = network(&shapes, &speeds);
        assert_eq!(network.links_of(0).expect("дорога").links.len(), 20);
        assert_eq!(network.links_of(30).expect("дорога").links.len(), 30);
    }

    fn arc_road(bend: f64) -> Road {
        Road::from_form(&bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, bend, [0.0, 0.0]))
    }

    #[test]
    fn an_arc_has_the_radius_and_length_of_its_chord_and_bend_and_keeps_its_ends() {
        let road = arc_road(90.0);
        let radius = 20.0 / (2.0 * (45.0_f64).to_radians().sin());
        near(road.radius(), radius, 1e-9);
        near(road.length(), radius * std::f64::consts::FRAC_PI_2, 1e-9);
        near_point(road.end(0), [0.0, 10.0]);
        near_point(road.end(1), [20.0, 10.0]);
        let (start, end) = (road.tangent_at(0.0), road.tangent_at(road.length()));
        let angle = |t: Vec2| libm::atan2(t[1], t[0]).to_degrees();
        near(angle(start), -45.0, 1e-9);
        near(angle(end), 45.0, 1e-9);
        near(road.height_at(road.length()), 0.0, 1e-12);
    }

    #[test]
    fn a_positive_bend_turns_the_axis_toward_a_positive_rotation_and_a_negative_one_away() {
        let positive = arc_road(60.0);
        let negative = arc_road(-60.0);
        let middle = |road: &Road| road.point_at(road.length() / 2.0);
        assert!(middle(&positive)[1] < 10.0, "ось прогнулась к меньшему y");
        assert!(middle(&negative)[1] > 10.0);
        near(
            middle(&positive)[1] - 10.0,
            -(10.0 - middle(&negative)[1] + 0.0).abs(),
            1e-9,
        );
        assert_eq!(arc_road(0.0).tangent_at(3.0), [1.0, 0.0]);
    }

    #[test]
    fn a_bend_is_taken_as_the_nearest_allowed_one() {
        near(clamp_bend(200.0, 20.0, 1.0), MAX_BEND_DEGREES, 1e-12);
        near(clamp_bend(-200.0, 20.0, 1.0), -MAX_BEND_DEGREES, 1e-12);
        let narrow = clamp_bend(170.0, 1.2, 1.0);
        let radius = 1.2 / (2.0 * (narrow.abs().to_radians() / 2.0).sin());
        near(radius, 1.0, 1e-9);
        assert_eq!(clamp_bend(f64::NAN, 20.0, 1.0), 0.0);
        assert_eq!(clamp_bend(30.0, 20.0, 1.0), 30.0);
        let road = Road::from_form(&bent(0, [0.0, 10.0], [2.0, 10.0], 1.0, 170.0, [0.0, 0.0]));
        assert!(road.radius() >= 1.0 - 1e-9);
    }

    #[test]
    fn the_closest_point_of_an_arc_and_its_strip_follow_the_circle() {
        let road = arc_road(90.0);
        let center = road.arc_center();
        let radius = road.radius();
        let probe = add_scaled(center, unit(sub(road.point_at(5.0), center)), radius + 0.3);
        let near_axis = road.closest(probe);
        near(near_axis.s, 5.0, 1e-9);
        near(near_axis.distance, 0.3, 1e-9);
        assert!(road.contains(probe));
        let outside = add_scaled(center, unit(sub(road.point_at(5.0), center)), radius + 0.6);
        assert!(!road.contains(outside));
        let behind = add_scaled(road.end(0), road.tangent_at(0.0), -0.2);
        assert!(!road.contains(behind), "торец поперёк оси");
        let beyond = road.closest(add_scaled(road.end(1), road.tangent_at(road.length()), 3.0));
        near(beyond.s, road.length(), 1e-9);
    }

    #[test]
    fn an_arc_and_a_line_cross_where_they_meet_and_two_arcs_link_too() {
        let street = bent(1, [10.0, 0.0], [10.0, 20.0], 1.0, 0.0, [0.0, 0.0]);
        let arc = bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, 90.0, [0.0, 0.0]);
        let net = network_of(&[arc, street], &[None, None]);
        let links = net.links_of(0).expect("дорога");
        assert_eq!(links.links.len(), 1);
        let (point, id) = links.links[0];
        assert_eq!(id, 1);
        near(point[0], 10.0, 1e-6);
        let road = Road::from_form(&arc);
        near(road.closest([point[0], point[1]]).distance, 0.0, 1e-6);
        let second = bent(1, [10.0, 0.0], [10.0, 20.0], 1.0, -90.0, [0.0, 0.0]);
        let two = network_of(&[arc, second], &[None, None]);
        assert!(!two.links_of(0).expect("дорога").links.is_empty());
    }

    #[test]
    fn heights_are_compared_at_the_link_point_not_by_one_height_per_road() {
        // Одна дорога идёт вверх на 2 клетки; пересечение в её середине (высота 1), вторая — на высоте 1,2.
        let climbing = bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, 0.0, [0.0, 2.0]);
        let level = bent(1, [10.0, 0.0], [10.0, 20.0], 1.0, 0.0, [1.2, 1.2]);
        let net = network_of(&[climbing, level], &[None, None]);
        assert_eq!(net.links_of(0).expect("дорога").links.len(), 1);
        let low = bent(1, [10.0, 0.0], [10.0, 20.0], 1.0, 0.0, [0.0, 0.0]);
        let net = network_of(&[climbing, low], &[None, None]);
        assert!(net.links_of(0).expect("дорога").links.is_empty());
    }

    #[test]
    fn the_route_along_an_arc_is_measured_along_the_arc_and_the_side_is_a_concentric_arc() {
        let road = bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, 90.0, [0.0, 0.0]);
        let net = network_of(&[road], &[None]);
        let arc_length = Road::from_form(&road).length();
        let measure = net.measure([0.0, 10.0], [20.0, 10.0], 4.0).expect("мера");
        near(measure.length, arc_length, 1e-6);
        near(measure.time, arc_length / 4.0, 1e-6);
        let route = net
            .route([0.0, 10.0], [20.0, 10.0], 4.0, None)
            .expect("путь");
        let path = Path::new([0.0, 10.0], [20.0, 10.0], Some(&route), Side::Right);
        let center = Road::from_form(&road).arc_center();
        let mut cursor = Cursor::default();
        let mut worst: f64 = 0.0;
        while !cursor.advance(&path, 4.0, STEP) {
            let at = cursor.position(&path);
            if at[0] > 1.0 && at[0] < 19.0 {
                worst = worst
                    .max((distance(at, center) - (Road::from_form(&road).radius() - 0.25)).abs());
            }
        }
        assert!(worst < 2e-3, "сторона отошла от дуги на {worst}");
    }

    #[test]
    fn a_car_on_a_sloping_road_gets_the_height_and_the_tilt_of_its_point() {
        let road = bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, 0.0, [0.0, 2.0]);
        let net = network_of(&[road], &[None]);
        let route = net
            .route([5.0, 10.0], [15.0, 10.0], 4.0, None)
            .expect("путь");
        let path = Path::new([5.0, 10.0], [15.0, 10.0], Some(&route), Side::Middle);
        let mut cursor = Cursor::default();
        cursor.advance(&path, 4.0, 1.0);
        let lift = cursor.lift(&path).expect("на дороге");
        near(lift.z, 0.1 * cursor.position(&path)[0], 1e-9);
        near(lift.slope, 0.1, 1e-9);
        let back = net
            .route([15.0, 10.0], [5.0, 10.0], 4.0, None)
            .expect("путь");
        let path = Path::new([15.0, 10.0], [5.0, 10.0], Some(&back), Side::Middle);
        let mut cursor = Cursor::default();
        cursor.advance(&path, 4.0, 1.0);
        near(cursor.lift(&path).expect("на дороге").slope, -0.1, 1e-9);
    }

    #[test]
    fn the_approach_from_beside_the_road_has_no_lift() {
        let road = bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, 0.0, [0.0, 2.0]);
        let net = network_of(&[road], &[None]);
        let route = net
            .route([5.0, 14.0], [15.0, 10.0], 4.0, None)
            .expect("путь");
        let path = Path::new([5.0, 14.0], [15.0, 10.0], Some(&route), Side::Middle);
        let mut cursor = Cursor::default();
        cursor.advance(&path, 4.0, 0.2);
        assert_eq!(cursor.lift(&path), None);
    }

    #[test]
    fn a_chain_of_two_roads_with_two_slopes_breaks_the_profile_at_their_link() {
        let first = bent(0, [0.0, 10.0], [10.0, 10.0], 1.0, 0.0, [0.0, 1.0]);
        let second = bent(1, [10.0, 10.0], [20.0, 10.0], 1.0, 0.0, [1.0, 1.5]);
        let net = network_of(&[first, second], &[None, None]);
        let route = net
            .route([2.0, 10.0], [18.0, 10.0], 4.0, None)
            .expect("путь");
        assert_eq!(route.legs.len(), 2);
        near(route.legs[0].rise[1].z, 1.0, 1e-9);
        near(route.legs[0].rise[1].slope, 0.1, 1e-9);
        near(route.legs[1].rise[0].slope, 0.05, 1e-9);
    }

    /// Ближайшая точка дуги и принадлежность полосе — как у перебора точек оси.
    #[test]
    fn the_closest_point_and_the_strip_of_arcs_agree_with_a_brute_force_walk_along_the_axis() {
        for bend in [-170.0, -90.0, -20.0, 25.0, 90.0, 150.0] {
            let road = arc_road(bend);
            let walk: Vec<(f64, Vec2)> = (0..=4000)
                .map(|k| {
                    let s = road.length() * f64::from(k) / 4000.0;
                    (s, road.point_at(s))
                })
                .collect();
            let mut state = 12345_u64;
            let mut random = || {
                state = state
                    .wrapping_mul(6364136223846793005)
                    .wrapping_add(1442695040888963407);
                (state >> 33) as f64 / (1_u64 << 31) as f64
            };
            for _ in 0..400 {
                let point = [-10.0 + 40.0 * random(), -15.0 + 40.0 * random()];
                let found = road.closest(point);
                let best = walk
                    .iter()
                    .map(|(_, axis)| distance(point, *axis))
                    .fold(f64::INFINITY, f64::min);
                assert!(
                    found.distance <= best + 1e-9 && found.distance >= best - 5e-3,
                    "bend {bend}: {point:?} → {} против {best}",
                    found.distance
                );
                near(
                    distance(point, road.point_at(found.s)),
                    found.distance,
                    1e-9,
                );
                let (nearest, _) = walk
                    .iter()
                    .enumerate()
                    .map(|(at, (_, axis))| (at, distance(point, *axis)))
                    .fold((0, f64::INFINITY), |(known, least), (at, gap)| {
                        if gap < least {
                            (at, gap)
                        } else {
                            (known, least)
                        }
                    });
                let interior = nearest > 0 && nearest < walk.len() - 1;
                if interior && (best - 0.5).abs() > 5e-3 {
                    assert_eq!(road.contains(point), best <= 0.5, "bend {bend}: {point:?}");
                }
            }
        }
    }

    /// Пересечения осей совпадают с пересечением их ломаных, взятых мелко: прямая с дугой и дуга с дугой.
    #[test]
    fn crossings_of_arcs_and_lines_agree_with_the_crossings_of_their_fine_polylines() {
        let mut state = 99_u64;
        let mut random = || {
            state = state
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (state >> 33) as f64 / (1_u64 << 31) as f64
        };
        let polyline = |road: &Road| -> Vec<Vec2> {
            (0..=500)
                .map(|k| road.point_at(road.length() * f64::from(k) / 500.0))
                .collect()
        };
        let crossing = |a: &[Vec2], b: &[Vec2]| {
            a.windows(2).any(|first| {
                b.windows(2).any(|second| {
                    let (r, s) = (sub(first[1], first[0]), sub(second[1], second[0]));
                    let denominator = cross(r, s);
                    if denominator.abs() < 1e-12 {
                        return false;
                    }
                    let between = sub(second[0], first[0]);
                    let t = cross(between, s) / denominator;
                    let u = cross(between, r) / denominator;
                    (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)
                })
            })
        };
        let mut found = 0;
        for _ in 0..100 {
            let mut road = |id: u32| {
                let from = [30.0 * random(), 30.0 * random()];
                let to = [30.0 * random(), 30.0 * random()];
                let bend = if random() < 0.25 {
                    0.0
                } else {
                    200.0 * random() - 100.0
                };
                Road::from_form(&bent(id, from, to, 0.5, bend, [0.0, 0.0]))
            };
            let (first, second) = (road(0), road(1));
            let reported = axes_cross(&first, &second);
            for point in &reported {
                assert!(first.closest(*point).distance < 1e-6);
                assert!(second.closest(*point).distance < 1e-6);
            }
            let by_walk = crossing(&polyline(&first), &polyline(&second));
            assert_eq!(
                !reported.is_empty(),
                by_walk,
                "{:?} {:?}: оси {reported:?}",
                first.axis(),
                second.axis()
            );
            found += usize::from(by_walk);
        }
        assert!(found > 10, "слишком мало пересечений для проверки: {found}");
    }

    #[test]
    fn the_axis_samples_of_an_arc_turn_no_more_than_the_step_and_carry_the_profile() {
        let road = Road::from_form(&bent(0, [0.0, 10.0], [20.0, 10.0], 1.0, 90.0, [0.0, 1.0]));
        let samples = road.axis_samples();
        assert_eq!(samples.len(), 46);
        near(samples[0][2], 0.0, 1e-12);
        near(samples[45][2], 1.0, 1e-12);
        near(
            samples[22][2],
            road.height_at(road.length() * 22.0 / 45.0),
            1e-12,
        );
        let straight = Road::from_shape(&shape(0, [0.0, 9.5], [20.0, 1.0]));
        assert_eq!(straight.axis_samples().len(), 2);
    }
}

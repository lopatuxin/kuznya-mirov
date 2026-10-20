//! «Трёхмерная сцена» → «Вид дороги», перекрёстки готовыми моделями: выбор модели из `scene.road_junctions` по
//! рукавам перекрёстка на карте и изогнутая копия её частей. Ни дорог мира, ни видеокарты: рукава приходят углами,
//! видами и ширинами, а на выходе — выбранная модель и части с вершинами, уже лежащими на карте вокруг середины.

use std::f64::consts::{FRAC_PI_2, PI, TAU};

use super::super::model::{ModelTable, ModelVertex, Part};
use super::{JunctionArm, JunctionModel, RoadKindTable};

/// Цена рукава, чей вид не совпал с видом рукава модели, — как изгиб на 45° (по сумме изгибов всех рукавов): модель
/// своих видов, которую пришлось бы гнуть сильнее, проигрывает модели без изгиба с рукавом другого вида, суженным под
/// дорогу. Изгиб на 90° растягивает угол модели вдвое, и скругление вылезает горбом за дорогу.
const KIND_MISMATCH_COST: f64 = 45.0;
/// На сколько несовпавший рукав дешевле, если вид рукава модели шире.
const WIDER_MODEL_DISCOUNT: f64 = 1.0;
/// Цена зеркального отражения модели.
const MIRROR_COST: f64 = 1.0;
/// Цены ближе этого равны: при равной цене выигрывает модель, что раньше в `scene.road_junctions`.
const COST_EPS: f64 = 1e-9;
/// Доля половины угла между соседними рукавами, где ширина рукава подгоняется целиком, если полоса рукава короче;
/// дальше до середины между рукавами подгонка плавно сходит на нет.
const FIT_SHARE: f64 = 0.95;
/// Запас полосы рукава на кромку сетки: полуширина рукава модели, увеличенная в эту долю.
const STRIP_MARGIN: f64 = 1.1;
/// Свободный грунт между полосами двух соседних рукавов — не меньше этой доли меньшего из углов между рукавами в
/// модели и после поворота: иначе сужаются полосы.
const MIN_FREE_GAP: f64 = 0.3;

/// Рукав перекрёстка на карте. Углы — в радианах от +X против часовой, если смотреть сверху, как `dir` в
/// `scene.road_junctions`; на карте, где +Y идёт к игроку, это от +X к −Y.
#[derive(Debug, Clone, Copy)]
pub struct SpotArm {
    pub angle: f64,
    pub kind: usize,
    /// Ширина дороги рукава в клетках.
    pub width: f64,
    /// Насколько ось дороги сдвинута от прямой через середину вдоль рукава; положительный — влево от рукава.
    pub offset: f64,
}

/// Выбранная модель: её номер в `RoadKindTable::junctions`, отражена ли она и какой рукав модели (по номеру в
/// её описании) сопоставлен каждому рукаву перекрёстка. Рукава перекрёстка идут по возрастанию угла.
#[derive(Debug, Clone, PartialEq)]
pub struct Fit {
    pub model: usize,
    pub mirrored: bool,
    pub matched: Vec<usize>,
}

/// Изогнутая копия модели: масштаб, вырезы раскладки и части.
pub struct Placed {
    pub scale: f64,
    /// Расстояние от середины до конца вырезки раскладки у каждого рукава перекрёстка и удлинение рукава.
    pub mouths: Vec<f64>,
    pub extensions: Vec<f64>,
    pub parts: Vec<Part>,
}

fn wrap_pi(angle: f64) -> f64 {
    (angle + PI).rem_euclid(TAU) - PI
}

/// Угол рукава модели после возможного отражения: отражение меняет направление против часовой на обратное.
fn model_angle(arm: &JunctionArm, mirrored: bool) -> f64 {
    if mirrored {
        (TAU - arm.dir).rem_euclid(TAU)
    } else {
        arm.dir
    }
}

/// Рукава модели по возрастанию угла: угол и номер рукава в описании.
fn arms_in_order(model: &JunctionModel, mirrored: bool) -> Vec<(f64, usize)> {
    let mut order: Vec<(f64, usize)> = model
        .arms
        .iter()
        .enumerate()
        .map(|(index, arm)| (model_angle(arm, mirrored), index))
        .collect();
    order.sort_by(|a, b| a.0.total_cmp(&b.0));
    order
}

fn mismatch_cost(kinds: &RoadKindTable, model_arm: &JunctionArm, arm: &SpotArm) -> f64 {
    if model_arm.kind == arm.kind {
        return 0.0;
    }
    let wider = kinds
        .get(arm.kind)
        .is_some_and(|kind| model_arm.width > kind.width());
    KIND_MISMATCH_COST - if wider { WIDER_MODEL_DISCOUNT } else { 0.0 }
}

/// Цена сопоставления: сумма модулей разностей направлений после лучшего поворота модели в градусах, цена
/// несовпавших видов и цена отражения. Лучший поворот — один из тех, что совмещают пару рукавов: сумма
/// модулей разностей линейна между ними.
fn match_cost(
    kinds: &RoadKindTable,
    model: &JunctionModel,
    mirrored: bool,
    arms: &[SpotArm],
    matched: &[usize],
) -> f64 {
    let turns: Vec<f64> = arms
        .iter()
        .zip(matched)
        .map(|(arm, &index)| wrap_pi(arm.angle - model_angle(&model.arms[index], mirrored)))
        .collect();
    let angular = turns
        .iter()
        .map(|&turn| {
            turns
                .iter()
                .map(|&other| wrap_pi(other - turn).abs())
                .sum::<f64>()
        })
        .fold(f64::INFINITY, f64::min)
        .to_degrees();
    let kinds_cost: f64 = arms
        .iter()
        .zip(matched)
        .map(|(arm, &index)| mismatch_cost(kinds, &model.arms[index], arm))
        .sum();
    angular + kinds_cost + if mirrored { MIRROR_COST } else { 0.0 }
}

/// Самая дешёвая модель и сопоставление для рукавов `arms` (по возрастанию угла); при равной цене — модель,
/// что раньше в таблице. Нет модели с таким числом рукавов и частей — `None`.
pub fn select(kinds: &RoadKindTable, models: &ModelTable, arms: &[SpotArm]) -> Option<Fit> {
    let mut best: Option<(f64, Fit)> = None;
    for (index, candidate) in kinds.junctions().iter().enumerate() {
        let drawable = models
            .get(candidate.model)
            .is_some_and(|model| !model.parts.is_empty());
        if !drawable || candidate.arms.len() != arms.len() {
            continue;
        }
        for mirrored in [false, true] {
            let order = arms_in_order(candidate, mirrored);
            for shift in 0..order.len() {
                let matched: Vec<usize> = (0..order.len())
                    .map(|at| order[(at + shift) % order.len()].1)
                    .collect();
                let cost = match_cost(kinds, candidate, mirrored, arms, &matched);
                if best
                    .as_ref()
                    .is_none_or(|(known, _)| cost < known - COST_EPS)
                {
                    best = Some((
                        cost,
                        Fit {
                            model: index,
                            mirrored,
                            matched,
                        },
                    ));
                }
            }
        }
    }
    best.map(|(_, fit)| fit)
}

/// Рукав в изгибе: угол модели и угол, в который он переходит, накопленный поворот рукава, полуширина полосы
/// рукава с запасом, множитель ширины `f`, расстояние от середины до устья прямой части рукава модели и сдвиг оси
/// дороги. Рукава идут по возрастанию угла модели.
#[derive(Debug, Clone, Copy)]
struct Knot {
    arm: usize,
    from: f64,
    to: f64,
    turn: f64,
    half_width: f64,
    factor: f64,
    /// Расстояние от середины до устья в модели и после удлинения рукава на `extension`.
    base_mouth: f64,
    extension: f64,
    mouth: f64,
    offset: f64,
}

/// Наибольшая сумма углов полос двух соседних рукавов у устий, при которой они помещаются в угол между рукавами в
/// модели `model` и после поворота `image`: свободного грунта остаётся не меньше `MIN_FREE_GAP` меньшего из углов.
fn strips_bound(model: f64, image: f64) -> f64 {
    (1.0 - MIN_FREE_GAP) * model.min(image)
}

/// Доля, в которой сужаются полосы двух соседних рукавов, чьи углы у устий в сумме `need`: единица, когда они
/// помещаются (`strips_bound`). У перекрёстка после удлинения рукавов она единица всегда.
fn strip_share(model: f64, image: f64, need: f64) -> f64 {
    (strips_bound(model, image) / need).clamp(0.0, 1.0)
}

/// Угол от оси рукава, в котором точка на расстоянии `radius` от середины ещё в полосе рукава полушириной `half`.
fn strip_angle(half: f64, radius: f64) -> f64 {
    libm::asin((half / radius.max(1e-12)).min(1.0))
}

/// Угол против часовой от `from` до `to` у перекрёстка с `count` рукавами: у одного рукава — полный круг.
fn turn_between(count: usize, from: f64, to: f64) -> f64 {
    if count == 1 {
        TAU
    } else {
        (to - from).rem_euclid(TAU)
    }
}

/// Гладко от 0 при `along` ≤ 0 до 1 при `along` ≥ `mouth`.
fn smoothstep(along: f64, mouth: f64) -> f64 {
    if mouth <= 0.0 {
        return if along > 0.0 { 1.0 } else { 0.0 };
    }
    let t = (along / mouth).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Перевод точек модели на карту: отражение, масштаб, поворот рукавов вокруг середины с изгибом грунта в углах и
/// подгонка ширины рукавов. Внутри углы и точки лежат в плоскости «против часовой, если смотреть сверху», то есть
/// `y` карты со знаком минус.
struct Warp {
    origin: [f32; 2],
    mirrored: bool,
    scale: f64,
    knots: Vec<Knot>,
}

impl Warp {
    fn next(&self, index: usize) -> &Knot {
        &self.knots[(index + 1) % self.knots.len()]
    }

    /// Углы в долях круга между рукавом `index` и следующим: в модели и после поворота.
    fn gaps(&self, index: usize) -> (f64, f64) {
        let (first, next) = (&self.knots[index], self.next(index));
        let count = self.knots.len();
        (
            turn_between(count, first.from, next.from),
            turn_between(count, first.to, next.to),
        )
    }

    /// Углы от осей до краёв полос двух рукавов между `index` и следующим на расстоянии `radius` от середины.
    /// Полоса рукава — точки не дальше его полуширины от оси: до `asin(полуширина / radius)` от оси, а внутри
    /// расстояния от середины до края устья — тот же угол, что на этом расстоянии, чтобы граница полосы не бежала
    /// по расстоянию. Если две соседние полосы у устий не помещаются в угол между рукавами в модели и после
    /// поворота, обе сужаются в одной доле, общей для всех расстояний. `stretched` — устья после удлинения
    /// рукавов или в самой модели.
    fn strips(&self, index: usize, radius: f64, stretched: bool) -> (f64, f64) {
        if self.knots.len() == 1 {
            return (0.0, 0.0);
        }
        let (first, next) = (&self.knots[index], self.next(index));
        let mouth = |knot: &Knot| {
            if stretched {
                knot.mouth
            } else {
                knot.base_mouth
            }
        };
        let at = |knot: &Knot| libm::atan2(knot.half_width, mouth(knot));
        let (model, image) = self.gaps(index);
        let image = if stretched { image } else { model };
        let share = strip_share(model, image, at(first) + at(next));
        let angle = |knot: &Knot| {
            let edge = mouth(knot).hypot(knot.half_width);
            strip_angle(knot.half_width, radius.max(edge)) * share
        };
        (angle(first), angle(next))
    }

    /// Точка модели на карте до подгонки ширины. Точка в полосе рукава (не дальше его полуширины от оси)
    /// поворачивается на поворот рукава целиком и сдвигается вдоль рукава на его удлинение, линейно растущее от нуля
    /// в середине до полного в устье. Между полосами двух соседних рукавов поворот и сдвиг — смесь поворотов и
    /// сдвигов обоих, меняющаяся по углу точки плавно (smoothstep). Углы и полосы — в самой модели, до удлинения.
    fn carry(&self, point: [f64; 2]) -> [f64; 2] {
        let count = self.knots.len();
        let (radius, angle) = (
            point[0].hypot(point[1]),
            libm::atan2(point[1], point[0]).rem_euclid(TAU),
        );
        let at = self
            .knots
            .iter()
            .rposition(|knot| knot.from <= angle)
            .unwrap_or(count - 1);
        let first = &self.knots[at];
        let turn_by = |turn: f64| {
            let (sin, cos) = libm::sincos(turn);
            [
                point[0] * cos - point[1] * sin,
                point[0] * sin + point[1] * cos,
            ]
        };
        if count == 1 {
            return turn_by(first.turn);
        }
        let (model, image) = self.gaps(at);
        let (before_strip, after_strip) = self.strips(at, radius, false);
        let along = (angle - first.from).rem_euclid(TAU);
        let next_turn = first.turn + image - model;
        let share = if along <= before_strip {
            0.0
        } else if model - along <= after_strip {
            1.0
        } else {
            let x = (along - before_strip) / (model - before_strip - after_strip);
            x * x * (3.0 - 2.0 * x)
        };
        let next = self.next(at);
        let push = |knot: &Knot| {
            let (model_sin, model_cos) = libm::sincos(knot.from);
            let reached = (point[0] * model_cos + point[1] * model_sin) / knot.base_mouth;
            let shift = knot.extension * reached.clamp(0.0, 1.0);
            let (sin, cos) = libm::sincos(knot.to);
            [shift * cos, shift * sin]
        };
        let turned = turn_by(first.turn + (next_turn - first.turn) * share);
        let (a, b) = (push(first), push(next));
        [
            turned[0] + a[0] * (1.0 - share) + b[0] * share,
            turned[1] + a[1] * (1.0 - share) + b[1] * share,
        ]
    }

    /// Ширина рукава: угол точки от оси рукава сжимается так, что расстояние точки до оси рукава умножается на
    /// множитель, который плавно растёт от 1 в середине до `f` в устье, а ось дороги плавно сдвигается на её сдвиг.
    /// Точка остаётся на том же расстоянии от середины и идёт по окружности к оси рукава, а не поперёк него. Точка
    /// принадлежит тому из двух соседних рукавов, к чьей полосе она ближе: граница — середина свободного грунта
    /// между полосами. Сжатие полно в полосе рукава и до `FIT_SHARE` угла до границы, дальше оно плавно сходит на
    /// нет, поэтому соседние рукава не сходят друг на друга и не рвутся. Множитель не больше единицы.
    fn fit(&self, radius: f64, angle: f64) -> [f64; 2] {
        let count = self.knots.len();
        let sector = (0..count)
            .min_by(|&a, &b| {
                let passed = |index: usize| (angle - self.knots[index].to).rem_euclid(TAU);
                passed(a).total_cmp(&passed(b))
            })
            .unwrap_or(0);
        let (first_strip, next_strip) = self.strips(sector, radius, true);
        let image = self.gaps(sector).1;
        let border = (first_strip + image - next_strip) / 2.0;
        let passed = (angle - self.knots[sector].to).rem_euclid(TAU);
        let (arm, apart, half, edge) = if passed <= border {
            (&self.knots[sector], passed, border, first_strip)
        } else {
            (
                self.next(sector),
                passed - image,
                image - border,
                next_strip,
            )
        };
        let delta = apart.abs();
        let grow = smoothstep(radius * libm::cos(delta), arm.mouth);
        let factor = 1.0 + (arm.factor - 1.0) * grow;
        let squeezed = if delta < FRAC_PI_2 {
            libm::asin((factor * libm::sin(delta)).clamp(-1.0, 1.0))
        } else {
            delta
        };
        let keep = (FIT_SHARE * half).max(edge).min(half);
        let weight = if delta <= keep {
            1.0
        } else {
            1.0 - smoothstep(delta - keep, half - keep)
        };
        let moved = delta + weight * (squeezed - delta);
        let (sin, cos) = libm::sincos(arm.to + moved.copysign(apart));
        let [shift_x, shift_y] = self.shift(sector, radius, angle, (first_strip, next_strip));
        [radius * cos + shift_x, radius * sin + shift_y]
    }

    /// Сдвиг осей дорог точки между рукавом `sector` и следующим: сдвиг каждого рукава (ось дороги против оси рукава
    /// модели) полностью держится в его полосе, растёт от нуля в середине до полного в устье и сходит на нет по всему
    /// свободному грунту до полосы соседа, а не на узком участке сжатия: тогда сдвиг сходит на столько, на сколько
    /// грунта хватает, и сетка не сминается в складки, пока сдвиг не больше свободного грунта.
    fn shift(&self, sector: usize, radius: f64, angle: f64, strips: (f64, f64)) -> [f64; 2] {
        let (first, next) = (&self.knots[sector], self.next(sector));
        let image = self.gaps(sector).1;
        let passed = (angle - first.to).rem_euclid(TAU);
        let share = smoothstep(passed - strips.0, image - strips.0 - strips.1);
        let push = |knot: &Knot, weight: f64| {
            if knot.offset == 0.0 || weight == 0.0 {
                return [0.0, 0.0];
            }
            let delta = wrap_pi(angle - knot.to);
            let reach = smoothstep(radius * libm::cos(delta), knot.mouth);
            let (sin, cos) = libm::sincos(knot.to);
            let length = knot.offset * reach * weight;
            [-length * sin, length * cos]
        };
        let (a, b) = (push(first, 1.0 - share), push(next, share));
        [a[0] + b[0], a[1] + b[1]]
    }

    fn plane(&self, x: f32, y: f32) -> [f64; 2] {
        let flat_y = if self.mirrored {
            f64::from(y)
        } else {
            -f64::from(y)
        };
        [f64::from(x), flat_y]
    }

    fn vertex(&self, vertex: &ModelVertex) -> ModelVertex {
        let at = self.plane(
            vertex.position[0] - self.origin[0],
            vertex.position[1] - self.origin[1],
        );
        let at = [at[0] * self.scale, at[1] * self.scale];
        let angle = libm::atan2(at[1], at[0]).rem_euclid(TAU);
        let moved = self.carry(at);
        let (radius, bent) = (
            moved[0].hypot(moved[1]),
            libm::atan2(moved[1], moved[0]).rem_euclid(TAU),
        );
        let placed = self.fit(radius, bent);
        let turn = if radius > 0.0 {
            libm::atan2(placed[1], placed[0]) - angle
        } else {
            bent - angle
        };
        let (turn_sin, turn_cos) = libm::sincos(turn);
        let rotate = |x: f32, y: f32| {
            let v = self.plane(x, y);
            let turned = [
                v[0] * turn_cos - v[1] * turn_sin,
                v[0] * turn_sin + v[1] * turn_cos,
            ];
            [turned[0] as f32, -turned[1] as f32]
        };
        let normal = rotate(vertex.normal[0], vertex.normal[1]);
        let tangent = rotate(vertex.tangent[0], vertex.tangent[1]);
        ModelVertex {
            position: [
                placed[0] as f32,
                -placed[1] as f32,
                (f64::from(vertex.position[2]) * self.scale) as f32,
            ],
            normal: [normal[0], normal[1], vertex.normal[2]],
            uv: vertex.uv,
            tangent: [
                tangent[0],
                tangent[1],
                vertex.tangent[2],
                if self.mirrored {
                    -vertex.tangent[3]
                } else {
                    vertex.tangent[3]
                },
            ],
            color: vertex.color,
        }
    }

    fn part(&self, part: &Part) -> Part {
        let mut indices = part.indices.clone();
        if self.mirrored {
            for triangle in indices.as_chunks_mut::<3>().0 {
                triangle.swap(1, 2);
            }
        }
        Part {
            vertices: part.vertices.iter().map(|v| self.vertex(v)).collect(),
            indices,
            material: part.material,
        }
    }
}

/// Удлинение рукавов (требование 10а): для каждой пары соседних рукавов — наименьшее общее удлинение, при котором
/// полосы обоих у устьев помещаются в угол между ними (в модели и после поворота) с запасом свободного грунта
/// `MIN_FREE_GAP`; рукав берёт большее из двух своих.
fn stretch_arms(knots: &mut [Knot]) {
    let count = knots.len();
    if count == 1 {
        return;
    }
    let mut extensions = vec![0.0_f64; count];
    for index in 0..count {
        let next = (index + 1) % count;
        let (first, second) = (knots[index], knots[next]);
        let bound = strips_bound(
            turn_between(count, first.from, second.from),
            turn_between(count, first.to, second.to),
        );
        let need = |extension: f64| {
            libm::atan2(first.half_width, first.base_mouth + extension)
                + libm::atan2(second.half_width, second.base_mouth + extension)
        };
        if bound <= 0.0 || need(0.0) <= bound {
            continue;
        }
        let (mut low, mut high) = (0.0, (first.half_width + second.half_width) / bound);
        for _ in 0..60 {
            let middle = (low + high) / 2.0;
            if need(middle) <= bound {
                high = middle;
            } else {
                low = middle;
            }
        }
        extensions[index] = extensions[index].max(high);
        extensions[next] = extensions[next].max(high);
    }
    for (knot, extension) in knots.iter_mut().zip(extensions) {
        knot.extension = extension;
        knot.mouth = knot.base_mouth + extension;
    }
}

/// Изогнутая копия частей выбранной модели вокруг середины в начале координат. `None`, если у модели нет файла.
///
/// Масштаб — наибольшее из отношений ширины дороги к ширине сопоставленного с ней рукава модели. У самой широкой
/// дороги оно наибольшее, когда рукава модели подходят дорогам по ширине вида; иначе множитель ширины остальных
/// рукавов не выше единицы: расширение рукава выворачивало бы треугольники в углах между рукавами.
pub fn place(
    kinds: &RoadKindTable,
    models: &ModelTable,
    fit: &Fit,
    arms: &[SpotArm],
) -> Option<Placed> {
    let candidate = &kinds.junctions()[fit.model];
    let model = models.get(candidate.model)?;
    let scale = arms
        .iter()
        .zip(&fit.matched)
        .map(|(arm, &index)| arm.width / candidate.arms[index].width)
        .fold(0.0, f64::max);
    let mut knots: Vec<Knot> = Vec::with_capacity(arms.len());
    for (number, (arm, &index)) in arms.iter().zip(&fit.matched).enumerate() {
        let model_arm = &candidate.arms[index];
        let mouth = (model_arm.reach - model_arm.fade) * scale;
        knots.push(Knot {
            arm: number,
            from: model_angle(model_arm, fit.mirrored),
            to: arm.angle,
            turn: 0.0,
            half_width: model_arm.width * scale / 2.0 * STRIP_MARGIN,
            factor: arm.width / (model_arm.width * scale),
            base_mouth: mouth,
            extension: 0.0,
            mouth,
            offset: arm.offset,
        });
    }
    knots.sort_by(|a, b| a.from.total_cmp(&b.from));
    let count = knots.len();
    knots[0].turn = wrap_pi(knots[0].to - knots[0].from);
    for index in 0..count - 1 {
        let (model, image) = (
            turn_between(count, knots[index].from, knots[index + 1].from),
            turn_between(count, knots[index].to, knots[index + 1].to),
        );
        knots[index + 1].turn = knots[index].turn + image - model;
    }
    stretch_arms(&mut knots);
    let mut mouths = vec![0.0; count];
    let mut extensions = vec![0.0; count];
    for knot in &knots {
        mouths[knot.arm] = knot.mouth;
        extensions[knot.arm] = knot.extension;
    }
    let warp = Warp {
        origin: model.origin,
        mirrored: fit.mirrored,
        scale,
        knots,
    };
    Some(Placed {
        scale,
        mouths,
        extensions,
        parts: model.parts.iter().map(|part| warp.part(part)).collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::fixtures::{self, MAIN, STREET};
    use super::*;
    use crate::core::model::Model;

    fn near(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-5
    }

    fn width_of(kind: usize) -> f64 {
        if kind == MAIN { 1.5 } else { 1.0 }
    }

    /// Рукав перекрёстка на карте: угол в градусах, вид; ширина дороги вдвое меньше ширины куска, так что масштаб
    /// модели 0,5, а множитель ширины — единица.
    fn arm(degrees: f64, kind: usize) -> SpotArm {
        SpotArm {
            angle: degrees.to_radians().rem_euclid(TAU),
            kind,
            width: width_of(kind),
            offset: 0.0,
        }
    }

    fn sorted(mut arms: Vec<SpotArm>) -> Vec<SpotArm> {
        arms.sort_by(|a, b| a.angle.total_cmp(&b.angle));
        arms
    }

    fn name_of(kinds: &RoadKindTable, models: &ModelTable, fit: &Fit) -> String {
        models
            .get(kinds.junctions()[fit.model].model)
            .map(|model| model.name.clone())
            .unwrap_or_default()
    }

    fn pick(arms: Vec<SpotArm>) -> Option<(String, Fit)> {
        let (kinds, models) = fixtures::tables();
        let fit = select(&kinds, &models, &sorted(arms))?;
        Some((name_of(&kinds, &models, &fit), fit))
    }

    #[test]
    fn a_model_is_picked_by_the_number_of_arms() {
        let end = pick(vec![arm(40.0, MAIN)]).expect("тупик");
        assert_eq!(end.0, "end_main");
        let cross = pick(vec![
            arm(10.0, STREET),
            arm(100.0, STREET),
            arm(190.0, STREET),
            arm(280.0, STREET),
        ])
        .expect("крест");
        assert_eq!(cross.0, "cross_street_street");
        assert!(!cross.1.mirrored);
        let tee = pick(vec![
            arm(0.0, STREET),
            arm(90.0, STREET),
            arm(180.0, STREET),
        ]);
        assert_eq!(tee.expect("тройник").0, "tee_street_street");
        let line = pick(vec![arm(0.0, MAIN), arm(180.0, STREET)]).expect("переход");
        assert_eq!(line.0, "line_main_street");
    }

    #[test]
    fn five_arms_find_no_model() {
        let arms = (0..5).map(|k| arm(72.0 * f64::from(k), STREET)).collect();
        assert!(pick(arms).is_none());
    }

    #[test]
    fn a_model_is_turned_to_any_angle_without_a_mirror_when_the_kinds_allow_it() {
        // Тройник главной улицы с улицей: улица на другой стороне — поворот на 180°, без отражения.
        let (name, fit) =
            pick(vec![arm(0.0, MAIN), arm(180.0, MAIN), arm(270.0, STREET)]).expect("тройник");
        assert_eq!(name, "tee_main_street");
        assert!(!fit.mirrored);
        // Сопоставление идёт по кругу: рукав 0° — рукав модели 180°, 180° — 0°, 270° — 90°.
        assert_eq!(fit.matched, [2, 0, 1]);
    }

    #[test]
    fn a_bend_from_main_into_street_clockwise_needs_the_mirror() {
        // Пример 15: рукав главной 0°, улицы — 270°, то есть по часовой от главной; модель (0° главная, 90° улица)
        // подходит только отражённой.
        let (name, fit) = pick(vec![arm(0.0, MAIN), arm(270.0, STREET)]).expect("поворот");
        assert_eq!(name, "bend_main_street");
        assert!(fit.mirrored, "{fit:?}");
        let (_, counter) = pick(vec![arm(0.0, MAIN), arm(90.0, STREET)]).expect("поворот");
        assert!(!counter.mirrored, "против часовой отражение не нужно");
    }

    #[test]
    fn a_kind_that_does_not_match_costs_less_with_a_wider_model_arm() {
        // Пример 16: крест, главная насквозь (0° и 180°), улица (90°) и главная (270°) вместо улицы: берётся
        // `cross_main_main` — у неё несовпал рукав улицы, а модель шире; `cross_main_street` несовпала бы на 270°
        // более узкой.
        let (name, _) = pick(vec![
            arm(0.0, MAIN),
            arm(90.0, STREET),
            arm(180.0, MAIN),
            arm(270.0, MAIN),
        ])
        .expect("крест");
        assert_eq!(name, "cross_main_main");
        // И наоборот: вместо главной улица — узкая, и самая дешёвая модель с совпавшими видами снова главная, где
        // не совпал один рукав.
        let (name, _) = pick(vec![
            arm(0.0, MAIN),
            arm(90.0, STREET),
            arm(180.0, STREET),
            arm(270.0, STREET),
        ])
        .expect("крест");
        assert_eq!(name, "cross_main_street");
    }

    #[test]
    fn a_model_of_other_kinds_without_a_bend_beats_its_own_kinds_bent_more_than_45_degrees() {
        // Главная поворачивает (0° и 270°), прямо продолжает улица (180°): тройник «главная насквозь, улица вбок»
        // совпал бы видами, но его главную пришлось бы согнуть на 90°, и угол модели вылез бы горбом за дорогу.
        // Тройник двух главных садится без изгиба — несовпал только рукав улицы, и модель шире.
        let (name, fit) =
            pick(vec![arm(0.0, MAIN), arm(180.0, STREET), arm(270.0, MAIN)]).expect("тройник");
        assert_eq!(name, "tee_main_main");
        assert!(!fit.mirrored);
        // То же, но рукава главной разошлись на 120° (главная на 240°): своей модели нужен изгиб 90° по сумме, чужой —
        // 30° и несовпавший вид (30 + 45 − 1 = 74); цена несовпадения больше 60 отдала бы стык согнутой своей.
        let (name, _) =
            pick(vec![arm(0.0, MAIN), arm(180.0, STREET), arm(240.0, MAIN)]).expect("тройник");
        assert_eq!(name, "tee_main_main");
        // Две главные разной ширины почти по прямой — переход главной в главную без изгиба, а не согнутый поворот
        // и не переход в более узкий вид, который пришлось бы раздувать.
        let (name, _) = pick(vec![arm(0.0, MAIN), arm(175.0, MAIN)]).expect("переход");
        assert_eq!(name, "line_main_main");
        // Т-стык улицы с главной под 60° — своя модель согнута на 30°, это дешевле несовпавшего вида.
        let (name, _) =
            pick(vec![arm(0.0, MAIN), arm(60.0, STREET), arm(180.0, MAIN)]).expect("тройник");
        assert_eq!(name, "tee_main_street");
    }

    #[test]
    fn the_cost_is_the_angle_error_after_the_best_turn_so_the_gentlest_bend_wins() {
        // Рукава на 0° и 330°: модель (0°, 90°) садится так, чтобы 90° стал 0°, а 0° — 330°: сумма ошибок 60° против
        // 120° у другого сопоставления.
        let (_, fit) = pick(vec![arm(0.0, STREET), arm(330.0, STREET)]).expect("поворот");
        assert_eq!(fit.matched, [1, 0]);
        assert!(!fit.mirrored);
    }

    #[test]
    fn the_first_model_in_the_table_wins_an_equal_price_and_a_mirror_costs_one_degree() {
        let (kinds, models) =
            fixtures::tables_with(&["cross_street_street", "cross_street_street"]);
        let arms = sorted(vec![
            arm(0.0, STREET),
            arm(90.0, STREET),
            arm(180.0, STREET),
            arm(270.0, STREET),
        ]);
        assert_eq!(select(&kinds, &models, &arms).map(|fit| fit.model), Some(0));
        // Зеркало стоит 1: поворот улиц под 95° садится без зеркала, хотя с ним ошибка по углу та же.
        let (name, fit) = pick(vec![arm(0.0, STREET), arm(95.0, STREET)]).expect("поворот");
        assert_eq!(name, "bend_street_street");
        assert!(!fit.mirrored);
    }

    #[test]
    fn a_model_without_parts_is_never_picked() {
        let (kinds, models) = fixtures::tables();
        let hollow: Vec<Model> = models
            .iter()
            .map(|model| {
                if model.name == "end_main" {
                    Model::placeholder("end_main")
                } else {
                    model.clone()
                }
            })
            .collect();
        let models = ModelTable::new(hollow);
        // Тупик главной улицы без частей не выбирается: берётся тупик улицы с несовпавшим видом.
        let fit = select(&kinds, &models, &[arm(0.0, MAIN)]).expect("тупик");
        assert_eq!(name_of(&kinds, &models, &fit), "end_street");
    }

    /// Точка модели в полярных координатах «против часовой, если смотреть сверху»: градусы и расстояние.
    fn polar(degrees: f64, radius: f64) -> ModelVertex {
        let (sin, cos) = degrees.to_radians().sin_cos();
        ModelVertex {
            position: [(radius * cos) as f32, (-radius * sin) as f32, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [0.25, 0.75],
            tangent: [1.0, 0.0, 0.0, -1.0],
            color: [1.0, 0.5, 0.25, 0.75],
        }
    }

    /// Угол и расстояние точки изогнутой копии, как в `polar`.
    fn unpolar(vertex: &ModelVertex) -> (f64, f64) {
        let (x, y) = (
            f64::from(vertex.position[0]),
            -f64::from(vertex.position[1]),
        );
        (libm::atan2(y, x).to_degrees().rem_euclid(360.0), x.hypot(y))
    }

    /// Модель `name` из `fixtures`, чью сетку заменяют `points` (три на треугольник), поставленная на рукава `arms`.
    fn placed(name: &str, arms: Vec<SpotArm>, points: Vec<ModelVertex>) -> Placed {
        let (kinds, models) = fixtures::tables();
        let at = fixtures::junction_specs()
            .iter()
            .position(|(known, _)| *known == name)
            .expect("такая модель есть");
        let index = kinds.junctions()[at].model;
        let mut all: Vec<Model> = models.iter().cloned().collect();
        all[index].origin = [0.0, 0.0];
        all[index].parts = vec![Part {
            indices: (0..points.len() as u32 / 3 * 3).collect(),
            vertices: points,
            material: 0,
        }];
        let models = ModelTable::new(all);
        let arms = sorted(arms);
        let fit = select(&kinds, &models, &arms).expect("модель выбрана");
        assert_eq!(name_of(&kinds, &models, &fit), name);
        place(&kinds, &models, &fit, &arms).expect("копия построена")
    }

    #[test]
    fn a_tee_under_75_degrees_turns_each_arm_whole_bends_only_the_ground_between_and_keeps_the_distance()
     {
        // Пример 14: главная насквозь, улица кончается в точке под 75°. Рукава главной на месте, рукав улицы — на −15°
        // целиком, точки в углах между ними — на углы между 0 и −15°.
        let near_axes = [0.0, 3.0, 90.0, 93.0, 87.0, 180.0, 177.0, 270.0];
        let wanted = [0.0, 3.0, 75.0, 78.0, 72.0, 180.0, 177.0, 270.0];
        let between = [45.0, 135.0, 40.0, 130.0];
        let points: Vec<ModelVertex> = near_axes
            .iter()
            .chain(&between)
            .map(|&d| polar(d, 12.0))
            .collect();
        let copy = placed(
            "tee_main_street",
            vec![arm(0.0, MAIN), arm(75.0, STREET), arm(180.0, MAIN)],
            points,
        );
        assert!(near(copy.scale, 0.5));
        let images: Vec<(f64, f64)> = copy.parts[0].vertices.iter().map(unpolar).collect();
        for ((angle, radius), want) in images.iter().zip(wanted) {
            assert!((angle - want).abs() < 1e-4, "{angle} вместо {want}");
            assert!(near(*radius, 6.0), "расстояние до середины то же: {radius}");
        }
        // Между полосами поворот лежит между поворотом главной (0) и поворотом улицы (−15°).
        for ((angle, radius), (low, high)) in
            images[8..]
                .iter()
                .zip([(30.0, 45.0), (120.0, 135.0), (25.0, 40.0), (115.0, 130.0)])
        {
            assert!(
                *angle > low && *angle < high,
                "{angle}° не между {low}° и {high}°"
            );
            assert!(near(*radius, 6.0));
        }
    }

    #[test]
    fn normals_and_tangents_turn_with_their_point_and_the_rest_of_the_vertex_stays() {
        let mut point = polar(45.0, 6.0);
        point.normal = [0.6, 0.0, 0.8];
        let copy = placed(
            "tee_main_street",
            vec![arm(0.0, MAIN), arm(75.0, STREET), arm(180.0, MAIN)],
            vec![point, polar(0.0, 1.0), polar(180.0, 1.0)],
        );
        let vertex = copy.parts[0].vertices[0];
        // Угол точки сменился: нормаль и касательная повернуты на столько же.
        let (image, _) = unpolar(&vertex);
        assert!(image < 45.0 - 1e-3, "{image}");
        let (sin, cos) = (image - 45.0).to_radians().sin_cos();
        let planar = |x: f32, y: f32| (f64::from(x), -f64::from(y));
        let (nx, ny) = planar(vertex.normal[0], vertex.normal[1]);
        assert!(
            near(nx, 0.6 * cos) && near(ny, 0.6 * sin),
            "{:?}",
            vertex.normal
        );
        assert!(near(f64::from(vertex.normal[2]), 0.8));
        let (tx, ty) = planar(vertex.tangent[0], vertex.tangent[1]);
        assert!(near(tx, cos) && near(ty, sin), "{:?}", vertex.tangent);
        assert_eq!(
            vertex.tangent[3], -1.0,
            "знак касательной без отражения тот же"
        );
        assert_eq!(
            (vertex.uv, vertex.color),
            ([0.25, 0.75], [1.0, 0.5, 0.25, 0.75])
        );
    }

    #[test]
    fn a_narrower_arm_gets_its_width_multiplier_growing_smoothly_to_the_factor_at_the_mouth() {
        // Крест: улица в 90° шириной 0,5 вместо 1 при масштабе 0,5 — множитель 0,5. Устье — на (4 − 2) × 0,5 = 1.
        let narrow = SpotArm {
            width: 0.5,
            ..arm(90.0, STREET)
        };
        let across = |along: f64| {
            // Точка на 0,4 в сторону от оси рукава 90° на расстоянии `along` вдоль неё.
            let mut point = polar(90.0, along);
            point.position[0] += 0.4;
            point
        };
        let points = [1.0, 2.0, 3.0, 4.0, 6.0].map(across).to_vec();
        let copy = placed(
            "cross_main_street",
            vec![arm(0.0, MAIN), narrow, arm(180.0, MAIN), arm(270.0, STREET)],
            points,
        );
        let lateral: Vec<f64> = copy.parts[0]
            .vertices
            .iter()
            .map(|v| f64::from(v.position[0]).abs())
            .collect();
        // Вдоль рукава после масштаба 0,5 — 0,5, 1, 1,5, 2, 3; поперёк — l × s × множитель.
        let multiplier = |along: f64| {
            let t = (along / 1.5).clamp(0.0, 1.0);
            0.4 * 0.5 * (1.0 + (0.5 - 1.0) * t * t * (3.0 - 2.0 * t))
        };
        for (got, along) in lateral.iter().zip([0.5, 1.0, 1.5, 2.0, 3.0]) {
            assert!(
                near(*got, multiplier(along)),
                "вдоль {along}: {got} вместо {}",
                multiplier(along)
            );
        }
        assert!(
            near(lateral[2], 0.1),
            "в устье (1,5) поперечный размер вдвое меньше"
        );
        assert!(near(lateral[4], 0.1), "дальше устья множитель тот же");
        assert!(lateral[0] > lateral[1], "до устья растёт плавно");
    }

    #[test]
    fn the_axis_of_a_shifted_road_is_reached_at_the_mouth_by_a_smooth_sideways_shift() {
        let shifted = SpotArm {
            offset: 0.3,
            ..arm(90.0, STREET)
        };
        let copy = placed(
            "cross_main_street",
            vec![
                arm(0.0, MAIN),
                shifted,
                arm(180.0, MAIN),
                arm(270.0, STREET),
            ],
            [0.0, 1.0, 2.0, 4.0, 8.0]
                .map(|along| polar(90.0, along))
                .to_vec(),
        );
        // Рукав 90° смотрит вверх по карте; влево от него — к −x. Устье — на 1,5: ось дороги смещена на 0,3.
        let x: Vec<f64> = copy.parts[0]
            .vertices
            .iter()
            .map(|v| f64::from(v.position[0]))
            .collect();
        let grown = |along: f64| {
            let t: f64 = along / 1.5;
            -0.3 * t * t * (3.0 - 2.0 * t)
        };
        assert!(
            near(x[0], 0.0)
                && near(x[1], grown(0.5))
                && near(x[2], grown(1.0))
                && near(x[3], -0.3)
                && near(x[4], -0.3),
            "{x:?}"
        );
    }

    #[test]
    fn a_mirrored_copy_mirrors_the_points_swaps_the_triangle_order_and_the_tangent_sign() {
        // Пример 15: улица по часовой от главной — модель отражена: рукав 90° улицы идёт на 270°.
        let copy = placed(
            "bend_main_street",
            vec![arm(0.0, MAIN), arm(270.0, STREET)],
            vec![polar(90.0, 3.0), polar(0.0, 3.0), polar(45.0, 3.0)],
        );
        let part = &copy.parts[0];
        let (angle, radius) = unpolar(&part.vertices[0]);
        assert!(
            (angle - 270.0).abs() < 1e-4 && near(radius, 1.5),
            "{angle} {radius}"
        );
        assert_eq!(part.indices, [0, 2, 1]);
        assert_eq!(part.vertices[0].tangent[3], 1.0);
        let (angle, _) = unpolar(&part.vertices[2]);
        assert!(
            (angle - 315.0).abs() < 1e-4,
            "45° отражается в 315°: {angle}"
        );
    }

    #[test]
    fn the_model_is_laid_by_the_file_origin_and_not_by_the_middle_of_its_box() {
        let (kinds, models) = fixtures::tables();
        let at = fixtures::junction_specs()
            .iter()
            .position(|(known, _)| *known == "tee_main_street")
            .expect("такая модель есть");
        let tee = models
            .get(kinds.junctions()[at].model)
            .expect("модель есть");
        // Тройник несимметричен: загрузка ставит в начало середину коробки, а середина файла там, где сошлись рукава.
        assert!(tee.origin[1].abs() > 0.1, "{:?}", tee.origin);
        let arms = sorted(vec![arm(0.0, MAIN), arm(90.0, STREET), arm(180.0, MAIN)]);
        let fit = select(&kinds, &models, &arms).expect("модель выбрана");
        let copy = place(&kinds, &models, &fit, &arms).expect("копия построена");
        // Рукав улицы кончается на 5 × 0,5 выше середины, а назад сетка идёт на полуширину главной: 1,5 × 0,5.
        let ys = copy.parts[0]
            .vertices
            .iter()
            .map(|v| f64::from(v.position[1]));
        let (top, bottom) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), y| {
            (low.min(y), high.max(y))
        });
        assert!(near(top, -2.5) && near(bottom, 0.75), "{top} {bottom}");
    }

    /// Углы соседних рукавов ближе 30° друг к другу: у такого перекрёстка модели нет.
    fn crowded(arms: &[SpotArm]) -> bool {
        arms.iter().enumerate().any(|(index, arm)| {
            let next = arms[(index + 1) % arms.len()];
            (next.angle - arm.angle).rem_euclid(TAU) < 30.0_f64.to_radians() - 1e-9
        })
    }

    /// Сколько треугольников копии на рукавах `arms` лежат не так, как у модели.
    fn folded(arms: &[SpotArm]) -> usize {
        let (kinds, models) = fixtures::tables();
        let fit = select(&kinds, &models, arms).expect("модель выбрана");
        let copy = place(&kinds, &models, &fit, arms).expect("копия построена");
        let model = models
            .get(kinds.junctions()[fit.model].model)
            .expect("модель");
        let area = |part: &Part, t: &[u32]| {
            let [a, b, c] = [0, 1, 2].map(|k| part.vertices[t[k] as usize].position);
            f64::from((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
        };
        copy.parts
            .iter()
            .zip(&model.parts)
            .flat_map(|(new, old)| {
                new.indices
                    .chunks(3)
                    .zip(old.indices.chunks(3))
                    .map(move |(n, o)| area(new, n) * area(old, o))
            })
            .filter(|product| *product <= 0.0)
            .count()
    }

    /// Сдвиг оси дороги на 0,1 и на 0,3 (десятая и треть ширины улицы) в обе стороны у любого рукава тройника, креста и
    /// поворота под углами от 30° до 170° не выворачивает ни одного треугольника: сдвиг сходит на нет по всему свободному
    /// грунту до соседнего рукава.
    #[test]
    fn a_road_axis_shifted_by_a_tenth_or_a_third_of_a_street_turns_no_triangle_over() {
        let mut checked = 0;
        for kinds in [[STREET, STREET], [MAIN, STREET]] {
            for shape in 0..3 {
                for degrees in [30.0, 45.0, 60.0, 90.0, 120.0, 135.0, 150.0, 170.0] {
                    for sign in [1.0, -1.0] {
                        let d = sign * degrees;
                        let arms = sorted(match shape {
                            0 => vec![arm(0.0, kinds[0]), arm(180.0, kinds[0]), arm(d, kinds[1])],
                            1 => vec![
                                arm(0.0, kinds[0]),
                                arm(180.0, kinds[0]),
                                arm(d, kinds[1]),
                                arm(d + 180.0, kinds[1]),
                            ],
                            _ => vec![arm(180.0, kinds[0]), arm(d, kinds[1])],
                        });
                        if crowded(&arms) {
                            continue;
                        }
                        for shifted in 0..arms.len() {
                            for offset in [0.1, -0.1, 0.3, -0.3] {
                                let mut moved = arms.clone();
                                moved[shifted].offset = offset;
                                assert_eq!(
                                    folded(&moved),
                                    0,
                                    "{:?}: рукав {shifted} сдвинут на {offset}",
                                    moved
                                        .iter()
                                        .map(|a| a.angle.to_degrees())
                                        .collect::<Vec<_>>()
                                );
                                checked += 1;
                            }
                        }
                    }
                }
            }
        }
        assert!(checked > 800, "{checked}");
    }
}

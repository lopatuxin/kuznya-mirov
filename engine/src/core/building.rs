//! «Трёхмерная сцена» → «Здания»: твёрдая модель в мире. Ход игры видит её сетку столбов (`core::solid`) в месте
//! объекта: части модели не пускают объект с телом между его основанием плюс ступенька и его верхом, верхи, годные как
//! пол, достраивают поверхности, на которые встаёт объект (`core::surface`), а коробки `inside` решают, виден ли
//! срез. Объект, стоящий внутри части, выходит наружу по кратчайшему пути при первом же сдвиге.

use super::camera;
use super::footprint::Footprint;
use super::math3::Vec3;
use super::model::InsideBox;
use super::property;
use super::runner::STEP_SECONDS;
use super::solid::{LocalRect, SolidGrid, Span};
use super::surface::{self, Kind, STEP, TOLERANCE};
use super::value::{Rotation, Vec2};
use super::world::World;

/// Шагов растворения среза: 0,3 секунды.
pub const DISSOLVE_STEPS: u8 = (0.3 / STEP_SECONDS + 0.5) as u8;

/// Дальше этого от места, где стоит объект внутри части, выход не ищется.
const ESCAPE_REACH: f64 = 6.0;

/// Шаг поиска выхода по радиусу и число направлений.
const ESCAPE_STEP: f64 = 0.05;
const ESCAPE_DIRECTIONS: usize = 16;

/// Шаг проверки вдоль сдвига, в клетках: быстрый сдвиг не проскакивает тонкую стену.
const SWEEP_STEP: f64 = 0.1;

/// Сколько раз делится пополам отрезок, на котором ищется самое дальнее свободное место.
const APPROACH_BISECTIONS: u32 = 8;

/// Перевод между осями сцены и осями модели объекта: середина места, поворот вида (`rotation` с `facing`, как
/// рисует модель), увеличение и основание.
pub(super) struct Frame {
    center: Vec2,
    sin: f64,
    cos: f64,
    scale: f64,
    base: f64,
}

impl Frame {
    pub(super) fn of(world: &World, id: u32) -> Option<Frame> {
        let model = world.model_of(id)?;
        let position = world.vec2(id, property::POSITION)?;
        let size = world.vec2(id, property::SIZE)?;
        let (sin, cos) = view_rotation(world, id).sin_cos();
        Some(Frame {
            center: [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0],
            sin,
            cos,
            scale: model.scale(world.number_like(id, property::HEIGHT)),
            base: world.base_z(id),
        })
    }

    /// Точка осей модели в осях сцены.
    pub(super) fn scene_point(&self, local: [f64; 3]) -> [f64; 3] {
        let (x, y) = (local[0] * self.scale, local[1] * self.scale);
        [
            self.center[0] + self.cos * x - self.sin * y,
            self.center[1] + self.sin * x + self.cos * y,
            self.base + local[2] * self.scale,
        ]
    }

    fn local_point(&self, point: Vec2) -> [f64; 2] {
        let (dx, dy) = (point[0] - self.center[0], point[1] - self.center[1]);
        [
            (self.cos * dx + self.sin * dy) / self.scale,
            (-self.sin * dx + self.cos * dy) / self.scale,
        ]
    }

    fn local_height(&self, z: f64) -> f64 {
        (z - self.base) / self.scale
    }

    fn local_rect(&self, place: &Footprint) -> LocalRect {
        let (center, half, sin, cos) = match place {
            Footprint::Aligned(rect) => (
                [rect.x + rect.w / 2.0, rect.y + rect.h / 2.0],
                [rect.w / 2.0, rect.h / 2.0],
                0.0,
                1.0,
            ),
            Footprint::Oriented(turned) => (turned.center, turned.half, turned.sin, turned.cos),
        };
        LocalRect {
            center: self.local_point(center),
            half: half.map(|side| side / self.scale),
            cos: cos * self.cos + sin * self.sin,
            sin: sin * self.cos - cos * self.sin,
        }
    }
}

/// Куда смотрит вид объекта: `rotation` вместе с `facing`.
pub fn view_rotation(world: &World, id: u32) -> Rotation {
    let angle = |prop| world.rotation(id, prop).map_or(0.0, Rotation::angle);
    let sum = angle(property::ROTATION) + angle(property::FACING);
    Rotation::from_degrees(sum).unwrap_or_else(|| Rotation::from_quarters(0))
}

/// Твёрдый объект мира: его сетка, перевод осей, высота верха модели и годятся ли её верхи как пол.
struct Placed<'a> {
    grid: &'a SolidGrid,
    frame: Frame,
    top: f64,
    floors: bool,
}

impl Placed<'_> {
    /// Ступенька над основанием тела, ниже которой части модели не мешают: на модель без пола не поднимаются.
    fn step(&self) -> f64 {
        if self.floors { STEP } else { 0.0 }
    }

    /// Задевает ли тело с местом `place` части здания между высотами `from` включительно и `to`.
    fn blocks(&self, place: &Footprint, from: f64, to: f64) -> bool {
        let (low, high) = (self.frame.local_height(from), self.frame.local_height(to));
        self.grid.any_under(&self.frame.local_rect(place), |spans| {
            spans
                .iter()
                .any(|span| f64::from(span.low) < high && f64::from(span.high) > low)
        })
    }

    /// Самый высокий пол под местом `place`, не выше `limit`.
    fn floor_under(&self, place: &Footprint, limit: Option<f64>) -> Option<f64> {
        if !self.floors {
            return None;
        }
        let mut best = None;
        self.grid.any_under(&self.frame.local_rect(place), |spans| {
            best = self.best_floor(spans, best, limit);
            false
        });
        best
    }

    /// Расстояние вдоль луча, идущего вниз, до первого пола модели в пределах `limit`: луч идёт по плану шагами в
    /// полстолбика, и на шаге ищется пол, чью высоту он пересёк над столбиком с этим полом.
    fn floor_hit(&self, origin: Vec3, direction: Vec3, limit: f64) -> Option<f64> {
        if !self.floors {
            return None;
        }
        let from = ((self.top - origin[2]) / direction[2]).max(0.0);
        let to = ((self.frame.base - origin[2]) / direction[2]).min(limit);
        let horizontal = direction[0].hypot(direction[1]);
        let step = (f64::from(self.grid.cell()) * self.frame.scale / 2.0 / horizontal)
            .min(to - from)
            .max(f64::EPSILON);
        let at = |t: f64| [origin[0] + t * direction[0], origin[1] + t * direction[1]];
        let tops = |t: f64| {
            let spans = self.grid.spans_at(self.frame.local_point(at(t)));
            spans.iter().filter_map(|span| self.top_of(span))
        };
        let mut start = from;
        while start < to {
            let end = (start + step).min(to);
            let hit = tops(start)
                .chain(tops(end))
                .map(|top| (top - origin[2]) / direction[2])
                .filter(|&t| t >= start && t <= end)
                .filter(|&t| tops(t).any(|top| (origin[2] + t * direction[2] - top).abs() < 1e-6))
                .reduce(f64::min);
            if hit.is_some() {
                return hit;
            }
            start = end;
        }
        None
    }

    /// Самый высокий пол под точкой `point`, не выше `limit`.
    fn floor_at(&self, point: Vec2, limit: Option<f64>) -> Option<f64> {
        if !self.floors {
            return None;
        }
        let spans = self.grid.spans_at(self.frame.local_point(point));
        self.best_floor(spans, None, limit)
    }

    /// Высота пола на верху отрезка в осях сцены.
    fn top_of(&self, span: &Span) -> Option<f64> {
        span.floor
            .map(|floor| self.frame.base + f64::from(floor) * self.frame.scale)
    }

    fn best_floor(&self, spans: &[Span], best: Option<f64>, limit: Option<f64>) -> Option<f64> {
        spans
            .iter()
            .filter_map(|span| self.top_of(span))
            .filter(|&top| limit.is_none_or(|limit| top <= limit + TOLERANCE))
            .fold(best, |best, top| {
                Some(best.map_or(top, |b: f64| b.max(top)))
            })
    }
}

/// Твёрдые объекты мира, кроме `skip`; без твёрдых моделей в игре мир не просматривается.
fn solids(world: &World, skip: Option<u32>) -> Vec<Placed<'_>> {
    if !world.three_d() || !world.models().any_solid() {
        return Vec::new();
    }
    world
        .ids()
        .filter(|&id| Some(id) != skip)
        .filter_map(|id| {
            let model = world.model_of(id)?;
            let grid = model.building.solid.as_deref()?;
            let frame = Frame::of(world, id)?;
            let top = frame.base + model.size[2] * frame.scale;
            Some(Placed {
                grid,
                frame,
                top,
                floors: !model.building.obstacle_only,
            })
        })
        .collect()
}

/// «Мышь»: расстояние вдоль луча, идущего вниз, до пола твёрдой модели — верха, годного как пол, — не дальше `limit`.
pub fn floor_hit(world: &World, origin: Vec3, direction: Vec3, limit: f64) -> Option<f64> {
    if direction[2] >= 0.0 {
        return None;
    }
    solids(world, None)
        .iter()
        .filter_map(|solid| solid.floor_hit(origin, direction, limit))
        .reduce(f64::min)
}

/// Твёрдая ли модель у объекта `id`.
pub fn is_solid(world: &World, id: u32) -> bool {
    world.models().any_solid()
        && world
            .model_of(id)
            .is_some_and(|model| model.building.solid.is_some())
}

/// Пол твёрдых моделей под местом `place` для объекта `id`: самый высокий, не выше `from` плюс ступенька (без
/// `from` — любой). Свой пол объекта не считается; твёрдый объект сам на пол не встаёт — это решает `surface`.
pub fn floor_under(
    world: &World,
    id: Option<u32>,
    place: &Footprint,
    from: Option<f64>,
) -> Option<f64> {
    solids(world, id)
        .iter()
        .filter_map(|solid| solid.floor_under(place, from.map(|from| from + STEP)))
        .reduce(f64::max)
}

/// То же под точкой `point`: по полу под ступнёй идущего.
pub fn floor_at(world: &World, id: u32, point: Vec2, from: f64) -> Option<f64> {
    if is_solid(world, id) {
        return None;
    }
    solids(world, Some(id))
        .iter()
        .filter_map(|solid| solid.floor_at(point, Some(from + STEP)))
        .reduce(f64::max)
}

/// Тело двигающегося объекта: что нужно знать, чтобы проверить его место.
struct Body {
    id: u32,
    size: Vec2,
    rotation: Option<Rotation>,
    height: f64,
    z: f64,
}

impl Body {
    /// Тело объёма, что не твёрдое само: настил, плоский объект и твёрдая модель частей зданий не боятся.
    fn of(world: &World, id: u32) -> Option<Body> {
        if surface::kind(world, id) != Kind::Volume || is_solid(world, id) {
            return None;
        }
        Some(Body {
            id,
            size: world.vec2(id, property::SIZE)?,
            rotation: world.rotation(id, property::ROTATION),
            height: surface::body_height(world, id),
            z: world.base_z(id),
        })
    }

    fn place(&self, position: Vec2) -> Footprint {
        Footprint::rotated(position, self.size, self.rotation)
    }

    /// Свободно ли место при основании сдвига: тело не задевает частей здания ни на прежней высоте основания, ни на
    /// той, где встанет после сдвига (`z`, а без неё — по поверхности под новым местом).
    fn clear_at(
        &self,
        world: &World,
        solids: &[Placed<'_>],
        position: Vec2,
        z: Option<f64>,
    ) -> bool {
        let place = self.place(position);
        let seat =
            z.unwrap_or_else(|| surface::rest_height(world, Some(self.id), &place, Some(self.z)));
        let free_at = |base: f64| {
            !solids
                .iter()
                .any(|solid| solid.blocks(&place, base + solid.step(), base + self.height))
        };
        free_at(self.z) && (seat == self.z || free_at(seat))
    }

    /// Самое дальнее свободное место на отрезке от `from` до `to`, куда можно дойти, не задев часть здания, и дошли ли
    /// до `to`: проверка идёт шагами не больше `SWEEP_STEP`, чтобы быстрый сдвиг не проскочил тонкую стену.
    fn sweep(
        &self,
        world: &World,
        solids: &[Placed<'_>],
        from: Vec2,
        to: Vec2,
        z: Option<f64>,
    ) -> (Vec2, bool) {
        let delta = [to[0] - from[0], to[1] - from[1]];
        let at = |t: f64| [from[0] + delta[0] * t, from[1] + delta[1] * t];
        let pieces = (delta[0].hypot(delta[1]) / SWEEP_STEP).ceil().max(1.0) as u32;
        let mut clear_t = 0.0;
        for piece in 1..=pieces {
            let t = f64::from(piece) / f64::from(pieces);
            if self.clear_at(world, solids, at(t), z) {
                clear_t = t;
                continue;
            }
            let (mut near, mut far) = (clear_t, t);
            for _ in 0..APPROACH_BISECTIONS {
                let middle = (near + far) / 2.0;
                if self.clear_at(world, solids, at(middle), z) {
                    near = middle;
                } else {
                    far = middle;
                }
            }
            return (at(near), false);
        }
        (to, true)
    }

    /// Ближайшее свободное место к `from`: по расходящимся лучам, на каждом — самое близкое.
    fn escape(
        &self,
        world: &World,
        solids: &[Placed<'_>],
        from: Vec2,
        z: Option<f64>,
    ) -> Option<Vec2> {
        let steps = (ESCAPE_REACH / ESCAPE_STEP) as usize;
        let directions: Vec<Vec2> = (0..ESCAPE_DIRECTIONS)
            .map(|at| {
                let turn = at as f64 * std::f64::consts::TAU / ESCAPE_DIRECTIONS as f64;
                [libm::cos(turn), libm::sin(turn)]
            })
            .collect();
        let at = |direction: Vec2, reach: f64| {
            [
                from[0] + direction[0] * reach,
                from[1] + direction[1] * reach,
            ]
        };
        for step in 1..=steps {
            let reach = step as f64 * ESCAPE_STEP;
            let Some(&direction) = directions
                .iter()
                .find(|&&direction| self.clear_at(world, solids, at(direction, reach), z))
            else {
                continue;
            };
            let (mut near, mut far) = (reach - ESCAPE_STEP, reach);
            for _ in 0..APPROACH_BISECTIONS {
                let middle = (near + far) / 2.0;
                if self.clear_at(world, solids, at(direction, middle), z) {
                    far = middle;
                } else {
                    near = middle;
                }
            }
            return Some(at(direction, far));
        }
        None
    }
}

/// «Здания»: куда придёт объект `id`, сдвигаясь из `from` в `to` (оба — `position`): туда же, если путь свободен,
/// иначе до препятствия и вдоль него — по одной из осей. Стоящий внутри части выходит наружу по кратчайшему пути.
/// `z` — высота основания, если сдвиг ставит её сам (`follow_mouse`).
pub fn constrain(world: &World, id: u32, from: Vec2, to: Vec2, z: Option<f64>) -> Vec2 {
    if from == to {
        return to;
    }
    let solids = solids(world, Some(id));
    if solids.is_empty() {
        return to;
    }
    let Some(body) = Body::of(world, id) else {
        return to;
    };
    let sweep = |start: Vec2, end: Vec2| body.sweep(world, &solids, start, end, z);
    if !body.clear_at(world, &solids, from, z) {
        return if body.clear_at(world, &solids, to, z) {
            to
        } else {
            body.escape(world, &solids, from, z).unwrap_or(from)
        };
    }
    let (reached, whole) = sweep(from, to);
    if whole {
        return to;
    }
    let rest = [to[0] - reached[0], to[1] - reached[1]];
    let along_x = [to[0], reached[1]];
    let along_y = [reached[0], to[1]];
    let axes = if rest[0].abs() >= rest[1].abs() {
        [along_x, along_y]
    } else {
        [along_y, along_x]
    };
    axes.into_iter()
        .map(|target| sweep(reached, target).0)
        .find(|&slid| slid != reached)
        .unwrap_or(reached)
}

/// Стоит ли точка `point` на высоте `z` в одной из коробок `inside` здания `id`.
fn inside_boxes(world: &World, id: u32, boxes: &[InsideBox], point: Vec2, z: f64) -> bool {
    let Some(frame) = Frame::of(world, id) else {
        return false;
    };
    let [x, y] = frame.local_point(point);
    let local = [x, y, frame.local_height(z)];
    boxes.iter().any(|inside| inside.contains(local))
}

/// Сколько шагов растворения набрал срез здания `id`, 0 — виден основной вид, `DISSOLVE_STEPS` — срез.
pub fn dissolve_share(world: &World, id: u32) -> f32 {
    f32::from(world.cutaway_steps(id)) / f32::from(DISSOLVE_STEPS)
}

/// Конец шага партии: у каждого здания со срезом доля среза идёт к единице, пока объект камеры стоит серединой
/// основания в коробке `inside` здания, и к нулю, когда вышел. Время идёт шагами партии: мир стоит — срез стоит.
pub fn advance_cutaways(world: &mut World) {
    if !world.three_d() || !world.models().any_cutaway() {
        return;
    }
    let followed = camera::followed_point(world);
    for id in 0..world.slot_count() as u32 {
        if !world.is_alive(id) {
            continue;
        }
        let Some(model) = world.model_of(id) else {
            continue;
        };
        if model.building.cutaway.is_none() {
            continue;
        }
        let entered = followed
            .is_some_and(|(center, z)| inside_boxes(world, id, &model.building.inside, center, z));
        let steps = world.cutaway_steps(id);
        let next = if entered {
            (steps + 1).min(DISSOLVE_STEPS)
        } else {
            steps.saturating_sub(1)
        };
        if next != steps {
            world.set_cutaway_steps(id, next);
        }
    }
}

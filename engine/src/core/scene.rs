use super::property;
use super::surface::{self, Lies};
use super::value::ImageId;
use super::world::World;

/// «Мир на экране» → «Земля», требования 14–17: one `ground` layer — `image` is a declared image
/// with no `frame_time`/`frame_by`/`size` (checked at load time, `data::load::parse_ground_layer`),
/// its frames read as tiles; `cells[y][x]` is the tile number for that scene cell, `-1` for none.
/// One row per scene cell of height, one number per row per scene cell of width — also checked at
/// load time, so drawing never has to re-check bounds beyond a plain index.
#[derive(Debug, Clone)]
pub struct GroundLayer {
    pub image: ImageId,
    pub cells: Vec<Vec<i32>>,
}

/// «Трёхмерная сцена» → «Камера»: `scene.camera` — наклон камеры к земле в градусах, 30–90.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraConfig {
    pub pitch: f64,
}

/// «Трёхмерная сцена» → «Земля, свет и плоские объекты»: `scene.light`; без него — эти значения.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LightConfig {
    /// Откуда светит солнце, градусы: 0 — от игрока, 90 — слева, 180 — из глубины, 270 — справа.
    pub sun_from: f64,
    /// Высота солнца над землёй, градусы.
    pub sun_height: f64,
    /// Насколько темна тень, 0–1.
    pub shadow: f64,
    /// Цвет солнечного света в sRGB: задаёт только оттенок.
    pub sun_color: [f32; 3],
    /// Цвет неба — рассеянного света сверху — в sRGB: задаёт только оттенок.
    pub sky_color: [f32; 3],
}

impl Default for LightConfig {
    fn default() -> Self {
        LightConfig {
            sun_from: 135.0,
            sun_height: 50.0,
            shadow: 0.4,
            sun_color: [1.0, 242.0 / 255.0, 220.0 / 255.0],
            sky_color: [169.0 / 255.0, 200.0 / 255.0, 238.0 / 255.0],
        }
    }
}

impl LightConfig {
    /// Единичный вектор от точки на земле к солнцу: `x` сцены вправо, `y` — к игроку, `z` — вверх.
    pub fn direction(&self) -> [f64; 3] {
        let from = self.sun_from.to_radians();
        let height = self.sun_height.to_radians();
        [
            -from.sin() * height.cos(),
            from.cos() * height.cos(),
            height.sin(),
        ]
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SceneConfig {
    pub width: u32,
    pub height: u32,
    pub background: [f32; 4],
    /// «Камера», требование 1, 3: клеток по высоте окна; `None` — прежнее вписывание сцены
    /// целиком, камера ни на что не влияет.
    pub view_height: Option<f64>,
    /// «Порядок рисования», требование 9: при равных `layer` поверх тот, чей нижний край ниже.
    pub y_sort: bool,
    /// «Трёхмерная сцена»: есть камера — сцена трёхмерная; `None` — плоская, как раньше.
    pub camera: Option<CameraConfig>,
    pub light: LightConfig,
}

impl SceneConfig {
    pub fn is_3d(&self) -> bool {
        self.camera.is_some()
    }
}

/// The scale and top-left offset (in the same unit `viewport` is given in) that letterboxes the
/// scene into `viewport`, keeping its aspect ratio, centered — «Формат игры»: «сцена вписывается
/// в окно целиком, с сохранением пропорций». Shared by the renderer's own world pass
/// (`render::gpu::world_globals_data`, in device pixels) and «Курсор в мире» below (in window CSS
/// pixels): the formula is unit-agnostic, only the two calls differ in which unit they pass.
pub fn letterbox(viewport: [f32; 2], scene_cells: [f32; 2]) -> (f32, [f32; 2]) {
    let scale = (viewport[0] / scene_cells[0]).min(viewport[1] / scene_cells[1]);
    let size = [scene_cells[0] * scale, scene_cells[1] * scale];
    let offset = [(viewport[0] - size[0]) / 2.0, (viewport[1] - size[1]) / 2.0];
    (scale, offset)
}

impl SceneConfig {
    /// «Трёхмерная сцена», требование 21: точка земли за краем сцены прижимается к краю сцены.
    pub fn clamp_point(&self, point: super::value::Vec2) -> super::value::Vec2 {
        [
            point[0].clamp(0.0, self.width as f64),
            point[1].clamp(0.0, self.height as f64),
        ]
    }

    /// «Курсор в мире», требование 25: translates a window-pixel cursor position into scene
    /// coordinates, by the same letterboxed mapping the renderer draws the scene with, then
    /// clamps to the scene's own edges. Used outside a battle (editor without a session) and by
    /// the plain wasm layer before any frame is known — see `window_to_scene_frame` for the
    /// camera-aware mapping used everywhere else.
    pub fn window_to_scene(&self, window_pos: [f32; 2], viewport: [f32; 2]) -> super::value::Vec2 {
        let scene_cells = [self.width as f32, self.height as f32];
        if viewport[0] <= 0.0 || viewport[1] <= 0.0 {
            return [0.0, 0.0];
        }
        let (scale, offset) = letterbox(viewport, scene_cells);
        self.window_to_scene_frame(window_pos, scale, offset)
    }

    /// «Курсор в мире», требование 11–12: same mapping, given an already-computed frame (camera's
    /// scale/offset, or the plain letterbox's) — за краем видимой части сцены точка прижимается к
    /// этому краю (уже гарантировано клампом к самой сцене, а видимая часть — это `viewport`).
    pub fn window_to_scene_frame(
        &self,
        window_pos: [f32; 2],
        scale: f32,
        offset: [f32; 2],
    ) -> super::value::Vec2 {
        let scale = scale.max(1e-6);
        let cell = [
            (window_pos[0] - offset[0]) / scale,
            (window_pos[1] - offset[1]) / scale,
        ];
        [
            (cell[0] as f64).clamp(0.0, self.width as f64),
            (cell[1] as f64).clamp(0.0, self.height as f64),
        ]
    }

    /// «Трёхмерная сцена»: целые клетки сцены, которых касается прямоугольник `[low, high]`, — в
    /// пределах сцены.
    pub fn cell_range_covering(
        &self,
        low: super::value::Vec2,
        high: super::value::Vec2,
    ) -> CellRange {
        let x0 = (low[0].floor().max(0.0) as u32).min(self.width);
        let y0 = (low[1].floor().max(0.0) as u32).min(self.height);
        let x1 = (high[0].ceil().max(0.0) as u32).clamp(x0, self.width);
        let y1 = (high[1].ceil().max(0.0) as u32).clamp(y0, self.height);
        CellRange { x0, y0, x1, y1 }
    }

    /// «Мир на экране» → «Земля», требование 20: whole scene cells `[x0, x1) × [y0, y1)` at least
    /// partly inside the visible part of the scene right now — given the same `scale`/`offset`
    /// the world itself just drew with (camera's, or the plain letterbox's), clamped to the
    /// scene's own bounds. A non-positive `scale` (no loaded game ever draws with one) answers the
    /// whole scene rather than an empty or inverted range.
    pub fn visible_cell_range(
        &self,
        scale: f32,
        offset: [f32; 2],
        viewport: [f32; 2],
    ) -> CellRange {
        if scale <= 0.0 {
            return CellRange {
                x0: 0,
                y0: 0,
                x1: self.width,
                y1: self.height,
            };
        }
        let visible_min = [-offset[0] / scale, -offset[1] / scale];
        let visible_max = [
            (viewport[0] - offset[0]) / scale,
            (viewport[1] - offset[1]) / scale,
        ];
        let x0 = (visible_min[0].floor().max(0.0) as u32).min(self.width);
        let y0 = (visible_min[1].floor().max(0.0) as u32).min(self.height);
        let x1 = (visible_max[0].ceil().max(0.0) as u32).clamp(x0, self.width);
        let y1 = (visible_max[1].ceil().max(0.0) as u32).clamp(y0, self.height);
        CellRange { x0, y0, x1, y1 }
    }
}

/// `[x0, x1) × [y0, y1)` scene cells, `x1`/`y1` exclusive — «Мир на экране» → «Земля»:
/// `SceneConfig::visible_cell_range`'s own result, and `render::atlas::compose_ground_paints`'s
/// own input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellRange {
    pub x0: u32,
    pub y0: u32,
    pub x1: u32,
    pub y1: u32,
}

/// «Порядок рисования», требования 8–10: the comparator every draw-order decision shares —
/// ascending `layer` first, then (with `y_sort`) the lower bottom edge (`position.y + size.y`),
/// then the smaller object number. `true` means `a` draws *before* `b` (`b` ends up on top of
/// `a`) — the same relation `[T]::sort_by`/`Iterator::max_by` expect from `Ordering`. An object
/// missing `position`/`size` sorts as if its bottom edge were at negative infinity — it never
/// affects a comparison against an object that does have them, and two such objects fall back to
/// their object number.
pub fn draw_order(world: &World, scene: &SceneConfig, a: u32, b: u32) -> std::cmp::Ordering {
    let layer_a = world.layer(a, property::LAYER).unwrap_or(0);
    let layer_b = world.layer(b, property::LAYER).unwrap_or(0);
    layer_a
        .cmp(&layer_b)
        .then_with(|| {
            if !scene.y_sort {
                return std::cmp::Ordering::Equal;
            }
            bottom_edge(world, a).total_cmp(&bottom_edge(world, b))
        })
        .then_with(|| a.cmp(&b))
}

fn bottom_edge(world: &World, id: u32) -> f64 {
    match (
        world.vec2(id, property::POSITION),
        world.vec2(id, property::SIZE),
    ) {
        (Some(p), Some(s)) => p[1] + s[1],
        _ => f64::NEG_INFINITY,
    }
}

/// A rectangle in canvas CSS pixels, top-left origin — `object_rect`'s result, handed to the
/// editor as `{x, y, width, height}`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CanvasRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// «Редактор», требование 19: object `id`'s canvas rectangle — `position`/`size` letterboxed the
/// same way the renderer draws the world outside a battle (`render::gpu::world_globals_data`
/// without a camera). `None` when the object doesn't exist (id past the world's slot count) or
/// lacks `position`/`size`. Rotation never changes this rectangle — a rotated image stretches to
/// fill the same one. See `object_rect_frame` for the camera-aware version used in a battle.
pub fn object_rect(
    world: &World,
    scene: &SceneConfig,
    id: u32,
    viewport: [f32; 2],
) -> Option<CanvasRect> {
    let scene_cells = [scene.width as f32, scene.height as f32];
    let (scale, offset) = letterbox(viewport, scene_cells);
    object_rect_frame(world, id, scale, offset)
}

/// Same as `object_rect`, given an already-computed frame — «Камера», требование 42: the wasm
/// layer computes the camera's own scale/offset once per call and passes it here instead of
/// letterboxing the whole scene.
pub fn object_rect_frame(
    world: &World,
    id: u32,
    scale: f32,
    offset: [f32; 2],
) -> Option<CanvasRect> {
    if id as usize >= world.slot_count() {
        return None;
    }
    let position = world.vec2(id, property::POSITION)?;
    let size = world.vec2(id, property::SIZE)?;
    Some(CanvasRect {
        x: offset[0] + position[0] as f32 * scale,
        y: offset[1] + position[1] as f32 * scale,
        width: size[0] as f32 * scale,
        height: size[1] as f32 * scale,
    })
}

/// «Редактор», требование 30: moves an already-loaded object to `position`, in the world alone —
/// the scene it was built from and the files on disk stay untouched, so the next `show_scene`
/// rebuilds the world from the unchanged file. Does nothing without that object or without its
/// own `position` property. «Рельеф»: в трёхмерной сцене с `z` основание встаёт ровно на него, без
/// — объект садится на поверхность под новым местом, как после сдвига.
pub fn move_object(world: &mut World, id: u32, position: super::value::Vec2, z: Option<f64>) {
    if id as usize >= world.slot_count() || !world.has(id, property::POSITION) {
        return;
    }
    match z.filter(|_| world.three_d()) {
        Some(z) => world.set_position_exact(id, position, z),
        None => world.set_vec2(id, property::POSITION, position),
    }
}

/// «Редактор», «Вызовы движка»: то, что `transform_object` ставит объекту в собранном мире —
/// `position` и `size` всегда, `height` и `rotation`, только если названы. `z` — высота основания:
/// с ней основание встаёт ровно, без неё объект садится на поверхность, как после сдвига.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ObjectTransform {
    pub position: super::value::Vec2,
    pub z: Option<f64>,
    pub size: super::value::Vec2,
    pub height: Option<f64>,
    pub rotation: Option<f64>,
}

/// «Редактор», «Правка сцены»: ставит объекту значения ручек в собранном мире — файлы и разобранная
/// сцена остаются как были. Не делает ничего без объекта или без его `position` и `size`; не названные
/// `height` и `rotation` не меняются.
pub fn transform_object(world: &mut World, id: u32, transform: ObjectTransform) {
    if id as usize >= world.slot_count()
        || !world.has_all(id, &[property::POSITION, property::SIZE])
    {
        return;
    }
    let previous = world.vec2(id, property::POSITION);
    let previous_z = world.base_z(id);
    world.set_vec2(id, property::SIZE, transform.size);
    if let Some(height) = transform.height {
        world.set_number(id, property::HEIGHT, height);
    }
    let previous_rotation = world.rotation(id, property::ROTATION);
    if let Some(rotation) = transform
        .rotation
        .and_then(super::value::Rotation::from_degrees)
    {
        world.set_rotation(id, property::ROTATION, rotation);
    }
    world.set_position_exact(id, transform.position, previous_z);
    if !world.three_d() {
        return;
    }
    let shifted = previous != Some(transform.position)
        || world.rotation(id, property::ROTATION) != previous_rotation;
    match transform.z {
        Some(z) => world.set_base_z(id, z),
        None if shifted => surface::seat_after_shift(world, id),
        None => {}
    }
}

/// «Трёхмерная сцена», «Редактор»: место объекта на земле — его прямоугольник, повёрнутый вокруг
/// середины на `rotation`. `None` без объекта или его `position` и `size`.
pub fn ground_footprint(world: &World, id: u32) -> Option<super::footprint::Footprint> {
    if id as usize >= world.slot_count() {
        return None;
    }
    let position = world.vec2(id, property::POSITION)?;
    let size = world.vec2(id, property::SIZE)?;
    Some(super::footprint::Footprint::rotated(
        position,
        size,
        world.rotation(id, property::ROTATION),
    ))
}

/// «Редактор», требование 19 трёхмерной сцены: четыре угла повёрнутого прямоугольника объекта на
/// высоте его основания в точках окна камеры, по кругу, в порядке `Footprint::corners`. `None` без
/// объекта, его `position` и `size` или если угол за камерой.
pub fn object_screen_corners(
    world: &World,
    id: u32,
    camera: &super::camera::Camera3d,
) -> Option<[[f64; 2]; 4]> {
    let mut corners = [[0.0; 2]; 4];
    let base = world.base_z(id);
    for (slot, corner) in corners
        .iter_mut()
        .zip(ground_footprint(world, id)?.corners())
    {
        *slot = camera.project([corner[0], corner[1], base])?;
    }
    Some(corners)
}

/// «Редактор», требование 18: the topmost object (по порядку рисования — `draw_order`, требование
/// 10) whose canvas rectangle contains `point` (CSS pixels, canvas top-left origin) — left/top
/// edges included, right/bottom excluded. A candidate needs `position`, `size` and a `color` or
/// `image` fill — the same shape `render::atlas::compose_world_paints` draws; an object with
/// neither is invisible and a click never picks it. `None` off every object, or in the margin
/// around the letterboxed scene. See `object_at_frame` for the camera-aware version.
pub fn object_at(
    world: &World,
    scene: &SceneConfig,
    point: [f32; 2],
    viewport: [f32; 2],
) -> Option<u32> {
    let scene_cells = [scene.width as f32, scene.height as f32];
    let (scale, offset) = letterbox(viewport, scene_cells);
    object_at_frame(world, scene, point, scale, offset)
}

/// Same as `object_at`, given an already-computed frame — see `object_rect_frame`.
pub fn object_at_frame(
    world: &World,
    scene: &SceneConfig,
    point: [f32; 2],
    scale: f32,
    offset: [f32; 2],
) -> Option<u32> {
    let mut best: Option<u32> = None;
    for id in world.ids() {
        let has_fill = world.color(id, property::COLOR).is_some()
            || world.image(id, property::IMAGE).is_some();
        if !has_fill {
            continue;
        }
        let Some(rect) = object_rect_frame(world, id, scale, offset) else {
            continue;
        };
        if point[0] < rect.x
            || point[1] < rect.y
            || point[0] >= rect.x + rect.width
            || point[1] >= rect.y + rect.height
        {
            continue;
        }
        let replace = match best {
            None => true,
            Some(best_id) => draw_order(world, scene, best_id, id) != std::cmp::Ordering::Greater,
        };
        if replace {
            best = Some(id);
        }
    }
    best
}

/// «Трёхмерная сцена» → «Мышь и надписи», требование 22: луч от глаза камеры через точку земли под
/// курсором. Фигура ловит его своим объёмом по форме, объект без фигуры — своим повёрнутым
/// прямоугольником на земле; из тех, в кого луч попал, срабатывает ближний к камере, при равной
/// дальности — нарисованный сверху (`draw_order`). Объект без `on_click` луч не задерживает. Без
/// `eye` (записи нет, камеры нет) луч идёт прямо вниз через точку.
pub fn on_click_target_ray(
    world: &World,
    scene: &SceneConfig,
    eye: Option<super::math3::Vec3>,
    point: super::value::Vec2,
) -> Option<u32> {
    on_click_target_ray_at(world, scene, eye, [point[0], point[1], 0.0])
}

/// То же для точки под курсором с высотой: луч идёт от глаза через `point`. «Рельеф»: рельеф и вода,
/// что луч встретил раньше объекта, его заслоняют.
pub fn on_click_target_ray_at(
    world: &World,
    scene: &SceneConfig,
    eye: Option<super::math3::Vec3>,
    point: super::math3::Vec3,
) -> Option<u32> {
    let (origin, direction) = match eye {
        Some(eye) => super::shapes::ray_through_point(eye, point),
        None => ([point[0], point[1], point[2] + 1000.0], [0.0, 0.0, -1.0]),
    };
    nearest_along_ray(world, scene, origin, direction, |id| {
        world.has(id, property::ON_CLICK)
    })
}

/// «Редактор», «Сцена», требование 7 трёхмерной сцены: объект под точкой окна `window` камеры — тем
/// же лучом от глаза, что у щелчка в игре, но `on_click` не нужен: выбирается любой нарисованный
/// объект (с `shape`, `image` или `color`). Ближний к камере, при равной дальности — нарисованный
/// сверху.
pub fn editor_target_ray(
    world: &World,
    scene: &SceneConfig,
    camera: &super::camera::Camera3d,
    window: [f64; 2],
) -> Option<u32> {
    nearest_along_ray(
        world,
        scene,
        camera.eye,
        camera.ray_direction(window),
        |id| {
            world.shape(id, property::SHAPE).is_some()
                || world.image(id, property::IMAGE).is_some()
                || world.color(id, property::COLOR).is_some()
        },
    )
}

/// Ближний к `origin` объект, в которого попал луч, из тех, что `pickable` пускает в выбор.
fn nearest_along_ray(
    world: &World,
    scene: &SceneConfig,
    origin: super::math3::Vec3,
    direction: super::math3::Vec3,
    pickable: impl Fn(u32) -> bool,
) -> Option<u32> {
    let mut best: Option<(f64, u32)> = None;
    let blocked_from = first_blocker(world, scene, origin, direction);
    for id in world.ids() {
        if !pickable(id) {
            continue;
        }
        let Some(distance) = ray_distance(world, scene, id, origin, direction) else {
            continue;
        };
        if distance > blocked_from + RAY_TIE {
            continue;
        }
        let replace = match best {
            None => true,
            Some((best_distance, best_id)) => {
                distance < best_distance - RAY_TIE
                    || (distance <= best_distance + RAY_TIE
                        && draw_order(world, scene, best_id, id) != std::cmp::Ordering::Greater)
            }
        };
        if replace {
            best = Some((distance, id));
        }
    }
    best.map(|(_, id)| id)
}

/// Равная дальность для `nearest_along_ray`: плоские объекты на одной земле.
const RAY_TIE: f64 = 1e-9;

/// Расстояние вдоль луча до объекта: до объёма фигуры или до поверхности, на которой лежит плоский
/// объект, — рельефа или верха настила — в пределах его прямоугольника.
fn ray_distance(
    world: &World,
    scene: &SceneConfig,
    id: u32,
    origin: super::math3::Vec3,
    direction: super::math3::Vec3,
) -> Option<f64> {
    if let Some(body) = super::shapes::Body::of_object(world, id) {
        return body.ray_hit(origin, direction);
    }
    let footprint = ground_footprint(world, id)?;
    if direction[2] >= 0.0 {
        return None;
    }
    let terrain = world.terrain();
    let t = match surface::lies_on(world, id)? {
        Lies::Deck { top } => (top - origin[2]) / direction[2],
        Lies::Terrain { .. } if terrain.squares()[0] == 0 => -origin[2] / direction[2],
        Lies::Terrain { .. } => terrain.ray_hit(scene_extent(scene), origin, direction)?,
    };
    if t < 0.0 {
        return None;
    }
    let ground = [origin[0] + t * direction[0], origin[1] + t * direction[1]];
    footprint.contains(ground).then_some(t)
}

fn scene_extent(scene: &SceneConfig) -> super::value::Vec2 {
    [scene.width as f64, scene.height as f64]
}

/// «Рельеф»: расстояние вдоль луча до первого, что заслоняет объекты, — рельефа в пределах сцены
/// или воды; бесконечность, если ничего.
fn first_blocker(
    world: &World,
    scene: &SceneConfig,
    origin: super::math3::Vec3,
    direction: super::math3::Vec3,
) -> f64 {
    let terrain = world.terrain();
    [
        terrain.ray_hit(scene_extent(scene), origin, direction),
        terrain.water_hit(origin, direction),
    ]
    .into_iter()
    .flatten()
    .fold(f64::INFINITY, f64::min)
}

/// «Трёхмерная сцена» → «Мышь и надписи», требование 26: место с высотой, где луч первым встретил
/// рельеф в пределах сцены, воду или верх настила. Настил встречает луч только своим верхом: луч,
/// прошедший под верхом моста, попадает в берег под ним.
pub fn surface_hit(
    world: &World,
    scene: &SceneConfig,
    origin: super::math3::Vec3,
    direction: super::math3::Vec3,
) -> Option<super::math3::Vec3> {
    let mut nearest = first_blocker(world, scene, origin, direction);
    if direction[2] < 0.0 {
        for (_, place, top) in surface::decks(world) {
            let t = (top - origin[2]) / direction[2];
            let (x, y) = (origin[0] + t * direction[0], origin[1] + t * direction[1]);
            if t >= 0.0 && t < nearest && place.contains([x, y]) {
                nearest = t;
            }
        }
    }
    nearest.is_finite().then(|| {
        [
            origin[0] + nearest * direction[0],
            origin[1] + nearest * direction[1],
            origin[2] + nearest * direction[2],
        ]
    })
}

/// «Рельеф» → «Мышь», требование 26: место с высотой под точкой окна `window` камеры — где луч первым
/// встретил рельеф в пределах сцены, воду или верх настила; мимо сцены — пересечение с плоскостью
/// высоты 0 без прижатия к краю. `None`, если луч не идёт вниз.
pub fn pointer_hit(
    world: &World,
    scene: &SceneConfig,
    camera: &super::camera::Camera3d,
    window: [f64; 2],
) -> Option<super::math3::Vec3> {
    surface_hit(world, scene, camera.eye, camera.ray_direction(window)).or_else(|| {
        camera
            .ground_hit(window)
            .map(|ground| [ground[0], ground[1], 0.0])
    })
}

/// «Редактор», «Вызовы движка», `terrain_at`: место с высотой под точкой окна `window` камеры — где луч
/// первым встретил рельеф в пределах сцены, минуя воду, настилы и объекты; ровная земля высоты 0, пока
/// файла рельефа нет. `None`, если луч не идёт вниз или проходит мимо рельефа сцены.
pub fn terrain_hit(
    world: &World,
    scene: &SceneConfig,
    camera: &super::camera::Camera3d,
    window: [f64; 2],
) -> Option<super::math3::Vec3> {
    let direction = camera.ray_direction(window);
    if direction[2] >= 0.0 {
        return None;
    }
    let t = world
        .terrain()
        .ray_hit(scene_extent(scene), camera.eye, direction)?;
    Some([
        camera.eye[0] + t * direction[0],
        camera.eye[1] + t * direction[1],
        camera.eye[2] + t * direction[2],
    ])
}

/// Высота верхней поверхности в точке: верх самого высокого настила над ней, иначе рельеф.
pub fn top_surface_height(world: &World, point: super::value::Vec2) -> f64 {
    surface::decks(world)
        .filter(|(_, place, _)| place.contains(point))
        .map(|(_, _, top)| top)
        .fold(None, |best: Option<f64>, top| {
            Some(best.map_or(top, |b| b.max(top)))
        })
        .unwrap_or_else(|| world.terrain().height_at(point[0], point[1]))
}

/// «Мышь в мире», требование 19: the topmost live object with `on_click` whose rectangle
/// (`position`/`size`, scene cells) contains `point` (scene cells) — left/top edges included,
/// right/bottom excluded, топ по `draw_order`. `None` off every such object.
pub fn on_click_target(
    world: &World,
    scene: &SceneConfig,
    point: super::value::Vec2,
) -> Option<u32> {
    let mut best: Option<u32> = None;
    for id in world.ids() {
        if !world.has(id, property::ON_CLICK) {
            continue;
        }
        let (Some(p), Some(s)) = (
            world.vec2(id, property::POSITION),
            world.vec2(id, property::SIZE),
        ) else {
            continue;
        };
        if point[0] < p[0] || point[1] < p[1] || point[0] >= p[0] + s[0] || point[1] >= p[1] + s[1]
        {
            continue;
        }
        let replace = match best {
            None => true,
            Some(best_id) => draw_order(world, scene, best_id, id) != std::cmp::Ordering::Greater,
        };
        if replace {
            best = Some(id);
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::PropertyTable;

    #[test]
    fn window_to_scene_maps_the_centered_scene_rectangle_one_to_one() {
        let scene = SceneConfig {
            width: 10,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        // A 10x20 scene into an 800x600 window scales by 30 (600/20), letterboxed 100px on
        // each side horizontally (800 - 10*30 = 500, halved).
        let cell = scene.window_to_scene([100.0, 0.0], [800.0, 600.0]);
        assert!((cell[0] - 0.0).abs() < 1e-4, "{cell:?}");
        assert!((cell[1] - 0.0).abs() < 1e-4, "{cell:?}");
        let cell = scene.window_to_scene([400.0, 300.0], [800.0, 600.0]);
        assert!((cell[0] - 5.0).abs() < 1e-3, "{cell:?}");
        assert!((cell[1] - 10.0).abs() < 1e-3, "{cell:?}");
    }

    #[test]
    fn window_to_scene_clamps_to_scene_edges() {
        let scene = SceneConfig {
            width: 10,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let cell = scene.window_to_scene([-500.0, -500.0], [800.0, 600.0]);
        assert_eq!(cell, [0.0, 0.0]);
        let cell = scene.window_to_scene([5000.0, 5000.0], [800.0, 600.0]);
        assert_eq!(cell, [10.0, 20.0]);
    }

    /// «Редактор», требование 19: same letterboxed scale/offset `world_globals_data` draws the
    /// world with — a 10×20 scene in an 800×600 canvas scales by 30 (`min(80, 30)`), centered
    /// with a 0px vertical margin and a 250px horizontal one (`(800 - 10*30) / 2`).
    #[test]
    fn object_rect_computes_the_letterboxed_pixel_rectangle() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let scene = SceneConfig {
            width: 10,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let id = world.create();
        world.set_vec2(id, property::POSITION, [1.0, 2.0]);
        world.set_vec2(id, property::SIZE, [2.0, 1.0]);

        let rect = object_rect(&world, &scene, id, [800.0, 600.0]).expect("has position and size");
        assert!((rect.x - 280.0).abs() < 1e-3, "{rect:?}");
        assert!((rect.y - 60.0).abs() < 1e-3, "{rect:?}");
        assert!((rect.width - 60.0).abs() < 1e-3, "{rect:?}");
        assert!((rect.height - 30.0).abs() < 1e-3, "{rect:?}");
    }

    #[test]
    fn object_rect_is_none_without_position_size_or_a_matching_object() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let scene = SceneConfig {
            width: 10,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let bare = world.create();
        assert_eq!(object_rect(&world, &scene, bare, [800.0, 600.0]), None);
        assert_eq!(object_rect(&world, &scene, 99, [800.0, 600.0]), None);
    }

    #[test]
    fn move_object_sets_position_and_leaves_other_properties_and_objects_alone() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let scene = SceneConfig {
            width: 10,
            height: 10,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let moved = world.create();
        world.set_vec2(moved, property::POSITION, [1.0, 2.0]);
        world.set_vec2(moved, property::SIZE, [1.0, 1.0]);
        world.set_color(moved, property::COLOR, [1.0, 0.0, 0.0, 1.0]);
        let other = world.create();
        world.set_vec2(other, property::POSITION, [4.0, 4.0]);

        move_object(&mut world, moved, [5.37, 7.0], None);

        assert_eq!(world.vec2(moved, property::POSITION), Some([5.37, 7.0]));
        assert_eq!(world.vec2(moved, property::SIZE), Some([1.0, 1.0]));
        assert_eq!(world.vec2(other, property::POSITION), Some([4.0, 4.0]));

        let viewport = [100.0, 100.0]; // scale 10, no margin
        let rect = object_rect(&world, &scene, moved, viewport).expect("has position and size");
        assert!((rect.x - 53.7).abs() < 1e-3, "{rect:?}");
        assert!((rect.y - 70.0).abs() < 1e-3, "{rect:?}");
        assert_eq!(
            object_at(&world, &scene, [54.0, 71.0], viewport),
            Some(moved)
        );
    }

    #[test]
    fn move_object_does_nothing_without_the_object_or_its_position() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let bare = world.create();

        move_object(&mut world, bare, [5.0, 5.0], None);
        assert_eq!(world.vec2(bare, property::POSITION), None);

        move_object(&mut world, 99, [5.0, 5.0], None);
        assert_eq!(world.slot_count(), 1, "no slot created for a missing id");
    }

    #[test]
    fn object_at_picks_the_highest_layer_ties_broken_by_the_larger_number() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let scene = SceneConfig {
            width: 10,
            height: 10,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let viewport = [100.0, 100.0]; // scale 10, no letterbox margin

        let mut stacked = |layer: i32, color: [f32; 4]| {
            let id = world.create();
            world.set_vec2(id, property::POSITION, [0.0, 0.0]);
            world.set_vec2(id, property::SIZE, [5.0, 5.0]);
            world.set_color(id, property::COLOR, color);
            world.set_layer(id, property::LAYER, layer);
            id
        };
        stacked(0, [1.0, 0.0, 0.0, 1.0]);
        stacked(5, [0.0, 1.0, 0.0, 1.0]);
        let tie = stacked(5, [0.0, 0.0, 1.0, 1.0]);

        assert_eq!(object_at(&world, &scene, [10.0, 10.0], viewport), Some(tie));
    }

    #[test]
    fn object_at_misses_the_margin_and_a_fill_less_object() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let scene = SceneConfig {
            width: 10,
            height: 20,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let viewport = [800.0, 600.0]; // scale 30, offset [250, 0]

        let invisible = world.create();
        world.set_vec2(invisible, property::POSITION, [0.0, 0.0]);
        world.set_vec2(invisible, property::SIZE, [10.0, 20.0]);

        // In the letterboxed margin, left of the scene entirely.
        assert_eq!(object_at(&world, &scene, [10.0, 300.0], viewport), None);
        // Inside the scene rectangle, but its only object has no color or image.
        assert_eq!(object_at(&world, &scene, [400.0, 300.0], viewport), None);
    }

    #[test]
    fn object_at_edges_are_left_and_top_inclusive_right_and_bottom_exclusive() {
        let table = PropertyTable::new();
        let mut world = World::new(&table);
        let scene = SceneConfig {
            width: 10,
            height: 10,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let viewport = [100.0, 100.0]; // scale 10, no margin

        let id = world.create();
        world.set_vec2(id, property::POSITION, [2.0, 2.0]);
        world.set_vec2(id, property::SIZE, [3.0, 3.0]);
        world.set_color(id, property::COLOR, [1.0, 1.0, 1.0, 1.0]);
        // rect is x: [20, 50), y: [20, 50)

        assert_eq!(
            object_at(&world, &scene, [20.0, 20.0], viewport),
            Some(id),
            "top-left included"
        );
        assert_eq!(object_at(&world, &scene, [49.9, 49.9], viewport), Some(id));
        assert_eq!(
            object_at(&world, &scene, [50.0, 30.0], viewport),
            None,
            "right excluded"
        );
        assert_eq!(
            object_at(&world, &scene, [30.0, 50.0], viewport),
            None,
            "bottom excluded"
        );
    }

    // -------------------------------------------------------------------------------------------
    // Фаза 13 — «Мир на экране» → «Земля», требование 20: видимые клетки под текущей камерой
    // -------------------------------------------------------------------------------------------

    fn ground_scene(width: u32, height: u32) -> SceneConfig {
        SceneConfig {
            width,
            height,
            background: [0.0; 4],
            view_height: Some(10.0),
            y_sort: false,
            camera: None,
            light: Default::default(),
        }
    }

    #[test]
    fn without_view_height_the_whole_scene_is_visible() {
        let scene = SceneConfig {
            width: 20,
            height: 15,
            background: [0.0; 4],
            view_height: None,
            y_sort: false,
            camera: None,
            light: Default::default(),
        };
        let (scale, offset) = letterbox([800.0, 600.0], [20.0, 15.0]);
        let visible = scene.visible_cell_range(scale, offset, [800.0, 600.0]);
        assert_eq!(
            visible,
            CellRange {
                x0: 0,
                y0: 0,
                x1: 20,
                y1: 15
            }
        );
    }

    /// Camera centered on a 40×40 scene, `view_height: 10`, 800×800 window — scale 80, showing a
    /// 10×10 window of cells centered on (20, 20): cells [15, 25) both axes.
    #[test]
    fn camera_shows_only_the_window_of_cells_under_it() {
        let scene = ground_scene(40, 40);
        let viewport = [800.0, 800.0];
        let (scale, offset) = crate::core::camera::frame(&scene, [20.0, 20.0], viewport);
        let visible = scene.visible_cell_range(scale, offset, viewport);
        assert_eq!(
            visible,
            CellRange {
                x0: 15,
                y0: 15,
                x1: 25,
                y1: 25
            }
        );
    }

    /// The camera pinned to the scene's own left edge — visible range starts at cell 0, not
    /// negative, and never exceeds the scene's own width.
    #[test]
    fn visible_range_clamps_to_the_scenes_own_edges() {
        let scene = ground_scene(40, 40);
        let viewport = [800.0, 800.0];
        let (scale, offset) = crate::core::camera::frame(&scene, [0.0, 0.0], viewport);
        let visible = scene.visible_cell_range(scale, offset, viewport);
        assert_eq!(visible.x0, 0, "{visible:?}");
        assert_eq!(visible.y0, 0, "{visible:?}");
        assert!(visible.x1 <= 40 && visible.y1 <= 40, "{visible:?}");
    }
}

/// One `scene.json` object, kept around after load so «Экраны и состояние»'s `new_game` can
/// rebuild the world from this parsed copy without reopening the file a second time.
#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub values: Vec<(super::property::PropertyId, super::value::Value)>,
    pub grid: Option<super::value::GridSpec>,
    pub keys: Option<super::keys::KeyTable>,
    /// «Мышь в мире», требование 19: `on_click` разобран отдельно от `values`, как `grid`/`keys` —
    /// свой вид свойства, не входящий в generic `Value`.
    pub on_click: Option<Vec<super::keys::KeyEdit>>,
}

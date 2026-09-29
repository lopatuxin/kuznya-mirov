//! «Камера», требования 4–7: масштаб и центр камеры по `SceneConfig`, размеру окна и точке
//! объекта камеры. Считает только ядро — правила и код игры камеры не видят, интерфейс экранов
//! от неё не зависит.

use super::math3::{self, Mat4, Vec3};
use super::property;
use super::scene::{SceneConfig, letterbox};
use super::value::Vec2;
use super::world::World;

/// «Камера», требование 5: объект камеры — живой объект с `camera_follows: true`, `position` и
/// `size`, с меньшим номером.
fn camera_object(world: &World) -> Option<u32> {
    world
        .ids()
        .filter(|&id| world.flag(id, property::CAMERA_FOLLOWS))
        .filter(|&id| world.has(id, property::POSITION) && world.has(id, property::SIZE))
        .min()
}

/// «Камера», требования 5–6: середина объекта камеры прямо сейчас, если такой объект есть.
/// `None` — камера держит последнюю точку (или середину сцены, если такой ещё не было), которую
/// хранит вызывающий (`Game::camera_last`).
pub fn followed_center(world: &World) -> Option<Vec2> {
    let id = camera_object(world)?;
    let p = world.vec2(id, property::POSITION)?;
    let s = world.vec2(id, property::SIZE)?;
    Some([p[0] + s[0] / 2.0, p[1] + s[1] / 2.0])
}

/// «Камера», требование 3–4, 118–120: масштаб и сдвиг (тем же видом, что и `letterbox`) для
/// отрисовки/мыши/выбора по камере. Без `view_height` — прежнее вписывание сцены целиком, центр
/// камеры не влияет ни на что. С ним — масштаб только по высоте окна, а по каждой оси отдельно:
/// сцена не меньше видимой части — центр прижат так, чтобы видимая часть не вышла за край сцены;
/// сцена меньше — по центру, с полями по краям.
pub fn frame(scene: &SceneConfig, center: Vec2, viewport: [f32; 2]) -> (f32, [f32; 2]) {
    let scene_cells = [scene.width as f32, scene.height as f32];
    let Some(view_height) = scene.view_height else {
        return letterbox(viewport, scene_cells);
    };
    if viewport[1] <= 0.0 {
        return (1.0, [0.0, 0.0]);
    }
    let scale = viewport[1] / view_height as f32;
    let offset_x = axis_offset(scene.width as f32, center[0] as f32, scale, viewport[0]);
    let offset_y = axis_offset(scene.height as f32, center[1] as f32, scale, viewport[1]);
    (scale, [offset_x, offset_y])
}

/// One axis of `frame` — «Крайние случаи»: сцена шире окна по этой оси — камера ходит за
/// объектом, прижатая к краям; сцена уже — стоит по центру, поля по краям.
fn axis_offset(scene_len: f32, center: f32, scale: f32, viewport_len: f32) -> f32 {
    let scene_px = scene_len * scale;
    if scene_px <= viewport_len {
        (viewport_len - scene_px) / 2.0
    } else {
        let visible_len = viewport_len / scale;
        let half = visible_len / 2.0;
        let clamped = center.clamp(half, scene_len - half);
        viewport_len / 2.0 - clamped * scale
    }
}

/// «Трёхмерная сцена» → «Камера», требование 2: угол зрения по высоте окна — один на все игры.
pub const FOV_Y_DEGREES: f64 = 35.0;

fn tan_half_fov() -> f64 {
    (FOV_Y_DEGREES.to_radians() / 2.0).tan()
}

/// «Трёхмерная сцена» → «Камера», требование 3: расстояние от камеры до середины окна по лучу
/// взгляда — такое, что клетка земли вглубь в середине окна занимает высоту окна, делённую на
/// `view_height`.
fn view_distance(pitch: f64, view_height: f64) -> f64 {
    view_height * pitch.to_radians().sin() / (2.0 * tan_half_fov())
}

/// «Трёхмерная сцена» → «Камера»: камера, смотрящая сверху под углом на землю. Земля — плоскость
/// `z = 0`, `x` сцены вправо, `y` — к игроку, `z` — вверх; камера смотрит вглубь сцены (к меньшим
/// `y`), не поворачиваясь. Ход игры и мышь считают всё здесь, в `f64`; видеокарта берёт готовую
/// матрицу `view_proj`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera3d {
    pub eye: Vec3,
    /// Точка земли в середине окна.
    pub target: Vec2,
    pub pitch: f64,
    pub view_height: f64,
    /// Окно в точках (CSS-пикселях), как у мыши и `screens.json`.
    pub viewport: [f64; 2],
}

impl Camera3d {
    /// Камера с серединой окна в `target`.
    pub fn looking_at(target: Vec2, pitch: f64, view_height: f64, viewport: [f64; 2]) -> Camera3d {
        let distance = view_distance(pitch, view_height);
        let radians = pitch.to_radians();
        Camera3d {
            eye: [
                target[0],
                target[1] + distance * radians.cos(),
                distance * radians.sin(),
            ],
            target,
            pitch,
            view_height,
            viewport,
        }
    }

    pub fn forward(&self) -> Vec3 {
        let radians = self.pitch.to_radians();
        [0.0, -radians.cos(), -radians.sin()]
    }

    pub fn up(&self) -> Vec3 {
        let radians = self.pitch.to_radians();
        [0.0, -radians.sin(), radians.cos()]
    }

    pub fn right(&self) -> Vec3 {
        [1.0, 0.0, 0.0]
    }

    /// Фокусное расстояние в точках окна.
    fn focal(&self) -> f64 {
        self.viewport[1] / 2.0 / tan_half_fov()
    }

    /// «Надписи и полоски», требование 25: клетка размеров и сдвига — высота окна, делённая на
    /// `view_height`, одна у всех объектов, ближних и дальних.
    pub fn cell_points(&self) -> f64 {
        self.viewport[1] / self.view_height
    }

    /// Точка в окне (в точках) для точки мира; `None`, если она за камерой.
    pub fn project(&self, p: Vec3) -> Option<[f64; 2]> {
        let rel = math3::sub(p, self.eye);
        let depth = math3::dot(rel, self.forward());
        if depth <= 1e-6 {
            return None;
        }
        let focal = self.focal();
        Some([
            self.viewport[0] / 2.0 + focal * math3::dot(rel, self.right()) / depth,
            self.viewport[1] / 2.0 - focal * math3::dot(rel, self.up()) / depth,
        ])
    }

    /// Единичное направление луча через точку окна (в точках).
    pub fn ray_direction(&self, window: [f64; 2]) -> Vec3 {
        let focal = self.focal();
        let dx = (window[0] - self.viewport[0] / 2.0) / focal;
        let dy = (window[1] - self.viewport[1] / 2.0) / focal;
        let direction = math3::add(
            self.forward(),
            math3::add(math3::scale(self.right(), dx), math3::scale(self.up(), -dy)),
        );
        math3::normalize(direction)
    }

    /// «Мышь», требование 21: место на земле, куда смотрит точка окна. Луч всегда идёт вниз
    /// (наклон от 30°, угол зрения 35°), так что земля есть везде; за сцену точку прижимает
    /// вызывающий.
    pub fn ground_point(&self, window: [f64; 2]) -> Vec2 {
        let direction = self.ray_direction(window);
        if direction[2] >= 0.0 {
            return self.target;
        }
        let t = -self.eye[2] / direction[2];
        [
            self.eye[0] + t * direction[0],
            self.eye[1] + t * direction[1],
        ]
    }

    /// Матрица «мир → вырез»: вид и перспектива с глубиной `0..1`.
    pub fn view_proj(&self) -> Mat4 {
        let distance = view_distance(self.pitch, self.view_height);
        let view = math3::view(self.eye, self.right(), self.up(), self.forward());
        let projection = math3::perspective(
            FOV_Y_DEGREES.to_radians(),
            self.viewport[0] / self.viewport[1].max(1.0),
            distance * 0.1,
            distance * 8.0 + 100.0,
        );
        math3::mul(&projection, &view)
    }

    /// Четыре точки земли под углами окна, по кругу от левого верхнего: дальние углы могут лежать
    /// далеко за сценой, ближние — тоже, если сцена меньше видимого.
    pub fn ground_corners(&self) -> [Vec2; 4] {
        let [w, h] = self.viewport;
        [[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]].map(|corner| self.ground_point(corner))
    }
}

/// «Камера», требование 5: середина камеры — середина объекта на земле, но так, чтобы точки земли
/// под левым и правым краем окна на его средней строке и под верхним и нижним краем на его среднем
/// столбце не выходили за сцену; если по оси сцена меньше этого отрезка, она стоит по середине отрезка.
pub fn clamp_center_3d(
    scene: &SceneConfig,
    pitch: f64,
    view_height: f64,
    viewport: [f64; 2],
    center: Vec2,
) -> Vec2 {
    let distance = view_distance(pitch, view_height);
    let radians = pitch.to_radians();
    let half_fov = FOV_Y_DEGREES.to_radians() / 2.0;
    let aspect = viewport[0] / viewport[1].max(1.0);
    let half_width = distance * aspect * tan_half_fov();
    let depth_offset =
        |angle: f64| distance * radians.cos() - distance * radians.sin() / angle.tan();
    let (top, bottom) = (
        depth_offset(radians - half_fov),
        depth_offset(radians + half_fov),
    );
    [
        clamp_axis(center[0], scene.width as f64, -half_width, half_width),
        clamp_axis(center[1], scene.height as f64, top, bottom),
    ]
}

/// Одна ось `clamp_center_3d`: видимая земля по этой оси — `[center + low, center + high]`.
fn clamp_axis(center: f64, scene_len: f64, low: f64, high: f64) -> f64 {
    if high - low >= scene_len {
        scene_len / 2.0 - (low + high) / 2.0
    } else {
        center.clamp(-low, scene_len - high)
    }
}

/// «Редактор», требование 27: неподвижная камера вне партии — наклон `pitch` игры, смотрит на
/// середину сцены с такого расстояния, чтобы вся земля поместилась в холст.
pub fn fit_scene_camera(scene: &SceneConfig, pitch: f64, viewport: [f64; 2]) -> Camera3d {
    let target = [scene.width as f64 / 2.0, scene.height as f64 / 2.0];
    let base = Camera3d::looking_at(target, pitch, 1.0, viewport);
    let (forward, up, right) = (base.forward(), base.up(), base.right());
    let tan = tan_half_fov();
    let aspect = viewport[0] / viewport[1].max(1.0);
    let mut distance = 0.0_f64;
    for corner in [
        [0.0, 0.0],
        [scene.width as f64, 0.0],
        [scene.width as f64, scene.height as f64],
        [0.0, scene.height as f64],
    ] {
        let q = [corner[0] - target[0], corner[1] - target[1], 0.0];
        let ahead = math3::dot(q, forward);
        distance = distance
            .max(math3::dot(q, right).abs() / (tan * aspect) - ahead)
            .max(math3::dot(q, up).abs() / tan - ahead);
    }
    // `looking_at` считает расстояние по `view_height`; подбираем его так, чтобы получилось `distance`.
    let view_height = distance * 2.0 * tan / pitch.to_radians().sin();
    Camera3d::looking_at(target, pitch, view_height, viewport)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::PropertyTable;

    fn scene(width: u32, height: u32, view_height: Option<f64>) -> SceneConfig {
        SceneConfig {
            width,
            height,
            background: [0.0; 4],
            view_height,
            y_sort: false,
            camera: None,
            light: Default::default(),
        }
    }

    #[test]
    fn without_view_height_frame_is_the_plain_letterbox() {
        let s = scene(10, 20, None);
        assert_eq!(
            frame(&s, [999.0, 999.0], [800.0, 600.0]),
            letterbox([800.0, 600.0], [10.0, 20.0])
        );
    }

    /// «Камера», требование 3: окно 1600×900 при view_height 12 — 75 пикселей в клетке.
    #[test]
    fn view_height_sets_the_cell_size_from_window_height() {
        let s = scene(100, 100, Some(12.0));
        let (scale, _) = frame(&s, [50.0, 50.0], [1600.0, 900.0]);
        assert!((scale - 75.0).abs() < 1e-4, "{scale}");
    }

    /// «Камера», требование 4: сцена шире видимой части — центр камеры держит середину объекта,
    /// прижатый так, чтобы видимая часть не вышла за край сцены.
    #[test]
    fn camera_follows_the_object_clamped_to_the_scenes_own_edge() {
        let s = scene(100, 100, Some(12.0));
        let viewport = [1600.0, 900.0]; // scale 75, visible width 21.333 cells
        // Дальняя от края точка — центр камеры прямо на объекте.
        let (scale, offset) = frame(&s, [50.0, 50.0], viewport);
        let cell = s.window_to_scene_frame([800.0, 450.0], scale, offset);
        assert!((cell[0] - 50.0).abs() < 1e-3, "{cell:?}");
        assert!((cell[1] - 50.0).abs() < 1e-3, "{cell:?}");
        // У самого края сцены — камера прижата ровно к краю, а не смотрит за него.
        let (scale, offset) = frame(&s, [0.0, 0.0], viewport);
        let cell = s.window_to_scene_frame([0.0, 0.0], scale, offset);
        assert!(cell[0] >= 0.0, "камера прижата к краю: {cell:?}");
        assert!(cell[1] >= 0.0, "камера прижата к краю: {cell:?}");
    }

    /// «Крайние случаи»: узкая и низкая сцена меньше видимой части — сцена по центру окна.
    #[test]
    fn a_scene_smaller_than_the_visible_area_sits_centered() {
        let s = scene(4, 4, Some(12.0));
        let viewport = [1600.0, 900.0]; // scale 75, scene is 300x300px, window is bigger
        let (scale, offset) = frame(&s, [2.0, 2.0], viewport);
        assert!((offset[0] - (1600.0 - 4.0 * scale) / 2.0).abs() < 1e-3);
        assert!((offset[1] - (900.0 - 4.0 * scale) / 2.0).abs() < 1e-3);
    }

    #[test]
    fn followed_center_picks_the_smaller_numbered_camera_object() {
        let properties = PropertyTable::new();
        let mut world = World::new(&properties);
        let a = world.create();
        world.set_vec2(a, property::POSITION, [0.0, 0.0]);
        world.set_vec2(a, property::SIZE, [2.0, 2.0]);
        world.set_flag(a, property::CAMERA_FOLLOWS, true);
        let b = world.create();
        world.set_vec2(b, property::POSITION, [10.0, 10.0]);
        world.set_vec2(b, property::SIZE, [2.0, 2.0]);
        world.set_flag(b, property::CAMERA_FOLLOWS, true);

        assert_eq!(followed_center(&world), Some([1.0, 1.0]));
    }

    #[test]
    fn followed_center_is_none_without_a_camera_object() {
        let properties = PropertyTable::new();
        let world = World::new(&properties);
        assert_eq!(followed_center(&world), None);
    }

    const WINDOW: [f64; 2] = [1920.0, 1080.0];

    fn scene_3d(width: u32, height: u32) -> SceneConfig {
        SceneConfig {
            camera: Some(crate::core::scene::CameraConfig { pitch: 55.0 }),
            ..scene(width, height, Some(12.0))
        }
    }

    /// «Трёхмерная сцена» → «Камера», требование 3: окно 1080, `view_height: 12`, `pitch: 55` —
    /// клетка земли в середине окна вглубь занимает 90 точек, поперёк около 110, камера стоит в
    /// 15,6 клетки от середины окна по лучу взгляда.
    #[test]
    fn the_example_window_gives_ninety_points_deep_and_about_a_hundred_ten_across() {
        let camera = Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, WINDOW);
        let to_eye = math3::sub(camera.eye, [16.0, 12.0, 0.0]);
        let distance = math3::dot(to_eye, to_eye).sqrt();
        assert!((distance - 15.588).abs() < 1e-3, "{distance}");

        let middle = camera.project([16.0, 12.0, 0.0]).expect("in front");
        assert!((middle[0] - 960.0).abs() < 1e-9 && (middle[1] - 540.0).abs() < 1e-9);
        let far = camera.project([16.0, 11.5, 0.0]).expect("in front");
        let near = camera.project([16.0, 12.5, 0.0]).expect("in front");
        assert!(
            ((near[1] - far[1]) - 90.0).abs() < 1.0,
            "{}",
            near[1] - far[1]
        );
        let left = camera.project([15.5, 12.0, 0.0]).expect("in front");
        let right = camera.project([16.5, 12.0, 0.0]).expect("in front");
        let across = 90.0 / 55.0_f64.to_radians().sin();
        assert!(
            ((right[0] - left[0]) - across).abs() < 1.0,
            "{}",
            right[0] - left[0]
        );
    }

    /// Требование 4: при `pitch: 90` земля видна так же, как в плоской сцене — та же клетка в
    /// точках и та же середина окна.
    #[test]
    fn at_ninety_degrees_the_ground_lands_where_the_flat_scene_puts_it() {
        let flat = scene(40, 30, Some(12.0));
        let center = [17.0, 11.0];
        let (scale, offset) = frame(&flat, center, [WINDOW[0] as f32, WINDOW[1] as f32]);
        let camera = Camera3d::looking_at(center, 90.0, 12.0, WINDOW);
        for point in [[17.0, 11.0], [12.5, 8.0], [21.25, 15.75]] {
            let flat_px = [
                offset[0] as f64 + point[0] * scale as f64,
                offset[1] as f64 + point[1] * scale as f64,
            ];
            let px = camera.project([point[0], point[1], 0.0]).expect("in front");
            assert!(
                (px[0] - flat_px[0]).abs() < 1e-3,
                "{px:?} против {flat_px:?}"
            );
            assert!(
                (px[1] - flat_px[1]).abs() < 1e-3,
                "{px:?} против {flat_px:?}"
            );
        }
    }

    #[test]
    fn a_ground_point_survives_the_trip_to_the_screen_and_back() {
        let camera = Camera3d::looking_at([10.0, 9.0], 40.0, 10.0, [1000.0, 700.0]);
        for point in [[10.0, 9.0], [6.5, 12.25], [14.0, 5.0]] {
            let px = camera.project([point[0], point[1], 0.0]).expect("in front");
            let back = camera.ground_point(px);
            assert!((back[0] - point[0]).abs() < 1e-9 && (back[1] - point[1]).abs() < 1e-9);
        }
        let straight = camera.ray_direction([500.0, 350.0]);
        assert!(
            (straight[1] - camera.forward()[1]).abs() < 1e-12,
            "{straight:?}"
        );
    }

    /// Требование 5: у каждого края земля под краем окна на средней строке или столбце не выходит
    /// за сцену; камера прижата ровно к нему.
    #[test]
    fn the_follow_camera_stops_at_every_edge_of_the_scene() {
        let scene = scene_3d(32, 24);
        let ground = |center: Vec2| {
            let center = clamp_center_3d(&scene, 55.0, 12.0, WINDOW, center);
            let camera = Camera3d::looking_at(center, 55.0, 12.0, WINDOW);
            (
                camera.ground_point([0.0, WINDOW[1] / 2.0]),
                camera.ground_point([WINDOW[0], WINDOW[1] / 2.0]),
                camera.ground_point([WINDOW[0] / 2.0, 0.0]),
                camera.ground_point([WINDOW[0] / 2.0, WINDOW[1]]),
            )
        };
        let (left, _, _, _) = ground([0.0, 12.0]);
        assert!(left[0].abs() < 1e-9, "{left:?}");
        let (_, right, _, _) = ground([99.0, 12.0]);
        assert!((right[0] - 32.0).abs() < 1e-9, "{right:?}");
        let (_, _, top, _) = ground([16.0, -50.0]);
        assert!(top[1].abs() < 1e-9, "{top:?}");
        let (_, _, _, bottom) = ground([16.0, 99.0]);
        assert!((bottom[1] - 24.0).abs() < 1e-9, "{bottom:?}");
        // Далеко от краёв камера просто на объекте.
        let free = clamp_center_3d(&scene, 55.0, 12.0, WINDOW, [16.0, 12.0]);
        assert_eq!(free, [16.0, 12.0]);
    }

    /// Требование 5: сцена меньше видимого отрезка — по его середине.
    #[test]
    fn a_scene_smaller_than_the_visible_stretch_stands_in_the_middle_of_it() {
        let scene = scene_3d(4, 4);
        let center = clamp_center_3d(&scene, 55.0, 12.0, WINDOW, [1.0, 1.0]);
        let camera = Camera3d::looking_at(center, 55.0, 12.0, WINDOW);
        let left = camera.ground_point([0.0, WINDOW[1] / 2.0]);
        let right = camera.ground_point([WINDOW[0], WINDOW[1] / 2.0]);
        let top = camera.ground_point([WINDOW[0] / 2.0, 0.0]);
        let bottom = camera.ground_point([WINDOW[0] / 2.0, WINDOW[1]]);
        assert!(((left[0] + right[0]) / 2.0 - 2.0).abs() < 1e-9);
        assert!(((top[1] + bottom[1]) / 2.0 - 2.0).abs() < 1e-9);
    }

    /// «Крайние случаи»: при `pitch: 30` верхний луч идёт к земле под 12,5°.
    #[test]
    fn the_top_ray_at_thirty_degrees_reaches_the_ground_at_twelve_and_a_half() {
        let camera = Camera3d::looking_at([20.0, 20.0], 30.0, 12.0, WINDOW);
        let direction = camera.ray_direction([WINDOW[0] / 2.0, 0.0]);
        let slope = (-direction[2]).asin().to_degrees();
        assert!((slope - 12.5).abs() < 1e-9, "{slope}");
        assert!(camera.ground_point([WINDOW[0] / 2.0, 0.0])[1] < 20.0);
    }

    /// «Редактор», требование 27: вся земля помещается в холст любой формы, а дальше уже нельзя.
    #[test]
    fn the_editor_camera_fits_the_whole_ground_and_touches_the_canvas() {
        for viewport in [[1200.0, 700.0], [500.0, 900.0]] {
            let scene = scene_3d(32, 24);
            let camera = fit_scene_camera(&scene, 55.0, viewport);
            let mut extreme = 0.0_f64;
            for corner in [[0.0, 0.0], [32.0, 0.0], [32.0, 24.0], [0.0, 24.0]] {
                let px = camera
                    .project([corner[0], corner[1], 0.0])
                    .expect("in front");
                assert!(px[0] >= -1e-6 && px[0] <= viewport[0] + 1e-6, "{px:?}");
                assert!(px[1] >= -1e-6 && px[1] <= viewport[1] + 1e-6, "{px:?}");
                extreme = extreme
                    .max((px[0] - viewport[0] / 2.0).abs() / (viewport[0] / 2.0))
                    .max((px[1] - viewport[1] / 2.0).abs() / (viewport[1] / 2.0));
            }
            assert!((extreme - 1.0).abs() < 1e-6, "{extreme}");
            let middle = camera.project([16.0, 12.0, 0.0]).expect("in front");
            assert!((middle[0] - viewport[0] / 2.0).abs() < 1e-9);
        }
    }
}

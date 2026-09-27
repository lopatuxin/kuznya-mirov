//! «Камера», требования 4–7: масштаб и центр камеры по `SceneConfig`, размеру окна и точке
//! объекта камеры. Считает только ядро — правила и код игры камеры не видят, интерфейс экранов
//! от неё не зависит.

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
}

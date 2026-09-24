use super::property;
use super::world::World;

#[derive(Debug, Clone, Copy)]
pub struct SceneConfig {
    pub width: u32,
    pub height: u32,
    pub background: [f32; 4],
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
    /// «Курсор в мире», требование 25: translates a window-pixel cursor position into scene
    /// coordinates, by the same letterboxed mapping the renderer draws the scene with, then
    /// clamps to the scene's own edges.
    pub fn window_to_scene(&self, window_pos: [f32; 2], viewport: [f32; 2]) -> super::value::Vec2 {
        let scene_cells = [self.width as f32, self.height as f32];
        if viewport[0] <= 0.0 || viewport[1] <= 0.0 {
            return [0.0, 0.0];
        }
        let (scale, offset) = letterbox(viewport, scene_cells);
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
/// same way the renderer draws the world (`render::gpu::world_globals_data`). `None` when the
/// object doesn't exist (id past the world's slot count) or lacks `position`/`size`. Rotation
/// never changes this rectangle — a rotated image stretches to fill the same one.
pub fn object_rect(
    world: &World,
    scene: &SceneConfig,
    id: u32,
    viewport: [f32; 2],
) -> Option<CanvasRect> {
    if id as usize >= world.slot_count() {
        return None;
    }
    let position = world.vec2(id, property::POSITION)?;
    let size = world.vec2(id, property::SIZE)?;
    let scene_cells = [scene.width as f32, scene.height as f32];
    let (scale, offset) = letterbox(viewport, scene_cells);
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
/// own `position` property.
pub fn move_object(world: &mut World, id: u32, position: super::value::Vec2) {
    if id as usize >= world.slot_count() || !world.has(id, property::POSITION) {
        return;
    }
    world.set_vec2(id, property::POSITION, position);
}

/// «Редактор», требование 18: the topmost object (largest `layer`, ties broken by the larger
/// object number) whose canvas rectangle contains `point` (CSS pixels, canvas top-left origin) —
/// left/top edges included, right/bottom excluded. A candidate needs `position`, `size` and a
/// `color` or `image` fill — the same shape `render::atlas::compose_world_paints` draws; an
/// object with neither is invisible and a click never picks it. `None` off every object, or in
/// the margin around the letterboxed scene.
pub fn object_at(
    world: &World,
    scene: &SceneConfig,
    point: [f32; 2],
    viewport: [f32; 2],
) -> Option<u32> {
    let mut best: Option<(i32, u32)> = None;
    for id in world.ids() {
        let has_fill = world.color(id, property::COLOR).is_some()
            || world.image(id, property::IMAGE).is_some();
        if !has_fill {
            continue;
        }
        let Some(rect) = object_rect(world, scene, id, viewport) else {
            continue;
        };
        if point[0] < rect.x
            || point[1] < rect.y
            || point[0] >= rect.x + rect.width
            || point[1] >= rect.y + rect.height
        {
            continue;
        }
        let layer = world.layer(id, property::LAYER).unwrap_or(0);
        let replace = match best {
            None => true,
            Some((best_layer, _)) => layer >= best_layer,
        };
        if replace {
            best = Some((layer, id));
        }
    }
    best.map(|(_, id)| id)
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
        };
        let moved = world.create();
        world.set_vec2(moved, property::POSITION, [1.0, 2.0]);
        world.set_vec2(moved, property::SIZE, [1.0, 1.0]);
        world.set_color(moved, property::COLOR, [1.0, 0.0, 0.0, 1.0]);
        let other = world.create();
        world.set_vec2(other, property::POSITION, [4.0, 4.0]);

        move_object(&mut world, moved, [5.37, 7.0]);

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

        move_object(&mut world, bare, [5.0, 5.0]);
        assert_eq!(world.vec2(bare, property::POSITION), None);

        move_object(&mut world, 99, [5.0, 5.0]);
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
}

/// One `scene.json` object, kept around after load so «Экраны и состояние»'s `new_game` can
/// rebuild the world from this parsed copy without reopening the file a second time.
#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub values: Vec<(super::property::PropertyId, super::value::Value)>,
    pub grid: Option<super::value::GridSpec>,
    pub keys: Option<super::keys::KeyTable>,
}

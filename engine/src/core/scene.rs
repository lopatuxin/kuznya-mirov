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

#[cfg(test)]
mod tests {
    use super::*;

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
}

/// One `scene.json` object, kept around after load so «Экраны и состояние»'s `new_game` can
/// rebuild the world from this parsed copy without reopening the file a second time.
#[derive(Debug, Clone)]
pub struct ObjectSpec {
    pub values: Vec<(super::property::PropertyId, super::value::Value)>,
    pub grid: Option<super::value::GridSpec>,
    pub keys: Option<super::keys::KeyTable>,
}

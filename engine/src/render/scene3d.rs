//! «Трёхмерная сцена» → «Отрисовка»: что рисовать в кадре трёхмерной сцены — список фигур, список
//! плоских прямоугольников на земле (земля из плиток и плоские объекты, по `layer`), матрицы камеры
//! и солнца. Ни видеокарты, ни браузера: `wasm::mod` переводит это в данные для `render::gpu`.

use crate::core::camera::Camera3d;
use crate::core::game::Game;
use crate::core::math3::{self, Mat4, Vec3};
use crate::core::property;
use crate::core::scene::SceneConfig;
use crate::core::shapes::Body;
use crate::core::value::Shape;
use crate::data::load::ImageDecl;

use super::atlas::{self, AtlasRect, RectPaint};

/// Одна фигура кадра: место, размеры, поворот и цвет — всё, что вершинному шейдеру нужно поверх
/// сетки единичной фигуры.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeDraw {
    pub shape: Shape,
    pub center: [f32; 2],
    pub size: [f32; 2],
    pub height: f32,
    pub cap_height: f32,
    pub cos: f32,
    pub sin: f32,
    pub color: [f32; 3],
}

/// Всё для видеокарты в одном кадре трёхмерной сцены.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame3d {
    pub shapes: Vec<ShapeDraw>,
    /// Плитки земли, затем плоские объекты в порядке рисования.
    pub ground: Vec<RectPaint>,
    pub view_proj: Mat4,
    pub light_view_proj: Mat4,
    /// Направление от земли к солнцу и синус его высоты.
    pub sun: [f32; 4],
    /// Насколько темна тень, и единица глубины карты теней на клетку сцены.
    pub shadow: f32,
    pub depth_per_cell: f32,
}

fn shape_draws(game: &Game) -> Vec<ShapeDraw> {
    game.world
        .ids()
        .filter_map(|id| {
            let body = Body::of_object(&game.world, id)?;
            let color = game.world.color(id, property::COLOR)?;
            let solid = body.size[0] > 0.0 && body.size[1] > 0.0 && body.height > 0.0;
            solid.then(|| ShapeDraw {
                shape: body.shape,
                center: [body.center[0] as f32, body.center[1] as f32],
                size: [body.size[0] as f32, body.size[1] as f32],
                height: body.height as f32,
                cap_height: body.cap_height() as f32,
                cos: body.cos as f32,
                sin: body.sin as f32,
                color: [color[0], color[1], color[2]],
            })
        })
        .collect()
}

/// Клетки сцены, которых касается видимая земля; пустой диапазон, если из сцены не видно ничего.
fn visible_cells(scene: &SceneConfig, camera: &Camera3d) -> crate::core::scene::CellRange {
    match camera.visible_ground(scene) {
        Some([low, high]) => scene.cell_range_covering(low, high),
        None => crate::core::scene::CellRange {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
        },
    }
}

/// «Свет и тени»: прямоугольная проекция от солнца, охватывающая видимую землю (в пределах сцены)
/// и всё, что над ней поднимают фигуры. Возвращает матрицу и глубину, которую она покрывает.
fn sun_projection(scene: &SceneConfig, camera: &Camera3d, tallest: f64) -> (Mat4, f64) {
    let toward_sun = scene.light.direction();
    let forward = math3::scale(toward_sun, -1.0);
    let right = math3::normalize(math3::cross(forward, [0.0, -1.0, 0.0]));
    let up = math3::cross(right, forward);

    let range = visible_cells(scene, camera);
    let corners = [
        [range.x0 as f64, range.y0 as f64],
        [range.x1 as f64, range.y0 as f64],
        [range.x1 as f64, range.y1 as f64],
        [range.x0 as f64, range.y1 as f64],
    ];
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    // Вершина самой высокой фигуры, чья тень падает на угол, — на луче от угла к солнцу: сдвиг вдоль
    // него не меняет положения в плоскости света, но приближает к солнцу.
    let reach = tallest / toward_sun[2];
    for corner in corners {
        for lift in [0.0, reach] {
            let ground: Vec3 = [corner[0], corner[1], 0.0];
            let point = math3::add(ground, math3::scale(toward_sun, lift));
            let light_space = [
                math3::dot(point, right),
                math3::dot(point, up),
                math3::dot(point, forward),
            ];
            for axis in 0..3 {
                low[axis] = low[axis].min(light_space[axis]);
                high[axis] = high[axis].max(light_space[axis]);
            }
        }
    }
    const MARGIN: f64 = 0.5;
    let (near, far) = (low[2] - 1.0, high[2] + 1.0);
    let view = math3::view([0.0, 0.0, 0.0], right, up, forward);
    let projection = math3::ortho(
        low[0] - MARGIN,
        high[0] + MARGIN,
        low[1] - MARGIN,
        high[1] + MARGIN,
        near,
        far,
    );
    (math3::mul(&projection, &view), far - near)
}

/// Кадр трёхмерной сцены по миру `game` и камере `camera`. `elapsed_steps` — шаги партии, по ним
/// выбирается кадр анимации картинок, как у плоской сцены.
pub fn compose_frame3d(
    game: &Game,
    camera: &Camera3d,
    elapsed_steps: f64,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Frame3d {
    let shapes = shape_draws(game);
    let tallest = shapes.iter().map(|s| s.height as f64).fold(0.0, f64::max);

    let mut ground = atlas::compose_ground_paints(
        &game.ground,
        visible_cells(&game.scene, camera),
        images,
        atlas_rects,
    );
    ground.extend(atlas::compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        elapsed_steps,
        images,
        atlas_rects,
    ));

    let toward_sun = game.scene.light.direction();
    let (light_view_proj, depth) = sun_projection(&game.scene, camera, tallest);
    Frame3d {
        shapes,
        ground,
        view_proj: camera.view_proj(),
        light_view_proj,
        sun: [
            toward_sun[0] as f32,
            toward_sun[1] as f32,
            toward_sun[2] as f32,
            toward_sun[2] as f32,
        ],
        shadow: game.scene.light.shadow as f32,
        depth_per_cell: (1.0 / depth.max(1e-6)) as f32,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::scene::{CameraConfig, LightConfig};

    fn scene() -> SceneConfig {
        SceneConfig {
            width: 32,
            height: 24,
            background: [0.0; 4],
            view_height: Some(12.0),
            y_sort: false,
            camera: Some(CameraConfig { pitch: 55.0 }),
            light: LightConfig::default(),
        }
    }

    /// «Свет и тени»: вершина самой высокой фигуры, чья тень падает на видимую землю, лежит внутри
    /// проекции солнца — иначе такая фигура не отбросила бы тени.
    #[test]
    fn the_sun_projection_holds_every_caster_whose_shadow_falls_on_the_visible_ground() {
        let scene = scene();
        let camera = Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, [1920.0, 1080.0]);
        let tallest = 4.2;
        let (matrix, depth) = sun_projection(&scene, &camera, tallest);
        assert!(depth > 0.0);
        let toward_sun = scene.light.direction();
        let range = visible_cells(&scene, &camera);
        for ground_x in [
            range.x0 as f64 + 0.5,
            (range.x0 + range.x1) as f64 / 2.0,
            range.x1 as f64 - 0.5,
        ] {
            for ground_y in [
                range.y0 as f64 + 0.5,
                (range.y0 + range.y1) as f64 / 2.0,
                range.y1 as f64 - 0.5,
            ] {
                for height in [0.0, tallest / 2.0, tallest] {
                    let lift = height / toward_sun[2];
                    let point =
                        math3::add([ground_x, ground_y, 0.0], math3::scale(toward_sun, lift));
                    let clip = math3::transform_point(&matrix, point);
                    let (x, y, z) = (clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]);
                    assert!(x.abs() <= 1.0 && y.abs() <= 1.0, "{point:?} → {x} {y}");
                    assert!((0.0..=1.0).contains(&z), "{point:?} → глубина {z}");
                }
            }
        }
    }
}

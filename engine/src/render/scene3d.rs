//! «Трёхмерная сцена» → «Отрисовка»: что рисовать в кадре трёхмерной сцены — список фигур, плитки
//! земли и плоские объекты, разложенные по рельефу треугольниками, матрицы камеры и солнца. Ни
//! видеокарты, ни браузера: `wasm::mod` переводит это в данные для `render::gpu`.

use crate::core::camera::{Camera3d, FOV_Y_DEGREES};
use crate::core::game::Game;
use crate::core::math3::{self, Mat4, Vec3};
use crate::core::property;
use crate::core::scene::{CellRange, LayerView, LightConfig, SceneConfig};
use crate::core::shapes::Body;
use crate::core::value::Shape;
use crate::data::load::ImageDecl;

use super::atlas::{self, AtlasRect, RectPaint};
use super::relief::{self, SurfaceVertex};
use super::wind::Motion;

/// Одна фигура кадра: место, размеры, поворот и цвет — всё, что вершинному шейдеру нужно поверх
/// сетки единичной фигуры.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeDraw {
    pub shape: Shape,
    pub center: [f32; 2],
    /// Высота основания над нулём сцены.
    pub base: f32,
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
    /// Те же плитки и плоские объекты треугольниками на рельефе и на верху настилов.
    pub surface: Vec<SurfaceVertex>,
    pub view_proj: Mat4,
    pub light_view_proj: Mat4,
    /// Направление от земли к солнцу и синус его высоты.
    pub sun: [f32; 4],
    /// Насколько темна тень, и единица глубины карты теней на клетку сцены.
    pub shadow: f32,
    pub depth_per_cell: f32,
    /// Место камеры: от него считается блик.
    pub eye: [f32; 3],
    /// Цвет и сила солнца и неба в линейной яркости.
    pub sun_light: [f32; 3],
    pub sky_light: [f32; 3],
}

/// Оттенок цвета данных `srgb`: цвет в линейной яркости, поделённый на свою яркость (Rec. 709), так что
/// яркость оттенка — единица. Чёрный цвет оттенка не имеет: источник не светит.
fn light_tint(srgb: [f32; 3]) -> [f64; 3] {
    let linear = srgb.map(|c| {
        let c = f64::from(c);
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    });
    let luminance = 0.2126 * linear[0] + 0.7152 * linear[1] + 0.0722 * linear[2];
    if luminance <= 0.0 {
        return [0.0; 3];
    }
    linear.map(|c| c / luminance)
}

/// Свет солнца и неба в линейной яркости: небо светит силой `1 − shadow`, солнце — `shadow / sin` высоты
/// солнца. Так ровная земля на солнце получает свет 1, а в тени — `1 − shadow`.
pub fn light_colors(light: &LightConfig) -> ([f32; 3], [f32; 3]) {
    let sun_strength = light.shadow / light.direction()[2];
    let sky_strength = 1.0 - light.shadow;
    let sun = light_tint(light.sun_color).map(|c| (c * sun_strength) as f32);
    let sky = light_tint(light.sky_color).map(|c| (c * sky_strength) as f32);
    (sun, sky)
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
                base: body.base as f32,
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

/// Прямоугольник по осям `[низ, верх]`, охватывающий видимую камерой землю сцены на любой высоте от
/// `low` до `high`; `None`, если ничего не видно. Видимое — выпуклый многогранник: сцена на этих
/// высотах, обрезанная четырьмя боковыми плоскостями взгляда; охват берётся по его вершинам, то
/// есть по тройкам плоскостей. Срезы на краях диапазона его не охватывают: у камеры ниже холма
/// срез на высоте холма пуст, а ближе всего к камере видна земля посередине.
fn visible_bounds(
    scene: &SceneConfig,
    camera: &Camera3d,
    low: f64,
    high: f64,
) -> Option<[[f64; 2]; 2]> {
    let tan_y = (FOV_Y_DEGREES.to_radians() / 2.0).tan();
    let tan_x = tan_y * camera.viewport[0] / camera.viewport[1].max(1.0);
    let (forward, up, right) = (camera.forward(), camera.up(), camera.right());
    let inward = |axis: Vec3, sign: f64, tan: f64| {
        math3::add(math3::scale(axis, sign), math3::scale(forward, tan))
    };
    let mut planes: Vec<(Vec3, f64)> = [
        inward(right, 1.0, tan_x),
        inward(right, -1.0, tan_x),
        inward(up, 1.0, tan_y),
        inward(up, -1.0, tan_y),
    ]
    .into_iter()
    .map(|normal| (normal, -math3::dot(normal, camera.eye)))
    .collect();
    planes.extend([
        ([1.0, 0.0, 0.0], 0.0),
        ([-1.0, 0.0, 0.0], f64::from(scene.width)),
        ([0.0, 1.0, 0.0], 0.0),
        ([0.0, -1.0, 0.0], f64::from(scene.height)),
        ([0.0, 0.0, 1.0], -low),
        ([0.0, 0.0, -1.0], high),
    ]);

    const TOLERANCE: f64 = 1e-7;
    let mut bounds: Option<[[f64; 2]; 2]> = None;
    for i in 0..planes.len() {
        for j in i + 1..planes.len() {
            for k in j + 1..planes.len() {
                let ((a, da), (b, db), (c, dc)) = (planes[i], planes[j], planes[k]);
                let det = math3::dot(a, math3::cross(b, c));
                if det.abs() < 1e-9 {
                    continue;
                }
                let sum = math3::add(
                    math3::add(
                        math3::scale(math3::cross(b, c), -da),
                        math3::scale(math3::cross(c, a), -db),
                    ),
                    math3::scale(math3::cross(a, b), -dc),
                );
                let vertex = math3::scale(sum, 1.0 / det);
                if planes
                    .iter()
                    .any(|&(normal, offset)| math3::dot(normal, vertex) + offset < -TOLERANCE)
                {
                    continue;
                }
                let [lo, hi] = bounds.unwrap_or([[vertex[0], vertex[1]]; 2]);
                bounds = Some([
                    [lo[0].min(vertex[0]), lo[1].min(vertex[1])],
                    [hi[0].max(vertex[0]), hi[1].max(vertex[1])],
                ]);
            }
        }
    }
    bounds
}

/// Клетки сцены, которых касается видимая земля на любой высоте от `low` до `high`; пустой
/// диапазон, если из сцены не видно ничего.
fn visible_cells(scene: &SceneConfig, camera: &Camera3d, low: f64, high: f64) -> CellRange {
    match visible_bounds(scene, camera, low, high) {
        Some([floor, ceiling]) => scene.cell_range_covering(floor, ceiling),
        None => CellRange {
            x0: 0,
            y0: 0,
            x1: 0,
            y1: 0,
        },
    }
}

/// «Свет и тени»: прямоугольная проекция от солнца, охватывающая видимую землю (в пределах сцены)
/// с высотами от `ground.0` до `ground.1` и всё, что над ней поднимает самая высокая точка мира
/// `top` — фигуры и холмы. Возвращает матрицу и глубину, которую она покрывает.
fn sun_projection(
    scene: &SceneConfig,
    camera: &Camera3d,
    ground: (f64, f64),
    top: f64,
) -> (Mat4, f64) {
    let toward_sun = scene.light.direction();
    let forward = math3::scale(toward_sun, -1.0);
    let right = math3::normalize(math3::cross(forward, [0.0, -1.0, 0.0]));
    let up = math3::cross(right, forward);

    let (floor, ceiling) = (ground.0, ground.1.max(top));
    let range = visible_cells(scene, camera, floor, ceiling);
    let corners = [
        [range.x0 as f64, range.y0 as f64],
        [range.x1 as f64, range.y0 as f64],
        [range.x1 as f64, range.y1 as f64],
        [range.x0 as f64, range.y1 as f64],
    ];
    let mut low = [f64::INFINITY; 3];
    let mut high = [f64::NEG_INFINITY; 3];
    // Вершина самого высокого тела, чья тень падает на угол, — на луче от угла к солнцу: сдвиг вдоль
    // него не меняет положения в плоскости света, но приближает к солнцу.
    for corner in corners {
        for level in [floor, ceiling] {
            let reach = (top - level).max(0.0) / toward_sun[2];
            for lift in [0.0, reach] {
                let point = math3::add(
                    [corner[0], corner[1], level],
                    math3::scale(toward_sun, lift),
                );
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

/// Кадр трёхмерной сцены по миру `game` и камере `camera`. `motion` — часы движения, по ним
/// выбирается кадр анимации картинок, как у плоской сцены.
pub fn compose_frame3d(
    game: &Game,
    camera: &Camera3d,
    motion: &Motion,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Frame3d {
    let shapes = shape_draws(game);
    let terrain = game.world.terrain();
    let (land_low, land_high) = terrain.height_range();
    let level = terrain.water().map(|water| water.level);
    let ground_low = level.map_or(land_low, |level| land_low.min(level));
    let ground_high = level.map_or(land_high, |level| land_high.max(level));
    let top = shapes
        .iter()
        .map(|s| f64::from(s.base + s.height))
        .fold(ground_high, f64::max);

    let mut ground = atlas::compose_ground_paints(
        &game.ground,
        visible_cells(&game.scene, camera, land_low, land_high),
        images,
        atlas_rects,
    );
    ground.extend(atlas::compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        motion,
        images,
        atlas_rects,
        &LayerView::default(),
    ));
    let surface = relief::surface_triangles(&game.world, &ground);

    let toward_sun = game.scene.light.direction();
    let (light_view_proj, depth) =
        sun_projection(&game.scene, camera, (ground_low, ground_high), top);
    let (sun_light, sky_light) = light_colors(&game.scene.light);
    Frame3d {
        shapes,
        ground,
        surface,
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
        eye: camera.eye.map(|c| c as f32),
        sun_light,
        sky_light,
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
            cell_pixels: None,
        }
    }

    /// «Свет и тени»: вершина самой высокой фигуры, чья тень падает на видимую землю, лежит внутри
    /// проекции солнца — иначе такая фигура не отбросила бы тени.
    #[test]
    fn the_sun_projection_holds_every_caster_whose_shadow_falls_on_the_visible_ground() {
        let scene = scene();
        let camera = Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, [1920.0, 1080.0]);
        let tallest = 4.2;
        let (matrix, depth) = sun_projection(&scene, &camera, (0.0, 0.0), tallest);
        assert!(depth > 0.0);
        let toward_sun = scene.light.direction();
        let range = visible_cells(&scene, &camera, 0.0, 0.0);
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

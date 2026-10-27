//! «Трёхмерная сцена» → «Отрисовка»: что рисовать в кадре трёхмерной сцены — список фигур, плитки
//! земли и плоские объекты, разложенные по рельефу треугольниками, матрицы камеры и солнца. Ни
//! видеокарты, ни браузера: `wasm::mod` переводит это в данные для `render::gpu`.

use std::rc::Rc;

use crate::core::camera::{Camera3d, FOV_Y_DEGREES};
use crate::core::game::Game;
use crate::core::math3::{self, Mat4, Vec3};
use crate::core::property;
use crate::core::road_kinds::Junction;
use crate::core::scene::{CellRange, LayerView, LightConfig, SceneConfig};
use crate::core::shapes::Body;
use crate::core::value::Shape;
use crate::core::water::{DEFAULT_CLARITY, DEFAULT_RIPPLE};
use crate::data::load::ImageDecl;

use super::atlas::{self, AtlasRect, RectPaint};
use super::models::{self, BlendedDraw, ModelInstance};
use super::relief::{self, FlatShapes, SurfaceVertex};
use super::wind::Motion;

/// Сторона квадратной карты теней в точках — предел `downlevel_webgl2_defaults`.
pub const SHADOW_MAP_SIZE: u32 = 2048;

/// «Свет и тени»: на сколько точек карты теней точка сдвигается вдоль нормали, когда спрашивает тень,
/// и меньше скольких клеток этот сдвиг не бывает. Выборка тени сравнивает глубину с четырьмя
/// соседними точками карты — до точки карты по каждой оси, до 1,4 вкось, — а земля, повёрнутая от
/// солнца на угол θ, уходит между ними вглубь на tg θ на клетку. Сдвиг на d вдоль нормали отодвигает
/// точку от земли на d / cos θ по лучу солнца, и полтора размера точки карты перекрывают этот уход
/// при любом θ.
/// Иначе земля затеняет сама себя мелкой сеткой, которая на экране сливается в кольца, как только
/// камера отдалена и одна точка карты покрывает заметную долю клетки.
const SHADOW_OFFSET_TEXELS: f64 = 1.5;
const MIN_SHADOW_OFFSET: f64 = 0.02;

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
    /// Объекты каждой модели игры по номеру модели.
    pub models: Vec<Vec<ModelInstance>>,
    /// Матрицы суставов объектов каждой модели со скелетом → `models::ModelDraws::bones`.
    pub model_bones: Vec<Vec<f32>>,
    /// «Вид дороги»: куски дорог по номеру модели и перекрёстки; рисуются после моделей и до плоских объектов.
    pub road_pieces: Vec<Vec<ModelInstance>>,
    pub road_junctions: Rc<Vec<Junction>>,
    pub road_bent: Rc<Vec<Junction>>,
    /// «Свет и материалы», прозрачность: объекты с частью `BLEND` от дальнего к ближнему.
    pub blended: Vec<BlendedDraw>,
    /// Плитки земли, затем плоские объекты в порядке рисования.
    pub ground: Vec<RectPaint>,
    /// Те же плитки и плоские объекты треугольниками на рельефе и на верху настилов.
    pub surface: Vec<SurfaceVertex>,
    pub view_proj: Mat4,
    pub light_view_proj: Mat4,
    /// Направление от земли к солнцу и синус его высоты.
    pub sun: [f32; 4],
    /// Насколько темна тень, единица глубины карты теней на клетку сцены и сдвиг точки вдоль нормали
    /// при выборке тени, в клетках.
    pub shadow: f32,
    pub depth_per_cell: f32,
    pub shadow_offset: f32,
    /// Место камеры: от него считается блик.
    pub eye: [f32; 3],
    /// Цвет и сила солнца и неба в линейной яркости.
    pub sun_light: [f32; 3],
    pub sky_light: [f32; 3],
    /// «Свет и материалы», «Вода»: на какой глубине дна уже не видно, длина ряби в клетках и часы воды в
    /// секундах — те же, по которым идут кадры по времени.
    pub water: [f32; 3],
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
            let (sin, cos) = models::view_rotation(&game.world, id).sin_cos();
            solid.then(|| ShapeDraw {
                shape: body.shape,
                center: [body.center[0] as f32, body.center[1] as f32],
                base: body.base as f32,
                size: [body.size[0] as f32, body.size[1] as f32],
                height: body.height as f32,
                cap_height: body.cap_height() as f32,
                cos: cos as f32,
                sin: sin as f32,
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
/// `top` — фигуры и холмы. Возвращает матрицу, глубину, которую она покрывает, и сколько клеток
/// приходится на точку карты теней по большей стороне.
fn sun_projection(
    scene: &SceneConfig,
    camera: &Camera3d,
    ground: (f64, f64),
    top: f64,
) -> (Mat4, f64, f64) {
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
    let side = (high[0] - low[0]).max(high[1] - low[1]) + 2.0 * MARGIN;
    (
        math3::mul(&projection, &view),
        far - near,
        side / f64::from(SHADOW_MAP_SIZE),
    )
}

/// Сдвиг точки вдоль нормали при выборке тени, в клетках, когда точка карты теней покрывает `texel`
/// клеток.
fn shadow_offset(texel: f64) -> f64 {
    (SHADOW_OFFSET_TEXELS * texel).max(MIN_SHADOW_OFFSET)
}

/// Кадр трёхмерной сцены по миру `game` и камере `camera`. `motion` — часы движения, по ним
/// выбирается кадр анимации картинок, как у плоской сцены. `flat_shapes` хранит готовые формы плоских
/// объектов от кадра к кадру.
pub fn compose_frame3d(
    game: &Game,
    camera: &Camera3d,
    motion: &Motion,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
    flat_shapes: &mut FlatShapes,
) -> Frame3d {
    let shapes = shape_draws(game);
    let terrain = game.world.terrain();
    let (land_low, land_high) = terrain.height_range();
    let (ground_low, ground_high) = terrain
        .water_range()
        .map_or((land_low, land_high), |(low, high)| {
            (land_low.min(low), land_high.max(high))
        });
    let (clarity, ripple) = terrain
        .water()
        .map_or((DEFAULT_CLARITY, DEFAULT_RIPPLE), |water| {
            (water.clarity(), water.ripple())
        });
    let model_draws = models::model_draws(game);
    let road_draws = models::road_draws(game);
    let blended = model_draws.blended_far_to_near(camera.eye);
    let top = shapes
        .iter()
        .map(|s| f64::from(s.base + s.height))
        .fold(ground_high, f64::max)
        .max(model_draws.top);

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
    let surface = relief::surface_triangles(&game.world, &ground, flat_shapes);

    let toward_sun = game.scene.light.direction();
    let (light_view_proj, depth, texel) =
        sun_projection(&game.scene, camera, (ground_low, ground_high), top);
    let (sun_light, sky_light) = light_colors(&game.scene.light);
    Frame3d {
        shapes,
        models: model_draws.instances,
        model_bones: model_draws.bones,
        road_pieces: road_draws.pieces,
        road_junctions: road_draws.junctions,
        road_bent: road_draws.bent,
        blended,
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
        shadow_offset: shadow_offset(texel) as f32,
        eye: camera.eye.map(|c| c as f32),
        sun_light,
        sky_light,
        water: [
            clarity as f32,
            ripple as f32,
            (motion.clock_steps() / 60.0) as f32,
        ],
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
            player_camera: None,
        }
    }

    /// «Свет и тени»: вершина самой высокой фигуры, чья тень падает на видимую землю, лежит внутри
    /// проекции солнца — иначе такая фигура не отбросила бы тени.
    #[test]
    fn the_sun_projection_holds_every_caster_whose_shadow_falls_on_the_visible_ground() {
        let scene = scene();
        let camera = Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, [1920.0, 1080.0]);
        let tallest = 4.2;
        let (matrix, depth, _) = sun_projection(&scene, &camera, (0.0, 0.0), tallest);
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

    /// «Свет и тени»: земля не затеняет сама себя ни вблизи, ни когда камера видит всю большую сцену.
    /// Выборка тени сравнивает глубину точки, сдвинутой вдоль нормали, с четырьмя соседними точками
    /// карты теней; земля под каждой из них не ближе к солнцу — на ровном месте и на склонах в 40° в
    /// любую сторону, даже без сдвига глубины, который шейдер добавляет сверху.
    #[test]
    fn the_ground_does_not_shadow_itself_however_far_the_camera_is() {
        let mut large = scene();
        large.width = 400;
        large.height = 400;
        large.light = LightConfig {
            sun_from: 35.0,
            sun_height: 48.0,
            ..LightConfig::default()
        };
        let cases = [
            (
                scene(),
                Camera3d::looking_at([16.0, 12.0], 55.0, 12.0, [1920.0, 1080.0]),
            ),
            (
                large,
                Camera3d::looking_at([200.0, 200.0], 55.0, 400.0, [1280.0, 800.0]),
            ),
        ];
        let tilt = 40f64.to_radians();
        let size = f64::from(SHADOW_MAP_SIZE);
        const STEPS: u32 = 20;
        for (scene, camera) in cases {
            // Склоны на четыре стороны, к солнцу и от него — самый скользящий случай.
            let toward_sun = scene.light.direction();
            let sun_azimuth = toward_sun[1].atan2(toward_sun[0]);
            let mut normals = vec![[0.0, 0.0, 1.0]];
            for azimuth in [0.0, 0.5, 1.0, 1.5]
                .map(|turn| turn * std::f64::consts::PI)
                .into_iter()
                .chain([sun_azimuth, sun_azimuth + std::f64::consts::PI])
            {
                normals.push([
                    tilt.sin() * azimuth.cos(),
                    tilt.sin() * azimuth.sin(),
                    tilt.cos(),
                ]);
            }
            let (matrix, _, texel) = sun_projection(&scene, &camera, (0.0, 6.5), 6.5);
            let offset = shadow_offset(texel);
            let ndc = |point: Vec3| {
                let clip = math3::transform_point(&matrix, point);
                [clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]]
            };
            let range = visible_cells(&scene, &camera, 0.0, 6.5);
            for &normal in &normals {
                let along = math3::normalize(math3::cross([0.0, 1.0, 0.0], normal));
                let across = math3::cross(normal, along);
                for (i, j) in (1..STEPS).flat_map(|i| (1..STEPS).map(move |j| (i, j))) {
                    let ground = [
                        range.x0 as f64
                            + (range.x1 - range.x0) as f64 * f64::from(i) / f64::from(STEPS),
                        range.y0 as f64
                            + (range.y1 - range.y0) as f64 * f64::from(j) / f64::from(STEPS),
                        3.0,
                    ];
                    let moved = ndc(math3::add(ground, math3::scale(normal, offset)));
                    let uv = [moved[0] * 0.5 + 0.5, 0.5 - moved[1] * 0.5];
                    // Глубина земли в середине точки карты: проекция плоская, так что место на карте
                    // и глубина меняются вдоль склона на постоянные доли.
                    let origin = ndc(ground);
                    let step = |axis: Vec3| {
                        let shifted = ndc(math3::add(ground, axis));
                        [
                            shifted[0] - origin[0],
                            shifted[1] - origin[1],
                            shifted[2] - origin[2],
                        ]
                    };
                    let (a, b) = (step(along), step(across));
                    let det = a[0] * b[1] - a[1] * b[0];
                    for corner in [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [1.0, 1.0]] {
                        let centre = [0, 1].map(|axis| {
                            ((uv[axis] * size - 0.5).floor() + corner[axis] + 0.5) / size
                        });
                        let rest = [
                            centre[0] * 2.0 - 1.0 - origin[0],
                            1.0 - centre[1] * 2.0 - origin[1],
                        ];
                        let s = (rest[0] * b[1] - rest[1] * b[0]) / det;
                        let t = (a[0] * rest[1] - a[1] * rest[0]) / det;
                        let stored = origin[2] + s * a[2] + t * b[2];
                        assert!(
                            moved[2] <= stored,
                            "{}×{}, земля {ground:?}, нормаль {normal:?}: глубина {} за землёй {stored}",
                            scene.width,
                            scene.height,
                            moved[2],
                        );
                    }
                }
            }
        }
    }
}

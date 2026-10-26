//! Транспортная компания (`games/transport`): игра грузится целиком без ошибок и предупреждений. Куски грунтовки
//! сходятся на стыках улиц по одному перекрёстку, и у каждого перекрёстка — готовая модель из `scene.road_junctions`;
//! модели изгибаются под любой угол улиц, не вывёртывая и не отражая картинку.

use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;

use engine::core::game::Game;
use engine::core::model::{Model, ModelTable};
use engine::core::road_kinds::junction::{SpotArm, place, select};
use engine::core::road_kinds::{Junction, LayoutRoad, MERGE_DISTANCE, RoadKindTable, lay_out};
use engine::core::roads::RoadShape;
use engine::data::error::GameError;
use engine::data::load::{
    ImageVerdict, ModelFile, load_rest_with_models, read_entry, terrain_image_paths,
};
use serde_json::Value;

fn game_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("transport")
        .join(name)
}

fn read(name: &str) -> String {
    fs::read_to_string(game_path(name)).unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"))
}

fn png_verdict(name: &str) -> ImageVerdict {
    let bytes =
        fs::read(game_path(name)).unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

/// Файлы кусков дорог, как их отдаёт страница: байты модели и её картинки — 4 × 4 точки, вид кусков тесту не
/// важен, а размеры кусков движок берёт из самой модели. Остальные модели тест не грузит.
fn road_models(game_json: &str) -> Vec<(String, ModelFile)> {
    let game: Value = serde_json::from_str(game_json).expect("game.json — JSON");
    game["files"]["models"]
        .as_object()
        .expect("в game.json нет моделей")
        .iter()
        .filter(|(name, _)| name.starts_with("road_"))
        .map(|(name, model)| {
            let path = model["path"].as_str().expect("у модели нет пути");
            let bytes = fs::read(game_path(path))
                .unwrap_or_else(|e| panic!("не смог прочитать {path}: {e}"));
            let images = (0..3)
                .map(|index| {
                    (
                        index,
                        ImageVerdict::Ok {
                            width: 4,
                            height: 4,
                            pixels: vec![200; 64],
                        },
                    )
                })
                .collect();
            (
                name.clone(),
                ModelFile {
                    bytes: Some(bytes),
                    images,
                },
            )
        })
        .collect()
}

fn load() -> Game {
    load_with_warnings().0
}

fn load_with_warnings() -> (Game, Vec<GameError>) {
    let game_json = read("game.json");
    let (config, _warnings) = read_entry(&game_json).expect("game.json должен разбираться");
    let tables: Vec<(String, Option<String>)> = config
        .files
        .tables
        .iter()
        .map(|(name, path)| (name.clone(), Some(read(path))))
        .collect();
    let fonts: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(game_path(path)).ok()))
        .collect();
    let terrain = read("terrain.json");
    let masks: Vec<(String, ImageVerdict)> = terrain_image_paths(&terrain)
        .into_iter()
        .map(|path| (path.clone(), png_verdict(&path)))
        .collect();
    let models = road_models(&game_json);
    let (game, _screens, warnings, _images) = load_rest_with_models(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &fonts,
        &[],
        &[],
        &[],
        Some(&read("code.lua")),
        true,
        &tables,
        Some(&terrain),
        &[],
        &masks,
        &[],
        &models,
    )
    .expect("Транспортная компания должна проходить предстартовую проверку");
    (game, warnings)
}

/// Виды и модели Транспортной компании: из самой игры, один раз на все проверки.
fn tables() -> &'static (RoadKindTable, ModelTable) {
    static TABLES: OnceLock<(RoadKindTable, ModelTable)> = OnceLock::new();
    TABLES.get_or_init(|| {
        let game = load();
        (game.world.road_kinds().clone(), game.world.models().clone())
    })
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[test]
fn road_kind_joints_get_one_junction_each() {
    let mut game = load();
    game.new_game();
    let layout = game.road_kind_layout();
    assert!(!layout.pieces().is_empty(), "на карте нет кусков вида");
    // Стык двух отрезков одной улицы — продолжение или один перекрёсток; три перекрёстка подряд встали бы ближе
    // ширины улицы.
    let junctions = layout.junctions();
    for (i, a) in junctions.iter().enumerate() {
        for b in &junctions[i + 1..] {
            assert!(
                distance(a.center, b.center) > a.width.max(b.width),
                "перекрёстки вплотную: {:?} и {:?}",
                a.center,
                b.center
            );
        }
    }
}

#[test]
fn the_game_loads_without_errors_or_warnings() {
    let (_, warnings) = load_with_warnings();
    assert!(warnings.is_empty(), "{warnings:#?}");
}

#[test]
fn every_place_of_the_map_where_roads_meet_is_a_model_from_road_junctions_with_its_parts_bent() {
    let mut game = load();
    game.new_game();
    let layout = game.road_kind_layout();
    let kinds = game.world.road_kinds();
    let models = game.world.models();
    let junctions = layout.junctions();
    assert_eq!(
        layout.bare_spots(),
        0,
        "стыки без модели: куски идут до точки, а на карте Транспортной компании модель есть у каждого"
    );
    assert_eq!(
        junctions.len(),
        JUNCTIONS_OF_THE_MAP,
        "перекрёстков с моделью"
    );
    assert_eq!(kinds.junctions().len(), 30, "30 моделей перекрёстков");
    for junction in junctions.iter() {
        assert!(
            kinds.junctions().iter().any(|m| m.model == junction.model),
            "модель не из road_junctions: {:?}",
            junction.center
        );
        let model = models.get(junction.model).expect("модель");
        assert!(!model.parts.is_empty());
        assert_eq!(junction.parts.len(), model.parts.len());
        assert!(
            junction
                .parts
                .iter()
                .zip(&model.parts)
                .all(|(copy, part)| copy.vertices.len() == part.vertices.len()
                    && copy.indices.len() == part.indices.len()),
            "копия — те же вершины и треугольники, что у модели"
        );
    }
}

/// Дорога по концам оси, ширине и виду.
fn segment(id: u32, from: [f64; 2], to: [f64; 2], width: f64, kind: usize) -> LayoutRoad {
    let (dx, dy) = (to[0] - from[0], to[1] - from[1]);
    let length = dx.hypot(dy);
    let center = [(from[0] + to[0]) / 2.0, (from[1] + to[1]) / 2.0];
    LayoutRoad {
        shape: RoadShape {
            id,
            position: [center[0] - length / 2.0, center[1] - width / 2.0],
            size: [length, width],
            sin_cos: (dy / length, dx / length),
            z: 0.0,
        },
        kind: Some(kind),
    }
}

/// Ширина вида в клетках: ширина куска в метрах на масштаб 0,05, как на карте Транспортной компании.
fn width_of(kinds: &RoadKindTable, kind: usize) -> f64 {
    kinds.get(kind).expect("вид").width() * 0.05
}

/// Мест на карте Транспортной компании, где сходятся дороги видов: тупики, повороты, тройники, кресты. Сейчас
/// это восточный тупик улицы деревни, тройник тракта с проездом к складу и тупик этого проезда; тракт кончается на
/// проезде по причалу без вида — там перекрёстка нет.
const JUNCTIONS_OF_THE_MAP: usize = 3;

const MEETING: [f64; 2] = [30.0, 30.0];

fn junction_at_meeting(roads: &[LayoutRoad]) -> Junction {
    let (kinds, models) = tables();
    let (_, junctions) = lay_out(roads, kinds, models);
    junctions
        .into_iter()
        .find(|junction| {
            (junction.center[0] - MEETING[0]).hypot(junction.center[1] - MEETING[1]) < 1e-6
        })
        .expect("перекрёсток в точке встречи")
}

/// Тени файла: границы позиций `POSITION` первой сетки из JSON-части glb.
fn file_bounds(model: &str) -> ([f64; 3], [f64; 3]) {
    let path = game_path(&format!("models/{model}.glb"));
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"));
    let json_length = u32::from_le_bytes(bytes[12..16].try_into().expect("длина")) as usize;
    let json: Value = serde_json::from_slice(&bytes[20..20 + json_length]).expect("JSON модели");
    let accessor = &json["accessors"][0];
    let numbers = |key: &str| -> [f64; 3] {
        std::array::from_fn(|axis| accessor[key][axis].as_f64().expect("число"))
    };
    (numbers("min"), numbers("max"))
}

/// Знак поворота отображения «точка картинки → место»: положителен, если картинка лежит на карте не зеркально.
fn picture_chirality(
    a: ([f32; 3], [f32; 2]),
    b: ([f32; 3], [f32; 2]),
    c: ([f32; 3], [f32; 2]),
) -> f64 {
    let place =
        f64::from((b.0[0] - a.0[0]) * (c.0[1] - a.0[1]) - (b.0[1] - a.0[1]) * (c.0[0] - a.0[0]));
    let picture =
        f64::from((b.1[0] - a.1[0]) * (c.1[1] - a.1[1]) - (b.1[1] - a.1[1]) * (c.1[0] - a.1[0]));
    place * picture
}

/// Тройник главной с улицей, где улица кончается на главной с верхней стороны экрана (угол 90° против часовой):
/// модель `road_tee_main_street` в таком положении встаёт как лежит в файле — не повёрнутой и не отражённой.
#[test]
fn the_real_tee_stands_as_it_lies_in_the_file_not_turned_not_mirrored() {
    let (kinds, models) = tables();
    let (main, street) = (
        kinds.index_of("main").expect("вид"),
        kinds.index_of("street").expect("вид"),
    );
    let roads = [
        segment(
            0,
            [MEETING[0] - 15.0, MEETING[1]],
            [MEETING[0] + 15.0, MEETING[1]],
            width_of(kinds, main),
            main,
        ),
        // Вверх по экрану — к меньшим y мира.
        segment(
            1,
            MEETING,
            [MEETING[0], MEETING[1] - 15.0],
            width_of(kinds, street),
            street,
        ),
    ];
    let junction = junction_at_meeting(&roads);
    let model = models.get(junction.model).expect("модель");
    assert_eq!(model.name, "road_tee_main_street");
    assert!(!junction.mirrored);
    let scale = junction.scale;
    assert!((scale - 0.05).abs() < 1e-9, "{scale}");

    // Без поворота и изгиба каждая вершина — вершина модели от середины файла в масштабе.
    for (copy, part) in junction.parts.iter().zip(&model.parts) {
        for (placed, source) in copy.vertices.iter().zip(&part.vertices) {
            for axis in 0..2 {
                let want = f64::from(source.position[axis] - model.origin[axis]) * scale;
                assert!(
                    (f64::from(placed.position[axis]) - want).abs() < 1e-5,
                    "ось {axis}: {} вместо {want}",
                    placed.position[axis]
                );
            }
        }
    }

    // Рукав улицы (90°) в файле — к −Z, на карте — к меньшим y, вверх по экрану; назад от середины сетка идёт на
    // полуширину главной.
    let (low, high) = file_bounds("road_tee_main_street");
    assert!(
        low[2] < -12.0 && high[2] > 6.0 && high[2] < 7.0,
        "{low:?} {high:?}"
    );
    let ys: Vec<f64> = junction
        .parts
        .iter()
        .flat_map(|part| part.vertices.iter().map(|v| f64::from(v.position[1])))
        .collect();
    let (top, bottom) = ys
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &y| {
            (a.min(y), b.max(y))
        });
    assert!(
        (top - low[2] * scale).abs() < 1e-5,
        "рукав улицы вверх: {top}"
    );
    assert!((bottom - high[2] * scale).abs() < 1e-5, "назад: {bottom}");

    // Картинка лежит как в файле: вправо по u, вниз по v, ни одна стрелка не меняет сторону.
    for (copy, part) in junction.parts.iter().zip(&model.parts) {
        for (triangle, source) in copy.indices.chunks(3).zip(part.indices.chunks(3)) {
            let at = |part: &engine::core::model::Part, index: u32| {
                let v = part.vertices[index as usize];
                (v.position, v.uv)
            };
            let after = picture_chirality(
                at(copy, triangle[0]),
                at(copy, triangle[1]),
                at(copy, triangle[2]),
            );
            let before = picture_chirality(
                at(part, source[0]),
                at(part, source[1]),
                at(part, source[2]),
            );
            assert!(after * before >= 0.0, "картинка зеркальна");
        }
    }
}

/// Площадь треугольника в плоскости карты.
fn area(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f64 {
    f64::from((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]))
}

/// Случай перебора: расположение дорог, угол второй от оси x по карте, виды, множители ширины и на сколько начало
/// второй дороги заходит за ось первой (у тройника; стык тогда в пересечении осей, а не в конце второй).
#[derive(Clone, Copy)]
struct Case {
    shape: &'static str,
    degrees: f64,
    kinds: [usize; 2],
    widths: [f64; 2],
    slide: f64,
}

impl Case {
    fn roads(&self) -> Vec<LayoutRoad> {
        let (kinds, _) = tables();
        let (sin, cos) = self.degrees.to_radians().sin_cos();
        let p = MEETING;
        let far = |sign: f64| [p[0] + sign * 15.0 * cos, p[1] + sign * 15.0 * sin];
        let width = |k: usize| width_of(kinds, self.kinds[k]) * self.widths[k];
        let first = |from: [f64; 2], to: [f64; 2]| segment(0, from, to, width(0), self.kinds[0]);
        let second = |from: [f64; 2], to: [f64; 2]| segment(1, from, to, width(1), self.kinds[1]);
        let (west, east) = ([p[0] - 15.0, p[1]], [p[0] + 15.0, p[1]]);
        match self.shape {
            "tee" => vec![first(west, east), second(far(-self.slide / 15.0), far(1.0))],
            "cross" => vec![first(west, east), second(far(-1.0), far(1.0))],
            _ => vec![first(west, p), second(p, far(1.0))],
        }
    }

    /// Рукава на карте: угол против часовой, если смотреть сверху, и ширина дороги.
    fn arms(&self) -> Vec<(f64, f64)> {
        let (kinds, _) = tables();
        let width = |k: usize| width_of(kinds, self.kinds[k]) * self.widths[k];
        let second = -self.degrees;
        match self.shape {
            "tee" => vec![(0.0, width(0)), (180.0, width(0)), (second, width(1))],
            "cross" => vec![
                (0.0, width(0)),
                (180.0, width(0)),
                (second, width(1)),
                (second + 180.0, width(1)),
            ],
            _ => vec![(180.0, width(0)), (second, width(1))],
        }
    }

    fn label(&self) -> String {
        format!(
            "{} {:?} {:?} сдвиг {} {}°",
            self.shape, self.kinds, self.widths, self.slide, self.degrees
        )
    }
}

/// Три расположения, все пары видов Транспортной компании, обычные и суженные дороги, у тройника ещё со сдвигом
/// начала второй дороги, углы от 30° до 150° в обе стороны через 5°. Плюс главное дело перехода внутри вида: две дороги
/// одного вида разной ширины почти по прямой — излом от 0° до 25° в обе стороны.
fn cases() -> Vec<Case> {
    let (kinds, _) = tables();
    let indices: Vec<usize> = ["main", "street", "drive"]
        .iter()
        .map(|name| kinds.index_of(name).expect("вид"))
        .collect();
    let mut out = Vec::new();
    for shape in ["tee", "cross", "bend"] {
        for &a in &indices {
            for &b in &indices {
                for widths in [[1.0, 1.0], [0.85, 1.0]] {
                    for slide in if shape == "tee" {
                        vec![0.0, 0.1]
                    } else {
                        vec![0.0]
                    } {
                        for degrees in (30..=150).step_by(5) {
                            for sign in [1.0, -1.0] {
                                out.push(Case {
                                    shape,
                                    degrees: sign * f64::from(degrees),
                                    kinds: [a, b],
                                    widths,
                                    slide,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    for &kind in &indices {
        for degrees in (0..=25).step_by(5) {
            for sign in if degrees == 0 {
                vec![1.0]
            } else {
                vec![1.0, -1.0]
            } {
                out.push(Case {
                    shape: "bend",
                    degrees: sign * f64::from(degrees),
                    kinds: [kind, kind],
                    widths: [0.85, 1.0],
                    slide: 0.0,
                });
            }
        }
    }
    out
}

#[test]
fn bent_real_models_keep_the_picture_and_every_triangle_at_any_angle_and_the_distance_where_no_arm_is_stretched()
 {
    let (kinds, models) = tables();
    let (mut checked, mut kept_distance) = (0, 0);
    for case in cases() {
        let junction = junction_at_meeting(&case.roads());
        let model: &Model = models.get(junction.model).expect("модель");
        assert!(
            kinds.junctions().iter().any(|m| m.model == junction.model),
            "{}",
            case.label()
        );
        let plain = junction.arms.iter().all(|arm| arm.extension == 0.0);
        let mut folded = 0;
        for (copy, part) in junction.parts.iter().zip(&model.parts) {
            // Расстояние до середины то же, с масштабом (требование 9), пока рукава не удлинены.
            if plain {
                for (placed, source) in copy.vertices.iter().zip(&part.vertices) {
                    let before = f64::from(source.position[0] - model.origin[0])
                        .hypot(f64::from(source.position[1] - model.origin[1]));
                    let after = f64::from(placed.position[0]).hypot(f64::from(placed.position[1]));
                    assert!(
                        (after - before * junction.scale).abs() < 1e-4,
                        "{}: расстояние {after} вместо {}",
                        case.label(),
                        before * junction.scale
                    );
                }
            }
            for (triangle, source) in copy.indices.chunks(3).zip(part.indices.chunks(3)) {
                let corners = |part: &engine::core::model::Part, indices: &[u32]| {
                    [0, 1, 2].map(|k| {
                        let vertex = part.vertices[indices[k] as usize];
                        (vertex.position, vertex.uv)
                    })
                };
                let (after, before) = (corners(copy, triangle), corners(part, source));
                let old = area(before[0].0, before[1].0, before[2].0);
                let old_picture = picture_chirality(before[0], before[1], before[2]);
                if old.abs() < 1e-9 || old_picture.abs() < 1e-12 {
                    continue;
                }
                let new = area(after[0].0, after[1].0, after[2].0);
                // Картинка лежит как в файле, когда модель не отражена, и зеркально, когда отражена.
                let mirror = if junction.mirrored { -1.0 } else { 1.0 };
                let picture =
                    picture_chirality(after[0], after[1], after[2]) * old_picture * mirror;
                if old * new <= 0.0 || picture <= 0.0 {
                    folded += 1;
                }
            }
        }
        assert_eq!(folded, 0, "{}: вывернутых треугольников", case.label());
        kept_distance += usize::from(plain);
        checked += 1;
    }
    assert_eq!(checked, 3633);
    assert!(kept_distance > 300, "{kept_distance}");
}

/// Ось и поперечное сечение рукавов настоящих моделей: при любом угле от 30° до 150° (со сдвигом точки стыка и без)
/// каждая точка рукава за устьем лежит от оси дороги на половине ширины дороги с точностью 2%, ось — на оси дороги, а
/// куски дороги начинаются на `(reach − fade) × s + Δ` от точки стыка.
#[test]
fn at_any_angle_every_arm_of_a_real_model_keeps_the_width_of_its_road_across_within_two_percent() {
    let (kinds, models) = tables();
    let (mut checked, mut stretched, mut points) = (0, 0, 0);
    for case in cases() {
        let (pieces, junction) = {
            let (pieces, junctions) = lay_out(&case.roads(), kinds, models);
            let joint = junctions
                .into_iter()
                .find(|j| (j.center[0] - MEETING[0]).hypot(j.center[1] - MEETING[1]) < 1e-6)
                .expect("перекрёсток в точке встречи");
            (pieces, joint)
        };
        let model = models.get(junction.model).expect("модель");
        let described = kinds
            .junctions()
            .iter()
            .find(|m| m.model == junction.model)
            .expect("описание модели");
        let roads = case.arms();
        let planar = |position: [f32; 3]| [f64::from(position[0]), -f64::from(position[1])];
        for arm in &described.arms {
            // Вершина у конца оси рукава показывает, на какой луч дороги он встал.
            let own = if junction.mirrored { -arm.dir } else { arm.dir };
            let (sin, cos) = own.sin_cos();
            let model_point = |source: &engine::core::model::ModelVertex| {
                let x = f64::from(source.position[0] - model.origin[0]);
                let y = -f64::from(source.position[1] - model.origin[1]);
                let y = if junction.mirrored { -y } else { y };
                (x * cos + y * sin, y * cos - x * sin)
            };
            let end = junction.parts[0]
                .vertices
                .iter()
                .zip(&model.parts[0].vertices)
                .filter_map(|(placed, source)| {
                    let (along, lateral) = model_point(source);
                    (along > arm.reach - 0.2 && lateral.abs() < 0.6)
                        .then_some((planar(placed.position), lateral.abs()))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1))
                .expect("конец рукава");
            let angle = end.0[1].atan2(end.0[0]).to_degrees();
            let road = roads
                .iter()
                .enumerate()
                .min_by(|a, b| {
                    let apart = |road: f64| (angle - road + 180.0).rem_euclid(360.0) - 180.0;
                    apart(a.1.0).abs().total_cmp(&apart(b.1.0).abs())
                })
                .expect("дороги")
                .0;
            let (road_angle, road_width) = (roads[road].0.to_radians(), roads[road].1);
            let start = arm.reach - arm.fade;
            for (placed, source) in junction.parts[0]
                .vertices
                .iter()
                .zip(&model.parts[0].vertices)
            {
                let (along, lateral) = model_point(source);
                if along < start + 1e-6 || lateral.abs() > arm.width / 2.0 * 1.1 {
                    continue;
                }
                let image = planar(placed.position);
                let across = image[1] * road_angle.cos() - image[0] * road_angle.sin();
                let want = lateral.abs() * road_width / arm.width;
                assert!(
                    (across.abs() - want).abs() < 0.02 * want + 1e-4,
                    "{}: точка рукава в {} от оси вместо {want}",
                    case.label(),
                    across.abs()
                );
                points += 1;
            }
            // Куски этой дороги начинаются на `(reach − fade) × s + Δ` от точки стыка.
            let placed_arm = junction
                .arms
                .iter()
                .find(|placed| {
                    (placed.angle.to_degrees() - roads[road].0 + 180.0).rem_euclid(360.0) - 180.0
                        < 1e-3
                        && (placed.angle.to_degrees() - roads[road].0 + 180.0).rem_euclid(360.0)
                            - 180.0
                            > -1e-3
                })
                .expect("рукав на дороге");
            assert!(
                (placed_arm.mouth - (start * junction.scale + placed_arm.extension)).abs() < 1e-9
            );
            let dir = [placed_arm.angle.cos(), -placed_arm.angle.sin()];
            let first_piece = pieces
                .iter()
                .filter(|piece| {
                    let from = [piece.center[0] - MEETING[0], piece.center[1] - MEETING[1]];
                    (from[0] * dir[1] - from[1] * dir[0]).abs() < 1e-6
                        && from[0] * dir[0] + from[1] * dir[1] > 0.0
                })
                .map(|piece| {
                    let from = [piece.center[0] - MEETING[0], piece.center[1] - MEETING[1]];
                    from[0] * dir[0] + from[1] * dir[1] - piece.length / 2.0
                })
                .fold(f64::INFINITY, f64::min);
            assert!(
                (first_piece - placed_arm.mouth).abs() < 1e-6,
                "{}: куски начинаются в {first_piece} вместо {}",
                case.label(),
                placed_arm.mouth
            );
            stretched += usize::from(placed_arm.extension > 0.0);
            checked += 1;
        }
    }
    assert!(checked > 10_000, "{checked}");
    assert!(stretched > 1000, "{stretched}");
    assert!(points > 200_000, "{points}");
}

/// Пример 13 плана на настоящих моделях: крест двух главных шириной 0,6 под прямым углом — масштаб 0,05, поворот
/// кратен 90°, изгиба нет, куски каждой дороги начинаются в 0,55 клетки от точки стыка.
#[test]
fn two_main_roads_crossing_at_right_angles_scale_the_model_and_start_the_pieces_at_0_55() {
    let (kinds, models) = tables();
    let main = kinds.index_of("main").expect("вид");
    let width = width_of(kinds, main);
    assert!((width - 0.6).abs() < 1e-9);
    let p = MEETING;
    let roads = [
        segment(0, [p[0] - 15.0, p[1]], [p[0] + 15.0, p[1]], width, main),
        segment(1, [p[0], p[1] - 15.0], [p[0], p[1] + 15.0], width, main),
    ];
    let (pieces, junctions) = lay_out(&roads, kinds, models);
    let cross = junctions
        .iter()
        .find(|j| (j.center[0] - p[0]).hypot(j.center[1] - p[1]) < 1e-6)
        .expect("крест");
    assert_eq!(
        models.get(cross.model).expect("модель").name,
        "road_cross_main_main"
    );
    assert!((cross.scale - 0.05).abs() < 1e-9 && !cross.mirrored);
    // Вершины не сдвинуты: поворот кратен 90°, а изгиба нет — каждая вершина лежит там же в масштабе.
    let model = models.get(cross.model).expect("модель");
    let shifted = cross.parts[0]
        .vertices
        .iter()
        .zip(&model.parts[0].vertices)
        .map(|(placed, source)| {
            let before = [
                f64::from(source.position[0] - model.origin[0]) * 0.05,
                f64::from(source.position[1] - model.origin[1]) * 0.05,
            ];
            let turned_quarter = [f64::from(placed.position[0]), f64::from(placed.position[1])];
            (before[0].hypot(before[1]) - turned_quarter[0].hypot(turned_quarter[1])).abs()
        })
        .fold(0.0, f64::max);
    assert!(shifted < 1e-5, "{shifted}");
    for dir in [[1.0, 0.0], [-1.0, 0.0], [0.0, 1.0], [0.0, -1.0]] {
        let start = pieces
            .iter()
            .filter(|piece| {
                let from = [piece.center[0] - p[0], piece.center[1] - p[1]];
                (from[0] * dir[1] - from[1] * dir[0]).abs() < 1e-6
                    && from[0] * dir[0] + from[1] * dir[1] > 0.0
            })
            .map(|piece| {
                let from = [piece.center[0] - p[0], piece.center[1] - p[1]];
                from[0] * dir[0] + from[1] * dir[1] - piece.length / 2.0
            })
            .fold(f64::INFINITY, f64::min);
        assert!((start - 0.55).abs() < 1e-6, "{dir:?}: {start}");
    }
}

/// Планарные «против часовой» угол и расстояние вершины модели от середины файла.
fn polar_of(model: &Model, position: [f32; 3]) -> (f64, f64) {
    let x = f64::from(position[0] - model.origin[0]);
    let y = -f64::from(position[1] - model.origin[1]);
    (y.atan2(x).to_degrees().rem_euclid(360.0), x.hypot(y))
}

/// Пример 14 на настоящем тройнике: главная насквозь шириной 0,6, улица шириной 0,35 кончается в точке под 75°
/// против часовой от оси x. Поворот 0°, без отражения.
#[test]
fn the_real_tee_under_75_degrees_bends_every_vertex_by_the_example_rule() {
    let (kinds, models) = tables();
    let (main, street) = (
        kinds.index_of("main").expect("вид"),
        kinds.index_of("street").expect("вид"),
    );
    let (sin, cos) = (-75.0_f64).to_radians().sin_cos();
    let roads = [
        segment(
            0,
            [MEETING[0] - 15.0, MEETING[1]],
            [MEETING[0] + 15.0, MEETING[1]],
            width_of(kinds, main),
            main,
        ),
        segment(
            1,
            MEETING,
            [MEETING[0] + 15.0 * cos, MEETING[1] + 15.0 * sin],
            width_of(kinds, street),
            street,
        ),
    ];
    let junction = junction_at_meeting(&roads);
    let model = models.get(junction.model).expect("модель");
    assert_eq!(model.name, "road_tee_main_street");
    assert!(!junction.mirrored);
    assert!((junction.scale - 0.05).abs() < 1e-9);
    // Между главной на 0° и улицей остаётся 75°: оба рукава удлиняются на `Δ` ≈ 0,103 клетки, главная на 180° — нет.
    // Устье рукава — на `(reach − fade) × s + Δ` от середины.
    let placed = |degrees: f64| {
        junction
            .arms
            .iter()
            .find(|arm| (arm.angle.to_degrees() - degrees).abs() < 1e-6)
            .copied()
            .expect("рукав")
    };
    for (degrees, extension, base) in [
        (0.0, 0.103, (11.458 - 4.0) * 0.05),
        (75.0, 0.103, (12.292 - 2.333) * 0.05),
        (180.0, 0.0, (11.458 - 4.0) * 0.05),
    ] {
        let arm = placed(degrees);
        assert!(
            (arm.extension - extension).abs() < 5e-4
                && (arm.mouth - base - arm.extension).abs() < 1e-6,
            "рукав {degrees}°: удлинение {} устье {}",
            arm.extension,
            arm.mouth
        );
    }
    // Рукава главной (0° и 180°) не поворачиваются, рукав улицы (90°) поворачивается на −15° целиком; каждый рукав
    // сдвинут вдоль себя на своё удлинение, линейно растущее от нуля в середине до полного в устье. Полосы рукавов:
    // точки не дальше полуширины рукава с запасом 10% от оси.
    let arms: [(f64, f64, f64, f64); 3] = [
        (0.0, 12.0, 7.458, 0.0),
        (90.0, 7.0, 9.959, -15.0),
        (180.0, 12.0, 7.458, 0.0),
    ];
    let extensions = |angle: f64| {
        junction
            .arms
            .iter()
            .find(|arm| {
                ((arm.angle.to_degrees() - angle + 180.0).rem_euclid(360.0) - 180.0).abs() < 1e-6
            })
            .map(|arm| arm.extension)
            .expect("рукав")
    };
    let (mut in_strips, mut between) = (0, 0);
    for (placed, source) in junction.parts[0]
        .vertices
        .iter()
        .zip(&model.parts[0].vertices)
    {
        let (angle, radius) = polar_of(model, source.position);
        if radius < 0.5 {
            continue;
        }
        let strip = arms.iter().find(|&&(dir, width, mouth, _)| {
            let half = width / 2.0 * 1.1;
            let reach = radius.max(f64::hypot(mouth, half));
            let apart = (angle - dir + 180.0).rem_euclid(360.0) - 180.0;
            apart.abs() <= (half / reach).asin().to_degrees() - 1e-6
        });
        let Some(&(dir, _, mouth, rotation)) = strip else {
            between += 1;
            continue;
        };
        // Точка полосы: поворот на поворот рукава и сдвиг вдоль рукава на его удлинение.
        let planar = [
            f64::from(source.position[0] - model.origin[0]) * 0.05,
            -f64::from(source.position[1] - model.origin[1]) * 0.05,
        ];
        let (sin, cos) = rotation.to_radians().sin_cos();
        let turned = [
            planar[0] * cos - planar[1] * sin,
            planar[0] * sin + planar[1] * cos,
        ];
        let (model_sin, model_cos) = f64::sin_cos(f64::to_radians(dir));
        let reached =
            ((planar[0] * model_cos + planar[1] * model_sin) / (mouth * 0.05)).clamp(0.0, 1.0);
        let image_dir = (dir + rotation).to_radians();
        let shift = extensions(dir + rotation) * reached;
        let want = [
            turned[0] + shift * image_dir.cos(),
            turned[1] + shift * image_dir.sin(),
        ];
        let got = [
            f64::from(placed.position[0]),
            -f64::from(placed.position[1]),
        ];
        assert!(
            (got[0] - want[0]).abs() < 1e-5 && (got[1] - want[1]).abs() < 1e-5,
            "точка рукава {dir}° на {angle}°: {got:?} вместо {want:?}"
        );
        in_strips += 1;
    }
    assert!(in_strips > 200 && between > 50, "{in_strips} {between}");
}

/// Пример 16 на настоящих моделях: главная насквозь, с одной стороны улица, с другой проезд — моделей с тремя видами
/// нет, берётся `road_cross_main_street` (не совпал один рукав, модель шире), рукав проезда сужен множителем
/// 0,225 / (7 × 0,05) ≈ 0,64: его поперечный размер за устьем — 0,64 того, что в модели, в масштабе.
#[test]
fn a_cross_of_main_street_and_drive_takes_the_wider_model_and_narrows_the_drive_arm() {
    let (kinds, models) = tables();
    let (main, street, drive) = (
        kinds.index_of("main").expect("вид"),
        kinds.index_of("street").expect("вид"),
        kinds.index_of("drive").expect("вид"),
    );
    let p = MEETING;
    let roads = [
        segment(
            0,
            [p[0] - 15.0, p[1]],
            [p[0] + 15.0, p[1]],
            width_of(kinds, main),
            main,
        ),
        // Улица вверх по экрану, проезд вниз.
        segment(1, [p[0], p[1] - 15.0], p, width_of(kinds, street), street),
        segment(2, p, [p[0], p[1] + 15.0], width_of(kinds, drive), drive),
    ];
    let junction = junction_at_meeting(&roads);
    let model = models.get(junction.model).expect("модель");
    assert_eq!(model.name, "road_cross_main_street");
    assert!((junction.scale - 0.05).abs() < 1e-9 && !junction.mirrored);
    let factor: f64 = 0.225 / (7.0 * 0.05);
    assert!((factor - 0.6428).abs() < 1e-3);
    let described = kinds
        .junctions()
        .iter()
        .find(|m| m.model == junction.model)
        .expect("описание");
    let drive_arm = described
        .arms
        .iter()
        .find(|arm| (arm.dir.to_degrees() - 270.0).abs() < 1e-6)
        .expect("рукав вниз по экрану");
    let mut narrowed = 0;
    for (placed, source) in junction.parts[0]
        .vertices
        .iter()
        .zip(&model.parts[0].vertices)
    {
        // Точки рукава проезда за устьем: вдоль оси больше `reach − fade`, вдоль неё же — до устья рукава.
        let x = f64::from(source.position[0] - model.origin[0]);
        let along = f64::from(source.position[1] - model.origin[1]);
        if along < drive_arm.reach - drive_arm.fade || x.abs() > 3.0 {
            continue;
        }
        let want = x * 0.05 * factor;
        let got = f64::from(placed.position[0]);
        assert!((got - want).abs() < 1e-5, "поперёк {got} вместо {want}");
        narrowed += 1;
    }
    assert!(narrowed > 10, "{narrowed}");
}

/// Углы улиц Транспортной компании около 90°: рукава перекрёстков почти не удлиняются. У двух тройников у набережной,
/// где главная улица поворачивает под 90°, а прямо продолжается улица, своих видов модели нет; согнуть тройник «главная
/// насквозь, улица вбок» на 90° дороже, чем взять тройник двух главных без изгиба с рукавом улицы, суженным под неё, —
/// поэтому и там удлинения нет (раньше согнутая модель вылезала горбом за дорогу).
#[test]
fn the_junctions_of_the_map_are_stretched_by_nothing_or_almost_nothing() {
    let mut game = load();
    game.new_game();
    let layout = game.road_kind_layout();
    let stretched: Vec<f64> = layout
        .junctions()
        .iter()
        .map(|junction| {
            junction
                .arms
                .iter()
                .map(|arm| arm.extension)
                .fold(0.0, f64::max)
        })
        .filter(|&extension| extension > 0.05)
        .collect();
    assert!(stretched.is_empty(), "{stretched:?}");
}

/// Рукава случая для `junction::select` и `place` по возрастанию угла: угол в радианах, вид и ширина дороги; у
/// рукава номер `shifted` ось дороги сдвинута от прямой через середину на `offset`.
fn spot_arms(case: &Case, shifted: usize, offset: f64) -> Vec<SpotArm> {
    let roads: &[usize] = match case.shape {
        "tee" => &[0, 0, 1],
        "cross" => &[0, 0, 1, 1],
        _ => &[0, 1],
    };
    let mut arms: Vec<SpotArm> = case
        .arms()
        .iter()
        .zip(roads)
        .map(|(&(degrees, width), &road)| SpotArm {
            angle: degrees.to_radians().rem_euclid(std::f64::consts::TAU),
            kind: case.kinds[road],
            width,
            offset: 0.0,
        })
        .collect();
    arms.sort_by(|a, b| a.angle.total_cmp(&b.angle));
    if let Some(arm) = arms.get_mut(shifted) {
        arm.offset = offset;
    }
    arms
}

/// Случаи без сдвига начала второй дороги, углы от 30° до 170° в обе стороны через 5°.
fn far_cases() -> Vec<Case> {
    let mut out = Vec::new();
    for base in cases()
        .into_iter()
        .filter(|case| case.slide == 0.0 && case.degrees == 30.0)
    {
        for degrees in (30..=170).step_by(5) {
            for sign in [1.0, -1.0] {
                out.push(Case {
                    degrees: sign * f64::from(degrees),
                    ..base
                });
            }
        }
    }
    out
}

/// Узкие углы между соседними рукавами: у таких рукавов модели нет (меньше 30°).
fn crowded(arms: &[SpotArm]) -> bool {
    arms.iter().enumerate().any(|(index, arm)| {
        let next = arms[(index + 1) % arms.len()];
        (next.angle - arm.angle).rem_euclid(std::f64::consts::TAU) < 30.0_f64.to_radians() - 1e-9
    })
}

#[test]
fn a_road_axis_shifted_by_the_most_the_layout_can_shift_it_never_turns_a_triangle_of_a_real_model_over()
 {
    let (kinds, models) = tables();
    let (mut checked, mut flipped, mut broken) = (0, Vec::new(), Vec::new());
    for case in far_cases() {
        for shifted in 0..spot_arms(&case, usize::MAX, 0.0).len() {
            for offset in [
                MERGE_DISTANCE,
                -MERGE_DISTANCE,
                MERGE_DISTANCE / 2.0,
                -MERGE_DISTANCE / 2.0,
            ] {
                let arms = spot_arms(&case, shifted, offset);
                if crowded(&arms) {
                    continue;
                }
                let fit = select(kinds, models, &arms)
                    .unwrap_or_else(|| panic!("{}: модели нет", case.label()));
                let placed = place(kinds, models, &fit, &arms).expect("копия");
                let model = models
                    .get(kinds.junctions()[fit.model].model)
                    .expect("модель");
                let mut count = 0;
                for (copy, source) in placed.parts.iter().zip(&model.parts) {
                    if copy
                        .vertices
                        .iter()
                        .any(|v| v.position.iter().any(|c| !c.is_finite()))
                    {
                        broken.push(format!("{} рукав {shifted} сдвиг {offset}", case.label()));
                    }
                    for (new, old) in copy.indices.chunks(3).zip(source.indices.chunks(3)) {
                        let flat = |part: &engine::core::model::Part, t: &[u32]| {
                            area(
                                part.vertices[t[0] as usize].position,
                                part.vertices[t[1] as usize].position,
                                part.vertices[t[2] as usize].position,
                            )
                        };
                        if flat(copy, new) * flat(source, old) <= 0.0 {
                            count += 1;
                        }
                    }
                }
                if count > 0 {
                    flipped.push(format!(
                        "{} рукав {shifted} сдвиг {offset}: {count}",
                        case.label()
                    ));
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 10_000, "{checked}");
    assert!(broken.is_empty(), "не числа: {broken:#?}");
    assert!(
        flipped.is_empty(),
        "вывернуто {} из {checked}: {:#?}",
        flipped.len(),
        &flipped[..flipped.len().min(20)]
    );
}

//! Фаза 43 — вид дороги. Игры собираются в коде теста, как в `phase25_models.rs`. Модели видов — маленькие
//! плоские прямоугольники, собранные здесь же (кусок 1 × 2, узел 1 × 1, как в примерах плана), и настоящие
//! файлы: куски дорог с `BLEND` из `games/transport/models`, забор с `MASK` и окно со стеклом из `art/modelcheck`. Проверяются
//! `scene.road_kinds` и ошибки перед запуском, `road_kind` дороги, раскладка по миру и её пересборка, `road_kind`
//! в коде, выбор дороги в редакторе, кадр, порядок частей `BLEND` и шейдер.

use std::fs;
use std::path::PathBuf;

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::model::{AlphaMode, Model};
use engine::core::property;
use engine::core::scene;
use engine::core::surface::{self, Kind};
use engine::data::error::GameError;
use engine::data::load::{ImageVerdict, ModelFile, load_rest_with_models, read_entry};
use engine::render::models::{BlendedDraw, model_draws, road_draws};
use engine::render::relief::FlatShapes;
use engine::render::scene3d::compose_frame3d;
use engine::render::wind::Motion;

const WINDOW: [f32; 2] = [1920.0, 1080.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag","tag":"flag"}}"#;
const KINDS: &str = r#"{"street":{"piece":"piece","node":"node"}}"#;
const MODELS: &str = r#"{"piece":{"path":"models/piece.glb"},"node":{"path":"models/node.glb"}}"#;
const REAL_KINDS: &str = r#"{"street":{"piece":["road_street_a","road_street_b"],"node":"road_node"},"main":{"piece":"road_main_a","node":"road_node"}}"#;
const REAL_MODELS: &str = r#"{"road_street_a":{"path":"models/road_street_a.glb"},"road_street_b":{"path":"models/road_street_b.glb"},"road_main_a":{"path":"models/road_main_a.glb"},"road_node":{"path":"models/road_node.glb"},"fence":{"path":"models/fence.glb"},"window":{"path":"models/window.glb"}}"#;

fn art(name: &str) -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push(if name.starts_with("road_") {
        "../games/transport/models"
    } else {
        "../art/modelcheck"
    });
    path.push(format!("{name}.glb"));
    fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

/// Ответ страницы по модели: байты файла и картинки внутри него (в настоящих файлах их до трёх).
fn page_file(bytes: Vec<u8>) -> ModelFile {
    ModelFile {
        bytes: Some(bytes),
        images: (0..4)
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
            .collect(),
    }
}

/// `.glb` из одного плоского прямоугольника в плоскости XZ файла: ширина по X, длина по Z; `materials` — JSON-
/// список материалов файла (пусто — без материала).
fn flat_glb(width: f32, length: f32, materials: &str) -> Vec<u8> {
    let (w, l) = (width / 2.0, length / 2.0);
    let positions = [-w, 0.0, -l, w, 0.0, -l, w, 0.0, l, -w, 0.0, l];
    let normals = [0.0f32, 1.0, 0.0].repeat(4);
    let uvs = [0.0f32, 0.0, 1.0, 0.0, 1.0, 1.0, 0.0, 1.0];
    let mut bin: Vec<u8> = Vec::new();
    for value in positions.iter().chain(&normals).chain(&uvs) {
        bin.extend(value.to_le_bytes());
    }
    for index in [0u16, 2, 1, 0, 3, 2] {
        bin.extend(index.to_le_bytes());
    }
    let (material_list, material_ref) = if materials.is_empty() {
        (String::new(), "")
    } else {
        (format!(r#","materials":{materials}"#), r#","material":0"#)
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":[{{"mesh":0}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}},"indices":3{material_ref}}}]}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3","min":[{},0,{}],"max":[{w},0,{l}]}},{{"bufferView":1,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":4,"type":"VEC2"}},{{"bufferView":3,"componentType":5123,"count":6,"type":"SCALAR"}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":48}},{{"buffer":0,"byteOffset":96,"byteLength":32}},{{"buffer":0,"byteOffset":128,"byteLength":12}}],"buffers":[{{"byteLength":{}}}]{material_list}}}"#,
        -w,
        -l,
        bin.len()
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + bin.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((json.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(json);
    file.extend((bin.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(bin);
    file
}

fn tiny_files() -> Vec<(String, ModelFile)> {
    vec![
        ("piece".to_string(), page_file(flat_glb(1.0, 2.0, ""))),
        ("node".to_string(), page_file(flat_glb(1.0, 1.0, ""))),
    ]
}

fn real_files() -> Vec<(String, ModelFile)> {
    [
        "road_street_a",
        "road_street_b",
        "road_main_a",
        "road_node",
        "fence",
        "window",
    ]
    .into_iter()
    .map(|name| (name.to_string(), page_file(art(name))))
    .collect()
}

struct Setup<'a> {
    camera: bool,
    road_kinds: &'a str,
    models: &'a str,
    scene: &'a str,
    rules: &'a str,
    code: Option<&'a str>,
    files: Vec<(String, ModelFile)>,
}

impl<'a> Setup<'a> {
    fn new(scene: &'a str) -> Setup<'a> {
        Setup {
            camera: true,
            road_kinds: KINDS,
            models: MODELS,
            scene,
            rules: r#"{"rules":[]}"#,
            code: None,
            files: tiny_files(),
        }
    }

    fn real(scene: &'a str) -> Setup<'a> {
        Setup {
            road_kinds: REAL_KINDS,
            models: REAL_MODELS,
            files: real_files(),
            ..Setup::new(scene)
        }
    }

    fn game_json(&self) -> String {
        let camera = if self.camera {
            r#","view_height":20,"camera":{"pitch":55}"#
        } else {
            ""
        };
        let kinds = if self.road_kinds.is_empty() {
            String::new()
        } else {
            format!(r#","road_kinds":{}"#, self.road_kinds)
        };
        let code = if self.code.is_some() {
            r#","code":"code.lua""#
        } else {
            ""
        };
        let models = if self.models.is_empty() {
            String::new()
        } else {
            format!(r#","models":{}"#, self.models)
        };
        format!(
            r##"{{"name":"R","scene":{{"width":60,"height":40,"background":"#000000"{camera}{kinds}}},
"random_seed":1,"max_objects":512,"start_screen":"main",
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{models}{code}}}}}"##
        )
    }

    fn load(&self) -> Result<(Game, Vec<GameError>), Vec<String>> {
        let text = self.game_json();
        let scene = format!(r#"{{"objects":[{}]}}"#, self.scene);
        let (config, _) = read_entry(&text).map_err(|f| describe(&f.errors))?;
        load_rest_with_models(
            &text,
            config,
            Some(PROPS),
            Some(&scene),
            Some(self.rules),
            Some(SCREENS),
            &[],
            &[],
            &[],
            &[],
            self.code,
            false,
            &[],
            None,
            &[],
            &[],
            &[],
            &self.files,
        )
        .map(|(game, _, warnings, _)| (game, warnings))
        .map_err(|failure| describe(&failure.errors))
    }

    fn game(&self) -> Game {
        match self.load() {
            Ok((game, _)) => game,
            Err(errors) => panic!("игра не загрузилась: {errors:#?}"),
        }
    }
}

fn describe(errors: &[GameError]) -> Vec<String> {
    errors
        .iter()
        .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
        .collect()
}

fn expect_error(setup: &Setup<'_>, fragments: &[&str]) {
    let errors = setup.load().err().unwrap_or_else(|| {
        panic!("игра должна была не загрузиться: нет ошибки со словами {fragments:?}")
    });
    assert!(
        errors
            .iter()
            .any(|e| fragments.iter().all(|f| e.contains(f))),
        "нет ошибки со словами {fragments:?}: {errors:#?}"
    );
}

fn road(name: &str, position: [f64; 2], size: [f64; 2], extra: &str) -> String {
    format!(
        r#"{{"name":"{name}","road":true,"road_kind":"street","position":[{},{}],"size":[{},{}]{extra}}}"#,
        position[0], position[1], size[0], size[1]
    )
}

/// Дороги примера 18 плана: две улицы крестом.
fn cross() -> String {
    [
        road("А", [0.0, 9.5], [21.0, 1.0], ""),
        road("Б", [9.5, 0.0], [1.0, 20.0], ""),
    ]
    .join(",")
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-5
}

fn model_of<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель есть")
}

// -------------------------------------------------------------------------------------------
// Виды дорог и ошибки перед запуском
// -------------------------------------------------------------------------------------------

#[test]
fn road_kinds_resolve_to_models_with_their_sizes() {
    let game = Setup::real("").game();
    let kinds = game.world.road_kinds();
    let street = kinds
        .get(kinds.index_of("street").expect("вид"))
        .expect("вид");
    assert_eq!(street.pieces.len(), 2, "сменные варианты");
    assert!(close(street.pieces[0].width, 7.0) && close(street.pieces[0].length, 14.0));
    assert!(close(street.node.width, 9.0));
    let main = kinds
        .get(kinds.index_of("main").expect("вид"))
        .expect("вид");
    assert_eq!(main.pieces.len(), 1, "имя куска строкой — один вариант");
    assert!(close(main.pieces[0].width, 12.0) && close(main.pieces[0].length, 16.0));
    assert_eq!(
        game.world.models().index_of("road_node"),
        Some(street.node.model)
    );
}

#[test]
fn road_kinds_in_a_flat_scene_or_not_a_table_are_errors() {
    let mut setup = Setup::new("");
    setup.camera = false;
    expect_error(&setup, &["\"road_kinds\"", "трёхмерной сцене"]);
    let mut setup = Setup::new("");
    setup.road_kinds = r#"["piece","node"]"#;
    expect_error(&setup, &["\"road_kinds\"", "таблица"]);
}

#[test]
fn a_kind_without_a_piece_or_a_node_or_with_a_bad_piece_is_an_error() {
    let cases: [(&str, &[&str]); 7] = [
        (r#"{"street":{"node":"node"}}"#, &["piece"]),
        (r#"{"street":{"piece":"piece"}}"#, &["node"]),
        (
            r#"{"street":{"piece":[],"node":"node"}}"#,
            &["piece", "пустой"],
        ),
        (r#"{"street":{"piece":3,"node":"node"}}"#, &["piece"]),
        (
            r#"{"street":{"piece":["piece",3],"node":"node"}}"#,
            &["piece", "[1]"],
        ),
        (r#"{"street":{"piece":"piece","node":7}}"#, &["node"]),
        (r#"{"street":"piece"}"#, &["street", "объект"]),
    ];
    for (kinds, words) in cases {
        let mut setup = Setup::new("");
        setup.road_kinds = kinds;
        expect_error(&setup, words);
    }
}

#[test]
fn a_model_that_files_models_does_not_declare_is_an_error() {
    let mut setup = Setup::new("");
    setup.road_kinds = r#"{"street":{"piece":"castle","node":"node"}}"#;
    expect_error(&setup, &["castle", "piece", "объявлены"]);
    setup.road_kinds = r#"{"street":{"piece":["piece","castle"],"node":"node"}}"#;
    expect_error(&setup, &["castle", "piece"]);
    setup.road_kinds = r#"{"street":{"piece":"piece","node":"castle"}}"#;
    expect_error(&setup, &["castle", "node"]);
}

#[test]
fn a_piece_model_flat_to_nothing_in_x_or_z_and_a_node_without_x_are_errors() {
    let mut setup = Setup::new("");
    setup.files[0].1 = page_file(flat_glb(0.0, 2.0, ""));
    expect_error(&setup, &["piece", "размер по X или Z"]);
    setup.files[0].1 = page_file(flat_glb(1.0, 0.0, ""));
    expect_error(&setup, &["piece", "размер по X или Z"]);
    let mut setup = Setup::new("");
    setup.files[1].1 = page_file(flat_glb(0.0, 1.0, ""));
    expect_error(&setup, &["node", "размер по X"]);
    // Узлу длина не нужна: плоская линия по X годится.
    let mut setup = Setup::new("");
    setup.files[1].1 = page_file(flat_glb(1.0, 0.0, ""));
    assert!(setup.load().is_ok());
}

#[test]
fn a_bad_alpha_cutoff_in_a_model_file_is_an_error() {
    for cutoff in ["2", "-1"] {
        let mut setup = Setup::new("");
        setup.files[0].1 = page_file(flat_glb(
            1.0,
            2.0,
            &format!(
                r#"[{{"alphaMode":"MASK","alphaCutoff":{cutoff},"pbrMetallicRoughness":{{}}}}]"#
            ),
        ));
        expect_error(&setup, &["piece", "\"alphaCutoff\""]);
    }
}

#[test]
fn road_kind_must_be_a_known_name_on_a_road_without_other_fills() {
    let wrong = road("А", [0.0, 0.0], [9.0, 1.0], "").replace("\"street\"", "5");
    expect_error(&Setup::new(&wrong), &["road_kind", "строка"]);
    let unknown = road("А", [0.0, 0.0], [9.0, 1.0], "").replace("street", "alley");
    expect_error(&Setup::new(&unknown), &["alley", "street"]);
    let no_road = r#"{"name":"А","road_kind":"street","position":[0,0],"size":[9,1]}"#;
    expect_error(&Setup::new(no_road), &["\"road_kind\"", "без road"]);
    for (extra, word) in [
        (r##","color":"#ffffff""##, "color"),
        (r##","shape":"box","color":"#ffffff""##, "shape"),
        (r#","model":"piece""#, "model"),
    ] {
        let filled = road("А", [0.0, 0.0], [9.0, 1.0], extra);
        expect_error(&Setup::new(&filled), &[word, "\"road_kind\""]);
    }
    let flat_road = road("А", [0.0, 0.0], [9.0, 1.0], "");
    let mut flat = Setup::new(&flat_road);
    flat.camera = false;
    flat.road_kinds = "";
    flat.models = "";
    expect_error(&flat, &["road_kind", "трёхмерной сцене"]);
}

#[test]
fn a_kind_named_nowhere_is_a_warning_and_a_model_named_only_by_a_kind_is_not() {
    let (_, warnings) = Setup::new("").load().expect("игра грузится");
    let paths: Vec<&str> = warnings.iter().map(|w| w.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("road_kinds → street")),
        "{warnings:?}"
    );
    assert!(
        !warnings.iter().any(|w| w.message.contains("модель")),
        "модели, названные видом, названы: {warnings:?}"
    );
    let named = road("А", [0.0, 0.0], [9.0, 1.0], "");
    let (_, warnings) = Setup::new(&named).load().expect("игра грузится");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("вид дороги") || w.message.contains("модель")),
        "{warnings:?}"
    );
    let mut by_code = Setup::new("");
    by_code.code = Some("function f(obj) obj.road_kind = \"street\" end");
    let (_, warnings) = by_code.load().expect("игра грузится");
    assert!(
        !warnings.iter().any(|w| w.message.contains("вид дороги")),
        "в тексте кода вид назван: {warnings:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Раскладка по миру
// -------------------------------------------------------------------------------------------

#[test]
fn a_road_with_a_kind_is_laid_in_pieces_and_nodes_and_drawn_by_instances_with_the_stretch() {
    let game = Setup::new(&cross()).game();
    let layout = game.road_kind_layout();
    assert_eq!(layout.pieces().len(), 20);
    assert_eq!(layout.nodes().len(), 5);
    drop(layout);

    let draws = road_draws(&game);
    let models = game.world.models();
    let (piece, node) = (
        models.index_of("piece").expect("модель"),
        models.index_of("node").expect("модель"),
    );
    assert_eq!(draws.pieces[piece].len(), 20);
    assert_eq!(draws.nodes[node].len(), 5);
    assert!(draws.pieces[node].is_empty() && draws.nodes[piece].is_empty());
    // Улица А — без растяжения, Б — сжата до 0,95; увеличение — единица: ширина куска равна ширине дороги.
    for instance in &draws.pieces[piece][..10] {
        assert_eq!(instance.base[1..3], [1.0, 1.0]);
    }
    for instance in &draws.pieces[piece][10..] {
        assert!(close(f64::from(instance.base[2]), 0.95), "{instance:?}");
    }
    // Кусок смотрит вдоль оси: у горизонтальной улицы локальная +Y (cos, sin) = (0, −1).
    let first = draws.pieces[piece][0];
    assert!(first.placement[2].abs() < 1e-6 && close(f64::from(first.placement[3]), -1.0));
}

#[test]
fn real_road_models_are_scaled_to_the_road_width_and_stretched_to_the_layout() {
    // Улица шириной 1, длиной 11: модель 7 × 14 → увеличение 1/7, длина куска 2; раскладка от 0,5 до 10,5.
    let scene = road("А", [0.0, 0.0], [11.0, 1.0], "");
    let game = Setup::real(&scene).game();
    let layout = game.road_kind_layout();
    assert_eq!(layout.pieces().len(), 5);
    for piece in layout.pieces() {
        assert!(close(piece.scale, 1.0 / 7.0), "{piece:?}");
        assert!(close(piece.length, 2.0), "{piece:?}");
        assert!(close(piece.stretch, 1.0), "{piece:?}");
    }
    let models = game.world.models();
    let street_models: Vec<usize> = ["road_street_a", "road_street_b"]
        .iter()
        .map(|name| models.index_of(name).expect("модель"))
        .collect();
    let used: Vec<usize> = layout.pieces().iter().map(|p| p.model).collect();
    assert_eq!(
        used,
        [0, 1, 0, 1, 0].map(|k| street_models[k]),
        "варианты идут по кругу"
    );
    assert!(layout.nodes().iter().all(|n| close(n.scale, 1.0 / 9.0)));
}

#[test]
fn a_closed_road_is_laid_exactly_like_an_open_one() {
    let open = Setup::new(&cross()).game();
    let closed_scene = [
        road("А", [0.0, 9.5], [21.0, 1.0], r#","road_speed":0"#),
        road("Б", [9.5, 0.0], [1.0, 20.0], r#","road_speed":0"#),
    ]
    .join(",");
    let closed = Setup::new(&closed_scene).game();
    assert_eq!(
        open.road_kind_layout().pieces(),
        closed.road_kind_layout().pieces()
    );
    assert_eq!(
        open.road_kind_layout().nodes(),
        closed.road_kind_layout().nodes()
    );
    assert!(!closed.road_kind_layout().pieces().is_empty());
}

#[test]
fn the_layout_is_rebuilt_when_a_road_moves_changes_kind_or_goes_but_not_when_its_speed_changes() {
    let scene = cross();
    let mut setup = Setup::new(&scene);
    setup.road_kinds =
        r#"{"alley":{"piece":"piece","node":"node"},"street":{"piece":"piece","node":"node"}}"#;
    let mut game = setup.game();
    let version = |game: &Game| game.road_kind_layout().version();
    let built = version(&game);
    assert_eq!(
        version(&game),
        built,
        "кадр без перемен ничего не пересобирает"
    );

    game.world.set_number(0, property::ROAD_SPEED, 3.0);
    assert_eq!(version(&game), built, "road_speed раскладку не меняет");
    game.world.set_number(0, property::ROAD_SPEED, 0.0);
    assert_eq!(version(&game), built);

    game.world.set_vec2(0, property::POSITION, [0.0, 12.5]);
    let moved = version(&game);
    assert!(moved > built, "сдвиг пересобирает");
    assert!(close(game.road_kind_layout().pieces()[0].center[1], 13.0));

    game.world.set_vec2(0, property::SIZE, [21.0, 2.0]);
    let resized = version(&game);
    assert!(resized > moved, "размер пересобирает");

    game.world
        .set_text(0, property::ROAD_KIND, "alley".to_string());
    let renamed = version(&game);
    assert!(renamed > resized, "смена вида пересобирает");

    game.world.set_flag(1, property::ROAD, false);
    assert!(
        version(&game) > renamed,
        "дорога без road исчезает из раскладки"
    );
    game.world.delete(0);
    assert!(game.road_kind_layout().pieces().is_empty());
}

#[test]
fn a_game_without_kinds_never_builds_a_layout() {
    let mut setup =
        Setup::new(r##"{"name":"А","road":true,"position":[0,0],"size":[9,1],"color":"#888888"}"##);
    setup.road_kinds = "";
    let game = setup.game();
    assert_eq!(game.road_kind_layout().version(), 0);
    assert!(road_draws(&game).pieces.iter().all(Vec::is_empty));
}

#[test]
fn a_road_with_a_kind_lies_flat_is_picked_by_its_rectangle_and_is_not_drawn_itself() {
    let clickable = [
        road(
            "А",
            [0.0, 9.5],
            [21.0, 1.0],
            r#","on_click":[["tag",true]]"#,
        ),
        road("Б", [9.5, 0.0], [1.0, 20.0], ""),
    ]
    .join(",");
    let game = Setup::new(&clickable).game();
    assert_eq!(surface::kind(&game.world, 0), Kind::Flat);
    assert_eq!(
        scene::on_click_target_ray(&game.world, &game.scene, None, [5.0, 10.0]),
        Some(0),
        "щелчок в игре ловит прямоугольник дороги на земле"
    );
    assert_eq!(
        scene::on_click_target_ray(&game.world, &game.scene, None, [30.0, 30.0]),
        None
    );
    let camera = game.camera_3d(WINDOW).expect("камера");
    let at = |x: f64, y: f64| camera.project([x, y, 0.0]).expect("в кадре");
    let pick =
        |x: f64, y: f64| scene::editor_target_ray(&game.world, &game.scene, &camera, at(x, y));
    assert_eq!(pick(5.0, 10.0), Some(0), "улица А");
    assert_eq!(pick(10.0, 3.0), Some(1), "улица Б");
    assert_eq!(pick(30.0, 30.0), None, "мимо дорог");

    let frame = compose_frame3d(
        &game,
        &camera,
        &Motion::default(),
        &[],
        &[],
        &mut FlatShapes::default(),
    );
    assert!(
        frame.shapes.is_empty() && frame.surface.is_empty(),
        "сама дорога не рисуется"
    );
    assert!(frame.models.iter().all(Vec::is_empty));
    let pieces: usize = frame.road_pieces.iter().map(Vec::len).sum();
    let nodes: usize = frame.road_nodes.iter().map(Vec::len).sum();
    assert_eq!((pieces, nodes), (20, 5));
}

// -------------------------------------------------------------------------------------------
// road_kind в коде
// -------------------------------------------------------------------------------------------

const KIND_CODE: &str = r#"
function to_alley(obj) obj.road_kind = "alley" end
function to_castle(obj) obj.road_kind = "castle" end
function to_number(obj) obj.road_kind = 5 end
function read_kind(obj) obj.tag = obj.road_kind == "street" end
function unroad(obj) obj.road = false end
function unroad_nil(obj) obj.road = nil end
"#;

fn code_game(function: &str, scene: &str) -> Game {
    let rules = format!(
        r#"{{"rules":[{{"kind":"check","for":{{"has":["actor"]}},"do":[["run","{function}"]]}}]}}"#
    );
    let mut setup = Setup::new(scene);
    setup.road_kinds =
        r#"{"alley":{"piece":"piece","node":"node"},"street":{"piece":"piece","node":"node"}}"#;
    setup.rules = &rules;
    setup.code = Some(KIND_CODE);
    setup.game()
}

fn actor_road() -> String {
    road("А", [0.0, 0.0], [9.0, 1.0], r#","actor":true"#)
}

fn step(game: &mut Game) {
    game.step(StepInput::empty());
}

#[test]
fn code_reads_and_writes_road_kind_by_name_and_the_layout_follows() {
    let mut game = code_game("read_kind", &actor_road());
    step(&mut game);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let tag = game.properties.resolve("tag").expect("tag");
    assert!(game.world.flag(0, tag), "код прочитал вид");

    let mut game = code_game("to_alley", &actor_road());
    let before = game.road_kind_layout().version();
    step(&mut game);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.text(0, property::ROAD_KIND), Some("alley"));
    assert!(
        game.road_kind_layout().version() > before,
        "куски нового вида — со следующего кадра"
    );
}

#[test]
fn code_writing_an_unknown_kind_or_road_kind_to_a_non_road_is_a_code_error() {
    for function in ["to_castle", "to_number"] {
        let mut game = code_game(function, &actor_road());
        step(&mut game);
        assert!(game.code_error().is_some(), "{function}: ошибки кода нет");
        assert_eq!(game.world.text(0, property::ROAD_KIND), Some("street"));
    }
    let mut game = code_game("to_castle", &actor_road());
    step(&mut game);
    let message = game
        .code_error()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("castle") && message.contains("street"),
        "{message}"
    );

    let plain = r##"{"name":"Б","actor":true,"color":"#ffffff","position":[0,0],"size":[9,1]}"##;
    let mut game = code_game("to_alley", plain);
    step(&mut game);
    let message = game
        .code_error()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("road_kind") && message.contains("road"),
        "{message}"
    );
}

#[test]
fn code_cannot_drop_road_from_a_road_with_a_kind() {
    // Требование 21: `road_kind` бывает только у дороги — снять `road` у дороги вида нельзя ни `false`, ни `nil`.
    for function in ["unroad", "unroad_nil"] {
        let mut game = code_game(function, &actor_road());
        step(&mut game);
        let message = game
            .code_error()
            .map(|e| e.message.clone())
            .unwrap_or_default();
        assert!(message.contains("road_kind"), "{function}: {message}");
        assert!(
            game.world.flag(0, property::ROAD),
            "{function}: признак снят"
        );
    }
}

// -------------------------------------------------------------------------------------------
// Прозрачность моделей
// -------------------------------------------------------------------------------------------

#[test]
fn alpha_modes_come_out_of_the_real_files() {
    let game = Setup::real("").game();
    let alpha = |model: &Model| -> Vec<(AlphaMode, f32)> {
        model
            .parts
            .iter()
            .map(|part| {
                let material = &model.materials[part.material];
                (material.alpha_mode, material.alpha_cutoff)
            })
            .collect()
    };
    let fence = model_of(&game, "fence");
    assert_eq!(alpha(fence), [(AlphaMode::Mask, 0.5)], "порог по умолчанию");
    assert!(!fence.has_blend());
    let window = model_of(&game, "window");
    let mut modes: Vec<AlphaMode> = alpha(window).into_iter().map(|(mode, _)| mode).collect();
    modes.sort_by_key(|mode| *mode as u8);
    assert_eq!(modes, [AlphaMode::Opaque, AlphaMode::Blend]);
    assert!(window.has_blend());
    let glass = window
        .materials
        .iter()
        .find(|m| m.alpha_mode == AlphaMode::Blend)
        .expect("стекло");
    assert!(close(f64::from(glass.base_color[3]), 0.35));
    assert!(
        model_of(&game, "road_street_a").has_blend(),
        "куски дороги — BLEND"
    );
}

#[test]
fn objects_with_a_blend_part_are_drawn_far_to_near_and_solid_ones_are_not_in_that_order() {
    let near_first = [
        r#"{"name":"ближнее","model":"window","position":[10,20],"size":[2,0.2]}"#,
        r#"{"name":"дальнее","model":"window","position":[10,10],"size":[2,0.2]}"#,
        r#"{"name":"забор","model":"fence","position":[20,20],"size":[4,0.2]}"#,
    ]
    .join(",");
    let game = Setup::real(&near_first).game();
    let camera = game.camera_3d(WINDOW).expect("камера");
    let draws = model_draws(&game);
    let window = game.world.models().index_of("window").expect("модель") as u32;
    assert_eq!(
        draws.blended_far_to_near(camera.eye),
        [
            BlendedDraw {
                model: window,
                index: 1
            },
            BlendedDraw {
                model: window,
                index: 0
            },
        ],
        "камера смотрит вглубь сцены: дальше — меньше y"
    );
    assert!(
        draws.instances.iter().flatten().all(|i| i.base[2] == 1.0),
        "модели не растянуты"
    );
    let frame = compose_frame3d(
        &game,
        &camera,
        &Motion::default(),
        &[],
        &[],
        &mut FlatShapes::default(),
    );
    assert_eq!(
        frame.blended.len(),
        2,
        "забор с MASK в этот порядок не входит"
    );
    assert_eq!(
        frame.models[window as usize].len(),
        2,
        "и стекло, и рама — те же объекты кадра"
    );
}

// -------------------------------------------------------------------------------------------
// Шейдер
// -------------------------------------------------------------------------------------------

fn parse_and_validate(source: &str) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL разбирается");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("WGSL проходит проверку");
    (module, info)
}

/// Как `wgpu-hal` собирает GLSL для WebGL2: биндинг → слот; у групп своя нумерация.
fn to_webgl2_glsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry_point: &str,
    bindings: &[(u32, u32)],
) -> String {
    use naga::back::glsl;
    let mut binding_map = glsl::BindingMap::default();
    let mut slots = [0u8; 3];
    for &(group, binding) in bindings {
        let kind = match (group, binding) {
            (0, 0 | 5) | (1, 0) => 0,
            (0, 1 | 3 | 6..=8) | (1, 1 | 3 | 5 | 7) => 1,
            _ => 2,
        };
        binding_map.insert(naga::ResourceBinding { group, binding }, slots[kind]);
        slots[kind] += 1;
    }
    let options = glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE
            | glsl::WriterFlags::FORCE_POINT_SIZE,
        binding_map,
        zero_initialize_workgroup_memory: true,
    };
    let pipeline = glsl::PipelineOptions {
        shader_stage: stage,
        entry_point: entry_point.to_string(),
        multiview: None,
    };
    let mut out = String::new();
    glsl::Writer::new(
        &mut out,
        module,
        info,
        &options,
        &pipeline,
        naga::proc::BoundsCheckPolicies::default(),
    )
    .unwrap_or_else(|e| panic!("{entry_point}: писатель GLSL не создан: {e}"))
    .write()
    .unwrap_or_else(|e| panic!("{entry_point}: GLSL не записан: {e}"));
    out
}

#[test]
fn the_alpha_shaders_validate_and_translate_to_glsl_es_300() {
    let (module, info) = parse_and_validate(include_str!("../shaders/scene3d.wgsl"));
    let shared = [(0, 0), (0, 3), (0, 4)];
    let material: Vec<(u32, u32)> = (0..9).map(|binding| (1, binding)).collect();
    let all: Vec<(u32, u32)> = shared.iter().chain(&material).copied().collect();
    let base_map = [(1, 0), (1, 1), (1, 2)];
    for (stage, entry, bindings) in [
        (
            naga::ShaderStage::Vertex,
            "vs_shadow_model_mask",
            &[(0, 0)][..],
        ),
        (
            naga::ShaderStage::Fragment,
            "fs_shadow_model_mask",
            &base_map[..],
        ),
        (naga::ShaderStage::Fragment, "fs_model_mask", &all[..]),
        (naga::ShaderStage::Fragment, "fs_model_blend", &all[..]),
    ] {
        let glsl = to_webgl2_glsl(&module, &info, stage, entry, bindings);
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
    }
    for entry in ["fs_model_mask", "fs_shadow_model_mask"] {
        let bindings = if entry == "fs_model_mask" {
            &all[..]
        } else {
            &base_map[..]
        };
        let glsl = to_webgl2_glsl(&module, &info, naga::ShaderStage::Fragment, entry, bindings);
        assert!(
            glsl.contains("discard"),
            "{entry}: точки ниже порога отбрасываются"
        );
    }
    let opaque = to_webgl2_glsl(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs_model",
        &all,
    );
    assert!(
        !opaque.contains("discard"),
        "у сплошных частей отбрасывания нет"
    );
}

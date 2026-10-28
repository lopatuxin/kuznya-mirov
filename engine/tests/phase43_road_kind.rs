//! Фаза 43 — вид дороги и фаза 46 — перекрёстки готовыми моделями. Игры собираются в коде теста, как в
//! `phase25_models.rs`. Модели видов и перекрёстков — маленькие плоские прямоугольники, собранные здесь же (кусок 1 × 2,
//! модели перекрёстков 3 × 3), и настоящие файлы: куски дорог и перекрёстки с `BLEND` из `games/transport/models`, забор
//! с `MASK` и окно со стеклом из `art/modelcheck`. Проверяются `scene.road_kinds` и `scene.road_junctions` с ошибками перед
//! запуском, `road_kind` дороги, раскладка по миру с перекрёстками и её пересборка, `road_kind` в коде, выбор дороги в
//! редакторе, кадр, изогнутые копии перекрёстков для видеокарты, порядок частей `BLEND` и шейдер.

use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::model::{AlphaMode, Model};
use engine::core::property;
use engine::core::scene;
use engine::core::surface::{self, Kind};
use engine::data::error::GameError;
use engine::data::load::{ImageVerdict, ModelFile, load_rest_with_models, read_entry};
use engine::render::models::{BlendedDraw, junction_mesh, model_draws, road_draws};
use engine::render::relief::FlatShapes;
use engine::render::scene3d::compose_frame3d;
use engine::render::wind::Motion;

const WINDOW: [f32; 2] = [1920.0, 1080.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag","tag":"flag"}}"#;
const KINDS: &str = r#"{"street":{"piece":"piece"}}"#;
const MODELS: &str = r#"{"piece":{"path":"models/piece.glb"},"end":{"path":"models/end.glb"},"bend":{"path":"models/bend.glb"},"tee":{"path":"models/tee.glb"},"cross":{"path":"models/cross.glb"}}"#;
/// Рукава моделей `MODELS`: устье в 3 от середины, гаснущая полоса 2 — куски кончаются в 1 от точки стыка (масштаб
/// единица: дорога шириной 1, кусок шириной 1); у тупика вырез 0,2.
const JUNCTIONS: &str = r#"{
"end":{"arms":[{"dir":0,"kind":"street","reach":1.2,"fade":1}]},
"bend":{"arms":[{"dir":0,"kind":"street","reach":3,"fade":2},{"dir":90,"kind":"street","reach":3,"fade":2}]},
"tee":{"arms":[{"dir":0,"kind":"street","reach":3,"fade":2},{"dir":90,"kind":"street","reach":3,"fade":2},{"dir":180,"kind":"street","reach":3,"fade":2}]},
"cross":{"arms":[{"dir":0,"kind":"street","reach":3,"fade":2},{"dir":90,"kind":"street","reach":3,"fade":2},{"dir":180,"kind":"street","reach":3,"fade":2},{"dir":270,"kind":"street","reach":3,"fade":2}]}}"#;
const REAL_KINDS: &str =
    r#"{"street":{"piece":["road_street_a","road_street_b"]},"main":{"piece":"road_main_a"}}"#;
const REAL_MODELS: &str = r#"{"road_street_a":{"path":"models/road_street_a.glb"},"road_street_b":{"path":"models/road_street_b.glb"},"road_main_a":{"path":"models/road_main_a.glb"},"road_end_street":{"path":"models/road_end_street.glb"},"fence":{"path":"models/fence.glb"},"window":{"path":"models/window.glb"}}"#;
const REAL_JUNCTIONS: &str =
    r#"{"road_end_street":{"arms":[{"dir":0,"kind":"street","reach":5.833,"fade":2.333}]}}"#;

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
    let plate = || page_file(flat_glb(3.0, 3.0, ""));
    vec![
        ("piece".to_string(), page_file(flat_glb(1.0, 2.0, ""))),
        ("end".to_string(), plate()),
        ("bend".to_string(), plate()),
        ("tee".to_string(), plate()),
        ("cross".to_string(), plate()),
    ]
}

fn real_files() -> Vec<(String, ModelFile)> {
    [
        "road_street_a",
        "road_street_b",
        "road_main_a",
        "road_end_street",
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
    road_junctions: &'a str,
    skip_media: bool,
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
            road_junctions: JUNCTIONS,
            skip_media: false,
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
            road_junctions: REAL_JUNCTIONS,
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
        let junctions = if self.road_junctions.is_empty() {
            String::new()
        } else {
            format!(r#","road_junctions":{}"#, self.road_junctions)
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
            r##"{{"name":"R","scene":{{"width":60,"height":40,"background":"#000000"{camera}{kinds}{junctions}}},
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
            self.skip_media,
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
fn road_kinds_and_junction_models_resolve_to_models_with_their_sizes() {
    let game = Setup::real("").game();
    let kinds = game.world.road_kinds();
    let street = kinds
        .get(kinds.index_of("street").expect("вид"))
        .expect("вид");
    assert_eq!(street.pieces.len(), 2, "сменные варианты");
    assert!(close(street.pieces[0].width, 7.0) && close(street.pieces[0].length, 14.0));
    let main = kinds
        .get(kinds.index_of("main").expect("вид"))
        .expect("вид");
    assert_eq!(main.pieces.len(), 1, "имя куска строкой — один вариант");
    assert!(close(main.pieces[0].width, 12.0) && close(main.pieces[0].length, 16.0));

    // Модель перекрёстка: номер в таблице моделей и рукава — направление в радианах, номер вида, устье, полоса и
    // ширина рукава — ширина первого куска его вида.
    let [end] = kinds.junctions() else {
        panic!("одна модель перекрёстка: {:?}", kinds.junctions())
    };
    assert_eq!(
        game.world.models().index_of("road_end_street"),
        Some(end.model)
    );
    let [arm] = end.arms[..] else {
        panic!("один рукав")
    };
    assert_eq!(arm.kind, kinds.index_of("street").expect("вид"));
    assert!(close(arm.dir, 0.0) && close(arm.reach, 5.833) && close(arm.fade, 2.333));
    assert!(close(arm.width, 7.0), "ширина куска улицы");
}

#[test]
fn junction_arm_directions_are_taken_modulo_a_full_turn() {
    let mut setup = Setup::new("");
    setup.road_junctions = r#"{"end":{"arms":[{"dir":-90,"kind":"street","reach":1,"fade":0},{"dir":450,"kind":"street","reach":1,"fade":0.5}]}}"#;
    let game = setup.game();
    let [end] = game.world.road_kinds().junctions() else {
        panic!("одна модель")
    };
    let degrees: Vec<f64> = end.arms.iter().map(|arm| arm.dir.to_degrees()).collect();
    assert!(
        close(degrees[0], 270.0) && close(degrees[1], 90.0),
        "{degrees:?}"
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
fn a_kind_without_a_piece_or_with_a_bad_piece_is_an_error() {
    let cases: [(&str, &[&str]); 5] = [
        (r#"{"street":{}}"#, &["piece"]),
        (r#"{"street":{"piece":[]}}"#, &["piece", "пустой"]),
        (r#"{"street":{"piece":3}}"#, &["piece"]),
        (r#"{"street":{"piece":["piece",3]}}"#, &["piece", "[1]"]),
        (r#"{"street":"piece"}"#, &["street", "объект"]),
    ];
    for (kinds, words) in cases {
        let mut setup = Setup::new("");
        setup.road_kinds = kinds;
        expect_error(&setup, words);
    }
}

/// Фаза 46, требование 19: `node`, `corner` и `edge` в виде дороги — неизвестные ключи, как любой лишний.
#[test]
fn node_corner_and_edge_of_a_kind_are_unknown_keys() {
    for key in ["node", "corner", "edge"] {
        let kinds = format!(r#"{{"street":{{"piece":"piece","{key}":"end"}}}}"#);
        let mut setup = Setup::new("");
        setup.road_kinds = &kinds;
        expect_error(&setup, &["неизвестное поле", key]);
    }
}

#[test]
fn a_model_that_files_models_does_not_declare_is_an_error() {
    let mut setup = Setup::new("");
    setup.road_kinds = r#"{"street":{"piece":"castle"}}"#;
    expect_error(&setup, &["castle", "piece", "объявлены"]);
    setup.road_kinds = r#"{"street":{"piece":["piece","castle"]}}"#;
    expect_error(&setup, &["castle", "piece"]);
}

#[test]
fn a_piece_model_flat_to_nothing_in_x_or_z_is_an_error() {
    let mut setup = Setup::new("");
    setup.files[0].1 = page_file(flat_glb(0.0, 2.0, ""));
    expect_error(&setup, &["piece", "размер по X или Z"]);
    setup.files[0].1 = page_file(flat_glb(1.0, 0.0, ""));
    expect_error(&setup, &["piece", "размер по X или Z"]);
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
    flat.road_junctions = "";
    flat.models = "";
    expect_error(&flat, &["road_kind", "трёхмерной сцене"]);
}

#[test]
fn a_kind_named_nowhere_is_a_warning_and_a_model_named_only_by_a_kind_or_a_junction_is_not() {
    let (_, warnings) = Setup::new("").load().expect("игра грузится");
    let paths: Vec<&str> = warnings.iter().map(|w| w.path.as_str()).collect();
    assert!(
        paths.iter().any(|p| p.ends_with("road_kinds → street")),
        "{warnings:?}"
    );
    assert!(
        !warnings.iter().any(|w| w.message.contains("модель")),
        "модели, названные видом и перекрёстками, названы: {warnings:?}"
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

/// Фаза 46, требование 20: предупреждение «модель нигде не названа» не касается моделей из `road_junctions`, а
/// модель, не названная нигде, по-прежнему его получает.
#[test]
fn a_junction_model_is_named_but_a_spare_model_still_gets_the_warning() {
    let mut setup = Setup::new("");
    setup.models = r#"{"piece":{"path":"models/piece.glb"},"end":{"path":"models/end.glb"},"bend":{"path":"models/bend.glb"},"tee":{"path":"models/tee.glb"},"cross":{"path":"models/cross.glb"},"spare":{"path":"models/spare.glb"}}"#;
    setup
        .files
        .push(("spare".to_string(), page_file(flat_glb(3.0, 3.0, ""))));
    let (_, warnings) = setup.load().expect("игра грузится");
    let about_models: Vec<&str> = warnings
        .iter()
        .filter(|w| w.message.contains("модель"))
        .map(|w| w.message.as_str())
        .collect();
    assert_eq!(about_models.len(), 1, "{warnings:?}");
    assert!(about_models[0].contains("spare"), "{about_models:?}");
}

/// Фаза 46, требование 18: каждая ошибка таблицы `scene.road_junctions`.
#[test]
fn every_error_of_the_road_junctions_table_stops_the_game() {
    let arm = |dir: &str, kind: &str, reach: &str, fade: &str| {
        format!(r#"{{"dir":{dir},"kind":{kind},"reach":{reach},"fade":{fade}}}"#)
    };
    let model = |arms: &str| format!(r#"{{"end":{{"arms":{arms}}}}}"#);
    let one = |dir: &str, kind: &str, reach: &str, fade: &str| {
        model(&format!("[{}]", arm(dir, kind, reach, fade)))
    };
    let cases: Vec<(String, Vec<&str>)> = vec![
        (
            r#"["end"]"#.to_string(),
            vec!["\"road_junctions\"", "таблица"],
        ),
        (r#"{"end":3}"#.to_string(), vec!["end", "объект"]),
        (r#"{"end":{}}"#.to_string(), vec!["arms", "обязательная"]),
        (model("3"), vec!["arms", "массив"]),
        (model("[]"), vec!["arms", "пустой"]),
        (model("[3]"), vec!["[0]", "объект"]),
        (
            model(r#"[{"kind":"street","reach":1,"fade":0}]"#),
            vec!["[0]", "\"dir\""],
        ),
        (
            model(r#"[{"dir":0,"reach":1,"fade":0}]"#),
            vec!["[0]", "\"kind\""],
        ),
        (
            model(r#"[{"dir":0,"kind":"street","fade":0}]"#),
            vec!["[0]", "\"reach\""],
        ),
        (
            model(r#"[{"dir":0,"kind":"street","reach":1}]"#),
            vec!["[0]", "\"fade\""],
        ),
        (
            one(r#""north""#, "\"street\"", "1", "0"),
            vec!["dir", "число"],
        ),
        (one("0", "7", "1", "0"), vec!["kind", "строка"]),
        (one("0", "\"street\"", "true", "0"), vec!["reach", "число"]),
        (one("0", "\"street\"", "1", "null"), vec!["fade", "число"]),
        (
            one("0", "\"alley\"", "1", "0"),
            vec!["kind", "alley", "street"],
        ),
        (one("0", "\"street\"", "0", "0"), vec!["reach", "больше 0"]),
        (one("0", "\"street\"", "-2", "0"), vec!["reach", "больше 0"]),
        (one("0", "\"street\"", "3", "-1"), vec!["fade", "от 0 до"]),
        (one("0", "\"street\"", "3", "3.5"), vec!["fade", "от 0 до"]),
        (one("0", "\"street\"", "3", "3"), vec!["fade", "не включая"]),
        (
            model(&format!(
                "[{},{}]",
                arm("0", "\"street\"", "3", "1"),
                arm("360", "\"street\"", "3", "1")
            )),
            vec!["dir", "уже занято"],
        ),
        (
            model(r#"[{"dir":0,"kind":"street","reach":3,"fade":1,"width":2}]"#),
            vec!["неизвестное поле", "width"],
        ),
        (
            r#"{"end":{"arms":[{"dir":0,"kind":"street","reach":3,"fade":1}],"extra":1}}"#
                .to_string(),
            vec!["неизвестное поле", "extra"],
        ),
        (
            r#"{"castle":{"arms":[{"dir":0,"kind":"street","reach":3,"fade":1}]}}"#.to_string(),
            vec!["castle", "объявлены"],
        ),
    ];
    for (junctions, words) in cases {
        let mut setup = Setup::new("");
        setup.road_junctions = &junctions;
        expect_error(&setup, &words);
    }
    let mut flat = Setup::new("");
    flat.camera = false;
    flat.road_kinds = "";
    flat.models = "";
    expect_error(&flat, &["\"road_junctions\"", "трёхмерной сцене"]);
}

/// Крайние значения требования 18 — не ошибки: `fade` от нуля включая до `reach` не включая.
#[test]
fn the_limits_of_reach_and_fade_are_allowed() {
    for (reach, fade) in [("0.5", "0"), ("3", "2.999"), ("3", "1.5")] {
        let junctions = format!(
            r#"{{"end":{{"arms":[{{"dir":0,"kind":"street","reach":{reach},"fade":{fade}}}]}}}}"#
        );
        let mut setup = Setup::new("");
        setup.road_junctions = &junctions;
        assert!(setup.load().is_ok(), "reach {reach}, fade {fade}");
    }
}

// -------------------------------------------------------------------------------------------
// Раскладка по миру
// -------------------------------------------------------------------------------------------

#[test]
fn a_road_with_a_kind_is_laid_in_pieces_cut_by_junctions_and_drawn_by_instances_with_the_stretch() {
    let game = Setup::new(&cross()).game();
    let layout = game.road_kind_layout();
    // Крест вырезает из каждой улицы отрезок от устья до устья: 4 + 5 кусков у А, 4 + 4 у Б.
    assert_eq!(layout.pieces().len(), 17);
    assert_eq!(layout.junctions().len(), 5);
    drop(layout);

    let draws = road_draws(&game);
    let models = game.world.models();
    let (piece, end, cross_model) = (
        models.index_of("piece").expect("модель"),
        models.index_of("end").expect("модель"),
        models.index_of("cross").expect("модель"),
    );
    assert_eq!(draws.pieces[piece].len(), 17);
    assert!(draws.pieces[end].is_empty() && draws.pieces[cross_model].is_empty());
    let count = |model: usize| draws.junctions.iter().filter(|j| j.model == model).count();
    assert_eq!(
        (count(cross_model), count(end)),
        (1, 4),
        "крест и четыре тупика"
    );
    // Куски А слева от креста (от 0,7 до 9) длиной 2,075 растянуты в 1,0375 раза, справа (от 11 до 20,3) длиной 1,86
    // сжаты до 0,93; увеличение — единица: ширина куска равна ширине дороги.
    let stretch = |k: usize| f64::from(draws.pieces[piece][k].base[2]);
    assert!(close(stretch(0), 1.0375) && close(stretch(4), 0.93));
    assert!(
        draws.pieces[piece]
            .iter()
            .all(|instance| instance.base[1] == 1.0)
    );
    // Кусок смотрит вдоль оси: у горизонтальной улицы локальная +Y (cos, sin) = (0, −1).
    let first = draws.pieces[piece][0];
    assert!(first.placement[2].abs() < 1e-6 && close(f64::from(first.placement[3]), -1.0));
}

#[test]
fn real_road_models_are_scaled_to_the_road_width_and_stretched_to_the_layout() {
    // Улица шириной 1, длиной 11: кусок 7 × 14 → увеличение 1/7, длина куска 2. Раскладка от 0,5 до 10,5; в тупиках —
    // `road_end_street` с масштабом 1/7: гаснущая полоса начинается в (5,833 − 2,333) / 7 = 0,5 от точки тупика.
    let scene = road("А", [0.0, 0.0], [11.0, 1.0], "");
    let game = Setup::real(&scene).game();
    let layout = game.road_kind_layout();
    // Куски лежат от 1 до 10: длина 9, `n` = 4,5 → 5, каждый сжат до 1,8.
    assert_eq!(layout.pieces().len(), 5);
    for piece in layout.pieces() {
        assert!(close(piece.scale, 1.0 / 7.0), "{piece:?}");
        assert!(close(piece.length, 1.8), "{piece:?}");
        assert!(close(piece.stretch, 0.9), "{piece:?}");
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
    // Два тупика одной моделью, увеличенной во столько же раз, что и куски.
    let end = models.index_of("road_end_street").expect("модель");
    assert_eq!(layout.junctions().len(), 2);
    assert!(layout.junctions().iter().all(|junction| {
        junction.model == end && close(junction.scale, 1.0 / 7.0) && !junction.mirrored
    }));
    let model = model_of(&game, "road_end_street");
    assert!(
        layout
            .junctions()
            .iter()
            .all(|junction| junction.parts.len() == model.parts.len())
    );
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
        open.road_kind_layout().junctions(),
        closed.road_kind_layout().junctions()
    );
    assert!(!closed.road_kind_layout().pieces().is_empty());
}

#[test]
fn the_layout_is_rebuilt_when_a_road_moves_changes_kind_or_goes_but_not_when_its_speed_changes() {
    let scene = cross();
    let mut setup = Setup::new(&scene);
    setup.road_kinds = r#"{"alley":{"piece":"piece"},"street":{"piece":"piece"}}"#;
    let mut game = setup.game();
    let version = |game: &Game| game.road_kind_layout().version();
    let junctions = |game: &Game| Rc::clone(game.road_kind_layout().junctions());
    let built = version(&game);
    let meshes = junctions(&game);
    assert_eq!(
        version(&game),
        built,
        "кадр без перемен ничего не пересобирает"
    );
    assert!(Rc::ptr_eq(&meshes, &road_draws(&game).junctions));

    game.world.set_number(0, property::ROAD_SPEED, 3.0);
    assert_eq!(version(&game), built, "road_speed раскладку не меняет");
    game.world.set_number(0, property::ROAD_SPEED, 0.0);
    assert_eq!(version(&game), built);
    assert!(
        Rc::ptr_eq(&meshes, &junctions(&game)),
        "и сетки перекрёстков те же: видеокарта их не заливает заново"
    );

    game.world.set_vec2(0, property::POSITION, [0.0, 12.5]);
    let moved = version(&game);
    assert!(moved > built, "сдвиг пересобирает");
    assert!(
        !Rc::ptr_eq(&meshes, &junctions(&game)),
        "и сетки перекрёстков новые"
    );
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
    setup.road_junctions = "";
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
    assert_eq!((pieces, frame.road_junctions.len()), (17, 5));
}

// -------------------------------------------------------------------------------------------
// Перекрёстки: изогнутые копии моделей для видеокарты
// -------------------------------------------------------------------------------------------

#[test]
fn junction_copies_are_handed_to_the_video_card_part_by_part_grouped_by_the_model() {
    let game = Setup::new(&cross()).game();
    let draws = road_draws(&game);
    let mesh = junction_mesh(&draws.junctions);
    let vertices: usize = draws
        .junctions
        .iter()
        .flat_map(|j| &j.parts)
        .map(|part| part.vertices.len())
        .sum();
    assert_eq!(mesh.floats.len(), vertices * 16);
    assert!(
        mesh.indices
            .iter()
            .all(|&index| (index as usize) < vertices)
    );
    // Крест и четыре тупика — две модели, у каждой одна часть: два прогона.
    assert_eq!(mesh.runs.len(), 2);
    assert!(mesh.runs.iter().all(|run| run.part == 0));
    let total: u32 = mesh.runs.iter().map(|run| run.count).sum();
    assert_eq!(total as usize, mesh.indices.len());
    let models = game.world.models();
    let cross_model = models.index_of("cross").expect("модель");
    let end = models.index_of("end").expect("модель");
    let count_of = |model: usize| {
        mesh.runs
            .iter()
            .find(|run| run.model == model)
            .map(|run| run.count)
    };
    // Плоский прямоугольник модели — два треугольника, шесть индексов; тупиков четыре, крест один.
    assert_eq!(count_of(cross_model), Some(6));
    assert_eq!(count_of(end), Some(24));
    // Вершины лежат в мире: середина креста — (10; 10).
    let near_middle = mesh
        .floats
        .chunks(16)
        .filter(|vertex| {
            (f64::from(vertex[0]) - 10.0).abs() < 2.0 && (f64::from(vertex[1]) - 10.0).abs() < 2.0
        })
        .count();
    assert!(near_middle >= 4, "{near_middle}");

    let none = Setup::new("").game();
    assert!(
        junction_mesh(&road_draws(&none).junctions)
            .indices
            .is_empty()
    );
}

/// Крайний случай плана: модель, названная в `road_junctions`, без частей не выбирается — перекрёстка с ней нет, и
/// куски его улиц доходят до точки стыка.
#[test]
fn a_junction_model_without_parts_is_never_chosen_and_the_pieces_run_up_to_the_joint() {
    let scene = cross();
    let mut setup = Setup::new(&scene);
    setup.skip_media = true;
    setup.files.clear();
    let game = setup.game();
    assert!(
        game.world
            .models()
            .iter()
            .all(|model| model.parts.is_empty())
    );
    let layout = game.road_kind_layout();
    assert!(layout.junctions().is_empty());
    assert!(!layout.pieces().is_empty());
    let nearest = layout
        .pieces()
        .iter()
        .map(|p| (p.center[0] - 10.0).abs().min((p.center[1] - 10.0).abs()) - p.length / 2.0)
        .fold(f64::INFINITY, f64::min);
    assert!(nearest < 1e-9, "куски доходят до точки стыка: {nearest}");
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
    setup.road_kinds = r#"{"alley":{"piece":"piece"},"street":{"piece":"piece"}}"#;
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
        draws.blended_far_to_near(&camera),
        [
            BlendedDraw {
                model: window,
                index: 1,
                dissolving: false
            },
            BlendedDraw {
                model: window,
                index: 0,
                dissolving: false
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
        (naga::ShaderStage::Vertex, "vs_junction", &[(0, 0)][..]),
        (naga::ShaderStage::Vertex, "vs_shadow_model", &[(0, 0)][..]),
        (naga::ShaderStage::Fragment, "fs_model_dissolve", &all[..]),
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
    let dissolving = to_webgl2_glsl(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs_model_dissolve",
        &all,
    );
    assert!(
        dissolving.contains("discard"),
        "«Здания», срез: точки, отданные модели среза, отбрасываются"
    );
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

/// Требование 6: при равной цене побеждает модель, что раньше в `scene.road_junctions` — по порядку записи в файле, а не по
/// алфавиту (`serde_json::Map` хранит ключи по алфавиту).
#[test]
fn an_equal_price_is_won_by_the_junction_model_written_first_in_the_file() {
    let arms = r#"[{"dir":0,"kind":"street","reach":3,"fade":2},{"dir":90,"kind":"street","reach":3,"fade":2},{"dir":180,"kind":"street","reach":3,"fade":2},{"dir":270,"kind":"street","reach":3,"fade":2}]"#;
    let junctions = format!(
        r#"{{"end":{{"arms":[{{"dir":0,"kind":"street","reach":1.2,"fade":1}}]}},"zeta":{{"arms":{arms}}},"alpha":{{"arms":{arms}}}}}"#
    );
    let scene = cross();
    let mut setup = Setup::new(&scene);
    setup.models = r#"{"piece":{"path":"models/piece.glb"},"end":{"path":"models/end.glb"},"zeta":{"path":"models/zeta.glb"},"alpha":{"path":"models/alpha.glb"}}"#;
    setup.road_junctions = &junctions;
    setup.files = vec![
        ("piece".to_string(), page_file(flat_glb(1.0, 2.0, ""))),
        ("end".to_string(), page_file(flat_glb(3.0, 3.0, ""))),
        ("zeta".to_string(), page_file(flat_glb(3.0, 3.0, ""))),
        ("alpha".to_string(), page_file(flat_glb(3.0, 3.0, ""))),
    ];
    let game = setup.game();
    let models = game.world.models();
    let (zeta, alpha) = (
        models.index_of("zeta").expect("модель"),
        models.index_of("alpha").expect("модель"),
    );
    let order: Vec<usize> = game
        .world
        .road_kinds()
        .junctions()
        .iter()
        .map(|junction| junction.model)
        .collect();
    assert_eq!(
        order,
        [models.index_of("end").expect("модель"), zeta, alpha]
    );
    let layout = game.road_kind_layout();
    let crossing = layout
        .junctions()
        .iter()
        .find(|junction| close(junction.center[0], 10.0) && close(junction.center[1], 10.0))
        .expect("крест");
    assert_eq!(
        crossing.model, zeta,
        "записана первой, хотя по алфавиту позже"
    );
}

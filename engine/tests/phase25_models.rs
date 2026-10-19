//! Фаза 25 — модели. Игры собираются в коде теста, как в `phase17_terrain.rs`; модели берутся из настоящих
//! файлов `art/modelcheck/*.glb` (дом, берёза, машина, оси) — картинкам внутри них тест отвечает за страницу
//! готовыми точками. Проверяются `files.models` и ошибки перед запуском, размер, место и поворот модели,
//! столб для хода игры, щелчок по коробке вокруг модели, `model` в коде, `model_sizes`, кадр без видеокарты и
//! шейдер модели.

use std::fs;
use std::path::PathBuf;

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::model::Model;
use engine::core::property;
use engine::core::scene;
use engine::core::shapes::Body;
use engine::core::surface;
use engine::data::error::GameError;
use engine::data::load::{ImageVerdict, ModelFile, load_rest_with_models, read_entry};
use engine::render::models::model_draws;
use engine::render::relief::FlatShapes;
use engine::render::scene3d::compose_frame3d;
use engine::render::wind::Motion;

const WINDOW: [f32; 2] = [1920.0, 1080.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"actor":"flag","blocker":"flag","tag":"flag"}}"#;
const ALL_MODELS: &str = r#"{"house":{"path":"models/house.glb"},"birch":{"path":"models/birch.glb"},"car":{"path":"models/car.glb"},"axes":{"path":"models/axes.glb"}}"#;

fn art(name: &str) -> Vec<u8> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("../art/modelcheck");
    path.push(format!("{name}.glb"));
    fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

/// Ответ страницы по модели: байты файла и две картинки внутри него — точки не те же, что в файле, но
/// разжатые и нужного вида.
fn page_file(bytes: Vec<u8>) -> ModelFile {
    let image = |_: usize| ImageVerdict::Ok {
        width: 4,
        height: 4,
        pixels: vec![200; 64],
    };
    ModelFile {
        bytes: Some(bytes),
        images: (0..2).map(|index| (index, image(index))).collect(),
    }
}

fn real_files() -> Vec<(String, ModelFile)> {
    ["house", "birch", "car", "axes"]
        .into_iter()
        .map(|name| (name.to_string(), page_file(art(name))))
        .collect()
}

struct Setup<'a> {
    camera: bool,
    models: &'a str,
    scene: &'a str,
    rules: &'a str,
    code: Option<&'a str>,
    files: Vec<(String, ModelFile)>,
    skip_media: bool,
}

impl<'a> Setup<'a> {
    fn new(scene: &'a str) -> Setup<'a> {
        Setup {
            camera: true,
            models: ALL_MODELS,
            scene,
            rules: r#"{"rules":[]}"#,
            code: None,
            files: real_files(),
            skip_media: false,
        }
    }

    fn game_json(&self) -> String {
        let camera = if self.camera {
            r#","view_height":16,"camera":{"pitch":55}"#
        } else {
            ""
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
            r##"{{"name":"M","scene":{{"width":40,"height":30,"background":"#000000"{camera}}},
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

fn object(name: &str, model: &str, position: [f64; 2], size: [f64; 2], extra: &str) -> String {
    format!(
        r#"{{"name":"{name}","model":"{model}","position":[{},{}],"size":[{},{}]{extra}}}"#,
        position[0], position[1], size[0], size[1]
    )
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

fn model_of<'a>(game: &'a Game, name: &str) -> &'a Model {
    let models = game.world.models();
    models
        .get(models.index_of(name).expect("модель объявлена"))
        .expect("модель есть")
}

// -------------------------------------------------------------------------------------------
// Файлы и загрузка
// -------------------------------------------------------------------------------------------

#[test]
fn the_four_test_models_load_with_their_sizes_materials_and_images() {
    let game = Setup::new("").game();
    let names: Vec<&str> = game.world.models().names().collect();
    assert_eq!(
        names,
        ["house", "birch", "car", "axes"],
        "порядок объявления"
    );

    let house = model_of(&game, "house");
    // Стены 8 × 6, крыша шире на 0,4 с каждой стороны, высота до конька — 7.
    assert!(close(house.size[0], 8.8), "{:?}", house.size);
    assert!(close(house.size[1], 6.8), "{:?}", house.size);
    assert!(close(house.size[2], 7.0));
    assert_eq!(house.images.len(), 2, "карта цвета и карта мелкого рельефа");
    let brick = house
        .materials
        .iter()
        .find(|m| m.normal_texture.is_some() && m.base_texture.is_some())
        .expect("кирпич с двумя картами");
    assert_eq!(brick.roughness, 0.9);
    assert!(
        house
            .parts
            .iter()
            .any(|p| p.vertices.iter().any(|v| v.tangent[3] != 0.0)),
        "касательные из файла"
    );

    let birch = model_of(&game, "birch");
    assert!(
        birch
            .parts
            .iter()
            .flat_map(|p| &p.vertices)
            .any(|v| v.color[..3] != [1.0, 1.0, 1.0]),
        "цвет вершин"
    );

    let car = model_of(&game, "car");
    assert_eq!(car.materials.len(), 4);
    let metallic: Vec<f32> = car.materials.iter().map(|m| m.metallic).collect();
    assert!(metallic.contains(&0.6) && metallic.contains(&0.0));
    assert!(
        car.parts.len() <= 4,
        "части с одним материалом сложены вместе"
    );

    let axes = model_of(&game, "axes");
    assert_eq!(axes.parts.len(), 3);
}

#[test]
fn model_sizes_gives_the_declared_models_in_cells() {
    let game = Setup::new("").game();
    let sizes: Vec<(&str, [f64; 3])> = game
        .world
        .models()
        .iter()
        .map(|model| (model.name.as_str(), model.size))
        .collect();
    assert_eq!(sizes[0].0, "house");
    assert!(close(sizes[0].1[0], 8.8) && close(sizes[0].1[2], 7.0));
    assert_eq!(sizes[3].0, "axes");
}

#[test]
fn a_model_in_a_flat_scene_and_files_models_in_a_flat_scene_are_errors() {
    let mut setup = Setup::new("");
    setup.camera = false;
    expect_error(&setup, &["files.models", "трёхмерной сцене"]);
    setup.models = "";
    let scene = object("дом", "house", [1.0, 1.0], [1.0, 1.0], "");
    setup.scene = &scene;
    expect_error(&setup, &["model", "трёхмерной сцене"]);
}

#[test]
fn every_prestart_error_about_models_names_the_model_or_the_object() {
    let scene = object("дом", "house", [1.0, 1.0], [2.0, 2.0], "");

    let mut setup = Setup::new(&scene);
    setup.models = r#"{"house":{"path":"models/house.gltf"}}"#;
    expect_error(&setup, &[".glb"]);

    let mut setup = Setup::new(&scene);
    setup.models = r#"{"house":{"path":"models/house.glb","idle":"Idle"}}"#;
    expect_error(&setup, &["idle", "фазе 27"]);
    setup.models = r#"{"house":{"path":"models/house.glb","walk":"Walk"}}"#;
    expect_error(&setup, &["walk", "фазе 27"]);
    setup.models = r#"{"house":{"path":"models/house.glb","feet":["a","b"]}}"#;
    expect_error(&setup, &["feet", "фазе 27"]);
    setup.models = r#"{"house":{"path":"models/house.glb","speed":1}}"#;
    expect_error(&setup, &["неизвестное поле", "speed"]);
    setup.models = r#"{"house":{}}"#;
    expect_error(&setup, &["path"]);

    let mut setup = Setup::new(&scene);
    setup.files = Vec::new();
    expect_error(&setup, &["house", "не найден"]);

    let mut setup = Setup::new(&scene);
    setup.files = vec![(
        "house".to_string(),
        ModelFile {
            bytes: Some(b"{\"asset\":{\"version\":\"2.0\"}}".to_vec()),
            images: Vec::new(),
        },
    )];
    expect_error(&setup, &["house", "glb"]);

    let mut setup = Setup::new(&scene);
    let mut files = real_files();
    files[0].1.images[0].1 = ImageVerdict::Rejected;
    setup.files = files;
    expect_error(&setup, &["house", "не разжалась"]);
}

#[test]
fn a_file_without_meshes_and_a_compressed_file_are_errors() {
    let scene = object("дом", "house", [1.0, 1.0], [2.0, 2.0], "");
    let mut setup = Setup::new(&scene);
    setup.files = vec![("house".to_string(), page_file(tiny_glb("", false)))];
    expect_error(&setup, &["house", "нет ни одной сетки"]);
    setup.files = vec![(
        "house".to_string(),
        page_file(tiny_glb(
            r#","extensionsUsed":["KHR_draco_mesh_compression"]"#,
            true,
        )),
    )];
    expect_error(&setup, &["house", "KHR_draco_mesh_compression"]);
}

#[test]
fn an_object_with_a_model_may_not_have_a_fill_a_flip_or_an_opacity_and_needs_a_place() {
    for (extra, word) in [
        (r##","shape":"box","color":"#ffffff""##, "shape"),
        (r##","color":"#ffffff""##, "color"),
        (r#","flip_x":true"#, "flip_x"),
        (r#","opacity":0.5"#, "opacity"),
    ] {
        let scene = object("дом", "house", [1.0, 1.0], [2.0, 2.0], extra);
        expect_error(&Setup::new(&scene), &[word, "model"]);
    }
    let without_size = r#"{"name":"дом","model":"house","position":[1,1]}"#;
    expect_error(&Setup::new(without_size), &["model", "position и size"]);
    let unknown = object("дом", "castle", [1.0, 1.0], [2.0, 2.0], "");
    expect_error(&Setup::new(&unknown), &["castle", "house"]);
}

#[test]
fn the_keys_in_the_model_errors_stand_in_quotes_so_the_editor_can_swap_them() {
    let with_shape = object(
        "дом",
        "house",
        [1.0, 1.0],
        [2.0, 2.0],
        r##","shape":"box","color":"#ffffff""##,
    );
    expect_error(&Setup::new(&with_shape), &["\"shape\"", "\"model\""]);
    let without_size = r#"{"name":"дом","model":"house","position":[1,1]}"#;
    expect_error(&Setup::new(without_size), &["свойство \"model\""]);
    let flat_height = object("дом", "house", [1.0, 1.0], [2.0, 2.0], r#","height":-1"#);
    expect_error(&Setup::new(&flat_height), &["свойство \"height\""]);
}

#[test]
fn the_animation_property_is_an_error_until_phase_27() {
    let scene = object(
        "дом",
        "house",
        [1.0, 1.0],
        [2.0, 2.0],
        r#","animation":"Walk""#,
    );
    expect_error(&Setup::new(&scene), &["animation", "фазе 27"]);
}

#[test]
fn a_model_that_is_declared_and_never_named_is_a_warning() {
    let scene = object("дом", "house", [1.0, 1.0], [2.0, 2.0], "");
    let (_, warnings) = Setup::new(&scene).load().expect("игра грузится");
    let unused: Vec<&str> = warnings
        .iter()
        .filter(|w| w.message.contains("нигде не названа"))
        .map(|w| w.path.as_str())
        .collect();
    assert_eq!(
        unused,
        [
            "files → models → birch",
            "files → models → car",
            "files → models → axes"
        ]
    );
    let mut with_code = Setup::new(&scene);
    with_code.code = Some("function f() local m = \"car\" end");
    let (_, warnings) = with_code.load().expect("игра грузится");
    assert!(
        !warnings.iter().any(|w| w.path.ends_with("car")),
        "{warnings:?}"
    );
}

#[test]
fn a_model_named_only_in_a_key_binding_of_an_object_is_named() {
    let scene = object(
        "дом",
        "house",
        [1.0, 1.0],
        [2.0, 2.0],
        r#","keys":{"Space":{"press":[["model","car"]],"release":[["model","birch"]]}},"on_click":[["model","axes"]]"#,
    );
    let (_, warnings) = Setup::new(&scene).load().expect("игра грузится");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("нигде не названа")),
        "{warnings:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Место, размер и поворот
// -------------------------------------------------------------------------------------------

#[test]
fn the_example_house_is_a_tenth_of_its_size_and_its_door_looks_left_after_a_quarter_turn() {
    let scene = object(
        "дом",
        "house",
        [10.0, 20.0],
        [0.8, 0.6],
        r#","height":0.7,"rotation":90"#,
    );
    let game = Setup::new(&scene).game();
    let body = Body::of_model(&game.world, 0).expect("коробка модели");
    let house = model_of(&game, "house");
    assert!(close(body.size[0], house.size[0] / 10.0) && close(body.size[1], house.size[1] / 10.0));
    assert!(close(body.height, 0.7));
    assert!(close(body.center[0], 10.4) && close(body.center[1], 20.3));
    assert_eq!((body.sin, body.cos), (1.0, 0.0), "четверть оборота");

    let draws = model_draws(&game);
    let instance = draws.instances[0][0];
    assert_eq!(instance.placement[0..2], [10.4, 20.3]);
    assert!(close(f64::from(instance.base[1]), 0.1), "увеличение в 0,1");
    // Дверь на +Z файла: вершина дома с наибольшей глубиной по сцене. После четверти оборота она левее
    // середины основания.
    let door = house
        .parts
        .iter()
        .flat_map(|p| &p.vertices)
        .max_by(|a, b| a.position[1].total_cmp(&b.position[1]))
        .expect("вершины");
    let (c, s) = (
        f64::from(instance.placement[2]),
        f64::from(instance.placement[3]),
    );
    let scale = f64::from(instance.base[1]);
    let (lx, ly) = (
        f64::from(door.position[0]) * scale,
        f64::from(door.position[1]) * scale,
    );
    let world_x = f64::from(instance.placement[0]) + c * lx - s * ly;
    assert!(world_x < 10.4 - 0.2, "дверь смотрит влево: {world_x}");
}

#[test]
fn without_height_one_unit_of_the_file_is_one_cell_and_the_model_stands_by_the_base_midpoint() {
    let scene = object("берёза", "birch", [5.0, 6.0], [3.0, 3.0], "");
    let game = Setup::new(&scene).game();
    let birch = model_of(&game, "birch");
    let draws = model_draws(&game);
    assert_eq!(draws.instances[1][0].base[1], 1.0);
    let body = Body::of_model(&game.world, 0).expect("коробка");
    assert!(close(body.height, birch.size[2]));
    assert!(close(body.size[0], birch.size[0]) && close(body.size[1], birch.size[1]));
    assert!(
        close(body.center[0], 6.5) && close(body.center[1], 7.5),
        "середина прямоугольника"
    );
}

#[test]
fn the_axes_arrow_looks_at_the_player_and_the_red_end_points_right() {
    let scene = object("оси", "axes", [10.0, 10.0], [2.4, 2.9], "");
    let game = Setup::new(&scene).game();
    let axes = model_of(&game, "axes");
    let by_colour = |predicate: &dyn Fn(&[f32; 4]) -> bool| -> Vec<[f32; 3]> {
        axes.parts
            .iter()
            .filter(|part| predicate(&axes.materials[part.material].base_color))
            .flat_map(|part| part.vertices.iter().map(|v| v.position))
            .collect()
    };
    let blue = by_colour(&|c| c[2] > 0.8);
    let red = by_colour(&|c| c[0] > 0.8 && c[1] < 0.2);
    let green = by_colour(&|c| c[1] > 0.6 && c[0] < 0.2);
    assert!(!blue.is_empty() && !red.is_empty() && !green.is_empty());
    let extreme = |points: &[[f32; 3]], axis: usize| {
        points
            .iter()
            .map(|p| p[axis])
            .fold(f32::NEG_INFINITY, f32::max)
    };
    assert!(
        extreme(&blue, 1) > 1.0,
        "синяя стрелка тянется к игроку (+y)"
    );
    assert!(extreme(&red, 0) > 1.0, "красный брусок — вправо (+x)");
    assert!(extreme(&green, 2) > 1.5, "зелёный брусок — вверх");
}

// -------------------------------------------------------------------------------------------
// Ход игры: столб, щелчок, код
// -------------------------------------------------------------------------------------------

#[test]
fn the_pillar_of_a_model_follows_its_height_with_and_without_height() {
    let scene = [
        object("дом", "house", [2.0, 2.0], [4.0, 3.0], ""),
        object(
            "дом ниже",
            "house",
            [10.0, 2.0],
            [4.0, 3.0],
            r#","height":2.5"#,
        ),
    ]
    .join(",");
    let game = Setup::new(&scene).game();
    let tall = surface::pillar(&game.world, 0);
    assert!(close(tall.high - tall.low, 7.0), "высота модели в файле");
    let low = surface::pillar(&game.world, 1);
    assert!(close(low.high - low.low, 2.5), "высота из height");
    assert!(close(surface::body_height(&game.world, 1), 2.5));
}

#[test]
fn a_click_hits_the_box_around_the_model_after_scale_and_turn_and_the_nearer_one_wins() {
    let scene = [
        object(
            "дальний",
            "house",
            [10.0, 10.0],
            [4.0, 3.0],
            r#","height":3.5"#,
        ),
        object(
            "ближний",
            "house",
            [10.0, 15.0],
            [4.0, 3.0],
            r#","height":3.5"#,
        ),
    ]
    .join(",");
    let game = Setup::new(&scene).game();
    let camera = game.camera_3d(WINDOW).expect("камера");
    let hit = |x: f64, y: f64, z: f64| {
        let window = camera.project([x, y, z]).expect("точка на экране");
        scene::editor_target_ray(&game.world, &game.scene, &camera, window)
    };
    assert_eq!(hit(12.0, 11.5, 1.0), Some(0), "в середину дальнего дома");
    assert_eq!(hit(12.0, 16.5, 1.0), Some(1), "в середину ближнего дома");
    assert_eq!(hit(4.0, 4.0, 0.0), None, "мимо");
    // Луч в дальний дом проходит мимо ближнего, а в ближний — сначала через него.
    assert_eq!(hit(12.0, 16.5, 0.2), Some(1));
}

#[test]
fn a_turned_model_is_hit_where_the_turn_put_its_box() {
    let scene = object(
        "дом",
        "house",
        [10.0, 10.0],
        [4.0, 3.0],
        r#","height":3.5,"rotation":90"#,
    );
    let game = Setup::new(&scene).game();
    let body = Body::of_model(&game.world, 0).expect("коробка");
    let from_above = |x: f64, y: f64| body.ray_hit([x, y, 20.0], [0.0, 0.0, -1.0]);
    // Центр (12; 11,5); модель 4,4 × 3,4 после четверти оборота: по x 3,4 (от 10,3 до 13,7), по y 4,4 (от
    // 9,3 до 13,7).
    assert!(from_above(10.5, 11.5).is_some());
    assert!(
        from_above(12.0, 9.5).is_some(),
        "по y коробка тянется выше прежнего прямоугольника"
    );
    assert!(from_above(10.1, 11.5).is_none(), "а по x короче прежнего");
    assert!(from_above(12.0, 9.1).is_none());
}

const SWITCH_CODE: &str = r#"
function to_birch(obj) obj.model = "birch" end
function to_nothing(obj) obj.model = "castle" end
function to_box(obj) obj.shape = "box" end
function read_name(obj) obj.tag = obj.model == "house" end
"#;

fn code_game(function: &str) -> Game {
    let scene =
        r##"{"name":"a","actor":true,"model":"house","position":[2,2],"size":[4,3],"height":3.5}"##;
    let rules = format!(
        r#"{{"rules":[{{"kind":"check","for":{{"has":["actor"]}},"do":[["run","{function}"]]}}]}}"#
    );
    let mut setup = Setup::new(scene);
    setup.rules = &rules;
    setup.code = Some(SWITCH_CODE);
    setup.game()
}

fn step(game: &mut Game) {
    game.step(StepInput::empty());
}

#[test]
fn code_writes_the_model_by_name_and_the_pillar_follows_the_new_model() {
    let mut game = code_game("to_birch");
    assert!(close(surface::pillar(&game.world, 0).high, 3.5));
    step(&mut game);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.text(0, property::MODEL), Some("birch"));
    let birch = model_of(&game, "birch");
    // Высота `height` осталась прежней: модель растянута под неё, а столб — по ней же.
    assert!(close(surface::pillar(&game.world, 0).high, 3.5));
    assert!(birch.size[2] > 7.0);
}

#[test]
fn code_reads_the_model_name_back() {
    let mut game = code_game("read_name");
    step(&mut game);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let tag = game.properties.resolve("tag").expect("tag");
    assert!(game.world.flag(0, tag));
}

#[test]
fn code_writing_an_unknown_model_or_a_forbidden_combination_is_a_code_error() {
    for function in ["to_nothing", "to_box"] {
        let mut game = code_game(function);
        step(&mut game);
        assert!(game.code_error().is_some(), "{function}: ошибки кода нет");
    }
    let mut game = code_game("to_nothing");
    step(&mut game);
    let message = game
        .code_error()
        .map(|e| e.message.clone())
        .unwrap_or_default();
    assert!(
        message.contains("castle") && message.contains("house"),
        "{message}"
    );
    assert_eq!(
        game.world.text(0, property::MODEL),
        Some("house"),
        "прежняя модель осталась"
    );
}

// -------------------------------------------------------------------------------------------
// Кадр
// -------------------------------------------------------------------------------------------

#[test]
fn many_objects_of_one_model_are_one_list_in_the_frame_and_the_shadow_reaches_their_top() {
    let mut scene = Vec::new();
    for i in 0..200 {
        scene.push(object(
            &format!("берёза {i}"),
            "birch",
            [1.0 + f64::from(i) * 0.18, 20.0],
            [0.44, 0.48],
            r#","height":1"#,
        ));
    }
    scene.push(object(
        "дом",
        "house",
        [20.0, 5.0],
        [4.0, 3.0],
        r#","height":3.5"#,
    ));
    let game = Setup::new(&scene.join(",")).game();
    let camera = game.camera_3d(WINDOW).expect("камера");
    let frame = compose_frame3d(
        &game,
        &camera,
        &Motion::default(),
        &[],
        &[],
        &mut FlatShapes::default(),
    );
    assert_eq!(frame.models.len(), 4);
    assert_eq!(frame.models[0].len(), 1, "дом");
    assert_eq!(frame.models[1].len(), 200, "берёзы разом");
    assert!(frame.models[2].is_empty() && frame.models[3].is_empty());
    assert!(frame.shapes.is_empty(), "модели не фигуры");
    assert!(frame.surface.is_empty(), "и не плоские объекты");
}

#[test]
fn a_model_stands_on_the_lowest_point_of_the_ground_under_it() {
    let heights: Vec<String> = (0..=60)
        .map(|_| {
            let row: Vec<String> = (0..=80)
                .map(|column| format!("{}", (f64::from(column) / 2.0 - 10.0).clamp(0.0, 1.0)))
                .collect();
            format!("[{}]", row.join(","))
        })
        .collect();
    let terrain = format!(r#"{{"heights":[{}]}}"#, heights.join(","));
    let scene = object("дом", "house", [8.0, 5.0], [4.0, 3.0], r#","height":3.5"#);
    let setup = Setup::new(&scene);
    let text = setup
        .game_json()
        .replace(r#""fonts":{}"#, r#""fonts":{},"terrain":"terrain.json""#);
    let scene_text = format!(r#"{{"objects":[{scene}]}}"#);
    let (config, _) = read_entry(&text).expect("game.json");
    let (game, _, _, _) = load_rest_with_models(
        &text,
        config,
        Some(PROPS),
        Some(&scene_text),
        Some(setup.rules),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &[],
        None,
        false,
        &[],
        Some(&terrain),
        &[],
        &[],
        &[],
        &setup.files,
    )
    .unwrap_or_else(|f| panic!("{:#?}", f.errors));
    // Склон поднимается от x = 10 к x = 11; дом от 8 до 12 стоит на самой нижней точке под ним — на нуле.
    assert!(close(game.world.base_z(0), 0.0));
    let draws = model_draws(&game);
    assert_eq!(draws.instances[0][0].base[0], 0.0);
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
fn the_model_shaders_validate_and_translate_to_glsl_es_300() {
    let (module, info) = parse_and_validate(include_str!("../shaders/scene3d.wgsl"));
    let shared = [(0, 0), (0, 3), (0, 4)];
    let material: Vec<(u32, u32)> = (0..9).map(|binding| (1, binding)).collect();
    let all: Vec<(u32, u32)> = shared.iter().chain(&material).copied().collect();
    for (stage, entry, bindings) in [
        (naga::ShaderStage::Vertex, "vs_shadow_model", &[(0, 0)][..]),
        (naga::ShaderStage::Vertex, "vs_model", &[(0, 0)][..]),
        (naga::ShaderStage::Fragment, "fs_model", &all[..]),
    ] {
        let glsl = to_webgl2_glsl(&module, &info, stage, entry, bindings);
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
    }
    let fragment = to_webgl2_glsl(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs_model",
        &all,
    );
    assert!(
        fragment.contains("sampler2DShadow"),
        "тень — выборка сравнением"
    );
    assert_eq!(
        fragment.matches("uniform highp sampler2D ").count(),
        4,
        "четыре карты материала"
    );
}

// -------------------------------------------------------------------------------------------
// Маленький .glb для проверок ошибок
// -------------------------------------------------------------------------------------------

/// `.glb` из одного узла: с треугольником (`with_mesh`) или без сетки; `extra_root` — остальные поля корня.
fn tiny_glb(extra_root: &str, with_mesh: bool) -> Vec<u8> {
    let positions: Vec<u8> = [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect();
    let (nodes, meshes, accessors, views) = if with_mesh {
        (
            r#"[{"mesh":0}]"#,
            r#","meshes":[{"primitives":[{"attributes":{"POSITION":0}}]}]"#,
            r#","accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3","min":[0,0,0],"max":[1,1,0]}]"#,
            r#","bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}]"#,
        )
    } else {
        ("[{}]", "", "", "")
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":{nodes}{meshes}{accessors}{views},"buffers":[{{"byteLength":36}}]{extra_root}}}"#
    );
    let mut json = json.into_bytes();
    while json.len() % 4 != 0 {
        json.push(b' ');
    }
    let total = 12 + 8 + json.len() + 8 + positions.len();
    let mut file = Vec::new();
    file.extend(b"glTF");
    file.extend(2u32.to_le_bytes());
    file.extend((total as u32).to_le_bytes());
    file.extend((json.len() as u32).to_le_bytes());
    file.extend(0x4E4F534Au32.to_le_bytes());
    file.extend(json);
    file.extend((positions.len() as u32).to_le_bytes());
    file.extend(0x004E4942u32.to_le_bytes());
    file.extend(positions);
    file
}

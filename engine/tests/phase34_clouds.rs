//! Фаза 34 — облака неба: свойства `clouds` и `cloud_images` (новый вид «список картинок»), их проверка
//! перед запуском, код, правила, `set_property`, облака в списке рисования, первая сборка мира и
//! проступание, ветер и таяние. Раскладка, кольцо и числа — в `render::clouds`'s own `mod tests`; сама
//! видеокарта и браузер — только QA на стенде.

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::scene::LayerView;
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::data::edit;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest, read_entry};
use engine::data::session::{self, PlaySession};
use engine::render::atlas::{
    AtlasImage, AtlasRect, RectPaint, cloud_base_sizes, cloud_skies, compose_world_paints, pack,
};
use engine::render::clouds::Camera;
use engine::render::wind::Motion;
use serde_json::json;

// Картинки игры лежат по алфавиту имён: так их отдаёт `files.images`.
const CLOUD_A: usize = 2;
const CLOUD_B: usize = 3;
const CLOUD_C: usize = 4;

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"mark":"flag","hits":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;
const CELL_PIXELS: &str = r#","cell_pixels":4"#;

const FILES: &str = r#","images":{
    "sky":{"path":"sky.png"},
    "cloud_a":{"path":"cloud_a.png","size":[6,3]},
    "cloud_b":{"path":"cloud_b.png"},
    "cloud_c":{"path":"cloud_c.png","size":[3,3],"smooth":true,"glow":true},
    "strip":{"path":"strip.png","frames":4},
    "ticking":{"path":"ticking.png","frames":2,"frame_time":0.5},
    "picked":{"path":"picked.png","frames":2,"frame_by":"hits"},
    "anchored":{"path":"anchored.png","size":[2,2],"anchor":"bottom"},
    "shifted":{"path":"shifted.png","size":[2,2],"offset":[1,0]},
    "clip":{"path":"clip.mp4"}}"#;

fn game_json(scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

fn opaque(width: u32, height: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![255u8; (width * height * 4) as usize],
    }
}

fn verdicts() -> Vec<(&'static str, ImageVerdict)> {
    vec![
        ("sky", opaque(8, 8)),
        ("cloud_a", opaque(12, 6)),
        ("cloud_b", opaque(16, 8)),
        ("cloud_c", opaque(6, 6)),
        ("strip", opaque(16, 4)),
        ("ticking", opaque(8, 4)),
        ("picked", opaque(8, 4)),
        ("anchored", opaque(4, 4)),
        ("shifted", opaque(4, 4)),
        (
            "clip",
            ImageVerdict::Video {
                width: 8,
                height: 16,
            },
        ),
    ]
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn load_with(
    game: &str,
    scene: &str,
    rules: &str,
    code: Option<&str>,
) -> Result<Loaded, LoadFailure> {
    let (config, _warnings) = read_entry(game).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = verdicts()
        .into_iter()
        .map(|(name, verdict)| (name.to_string(), verdict))
        .collect();
    load_rest(
        game,
        config,
        Some(PROPS),
        Some(scene),
        Some(rules),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &image_data,
        code,
        false,
    )
}

fn load_flat(scene: &str, rules: &str, code: Option<&str>) -> Result<Loaded, LoadFailure> {
    let files = if code.is_some() {
        format!(r#"{FILES},"code":"code.lua""#)
    } else {
        FILES.to_string()
    };
    load_with(&game_json(CELL_PIXELS, &files), scene, rules, code)
}

fn errors_of(result: Result<Loaded, LoadFailure>) -> Vec<GameError> {
    result.expect_err("загрузка должна провалиться").errors
}

fn assert_error(errors: &[GameError], file: &str, path: &str, message: &str) {
    assert!(
        errors
            .iter()
            .any(|e| e.file == file && e.path.contains(path) && e.message.contains(message)),
        "нет ошибки {file} → {path}: {message}: {errors:?}"
    );
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

fn sky_object(extra: &str) -> String {
    format!(
        r#"{{"objects":[{{"position":[0,0],"size":[40,12],"image":"sky","parallax":0,"repeat_x":true,{extra}}}]}}"#
    )
}

const SKY_WITH_CLOUDS: &str = r#""clouds":0.3,"cloud_images":["cloud_a","cloud_b"]"#;

// -------------------------------------------------------------------------------------------
// Два свойства: загрузка и проверка перед запуском
// -------------------------------------------------------------------------------------------

#[test]
fn the_two_properties_load_and_a_list_may_be_empty_repeat_a_picture_or_stand_without_clouds() {
    let (game, _screens, warnings, _images) =
        load_flat(&sky_object(SKY_WITH_CLOUDS), NO_RULES, None).expect("должно загрузиться");
    assert_eq!(game.world.number_like(0, property::CLOUDS), Some(0.3));
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[CLOUD_A, CLOUD_B][..])
    );
    assert!(
        warnings.iter().all(|w| !w.message.contains("облак")),
        "{warnings:?}"
    );

    for extra in [
        r#""clouds":0.3,"cloud_images":["cloud_a","cloud_a"]"#,
        r#""cloud_images":["cloud_a"]"#,
        r#""clouds":0,"cloud_images":[]"#,
        r#""clouds":1,"cloud_images":["cloud_c"]"#,
    ] {
        let (game, _screens, warnings, _images) =
            load_flat(&sky_object(extra), NO_RULES, None).expect(extra);
        assert!(game.world.has(0, property::CLOUD_IMAGES) || extra.starts_with(r#""clouds":0"#));
        assert!(
            warnings.iter().all(|w| !w.message.contains("облаков нет")),
            "{extra}: {warnings:?}"
        );
    }
    let (game, ..) = load_flat(&sky_object(r#""cloud_images":[]"#), NO_RULES, None).unwrap();
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[][..])
    );
}

#[test]
fn the_edges_of_the_range_are_allowed() {
    for amount in ["0", "1", "0.05"] {
        let extra = format!(r#""clouds":{amount},"cloud_images":["cloud_a"]"#);
        load_flat(&sky_object(&extra), NO_RULES, None).expect(&extra);
    }
}

#[test]
fn every_check_before_the_start_names_the_property_and_the_place() {
    let cases = [
        (r#""clouds":1.5"#, "clouds", "clouds: нужно от 0 до 1"),
        (r#""clouds":-0.1"#, "clouds", "clouds: нужно от 0 до 1"),
        (r#""clouds":"много""#, "clouds", "ожидалось число"),
        (
            r#""cloud_images":"cloud_a""#,
            "cloud_images",
            "ожидался список имён картинок",
        ),
        (
            r#""cloud_images":{"a":1}"#,
            "cloud_images",
            "ожидался список имён картинок",
        ),
        (r#""cloud_images":[5]"#, "cloud_images", "ожидалась строка"),
        (
            r#""cloud_images":["cloud_a","nope"]"#,
            "cloud_images → [1]",
            "картинки \"nope\" нет",
        ),
        (
            r#""cloud_images":["clip"]"#,
            "cloud_images",
            "не годится облакам: видео",
        ),
        (
            r#""cloud_images":["strip"]"#,
            "cloud_images",
            "не годится облакам: картинка с кадрами",
        ),
        (
            r#""cloud_images":["ticking"]"#,
            "cloud_images",
            "картинка с кадрами",
        ),
        (
            r#""cloud_images":["picked"]"#,
            "cloud_images",
            "картинка с кадрами",
        ),
        (
            r#""cloud_images":["anchored"]"#,
            "cloud_images",
            "картинка с anchor или offset",
        ),
        (
            r#""cloud_images":["shifted"]"#,
            "cloud_images",
            "картинка с anchor или offset",
        ),
    ];
    for (field, path, message) in cases {
        let errors = errors_of(load_flat(&sky_object(field), NO_RULES, None));
        assert_error(&errors, "scene.json", path, message);
    }
}

#[test]
fn a_picture_without_a_size_is_an_error_in_a_game_without_cell_pixels() {
    let scene = sky_object(r#""cloud_images":["cloud_b"]"#);
    let errors = errors_of(load_with(&game_json("", FILES), &scene, NO_RULES, None));
    assert_error(
        &errors,
        "scene.json",
        "cloud_images",
        "картинка без size в игре без cell_pixels",
    );
    let fine = sky_object(r#""cloud_images":["cloud_a","cloud_c"]"#);
    load_with(&game_json("", FILES), &fine, NO_RULES, None).expect("у обеих есть size");
}

#[test]
fn the_properties_need_a_position_a_size_and_repeat_x() {
    for object in [
        r#"{"size":[40,12],"repeat_x":true,"clouds":0.3}"#,
        r#"{"position":[0,0],"repeat_x":true,"cloud_images":["cloud_a"]}"#,
        r#"{"position":[0,0],"size":[40,12],"clouds":0.3}"#,
        r#"{"position":[0,0],"size":[40,12],"cloud_images":["cloud_a"]}"#,
    ] {
        let scene = format!(r#"{{"objects":[{object}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(&errors, "scene.json", "objects[0]", "облаков");
    }
}

#[test]
fn the_properties_in_a_three_dimensional_scene_are_errors_everywhere() {
    let flat3d = game_json(&format!("{CAMERA_3D}{CELL_PIXELS}"), "");
    for field in [r#""clouds":0.5"#, r#""cloud_images":[]"#] {
        let scene = format!(
            r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff","repeat_x":false,{field}}}]}}"##
        );
        let errors = errors_of(load_with(&flat3d, &scene, NO_RULES, None));
        assert_error(&errors, "scene.json", "", "только в плоской сцене");
    }
    let plain3d = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let rule = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","clouds",0.5]]}]}"#;
    let errors = errors_of(load_with(&flat3d, plain3d, rule, None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "clouds есть только в плоской сцене",
    );
}

#[test]
fn clouds_without_pictures_are_a_warning_and_never_an_error() {
    for extra in [r#""clouds":0.3"#, r#""clouds":0.3,"cloud_images":[]"#] {
        let (_game, _screens, warnings, _images) =
            load_flat(&sky_object(extra), NO_RULES, None).expect("предупреждение не мешает");
        let warning = warnings
            .iter()
            .find(|w| {
                w.message
                    .contains("облаков нет: не выбраны картинки облаков")
            })
            .unwrap_or_else(|| panic!("{extra}: {warnings:?}"));
        assert_eq!(warning.file, "scene.json");
        assert!(warning.path.contains("clouds"), "{warning:?}");
    }
    let (_game, _screens, warnings, _images) =
        load_flat(&sky_object(r#""clouds":0"#), NO_RULES, None).unwrap();
    assert!(
        warnings.iter().all(|w| !w.message.contains("облаков нет")),
        "{warnings:?}"
    );
}

#[test]
fn a_picture_named_only_by_the_clouds_is_not_reported_as_unused() {
    let (_game, _screens, warnings, _images) =
        load_flat(&sky_object(SKY_WITH_CLOUDS), NO_RULES, None).unwrap();
    for name in ["cloud_a", "cloud_b"] {
        assert!(
            warnings.iter().all(|w| !w
                .message
                .contains(&format!("картинка \"{name}\" объявлена"))),
            "{name}: {warnings:?}"
        );
    }
}

#[test]
fn rules_and_keys_write_the_list_as_json_and_check_it_at_load_time() {
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[40,12],"parallax":0,"repeat_x":true,"color":"#336699","mark":true,
         "clouds":0.1,"cloud_images":["cloud_a"],
         "keys":{"KeyC":{"press":[["cloud_images",["cloud_c"]]]}}}]}"##;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},
        "do":[["set","cloud_images",["cloud_a","cloud_b"]],["set","clouds",0.4]]}]}"#;
    let (mut game, ..) = load_flat(scene, rules, None).expect("должно загрузиться");
    step(&mut game, 1);
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[CLOUD_A, CLOUD_B][..])
    );
    assert_eq!(game.world.number_like(0, property::CLOUDS), Some(0.4));

    let broken_rule = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},
        "do":[["set","cloud_images",["cloud_a","nope"]]]}]}"#;
    let errors = errors_of(load_flat(scene, broken_rule, None));
    assert_error(&errors, "rules.json", "", "картинки \"nope\" нет");
    let video_rule = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},
        "do":[["set","cloud_images",["clip"]]]}]}"#;
    let errors = errors_of(load_flat(scene, video_rule, None));
    assert_error(&errors, "rules.json", "", "не годится облакам: видео");
    let range_rule = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},
        "do":[["set","clouds",2]]}]}"#;
    let errors = errors_of(load_flat(scene, range_rule, None));
    assert_error(&errors, "rules.json", "", "clouds: нужно от 0 до 1");

    let broken_key = scene.replace(r#"["cloud_c"]"#, r#"["strip"]"#);
    let errors = errors_of(load_flat(&broken_key, NO_RULES, None));
    assert_error(&errors, "scene.json", "keys", "картинка с кадрами");
}

// -------------------------------------------------------------------------------------------
// Код игры
// -------------------------------------------------------------------------------------------

const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;
const SKY_WITH_MARK: &str = r##"{"objects":[{"position":[0,0],"size":[40,12],"parallax":0,"repeat_x":true,"color":"#336699","mark":true,"clouds":0.3,"cloud_images":["cloud_a","cloud_b"]}]}"##;

fn load_code(code: &str) -> Game {
    let (game, ..) = load_flat(SKY_WITH_MARK, CODE_RULES, Some(code)).expect("должно загрузиться");
    game
}

#[test]
fn code_reads_the_list_as_a_new_table_of_names_and_the_count_as_a_number() {
    let mut game = load_code(
        "function tick(obj)\n  local names = obj.cloud_images\n  print(#names, names[1], names[2])\n  names[1] = \"cloud_c\"\n  print(obj.cloud_images[1])\n  print(obj.clouds)\nend",
    );
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.messages(),
        ["print: 2	cloud_a	cloud_b", "print: cloud_a", "print: 0.3"]
    );
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[CLOUD_A, CLOUD_B][..]),
        "правка элемента прочитанной таблицы объект не меняет"
    );
}

#[test]
fn code_reads_nil_where_there_is_no_list_and_writes_a_whole_list() {
    let (mut game, ..) = load_flat(
        r#"{"objects":[{"position":[1,1],"size":[1,1],"mark":true}]}"#,
        CODE_RULES,
        Some("function tick(obj)\n  print(obj.cloud_images)\n  obj.cloud_images = {\"cloud_c\", \"cloud_a\"}\n  obj.clouds = 1\n  print(#obj.cloud_images)\nend"),
    )
    .unwrap();
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.messages(), ["print: nil", "print: 2"]);
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[CLOUD_C, CLOUD_A][..])
    );
    assert_eq!(game.world.number_like(0, property::CLOUDS), Some(1.0));
}

#[test]
fn code_may_clear_the_list_with_an_empty_table_or_with_nil() {
    let mut game = load_code("function tick(obj)\n  obj.cloud_images = {}\nend");
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[][..])
    );

    let mut game = load_code("function tick(obj)\n  obj.cloud_images = nil\nend");
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert!(!game.world.has(0, property::CLOUD_IMAGES));
}

#[test]
fn a_bad_list_is_a_code_error_and_the_object_stays_as_it_was() {
    for value in [
        "{\"cloud_a\", \"nope\"}",
        "{\"clip\"}",
        "{\"strip\"}",
        "{\"ticking\"}",
        "{\"anchored\"}",
        "{\"shifted\"}",
        "{\"cloud_a\", 5}",
        "\"cloud_a\"",
        "5",
        "true",
        "{x = 1, y = 2}",
        "{[2] = \"cloud_a\"}",
        "{\"cloud_a\", nil, \"cloud_b\"}",
        "{name = \"cloud_a\"}",
    ] {
        let mut game = load_code(&format!(
            "function tick(obj)\n  obj.cloud_images = {value}\nend"
        ));
        step(&mut game, 1);
        let error = game
            .code_error()
            .unwrap_or_else(|| panic!("{value}: ошибки нет"));
        assert!(
            error
                .message
                .contains("ожидался список имён картинок облаков"),
            "{value}: {error:?}"
        );
        assert_eq!(
            game.world.image_list(0, property::CLOUD_IMAGES),
            Some(&[CLOUD_A, CLOUD_B][..]),
            "{value}"
        );
    }
}

#[test]
fn a_picture_without_a_size_is_refused_by_code_in_a_game_without_cell_pixels() {
    let (mut game, ..) = load_with(
        &game_json("", &format!(r#"{FILES},"code":"code.lua""#)),
        SKY_WITH_MARK
            .replace(
                r#""cloud_images":["cloud_a","cloud_b"]"#,
                r#""cloud_images":["cloud_a"]"#,
            )
            .as_str(),
        CODE_RULES,
        Some("function tick(obj)\n  obj.cloud_images = {\"cloud_b\"}\nend"),
    )
    .expect("должно загрузиться");
    step(&mut game, 1);
    let error = game.code_error().expect("ошибка кода");
    assert!(
        error
            .message
            .contains("ожидался список имён картинок облаков"),
        "{error:?}"
    );
}

#[test]
fn code_may_not_write_the_properties_in_a_three_dimensional_scene() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let files = r#","code":"code.lua""#;
    for write in ["obj.clouds = 0.5", "obj.cloud_images = {}"] {
        let (mut game, ..) = load_with(
            &game_json(CAMERA_3D, files),
            scene,
            CODE_RULES,
            Some(&format!("function tick(obj)\n  {write}\nend")),
        )
        .expect("должно загрузиться");
        step(&mut game, 1);
        let error = game.code_error().expect("ошибка кода");
        assert!(
            error.message.contains("только в плоской сцене"),
            "{error:?}"
        );
    }
}

// -------------------------------------------------------------------------------------------
// `set_property` и `remove_property`
// -------------------------------------------------------------------------------------------

fn loaded(scene: &str) -> (Game, ScreensConfig, Vec<ImageDecl>) {
    let (game, screens, _warnings, images) =
        load_flat(scene, NO_RULES, None).expect("должно загрузиться");
    (game, screens, images)
}

#[test]
fn set_property_takes_an_array_of_strings_and_a_wrong_one_changes_nothing() {
    let (mut game, _config, images) = loaded(&sky_object(SKY_WITH_CLOUDS));
    assert_eq!(
        session::set_property(
            None,
            &mut game,
            &images,
            0,
            "cloud_images",
            &json!(["cloud_c"])
        ),
        Ok(())
    );
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[CLOUD_C][..])
    );
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "cloud_images", &json!([])),
        Ok(())
    );
    assert_eq!(
        game.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[][..])
    );
    session::set_property(
        None,
        &mut game,
        &images,
        0,
        "cloud_images",
        &json!(["cloud_a"]),
    )
    .unwrap();

    for (value, part) in [
        (json!(["cloud_a", "nope"]), "картинки \"nope\" нет"),
        (json!(["clip"]), "не годится облакам: видео"),
        (json!("cloud_a"), "ожидался список имён картинок"),
        (json!([1]), "ожидалась строка"),
    ] {
        let error = session::set_property(None, &mut game, &images, 0, "cloud_images", &value)
            .expect_err(part);
        assert!(error.contains(part), "{error}");
        assert_eq!(
            game.world.image_list(0, property::CLOUD_IMAGES),
            Some(&[CLOUD_A][..]),
            "{value}"
        );
    }
    let error = session::set_property(None, &mut game, &images, 0, "clouds", &json!(2))
        .expect_err("вне отрезка");
    assert!(error.contains("от 0 до 1"), "{error}");
    assert_eq!(game.world.number_like(0, property::CLOUDS), Some(0.3));
}

#[test]
fn the_list_reads_back_as_names_in_file_form() {
    let (game, _config, images) = loaded(&sky_object(SKY_WITH_CLOUDS));
    let properties =
        edit::object_properties_json(&game.world, &game.properties, &images, 0).unwrap();
    assert_eq!(properties["cloud_images"], json!(["cloud_a", "cloud_b"]));
    assert_eq!(properties["clouds"], json!(0.3));
}

#[test]
fn the_properties_cannot_be_put_on_an_object_that_is_not_a_sky() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff"}]}"##;
    let (mut game, _config, images) = loaded(scene);
    for (name, value) in [("clouds", json!(0.3)), ("cloud_images", json!(["cloud_a"]))] {
        let error = session::set_property(None, &mut game, &images, 0, name, &value)
            .expect_err("без repeat_x");
        assert!(error.contains("repeat_x"), "{error}");
        assert!(!game.world.has(0, game.properties.resolve(name).unwrap()));
    }
    session::set_property(None, &mut game, &images, 0, "repeat_x", &json!(true)).unwrap();
    session::set_property(None, &mut game, &images, 0, "clouds", &json!(0.3)).unwrap();
    let error = session::set_property(None, &mut game, &images, 0, "repeat_x", &json!(false))
        .expect_err("у неба с облаками повтор не снять");
    assert!(error.contains("repeat_x"), "{error}");
}

#[test]
fn in_a_party_the_list_edit_is_recorded_and_in_a_replay_the_world_is_not_edited() {
    let (mut game, config, images) = loaded(&sky_object(SKY_WITH_CLOUDS));
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    assert_eq!(
        session::set_property(
            Some(&mut live),
            &mut game,
            &images,
            0,
            "cloud_images",
            &json!(["cloud_c"])
        ),
        Ok(())
    );
    let text = live.recording_text(&game);
    assert!(text.contains("cloud_images"), "{text}");

    let (mut replayed, config2, images2) = loaded(&sky_object(SKY_WITH_CLOUDS));
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut replayed, &config2, &mut state2).unwrap();
    assert_eq!(
        session::set_property(
            Some(&mut replay),
            &mut replayed,
            &images2,
            0,
            "cloud_images",
            &json!(["cloud_c"])
        ),
        Err("в повторе мир не правится".to_string())
    );
    assert_eq!(
        replayed.world.image_list(0, property::CLOUD_IMAGES),
        Some(&[CLOUD_A, CLOUD_B][..])
    );
}

// -------------------------------------------------------------------------------------------
// Облака в списке рисования
// -------------------------------------------------------------------------------------------

/// Небо, холмы поверх него и герой в слое неба; слой неба 0, холмы 1.
const SCENE_DRAWN: &str = r##"{"objects":[
    {"position":[0,0],"size":[40,12],"image":"sky","layer":0,"parallax":0,"repeat_x":true,
     "clouds":0.3,"cloud_images":["cloud_a","cloud_b"]},
    {"position":[0,9],"size":[40,11],"color":"#224422","layer":1,"parallax":0.5,"repeat_x":true},
    {"position":[5,5],"size":[1,1],"color":"#ffffff","layer":0,"mark":true}]}"##;

struct Rig {
    game: Game,
    images: Vec<ImageDecl>,
    rects: Vec<AtlasRect>,
    motion: Motion,
    view: LayerView,
}

impl Rig {
    fn new(scene: &str) -> Rig {
        Rig::with_rules(scene, NO_RULES)
    }

    fn with_rules(scene: &str, rules: &str) -> Rig {
        let (game, _screens, _warnings, images) =
            load_flat(scene, rules, None).expect("должно загрузиться");
        let atlas_images: Vec<AtlasImage> = images
            .iter()
            .map(|decl| {
                let (_, verdict) = verdicts()
                    .into_iter()
                    .find(|(name, _)| *name == decl.name)
                    .expect("картинка теста");
                match verdict {
                    ImageVerdict::Ok {
                        width,
                        height,
                        pixels,
                    } => AtlasImage {
                        width,
                        height,
                        pixels,
                    },
                    ImageVerdict::Video { width, height } => {
                        AtlasImage::video_frame(width, height / 2)
                    }
                    _ => unreachable!("все картинки теста — ok или видео"),
                }
            })
            .collect();
        let rects = pack(&atlas_images).expect("умещаются").rects;
        let mut motion = Motion::default();
        motion.set_cloud_sizes(cloud_base_sizes(
            &atlas_images,
            &images,
            game.scene.cell_pixels,
        ));
        Rig {
            game,
            images,
            rects,
            motion,
            view: LayerView {
                shift: [0.0, 0.0],
                window_cells: 40.0,
            },
        }
    }

    /// Окно нулевой ширины: ни одно облако не отсеивается как лежащее вне окна.
    fn unculled(mut self) -> Rig {
        self.view.window_cells = 0.0;
        self
    }

    /// То же, что `wasm::update_motion`: часы доведены, облака посчитаны.
    fn settle(&mut self) {
        let wind = self.game.wind();
        let camera = Camera {
            scene_middle: f64::from(self.game.scene.width) / 2.0,
            view: &self.view,
        };
        self.motion.update_clouds(
            self.game.has_world(),
            wind[0],
            camera,
            cloud_skies(&self.game.world),
        );
    }

    /// Кадры редактора вне партии: часы идут по `draw(dt)`.
    fn frames(&mut self, count: u32, dt: f64) {
        for _ in 0..count {
            self.motion.tick(None, dt);
            self.settle();
        }
    }

    fn clouds_of(&self, id: u32) -> Vec<engine::render::clouds::Sprite> {
        self.motion
            .clouds()
            .live_sprites(id, self.game.world.generation(id))
            .collect()
    }

    fn paints(&self) -> Vec<RectPaint> {
        compose_world_paints(
            &self.game.world,
            &self.game.scene,
            self.game.world.ids(),
            &self.motion,
            &self.images,
            &self.rects,
            &self.view,
        )
    }

    fn cloud_paints(&self) -> Vec<RectPaint> {
        self.paints()
            .into_iter()
            .filter(|paint| paint.object.is_none())
            .collect()
    }
}

fn order_of(paints: &[RectPaint]) -> Vec<Option<u32>> {
    paints.iter().map(|paint| paint.object).collect()
}

#[test]
fn the_sky_from_the_scene_stands_in_full_strength_on_the_first_assembly() {
    let mut rig = Rig::new(SCENE_DRAWN).unculled();
    rig.settle();
    let clouds = rig.clouds_of(0);
    assert_eq!(clouds.len(), 7);
    assert!(
        clouds.iter().all(|cloud| cloud.opacity >= 0.55),
        "{clouds:?}"
    );
    assert_eq!(rig.cloud_paints().len(), 7);
}

#[test]
fn the_clouds_follow_the_sky_and_its_copies_and_come_before_the_next_layer_far_to_near() {
    let mut rig = Rig::new(SCENE_DRAWN).unculled();
    rig.settle();
    let paints = rig.paints();
    let mut expected = vec![Some(0)];
    expected.extend(std::iter::repeat_n(None, 7));
    expected.extend([Some(2), Some(1)]);
    assert_eq!(order_of(&paints), expected);

    let clouds = &paints[1..8];
    let parallaxes: Vec<f64> = rig
        .clouds_of(0)
        .iter()
        .map(|cloud| cloud.parallax)
        .collect();
    assert!(parallaxes.windows(2).all(|pair| pair[0] <= pair[1]));
    for (paint, cloud) in clouds.iter().zip(rig.clouds_of(0)) {
        assert_eq!(paint.flip_x, cloud.mirrored);
        assert_eq!(paint.size, cloud.size.map(|side| side as f32));
        assert!(!paint.smooth && !paint.glow, "облака a и b не сглажены");
    }
}

#[test]
fn half_of_the_clouds_are_mirrored_and_a_cloud_takes_the_smoothing_and_glow_of_its_picture() {
    let scene = sky_object(r#""clouds":1,"cloud_images":["cloud_c"]"#);
    let mut rig = Rig::new(&scene).unculled();
    rig.settle();
    let paints = rig.cloud_paints();
    assert_eq!(paints.len(), 24);
    assert!(paints.iter().all(|paint| paint.smooth && paint.glow));
    let mirrored = paints.iter().filter(|paint| paint.flip_x).count();
    assert!((6..=18).contains(&mirrored), "{mirrored}");
}

#[test]
fn a_cloud_is_shifted_by_its_own_parallax_and_one_outside_the_window_is_not_drawn() {
    let mut rig = Rig::new(&sky_object(r#""clouds":1,"cloud_images":["cloud_a"]"#));
    rig.settle();
    let at_rest = rig.cloud_paints();
    assert!(
        !at_rest.is_empty() && at_rest.len() < 24,
        "{}",
        at_rest.len()
    );
    for paint in &at_rest {
        let right = f64::from(paint.position[0] + paint.size[0]);
        let left = f64::from(paint.position[0]);
        assert!(right > 0.0 && left < 40.0, "{paint:?}");
    }
    rig.view = LayerView {
        shift: [100.0, 0.0],
        window_cells: 40.0,
    };
    rig.settle();
    let moved = rig.cloud_paints();
    assert!(!moved.is_empty(), "впереди небо уже в облаках");
    for sprite in rig.clouds_of(0) {
        let middle = 20.0 + 100.0 * sprite.parallax;
        let half_ring = (3.0 * 40.0 + 2.0 * 6.0 * 1.15) / 2.0;
        assert!(
            (sprite.center[0] - middle).abs() <= half_ring + 1e-9,
            "{sprite:?}"
        );
    }
}

#[test]
fn a_sky_with_only_a_colour_is_drawn_and_its_clouds_lie_over_the_fill() {
    let scene = r##"{"objects":[{"position":[0,0],"size":[40,12],"parallax":0,"repeat_x":true,
        "color":"#336699","clouds":0.3,"cloud_images":["cloud_a"]}]}"##;
    let mut rig = Rig::new(scene).unculled();
    rig.settle();
    assert_eq!(
        order_of(&rig.paints()),
        [vec![Some(0)], vec![None; 7]].concat()
    );
}

#[test]
fn clouds_are_not_objects() {
    let mut rig = Rig::new(SCENE_DRAWN);
    rig.settle();
    rig.frames(30, 0.1);
    assert_eq!(rig.game.world.alive_count(), 3);
    assert_eq!(rig.game.world.ids().count(), 3);
    assert!(
        rig.cloud_paints()
            .iter()
            .all(|paint| paint.object.is_none())
    );
}

#[test]
fn clouds_do_not_count_against_max_objects() {
    let game = game_json(CELL_PIXELS, FILES).replace(r#""max_objects":100"#, r#""max_objects":1"#);
    let (mut loaded, ..) = load_with(
        &game,
        &sky_object(r#""clouds":1,"cloud_images":["cloud_a"]"#),
        NO_RULES,
        None,
    )
    .expect("один объект при потолке в один");
    step(&mut loaded, 5);
    assert_eq!(loaded.world.alive_count(), 1);
}

// -------------------------------------------------------------------------------------------
// Размер облака
// -------------------------------------------------------------------------------------------

#[test]
fn the_base_size_is_the_size_of_the_picture_or_its_points_over_cell_pixels() {
    let rig = Rig::new(&sky_object(SKY_WITH_CLOUDS));
    let atlas_images: Vec<AtlasImage> = rig
        .images
        .iter()
        .map(|decl| {
            match verdicts()
                .into_iter()
                .find(|(n, _)| *n == decl.name)
                .unwrap()
                .1
            {
                ImageVerdict::Ok {
                    width,
                    height,
                    pixels,
                } => AtlasImage {
                    width,
                    height,
                    pixels,
                },
                ImageVerdict::Video { width, height } => AtlasImage::video_frame(width, height / 2),
                _ => unreachable!(),
            }
        })
        .collect();
    let with_pixels = cloud_base_sizes(&atlas_images, &rig.images, Some(4.0));
    let index = |name: &str| rig.images.iter().position(|d| d.name == name).unwrap();
    assert_eq!(
        with_pixels[index("cloud_a")],
        Some([6.0, 3.0]),
        "size картинки"
    );
    assert_eq!(
        with_pixels[index("cloud_b")],
        Some([4.0, 2.0]),
        "16×8 точек на 4"
    );
    assert_eq!(with_pixels[index("clip")], None, "видео");
    let without = cloud_base_sizes(&atlas_images, &rig.images, None);
    assert_eq!(without[index("cloud_a")], Some([6.0, 3.0]));
    assert_eq!(without[index("cloud_b")], None);
}

#[test]
fn a_cloud_is_drawn_at_a_share_of_the_base_size_of_its_picture() {
    let mut rig = Rig::new(&sky_object(r#""clouds":1,"cloud_images":["cloud_b"]"#));
    rig.settle();
    for paint in rig.cloud_paints() {
        let width = f64::from(paint.size[0]);
        assert!(
            (4.0 * 0.5 * 0.85 - 1e-4..=4.0 * 1.15 + 1e-4).contains(&width),
            "{width}"
        );
        assert!((f64::from(paint.size[1]) / width - 0.5).abs() < 1e-5);
    }
}

// -------------------------------------------------------------------------------------------
// Проступание, таяние, ветер, часы
// -------------------------------------------------------------------------------------------

const SKY_FOR_RULES: &str = r#"{"objects":[
    {"position":[0,0],"size":[40,12],"image":"sky","parallax":0,"repeat_x":true,"mark":true,"clouds":0,"cloud_images":["cloud_a"]}]}"#;

#[test]
fn a_sky_that_gets_its_clouds_from_a_rule_starts_from_nothing_and_grows_in_four_seconds() {
    let rules =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","clouds",0.3]]}]}"#;
    let mut rig = Rig::with_rules(SKY_FOR_RULES, rules);
    rig.settle();
    assert!(rig.clouds_of(0).is_empty());
    rig.game.step(StepInput::empty());
    rig.settle();
    assert_eq!(rig.game.world.number_like(0, property::CLOUDS), Some(0.3));
    let clouds = rig.clouds_of(0);
    assert_eq!(clouds.len(), 7);
    assert!(
        clouds.iter().all(|cloud| cloud.opacity == 0.0),
        "{clouds:?}"
    );

    rig.frames(20, 0.1);
    let half = rig.clouds_of(0);
    assert!(
        half.iter()
            .all(|cloud| cloud.opacity > 0.0 && cloud.opacity < 0.55 + 0.45),
        "{half:?}"
    );
    rig.frames(25, 0.1);
    let full = rig.clouds_of(0);
    assert!(full.iter().all(|cloud| cloud.opacity >= 0.55), "{full:?}");
}

#[test]
fn a_sky_given_clouds_in_the_editor_starts_from_nothing_too() {
    let mut rig = Rig::new(&sky_object(r#""cloud_images":["cloud_a"]"#));
    rig.settle();
    assert!(rig.clouds_of(0).is_empty());
    let (images, id) = (rig.images.clone(), 0);
    session::set_property(None, &mut rig.game, &images, id, "clouds", &json!(0.5)).unwrap();
    rig.settle();
    let clouds = rig.clouds_of(0);
    assert_eq!(clouds.len(), 12);
    assert!(clouds.iter().all(|cloud| cloud.opacity == 0.0));
}

#[test]
fn fewer_clouds_melt_and_go_after_four_seconds() {
    let mut rig = Rig::new(&sky_object(
        r#""clouds":1,"cloud_images":["cloud_a","cloud_b"]"#,
    ));
    rig.settle();
    assert_eq!(rig.clouds_of(0).len(), 24);
    rig.game.world.set_number(0, property::CLOUDS, 0.3);
    rig.frames(1, 0.05);
    assert_eq!(rig.clouds_of(0).len(), 24, "17 тают");
    rig.frames(45, 0.1);
    assert_eq!(rig.clouds_of(0).len(), 7);
}

#[test]
fn a_zero_a_removed_property_an_empty_list_and_a_deleted_sky_melt_every_cloud() {
    for cut in 0..4 {
        let mut rig = Rig::new(&sky_object(SKY_WITH_CLOUDS));
        rig.settle();
        match cut {
            0 => rig.game.world.set_number(0, property::CLOUDS, 0.0),
            1 => rig.game.world.clear_property(0, property::CLOUDS),
            2 => rig
                .game
                .world
                .set_image_list(0, property::CLOUD_IMAGES, vec![]),
            _ => rig.game.world.delete(0),
        }
        rig.frames(1, 0.05);
        let melting = rig.motion.clouds().live_sprites(0, 0).count()
            + rig.motion.clouds().orphan_sprites().count();
        assert_eq!(melting, 7, "ещё тают: {cut}");
        rig.frames(45, 0.1);
        let left = rig.motion.clouds().live_sprites(0, 0).count()
            + rig.motion.clouds().orphan_sprites().count();
        assert_eq!(left, 0, "растаяли: {cut}");
    }
}

#[test]
fn the_clouds_of_a_deleted_sky_stay_drawn_while_they_melt() {
    let mut rig = Rig::new(SCENE_DRAWN).unculled();
    rig.settle();
    rig.game.world.delete(0);
    rig.frames(1, 0.05);
    let paints = rig.paints();
    assert_eq!(paints.len(), 7 + 2);
    assert_eq!(
        order_of(&paints),
        [vec![Some(2)], vec![None; 7], vec![Some(1)]].concat(),
        "после объектов своего слоя и до следующего"
    );
}

#[test]
fn a_picture_taken_out_of_the_list_melts_its_clouds_and_new_ones_take_their_place() {
    let mut rig = Rig::new(&sky_object(
        r#""clouds":0.5,"cloud_images":["cloud_a","cloud_b"]"#,
    ));
    rig.settle();
    assert_eq!(rig.clouds_of(0).len(), 12);
    let cloud_b = rig
        .clouds_of(0)
        .iter()
        .filter(|c| c.image == CLOUD_B)
        .count();
    assert!(cloud_b > 0 && cloud_b < 12, "{cloud_b}");
    rig.game
        .world
        .set_image_list(0, property::CLOUD_IMAGES, vec![CLOUD_A]);
    rig.frames(1, 0.05);
    assert!(
        rig.clouds_of(0).iter().any(|c| c.image == CLOUD_B),
        "облака с убранной картинкой ещё тают"
    );
    rig.frames(45, 0.1);
    let after = rig.clouds_of(0);
    assert_eq!(after.len(), 12);
    assert!(after.iter().all(|cloud| cloud.image == CLOUD_A));
}

fn drift_of_one_cloud(wind: Option<[f64; 2]>, seconds: f64) -> (f64, f64) {
    let mut rig = Rig::new(&sky_object(r#""clouds":0.04,"cloud_images":["cloud_a"]"#));
    if let Some(wind) = wind {
        rig.game.set_wind(wind).unwrap();
    }
    rig.settle();
    let before = rig.clouds_of(0);
    assert_eq!(before.len(), 1);
    rig.frames((seconds * 10.0) as u32, 0.1);
    let after = rig.clouds_of(0);
    assert_eq!(after.len(), 1);
    // Близость — по `parallax` облака: у неба он 0, у самого близкого облака 0,12 (требование 6).
    let nearness = before[0].parallax / 0.12;
    (after[0].center[0] - before[0].center[0], nearness)
}

#[test]
fn the_flat_wind_carries_the_clouds_and_the_wind_over_y_does_not() {
    let (moved, nearness) = drift_of_one_cloud(Some([1.5, 0.0]), 10.0);
    let expected = 10.0 * 0.15 * (0.3 + 0.7 * nearness);
    assert!((moved - expected).abs() < 1e-3, "{moved} против {expected}");

    let (moved, nearness) = drift_of_one_cloud(Some([-1.5, 0.0]), 10.0);
    let expected = -10.0 * 0.15 * (0.3 + 0.7 * nearness);
    assert!((moved - expected).abs() < 1e-3, "{moved} против {expected}");

    let (moved, nearness) = drift_of_one_cloud(None, 10.0);
    let expected = 10.0 * 0.03 * (0.3 + 0.7 * nearness);
    assert!((moved - expected).abs() < 1e-3, "{moved} против {expected}");

    let (moved, nearness) = drift_of_one_cloud(Some([0.0, 3.0]), 10.0);
    let expected = 10.0 * 0.03 * (0.3 + 0.7 * nearness);
    assert!(
        (moved - expected).abs() < 1e-3,
        "ветер по y облако не двигает: {moved}"
    );
}

#[test]
fn a_wind_changed_in_the_editor_turns_the_clouds_at_once() {
    let mut rig = Rig::new(&sky_object(r#""clouds":0.04,"cloud_images":["cloud_a"]"#));
    rig.settle();
    rig.game.set_wind([1.5, 0.0]).unwrap();
    let start = rig.clouds_of(0)[0].center[0];
    rig.frames(20, 0.1);
    let right = rig.clouds_of(0)[0].center[0];
    rig.game.set_wind([-1.5, 0.0]).unwrap();
    rig.frames(40, 0.1);
    let left = rig.clouds_of(0)[0].center[0];
    assert!(right > start && left < right, "{start} {right} {left}");
}

#[test]
fn a_stopped_world_stops_the_clouds_and_the_stop_and_the_party_start_put_the_sky_back_in_full() {
    let mut rig = Rig::new(SCENE_DRAWN);
    rig.settle();
    rig.motion.tick(Some(10), 0.0);
    rig.settle();
    let before = rig.clouds_of(0);
    for _ in 0..10 {
        rig.motion.tick(Some(10), 0.0);
        rig.settle();
    }
    assert_eq!(before, rig.clouds_of(0), "мир стоит — облака стоят");

    rig.game.world.set_number(0, property::CLOUDS, 0.0);
    rig.motion.tick(Some(11), 0.0);
    rig.settle();
    rig.motion.reset();
    rig.game.world.set_number(0, property::CLOUDS, 0.3);
    rig.motion.tick(Some(0), 0.0);
    rig.settle();
    let again = rig.clouds_of(0);
    assert_eq!(again.len(), 7);
    assert!(again.iter().all(|cloud| cloud.opacity >= 0.55), "{again:?}");
}

#[test]
fn a_reload_of_the_files_keeps_the_clouds_and_new_picture_sizes_regrow_them_from_nothing() {
    let mut rig = Rig::new(&sky_object(SKY_WITH_CLOUDS));
    rig.settle();
    let before = rig.clouds_of(0);
    let same = |rig: &Rig| {
        let atlas_images: Vec<AtlasImage> = rig
            .images
            .iter()
            .map(|decl| {
                match verdicts()
                    .into_iter()
                    .find(|(n, _)| *n == decl.name)
                    .unwrap()
                    .1
                {
                    ImageVerdict::Ok {
                        width,
                        height,
                        pixels,
                    } => AtlasImage {
                        width,
                        height,
                        pixels,
                    },
                    _ => AtlasImage::video_frame(8, 8),
                }
            })
            .collect();
        cloud_base_sizes(&atlas_images, &rig.images, rig.game.scene.cell_pixels)
    };
    let sizes = same(&rig);
    rig.motion.set_cloud_sizes(sizes);
    rig.motion.world_rebuilt();
    rig.settle();
    assert_eq!(before, rig.clouds_of(0), "те же размеры — облака остались");

    let mut other = same(&rig);
    other[1] = Some([9.0, 9.0]);
    rig.motion.set_cloud_sizes(other);
    rig.settle();
    let regrown = rig.clouds_of(0);
    assert_eq!(regrown.len(), 7);
    assert!(
        regrown.iter().all(|cloud| cloud.opacity == 0.0),
        "{regrown:?}"
    );
}

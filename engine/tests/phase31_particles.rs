//! Фаза 31 — частицы в плоской сцене: файл видов `particles.json`, свойство `particles`, их проверка
//! перед запуском и предупреждение о неиспользованном виде, вылет и прогрев источников, часы движения,
//! своя случайность частиц, рисунки частиц в списке рисования, `particles` в коде и правилах,
//! `set_wind_particles` с видами. Формулы полёта — в `render::particles`'s own `mod tests`; сама
//! видеокарта и браузер — только QA на стенде.

use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::particles::{ParticleKind, ParticleLook, ParticleShape};
use engine::core::property;
use engine::core::scene::LayerView;
use engine::core::screens::{ButtonCommand, ScreenState, ScreensConfig, apply_command};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest_with_particles, read_entry};
use engine::data::session::{self, PlaySession};
use engine::render::atlas::{
    ATLAS_SIZE, AtlasImage, AtlasRect, DOT_RECT, LEAF_RECT, RectPaint, SHEET_BYTES, SMOKE_RECT,
    SPARK_RECT, compose_world_paints, fill_sheet, pack, particle_emitters, sway_objects,
};
use engine::render::wind::Motion;
use serde_json::json;

const VIEWPORT: [f32; 2] = [800.0, 600.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"mark":"flag","hits":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;

const FILES: &str = r#","particles":"particles.json","images":{
    "puff":{"path":"puff.png"},
    "spark":{"path":"spark.png","glow":true},
    "wide":{"path":"wide.png"},
    "leaf":{"path":"leaf.png","frames":3},
    "sheet":{"path":"sheet.png","frames":4,"frame_time":0.25},
    "sized":{"path":"sized.png","size":[2,2]},
    "by":{"path":"by.png","frames":2,"frame_by":"hits"}}"#;

const SMOKE: &str = r#"{"дым":{"image":"puff","rate":6,"lifetime":[4,6],"size":[0.6,0.9],
    "grow":3,"speed":[0.6,1],"spread":20,"opacity":[0,0.7,0],"spin":[-20,20]}}"#;

const CHIMNEY: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"particles":"дым"}]}"#;
const PLAIN: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true}]}"#;

fn game_json(scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

fn verdicts() -> Vec<(&'static str, ImageVerdict)> {
    let ok = |width: u32, height: u32| ImageVerdict::Ok {
        width,
        height,
        pixels: vec![255u8; (width * height * 4) as usize],
    };
    vec![
        ("puff", ok(8, 8)),
        ("spark", ok(4, 4)),
        ("wide", ok(16, 8)),
        ("leaf", ok(24, 8)),
        ("sheet", ok(32, 8)),
        ("sized", ok(8, 8)),
        ("by", ok(16, 8)),
    ]
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn load_with(
    game: &str,
    scene: &str,
    rules: &str,
    particles: Option<&str>,
    code: Option<&str>,
) -> Result<Loaded, LoadFailure> {
    let (config, _warnings) = read_entry(game).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = verdicts()
        .into_iter()
        .map(|(name, verdict)| (name.to_string(), verdict))
        .collect();
    load_rest_with_particles(
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
        &[],
        None,
        &[],
        &[],
        &[],
        particles,
    )
}

fn load_flat(
    scene: &str,
    rules: &str,
    particles: Option<&str>,
    code: Option<&str>,
) -> Result<Loaded, LoadFailure> {
    let files = if code.is_some() {
        format!(r#"{FILES},"code":"code.lua""#)
    } else {
        FILES.to_string()
    };
    load_with(&game_json("", &files), scene, rules, particles, code)
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

// -------------------------------------------------------------------------------------------
// Файл видов: разбор
// -------------------------------------------------------------------------------------------

#[test]
fn the_entry_names_the_particles_file_and_a_game_without_one_names_nothing() {
    let (config, _) = read_entry(&game_json("", FILES)).unwrap();
    assert_eq!(config.files.particles.as_deref(), Some("particles.json"));
    let (config, _) = read_entry(&game_json("", "")).unwrap();
    assert_eq!(config.files.particles, None);
}

#[test]
fn a_kind_from_the_example_loads_as_written() {
    let (game, ..) = load_flat(CHIMNEY, NO_RULES, Some(SMOKE), None).expect("должно загрузиться");
    let kind = game.particles().find("дым").expect("вид есть");
    assert_eq!(kind.rate, 6.0);
    assert_eq!((kind.lifetime.from, kind.lifetime.to), (4.0, 6.0));
    assert_eq!((kind.size.from, kind.size.to), (0.6, 0.9));
    assert_eq!(kind.grow, 3.0);
    assert_eq!((kind.speed.from, kind.speed.to), (0.6, 1.0));
    assert_eq!(kind.spread, 20.0);
    assert_eq!(kind.opacity, vec![0.0, 0.7, 0.0]);
    let spin = kind.spin.expect("spin есть");
    assert_eq!((spin.from, spin.to), (-20.0, 20.0));
}

#[test]
fn a_number_is_a_pair_with_equal_ends_and_the_optional_keys_have_their_defaults() {
    let text = r#"{"искра":{"image":"spark","rate":2,"lifetime":3,"size":0.5}}"#;
    let scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"particles":"искра"}]}"#;
    let (game, ..) = load_flat(scene, NO_RULES, Some(text), None).expect("должно загрузиться");
    let kind: &ParticleKind = game.particles().find("искра").unwrap();
    assert_eq!((kind.lifetime.from, kind.lifetime.to), (3.0, 3.0));
    assert_eq!((kind.size.from, kind.size.to), (0.5, 0.5));
    assert_eq!((kind.grow, kind.direction, kind.spread), (1.0, 0.0, 0.0));
    assert_eq!((kind.speed.from, kind.speed.to), (0.0, 0.0));
    assert_eq!((kind.gravity, kind.wobble, kind.wind), (0.0, 0.0, 1.0));
    assert_eq!(kind.opacity, vec![1.0]);
    assert_eq!(kind.spin, None);

    let single =
        r#"{"a":{"image":"spark","rate":1,"lifetime":1,"size":1,"opacity":0.4,"spin":-5}}"#;
    let scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"particles":"a"}]}"#;
    let (game, ..) = load_flat(scene, NO_RULES, Some(single), None).unwrap();
    let kind = game.particles().find("a").unwrap();
    assert_eq!(kind.opacity, vec![0.4]);
    let spin = kind.spin.unwrap();
    assert_eq!((spin.from, spin.to), (-5.0, -5.0));
}

#[test]
fn every_broken_kind_is_reported_with_the_file_and_the_place() {
    let base = r#""image":"puff","rate":6,"lifetime":4,"size":1"#;
    let cases: Vec<(String, &str, &str)> = vec![
        (
            format!(r#"{{"дым":{{{base},"колбаса":1}}}}"#),
            "дым → колбаса",
            "неизвестное поле",
        ),
        (
            r#"{"дым":{"image":"puff","lifetime":4,"size":1}}"#.to_string(),
            "дым",
            "rate",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"size":1}}"#.to_string(),
            "дым",
            "lifetime",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":4}}"#.to_string(),
            "дым",
            "size",
        ),
        (r#"{"дым":"облако"}"#.to_string(), "дым", "ожидался объект"),
        (
            r#"{"дым":{"image":"nope","rate":6,"lifetime":4,"size":1}}"#.to_string(),
            "дым → image",
            "картинки",
        ),
        (
            r#"{"дым":{"image":"sized","rate":6,"lifetime":4,"size":1}}"#.to_string(),
            "дым → image",
            "size",
        ),
        (
            r#"{"дым":{"image":"by","rate":6,"lifetime":4,"size":1}}"#.to_string(),
            "дым → image",
            "frame_by",
        ),
        (
            r#"{"дым":{"image":"puff","rate":0,"lifetime":4,"size":1}}"#.to_string(),
            "дым → rate",
            "больше нуля",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":0,"size":1}}"#.to_string(),
            "дым → lifetime",
            "больше нуля",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":[0,2],"size":1}}"#.to_string(),
            "дым → lifetime",
            "больше нуля",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":4,"size":-1}}"#.to_string(),
            "дым → size",
            "больше нуля",
        ),
        (
            format!(r#"{{"дым":{{{base},"grow":0}}}}"#),
            "дым → grow",
            "больше нуля",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":[6,4],"size":1}}"#.to_string(),
            "дым → lifetime",
            "первое число больше второго",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":[4],"size":1}}"#.to_string(),
            "дым → lifetime",
            "ровно два числа",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":[1,2,3],"size":1}}"#.to_string(),
            "дым → lifetime",
            "ровно два числа",
        ),
        (
            r#"{"дым":{"image":"puff","rate":6,"lifetime":"a","size":1}}"#.to_string(),
            "дым → lifetime",
            "число или пара",
        ),
        (
            format!(r#"{{"дым":{{{base},"speed":[2,1]}}}}"#),
            "дым → speed",
            "первое число больше второго",
        ),
        (
            format!(r#"{{"дым":{{{base},"spin":[5,1]}}}}"#),
            "дым → spin",
            "первое число больше второго",
        ),
        (
            format!(r#"{{"дым":{{{base},"spread":181}}}}"#),
            "дым → spread",
            "от 0 до 180",
        ),
        (
            format!(r#"{{"дым":{{{base},"spread":-1}}}}"#),
            "дым → spread",
            "от 0 до 180",
        ),
        (
            format!(r#"{{"дым":{{{base},"opacity":1.5}}}}"#),
            "дым → opacity",
            "от 0 до 1",
        ),
        (
            format!(r#"{{"дым":{{{base},"opacity":[0,2]}}}}"#),
            "дым → opacity",
            "от 0 до 1",
        ),
        (
            format!(r#"{{"дым":{{{base},"opacity":[]}}}}"#),
            "дым → opacity",
            "не может быть пустым",
        ),
        (
            format!(r#"{{"дым":{{{base},"opacity":"a"}}}}"#),
            "дым → opacity",
            "ожидался массив",
        ),
        (
            format!(r#"{{"дым":{{{base},"wind":2}}}}"#),
            "дым → wind",
            "от 0 до 1",
        ),
        (
            format!(r#"{{"дым":{{{base},"wind":-0.1}}}}"#),
            "дым → wind",
            "от 0 до 1",
        ),
        (
            format!(r#"{{"дым":{{{base},"wobble":-1}}}}"#),
            "дым → wobble",
            "меньше нуля",
        ),
        (format!(r#"{{"":{{{base}}}}}"#), "", "пустое имя вида"),
        (r#"[1]"#.to_string(), "", "ожидался объект"),
        ("{".to_string(), "", "не разбирается как JSON"),
    ];
    for (text, path, message) in &cases {
        let errors = errors_of(load_flat(PLAIN, NO_RULES, Some(text), None));
        assert_error(&errors, "particles.json", path, message);
    }
}

#[test]
fn a_kind_error_carries_the_line_of_the_field() {
    let text = "{\n  \"дым\": {\n    \"image\": \"puff\",\n    \"rate\": 0,\n    \"lifetime\": 4,\n    \"size\": 1\n  }\n}";
    let errors = errors_of(load_flat(PLAIN, NO_RULES, Some(text), None));
    let error = errors
        .iter()
        .find(|e| e.path == "дым → rate")
        .expect("ошибка rate");
    assert_eq!(error.line, Some(4), "{error:?}");
}

#[test]
fn a_named_particles_file_that_is_missing_is_an_error() {
    let errors = errors_of(load_flat(PLAIN, NO_RULES, None, None));
    assert_error(&errors, "particles.json", "", "файл не найден");
}

#[test]
fn particles_in_a_three_dimensional_scene_are_errors_in_the_game_and_in_the_scene() {
    let LoadFailure { errors, .. } =
        read_entry(&game_json(CAMERA_3D, FILES)).expect_err("files.particles в трёхмерной сцене");
    assert_error(
        &errors,
        "game.json",
        "files → particles",
        "particles есть только в плоской сцене: у scene в game.json есть camera",
    );
    let scene =
        r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","particles":"дым"}]}"##;
    let errors = errors_of(load_with(
        &game_json(CAMERA_3D, ""),
        scene,
        NO_RULES,
        None,
        None,
    ));
    assert_error(&errors, "scene.json", "particles", "только в плоской сцене");
    let plain = r##"{"objects":[{"position":[1,1],"size":[1,1],"color":"#ffffff","mark":true}]}"##;
    let rule =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","particles","дым"]]}]}"#;
    let errors = errors_of(load_with(
        &game_json(CAMERA_3D, ""),
        plain,
        rule,
        None,
        None,
    ));
    assert_error(
        &errors,
        "rules.json",
        "",
        "particles есть только в плоской сцене",
    );
}

// -------------------------------------------------------------------------------------------
// Свойство `particles`: проверка везде, где его можно записать
// -------------------------------------------------------------------------------------------

#[test]
fn an_unknown_kind_is_an_error_in_the_scene_a_template_a_rule_a_key_and_a_click() {
    let in_scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"particles":"туман"}]}"#;
    let errors = errors_of(load_flat(in_scene, NO_RULES, Some(SMOKE), None));
    assert_error(
        &errors,
        "scene.json",
        "particles",
        "неизвестный вид частиц: туман",
    );

    let with_mark = r#"{"objects":[{"position":[1,1],"size":[1,1],"mark":true}]}"#;
    let rule =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","particles","туман"]]}]}"#;
    let errors = errors_of(load_flat(with_mark, rule, Some(SMOKE), None));
    assert_error(&errors, "rules.json", "", "неизвестный вид частиц: туман");

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"size":[1,1],"particles":"туман"}}]}"#;
    let errors = errors_of(load_flat(with_mark, template, Some(SMOKE), None));
    assert_error(&errors, "rules.json", "", "неизвестный вид частиц: туман");

    for object in [
        r#"{"position":[1,1],"size":[1,1],"keys":{"Space":{"press":[["particles","туман"]]}}}"#,
        r#"{"position":[1,1],"size":[1,1],"on_click":[["particles","туман"]]}"#,
    ] {
        let scene = format!(r#"{{"objects":[{object}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, Some(SMOKE), None));
        assert_error(&errors, "scene.json", "", "неизвестный вид частиц: туман");
    }
}

#[test]
fn a_particles_value_that_is_not_a_string_is_an_error() {
    let scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"particles":5}]}"#;
    let errors = errors_of(load_flat(scene, NO_RULES, Some(SMOKE), None));
    assert_error(&errors, "scene.json", "particles", "ожидалась строка");
}

#[test]
fn a_source_needs_a_position_and_a_size_and_does_not_repeat() {
    for object in [
        r#"{"size":[1,1],"particles":"дым"}"#,
        r#"{"position":[1,1],"particles":"дым"}"#,
    ] {
        let scene = format!(r#"{{"objects":[{object}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, Some(SMOKE), None));
        assert_error(
            &errors,
            "scene.json",
            "",
            "particles разрешён только объекту с position и size",
        );
    }
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"particles":"дым","repeat_x":true,
        "color":"#ffffff"}]}"##;
    let errors = errors_of(load_flat(scene, NO_RULES, Some(SMOKE), None));
    assert_error(&errors, "scene.json", "", "вместе с repeat_x");

    let rules = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"particles":"дым"}}]}"#;
    let errors = errors_of(load_flat(PLAIN, rules, Some(SMOKE), None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "particles разрешён только объекту с position и size",
    );
}

#[test]
fn a_rule_may_create_an_object_with_a_known_kind_and_set_another_one() {
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
         "where":{"at":[3,3]},"template":{"hits":0,"size":[1,1],"particles":"дым"}},
        {"kind":"check","for":{"has":["particles"]},"do":[["set","particles","огонь"]]}]}"#;
    let (mut game, ..) =
        load_flat(PLAIN, rules, Some(CODE_KINDS), None).expect("должно загрузиться");
    step(&mut game, 1);
    let sources: Vec<Option<&str>> = game
        .world
        .ids()
        .map(|id| game.world.text(id, property::PARTICLES))
        .collect();
    assert_eq!(sources, [None, Some("дым")], "создан источник с видом дым");
    step(&mut game, 1);
    assert_eq!(game.world.text(1, property::PARTICLES), Some("огонь"));
}

// -------------------------------------------------------------------------------------------
// Предупреждение о неиспользованном виде
// -------------------------------------------------------------------------------------------

fn unused_warnings(warnings: &[GameError]) -> Vec<&GameError> {
    warnings
        .iter()
        .filter(|w| w.message.contains("вид частиц"))
        .collect()
}

#[test]
fn a_kind_nobody_names_is_a_warning_and_the_game_still_loads() {
    let (_game, _screens, warnings, _images) =
        load_flat(PLAIN, NO_RULES, Some(SMOKE), None).expect("предупреждение не мешает");
    let found = unused_warnings(&warnings);
    assert_eq!(found.len(), 1, "{warnings:?}");
    assert_eq!(found[0].file, "particles.json");
    assert_eq!(
        found[0].message,
        "вид частиц дым объявлен и не используется"
    );
}

#[test]
fn a_kind_named_by_the_scene_a_rule_or_the_text_of_the_code_is_not_warned_about() {
    let (_, _, warnings, _) = load_flat(CHIMNEY, NO_RULES, Some(SMOKE), None).unwrap();
    assert!(unused_warnings(&warnings).is_empty(), "{warnings:?}");

    let rule =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","particles","дым"]]}]}"#;
    let (_, _, warnings, _) = load_flat(PLAIN, rule, Some(SMOKE), None).unwrap();
    assert!(unused_warnings(&warnings).is_empty(), "{warnings:?}");

    let spawn = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"size":[1,1],"particles":"дым"}}]}"#;
    let (_, _, warnings, _) = load_flat(PLAIN, spawn, Some(SMOKE), None).unwrap();
    assert!(unused_warnings(&warnings).is_empty(), "{warnings:?}");

    let key = r#"{"objects":[{"position":[1,1],"size":[1,1],"keys":{"Space":{"press":[["particles","дым"]]}}}]}"#;
    let (_, _, warnings, _) = load_flat(key, NO_RULES, Some(SMOKE), None).unwrap();
    assert!(unused_warnings(&warnings).is_empty(), "{warnings:?}");

    let code = "function ignite(obj)\n  obj.particles = \"дым\"\nend";
    let (_, _, warnings, _) = load_flat(PLAIN, NO_RULES, Some(SMOKE), Some(code)).unwrap();
    assert!(unused_warnings(&warnings).is_empty(), "{warnings:?}");
}

#[test]
fn a_picture_that_only_a_kind_uses_is_not_reported_as_unused() {
    let (_, _, warnings, _) = load_flat(CHIMNEY, NO_RULES, Some(SMOKE), None).unwrap();
    assert!(
        !warnings.iter().any(|w| w.path.contains("puff")),
        "{warnings:?}"
    );
}

const DOT_KIND: &str = r#"{"точки":{"rate":6,"lifetime":4,"size":1}}"#;
const DOT_SOURCE: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"particles":"точки"}]}"#;

fn declared_images() -> Vec<String> {
    verdicts()
        .into_iter()
        .map(|(name, _)| name.to_string())
        .collect()
}

#[test]
fn a_kind_without_an_image_loads_with_the_built_in_dot_and_no_picture_warnings() {
    let (game, _screens, warnings, _images) =
        load_flat(DOT_SOURCE, NO_RULES, Some(DOT_KIND), None).expect("image необязателен");
    assert_eq!(
        game.particles().find("точки").unwrap().look,
        ParticleLook::Shape(ParticleShape::Dot)
    );
    let declared = declared_images();
    let pictures: Vec<&GameError> = warnings
        .iter()
        .filter(|warning| warning.path.starts_with("files → images → "))
        .collect();
    assert_eq!(pictures.len(), declared.len(), "{warnings:?}");
    for warning in pictures {
        assert!(
            declared
                .iter()
                .any(|name| warning.path == format!("files → images → {name}")),
            "предупреждение не про объявленную картинку: {warning:?}"
        );
    }
    assert!(
        warnings
            .iter()
            .all(|warning| warning.file != "particles.json")
    );
}

#[test]
fn a_kind_with_an_image_still_holds_its_picture() {
    let (game, ..) = load_flat(CHIMNEY, NO_RULES, Some(SMOKE), None).expect("должно загрузиться");
    let kind = game.particles().find("дым").unwrap();
    assert!(matches!(kind.look, ParticleLook::Image(_)));
}

#[test]
fn the_built_in_dot_takes_no_name_from_the_pictures_of_the_game() {
    let scene = r#"{"objects":[{"position":[10,10],"size":[2,1],"particles":"точки"},
        {"position":[1,1],"size":[1,1],"image":"dot"}]}"#;
    let errors = errors_of(load_flat(scene, NO_RULES, Some(DOT_KIND), None));
    assert_error(&errors, "scene.json", "", "картинки \"dot\" нет");
}

// -------------------------------------------------------------------------------------------
// Вылет, прогрев и часы
// -------------------------------------------------------------------------------------------

struct Rig {
    game: Game,
    images: Vec<ImageDecl>,
    rects: Vec<AtlasRect>,
    motion: Motion,
}

impl Rig {
    fn new(scene: &str, rules: &str, particles: &str) -> Rig {
        let (game, _screens, _warnings, images) =
            load_flat(scene, rules, Some(particles), None).expect("должно загрузиться");
        Rig::from_loaded(game, images)
    }

    fn from_loaded(game: Game, images: Vec<ImageDecl>) -> Rig {
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
                    _ => unreachable!("все картинки теста — ok"),
                }
            })
            .collect();
        let rects = pack(&atlas_images).expect("умещаются").rects;
        Rig {
            game,
            images,
            rects,
            motion: Motion::default(),
        }
    }

    /// То же, что `wasm::update_motion`: часы доведены, наклоны и частицы посчитаны.
    fn settle(&mut self) {
        let wind = self.game.wind();
        self.motion
            .update(wind[0], sway_objects(&self.game.world, &self.images));
        self.motion.update_particles(
            self.game.has_world(),
            wind,
            particle_emitters(&self.game.world),
            self.game.particles(),
        );
    }

    /// Кадры редактора вне партии: часы идут по `draw(dt)`.
    fn frames(&mut self, count: u32, dt: f64) {
        for _ in 0..count {
            self.motion.tick(None, dt);
            self.settle();
        }
    }

    fn live(&self) -> usize {
        self.motion.particles().count()
    }

    fn paints_in(&self, layers: &LayerView) -> Vec<RectPaint> {
        compose_world_paints(
            &self.game.world,
            &self.game.scene,
            self.game.world.ids(),
            &self.motion,
            &self.images,
            &self.rects,
            layers,
        )
    }

    fn paints(&self) -> Vec<RectPaint> {
        self.paints_in(&LayerView::default())
    }

    fn particle_paints(&self) -> Vec<RectPaint> {
        self.paints()
            .into_iter()
            .filter(|paint| paint.object.is_none())
            .collect()
    }
}

/// Бесконечно живущие клубы: считаются все вылетевшие.
const FOREVER: &str =
    r#"{"дым":{"image":"puff","rate":6,"lifetime":100000,"size":0.5,"speed":0.5,"spread":10}}"#;

#[test]
fn a_new_source_emits_its_rate_per_second_from_a_random_pool() {
    let mut rig = Rig::new(PLAIN, NO_RULES, FOREVER);
    rig.settle();
    rig.game
        .world
        .set_text(0, property::PARTICLES, "дым".to_string());
    rig.settle();
    assert_eq!(
        rig.live(),
        0,
        "источник, получивший particles, начинает с нуля"
    );
    rig.frames(100, 0.1);
    let emitted = rig.live();
    assert!((59..=61).contains(&emitted), "{emitted} за 10 секунд");
}

#[test]
fn a_source_from_the_scene_is_warmed_up_on_the_first_assembly() {
    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.settle();
    let live = rig.live();
    assert!((25..=35).contains(&live), "{live}");
    assert_eq!(
        rig.particle_paints().len(),
        live,
        "частицы доходят до списка рисования"
    );
}

#[test]
fn a_source_that_appears_after_a_reload_or_a_drop_starts_from_nothing() {
    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.settle();
    let warm = rig.live();
    // Файлы перечитаны вне партии: мир собран из сцены заново, прежний источник — тот же, а
    // перетащенный из вкладки — новый, он начинает с нуля.
    rig.game.show_scene();
    rig.motion.world_rebuilt();
    let dropped = rig.game.world.create();
    rig.game
        .world
        .set_vec2(dropped, property::POSITION, [20.0, 10.0]);
    rig.game.world.set_vec2(dropped, property::SIZE, [1.0, 1.0]);
    rig.game
        .world
        .set_text(dropped, property::PARTICLES, "дым".to_string());
    rig.settle();
    assert_eq!(
        rig.live(),
        warm,
        "прежний источник продолжает, новый — ноль"
    );
}

#[test]
fn the_stop_and_the_party_start_warm_the_sources_up_again() {
    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.settle();
    rig.frames(20, 0.1);
    rig.motion.reset();
    rig.settle();
    let live = rig.live();
    assert!((25..=35).contains(&live), "{live}");
}

#[test]
fn a_paused_world_freezes_the_particles_and_the_editor_clock_moves_them() {
    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.motion.tick(Some(60), 0.0);
    rig.settle();
    let before = rig.paints();
    for _ in 0..30 {
        rig.motion.tick(Some(60), 0.1);
        rig.settle();
    }
    assert_eq!(rig.paints(), before, "мир стоит — частицы стоят");
    rig.motion.tick(Some(66), 0.0);
    rig.settle();
    assert_ne!(rig.paints(), before, "шаги мира двигают частицы");

    let mut editor = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    editor.settle();
    let before = editor.paints();
    editor.frames(1, 0.1);
    assert_ne!(
        editor.paints(),
        before,
        "вне партии частицы идут по draw(dt)"
    );
}

#[test]
fn a_clock_that_went_back_or_far_ahead_clears_the_particles_and_warms_the_sources() {
    for jump_to in [10_u64, 900] {
        let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
        rig.motion.tick(Some(100), 0.0);
        rig.settle();
        rig.motion.tick(Some(130), 0.0);
        rig.settle();
        let before = rig.paints();
        rig.motion.tick(Some(jump_to), 0.0);
        rig.settle();
        let live = rig.live();
        assert!((25..=35).contains(&live), "прыжок на {jump_to}: {live}");
        assert_ne!(
            rig.paints(),
            before,
            "прыжок {jump_to}: частицы начались заново"
        );
    }
}

#[test]
fn a_source_with_the_property_taken_stops_and_its_particles_live_out() {
    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.settle();
    let warm = rig.live();
    rig.game.world.clear_property(0, property::PARTICLES);
    rig.frames(10, 0.1);
    assert!(
        rig.live() <= warm && rig.live() > 0,
        "{} из {warm}",
        rig.live()
    );
    rig.frames(100, 0.1);
    assert_eq!(rig.live(), 0, "дожили и исчезли");

    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.settle();
    let warm = rig.live();
    rig.game.world.delete(0);
    rig.frames(5, 0.1);
    assert!(
        rig.live() > 0 && rig.live() <= warm,
        "удалённый источник — то же"
    );
}

#[test]
fn the_particles_do_not_touch_the_course_of_the_party() {
    let code = r#"function tick(obj) print(math.random(1000)) end"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;
    let with_particles =
        r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true,"particles":"дым"}]}"#;
    let run = |scene: &str| {
        let (mut game, _, _, images) =
            load_flat(scene, rules, Some(SMOKE), Some(code)).expect("должно загрузиться");
        let mut motion = Motion::default();
        let mut snapshots = Vec::new();
        for index in 0..40_u64 {
            game.step(StepInput::empty());
            motion.tick(Some(game.step_count()), 0.0);
            motion.update(game.wind()[0], sway_objects(&game.world, &images));
            motion.update_particles(
                game.has_world(),
                game.wind(),
                particle_emitters(&game.world),
                game.particles(),
            );
            snapshots.push((
                index,
                game.messages().to_vec(),
                game.world.vec2(0, property::POSITION),
            ));
        }
        snapshots
    };
    assert_eq!(
        run(with_particles),
        run(PLAIN),
        "та же партия с частицами и без"
    );
}

// -------------------------------------------------------------------------------------------
// Рисунки частиц
// -------------------------------------------------------------------------------------------

const SCENE_ORDER: &str = r##"{"objects":[
    {"position":[2,2],"size":[1,1],"color":"#ffffff"},
    {"position":[10,10],"size":[2,1],"particles":"дым"},
    {"position":[4,2],"size":[1,1],"color":"#ffffff"},
    {"position":[6,2],"size":[1,1],"color":"#ffffff","layer":1}]}"##;

fn order_of(paints: &[RectPaint]) -> Vec<Option<u32>> {
    paints.iter().map(|paint| paint.object).collect()
}

#[test]
fn the_particles_of_a_live_source_follow_it_in_the_draw_order() {
    let mut rig = Rig::new(SCENE_ORDER, NO_RULES, SMOKE);
    rig.settle();
    let order = order_of(&rig.paints());
    let count = rig.live();
    assert!(count > 0);
    let mut expected = vec![Some(0)];
    expected.extend(std::iter::repeat_n(None, count));
    expected.extend([Some(2), Some(3)]);
    assert_eq!(
        order, expected,
        "источник без картинки виден только своими частицами"
    );
}

#[test]
fn the_particles_of_a_vanished_source_come_after_every_object_of_their_layer() {
    let mut rig = Rig::new(SCENE_ORDER, NO_RULES, SMOKE);
    rig.settle();
    let count = rig.live();
    rig.game.world.clear_property(1, property::PARTICLES);
    rig.settle();
    let mut expected = vec![Some(0), Some(2)];
    expected.extend(std::iter::repeat_n(None, count));
    expected.push(Some(3));
    assert_eq!(order_of(&rig.paints()), expected);
}

#[test]
fn a_source_that_lost_its_size_stops_emitting_but_its_particles_stay_drawn() {
    let mut rig = Rig::new(SCENE_ORDER, NO_RULES, SMOKE);
    rig.settle();
    let count = rig.live();
    assert!(count > 0);
    rig.game.world.clear_property(1, property::SIZE);
    rig.settle();
    assert!(rig.live() > 0 && rig.live() <= count);
    let mut expected = vec![Some(0), Some(2)];
    expected.extend(std::iter::repeat_n(None, rig.live()));
    expected.push(Some(3));
    assert_eq!(order_of(&rig.paints()), expected);
}

#[test]
fn a_particle_is_shifted_by_the_parallax_of_its_source_like_the_source_itself() {
    let scene = r##"{"objects":[{"position":[10,10],"size":[2,1],"particles":"дым","parallax":0.5,
        "color":"#ffffff"}]}"##;
    let mut rig = Rig::new(scene, NO_RULES, SMOKE);
    rig.settle();
    let still = rig.paints_in(&LayerView::default());
    let moved = rig.paints_in(&LayerView {
        shift: [10.0, 0.0],
        window_cells: 0.0,
    });
    assert_eq!(still.len(), moved.len());
    assert!(still.len() > 1);
    for (a, b) in still.iter().zip(&moved) {
        assert!(
            (b.position[0] - a.position[0] - 5.0).abs() < 1e-4,
            "{a:?} против {b:?}"
        );
        assert_eq!(a.position[1], b.position[1]);
    }
}

#[test]
fn a_particle_takes_the_glow_of_its_picture_its_proportions_its_opacity_and_a_free_angle() {
    let kinds = r#"{
        "искры":{"image":"spark","rate":10,"lifetime":2,"size":1,"opacity":0.5},
        "лента":{"image":"wide","rate":10,"lifetime":2,"size":1,"spin":30}}"#;
    let scene = r#"{"objects":[
        {"position":[10,10],"size":[2,1],"particles":"искры"},
        {"position":[20,10],"size":[2,1],"particles":"лента"}]}"#;
    let mut rig = Rig::new(scene, NO_RULES, kinds);
    rig.settle();
    let paints = rig.particle_paints();
    let (sparks, ribbons): (Vec<&RectPaint>, Vec<&RectPaint>) =
        paints.iter().partition(|paint| paint.position[0] < 15.0);
    assert!(!sparks.is_empty() && !ribbons.is_empty());
    assert!(sparks.iter().all(|paint| paint.glow), "светящаяся картинка");
    assert!(ribbons.iter().all(|paint| !paint.glow));
    assert!(
        sparks
            .iter()
            .all(|paint| paint.color == [1.0, 1.0, 1.0, 0.5])
    );
    assert!(
        sparks.iter().all(|paint| paint.angle == 0.0),
        "без spin не повёрнуты"
    );
    for ribbon in &ribbons {
        assert!(
            (ribbon.size[1] - ribbon.size[0] / 2.0).abs() < 1e-5,
            "высота по пропорциям кадра 16 × 8: {ribbon:?}"
        );
    }
    assert!(
        ribbons.iter().any(|paint| paint.angle.abs() > 1.0),
        "любой угол"
    );
}

#[test]
fn the_frames_of_a_particle_picture_come_from_its_own_strip() {
    let kinds = r#"{"листья":{"image":"leaf","rate":20,"lifetime":50,"size":1},
        "кадры":{"image":"sheet","rate":20,"lifetime":50,"size":1}}"#;
    let scene = r#"{"objects":[
        {"position":[10,10],"size":[2,1],"particles":"листья"},
        {"position":[20,10],"size":[2,1],"particles":"кадры"}]}"#;
    let mut rig = Rig::new(scene, NO_RULES, kinds);
    rig.frames(5, 0.1);
    let leaf = rig.rects[rig.images.iter().position(|d| d.name == "leaf").unwrap()];
    let paints = rig.particle_paints();
    let leaves: Vec<_> = paints
        .iter()
        .filter(|paint| paint.position[0] < 15.0)
        .collect();
    let mut seen: Vec<u32> = leaves
        .iter()
        .map(|paint| (paint.atlas_rect.x - leaf.x) / (leaf.w / 3))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert!(seen.len() > 1, "листья берут кадр наугад: {seen:?}");
    assert!(seen.iter().all(|frame| *frame < 3));
}

#[test]
fn a_kind_without_an_image_is_drawn_by_the_built_in_dot_in_a_square() {
    let kinds =
        r#"{"точки":{"rate":10,"lifetime":2,"size":1,"grow":2,"opacity":0.5,"spin":[20,40]}}"#;
    let mut rig = Rig::new(DOT_SOURCE, NO_RULES, kinds);
    rig.settle();
    let paints = rig.particle_paints();
    assert!(!paints.is_empty());
    for paint in &paints {
        assert_eq!(paint.atlas_rect, DOT_RECT);
        assert!(paint.smooth && !paint.glow, "{paint:?}");
        assert_eq!(paint.size[0], paint.size[1], "пропорции 1:1: {paint:?}");
        assert_eq!(paint.color, [1.0, 1.0, 1.0, 0.5]);
    }
    assert!(
        paints.iter().any(|paint| paint.angle.abs() > 1.0),
        "поворот как у любой частицы"
    );
    let first = paints
        .iter()
        .map(|paint| paint.size[0])
        .fold(f32::MAX, f32::min);
    let last = paints.iter().map(|paint| paint.size[0]).fold(0.0, f32::max);
    assert!(last > first, "рост как у любой частицы");
}

#[test]
fn kinds_with_and_without_an_image_are_drawn_each_by_its_own_picture() {
    let kinds = r#"{"точки":{"rate":10,"lifetime":2,"size":1},
        "облака":{"image":"puff","rate":10,"lifetime":2,"size":1}}"#;
    let scene = r#"{"objects":[
        {"position":[10,10],"size":[2,1],"particles":"точки"},
        {"position":[20,10],"size":[2,1],"particles":"облака"}]}"#;
    let mut rig = Rig::new(scene, NO_RULES, kinds);
    rig.settle();
    let puff = rig.rects[rig.images.iter().position(|d| d.name == "puff").unwrap()];
    let paints = rig.particle_paints();
    let (dots, clouds): (Vec<&RectPaint>, Vec<&RectPaint>) =
        paints.iter().partition(|paint| paint.position[0] < 15.0);
    assert!(!dots.is_empty() && !clouds.is_empty());
    assert!(dots.iter().all(|paint| paint.atlas_rect == DOT_RECT));
    assert!(clouds.iter().all(|paint| paint.atlas_rect == puff));
}

fn overlaps(a: AtlasRect, b: AtlasRect) -> bool {
    a.sheet == b.sheet && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
}

#[test]
fn the_atlas_always_holds_the_soft_dot_apart_from_the_pictures() {
    let tall = |width: u32, height: u32| AtlasImage {
        width,
        height,
        pixels: vec![255u8; (width * height * 4) as usize],
    };
    for images in [vec![], vec![tall(8, 8), tall(200, 100), tall(1900, 70)]] {
        let atlas = pack(&images).expect("умещаются");
        for rect in &atlas.rects {
            assert!(!overlaps(*rect, DOT_RECT), "{rect:?} лёг на точку");
        }
        let mut sheet = vec![0u8; SHEET_BYTES];
        fill_sheet(&atlas, &images, 0, &mut sheet);
        let alpha_at = |dx: u32, dy: u32| {
            let start = (((DOT_RECT.y + dy) * ATLAS_SIZE + DOT_RECT.x + dx) * 4) as usize;
            let pixel = &sheet[start..start + 4];
            assert_eq!(pixel[0], pixel[3], "цвет умножен на прозрачность");
            pixel[3]
        };
        let middle = DOT_RECT.w / 2;
        assert_eq!(alpha_at(middle, middle), 255);
        assert_eq!(alpha_at(0, 0), 0, "угол квадрата пуст");
        let row: Vec<u8> = (middle..DOT_RECT.w)
            .map(|dx| alpha_at(dx, middle))
            .collect();
        assert!(row.windows(2).all(|pair| pair[0] >= pair[1]), "{row:?}");
        assert!(row[row.len() / 2] > 0 && row[row.len() / 2] < 255);
        assert_eq!(*row.last().unwrap(), 0, "к краю прозрачность 0");
    }
}

#[test]
fn a_game_without_kinds_draws_as_before_without_the_dot() {
    let files = FILES.replace(r#","particles":"particles.json""#, "");
    let scene = r##"{"objects":[
        {"position":[2,2],"size":[1,1],"color":"#ffffff"},
        {"position":[4,2],"size":[1,1],"image":"puff"}]}"##;
    let (game, _screens, _warnings, images) =
        load_with(&game_json("", &files), scene, NO_RULES, None, None)
            .expect("грузится как раньше");
    assert!(game.particles().names().next().is_none());
    let mut rig = Rig::from_loaded(game, images);
    rig.settle();
    let paints = rig.paints();
    assert_eq!(paints.len(), 2);
    assert!(
        paints
            .iter()
            .all(|paint| !BUILT_IN_RECTS.contains(&paint.atlas_rect))
    );
}

// -------------------------------------------------------------------------------------------
// Встроенные рисунки: `shape`
// -------------------------------------------------------------------------------------------

const SHAPE_KINDS: &str = r#"{
    "дым":{"shape":"smoke","rate":10,"lifetime":20,"size":1,"spin":[10,20]},
    "искры":{"shape":"spark","rate":10,"lifetime":20,"size":1},
    "листья":{"shape":"leaf","rate":10,"lifetime":20,"size":1},
    "точки":{"shape":"dot","rate":10,"lifetime":20,"size":1}}"#;
const SHAPE_SOURCES: &str = r#"{"objects":[
    {"position":[0,10],"size":[1,1],"particles":"дым"},
    {"position":[10,10],"size":[1,1],"particles":"искры"},
    {"position":[20,10],"size":[1,1],"particles":"листья"},
    {"position":[30,10],"size":[1,1],"particles":"точки"}]}"#;

const BUILT_IN_RECTS: [AtlasRect; 4] = [DOT_RECT, SMOKE_RECT, SPARK_RECT, LEAF_RECT];

#[test]
fn every_shape_loads_and_a_kind_without_image_and_shape_is_the_dot() {
    let (game, ..) =
        load_flat(SHAPE_SOURCES, NO_RULES, Some(SHAPE_KINDS), None).expect("должно загрузиться");
    for (name, shape) in [
        ("дым", ParticleShape::Smoke),
        ("искры", ParticleShape::Spark),
        ("листья", ParticleShape::Leaf),
        ("точки", ParticleShape::Dot),
    ] {
        assert_eq!(
            game.particles().find(name).unwrap().look,
            ParticleLook::Shape(shape),
            "{name}"
        );
    }
    let (game, ..) = load_flat(DOT_SOURCE, NO_RULES, Some(DOT_KIND), None).unwrap();
    assert_eq!(
        game.particles().find("точки").unwrap().look,
        ParticleLook::Shape(ParticleShape::Dot)
    );
}

#[test]
fn an_unknown_shape_or_a_shape_with_an_image_is_an_error_in_the_file_and_in_an_edit() {
    let cases = [
        (
            r#"{"дым":{"shape":"fog","rate":6,"lifetime":4,"size":1}}"#,
            "дым → shape",
            "ожидалось одно из: smoke, spark, leaf, dot",
        ),
        (
            r#"{"дым":{"shape":5,"rate":6,"lifetime":4,"size":1}}"#,
            "дым → shape",
            "ожидалась строка",
        ),
        (
            r#"{"дым":{"image":"puff","shape":"smoke","rate":6,"lifetime":4,"size":1}}"#,
            "дым",
            "у вида и image, и shape — оставьте одно",
        ),
    ];
    for (text, path, message) in cases {
        let errors = errors_of(load_flat(PLAIN, NO_RULES, Some(text), None));
        assert_error(&errors, "particles.json", path, message);

        let (mut game, _config, images) = live_game();
        let table: serde_json::Value = serde_json::from_str(text).unwrap();
        let error = session::set_wind_particles(None, &mut game, &images, None, Some(&table))
            .expect_err(message);
        assert!(error.contains(path) && error.contains(message), "{error}");
        assert!(
            matches!(
                game.particles().find("дым").unwrap().look,
                ParticleLook::Image(_)
            ),
            "ничего не изменилось"
        );
    }
    let (mut game, _config, images) = live_game();
    let smoke = json!({"дым": {"shape": "smoke", "rate": 6, "lifetime": 4, "size": 1}});
    assert_eq!(
        session::set_wind_particles(None, &mut game, &images, None, Some(&smoke)),
        Ok(())
    );
    assert_eq!(
        game.particles().find("дым").unwrap().look,
        ParticleLook::Shape(ParticleShape::Smoke)
    );
}

/// Рисунки частиц у источника с `x` из `from..from + 10`.
fn painted_from(paints: &[RectPaint], from: f32) -> Vec<RectPaint> {
    paints
        .iter()
        .filter(|paint| (from - 5.0..from + 5.0).contains(&paint.position[0]))
        .copied()
        .collect()
}

#[test]
fn every_shape_is_drawn_by_its_own_place_in_the_atlas_and_only_the_spark_glows() {
    let mut rig = Rig::new(SHAPE_SOURCES, NO_RULES, SHAPE_KINDS);
    rig.frames(30, 0.1);
    let paints = rig.particle_paints();
    for (from, rect, glow) in [
        (0.0, SMOKE_RECT, false),
        (10.0, SPARK_RECT, true),
        (30.0, DOT_RECT, false),
    ] {
        let drawn = painted_from(&paints, from);
        assert!(!drawn.is_empty(), "{rect:?}");
        for paint in &drawn {
            assert_eq!(paint.atlas_rect, rect);
            assert_eq!((paint.smooth, paint.glow), (true, glow), "{paint:?}");
            assert_eq!(paint.size[0], paint.size[1], "пропорции 1:1: {paint:?}");
            assert_eq!(paint.color, [1.0, 1.0, 1.0, 1.0], "подкраски нет");
        }
    }
    assert!(
        painted_from(&paints, 0.0)
            .iter()
            .any(|paint| paint.angle.abs() > 1.0),
        "поворот как у любой частицы"
    );
    let leaves = painted_from(&paints, 20.0);
    assert!(leaves.len() > 20);
    let mut frames: Vec<u32> = leaves
        .iter()
        .map(|paint| {
            assert!(!paint.glow && paint.smooth, "{paint:?}");
            assert_eq!(paint.size[0], paint.size[1]);
            assert_eq!(
                (paint.atlas_rect.y, paint.atlas_rect.w, paint.atlas_rect.h),
                (LEAF_RECT.y, LEAF_RECT.w / 4, LEAF_RECT.h)
            );
            (paint.atlas_rect.x - LEAF_RECT.x) / (LEAF_RECT.w / 4)
        })
        .collect();
    frames.sort_unstable();
    frames.dedup();
    assert_eq!(frames, [0, 1, 2, 3], "лист — кадр наугад из четырёх");
}

fn many_images() -> Vec<AtlasImage> {
    let image = |width: u32, height: u32| AtlasImage {
        width,
        height,
        pixels: vec![255u8; (width * height * 4) as usize],
    };
    let mut images = vec![image(8, 8), image(1700, 70)];
    images.extend((0..40).map(|_| image(500, 300)));
    images
}

#[test]
fn the_built_in_pictures_lie_apart_from_each_other_and_from_the_pictures_on_every_sheet() {
    for (index, a) in BUILT_IN_RECTS.iter().enumerate() {
        assert_eq!(a.sheet, 0);
        for b in &BUILT_IN_RECTS[index + 1..] {
            assert!(!overlaps(*a, *b), "{a:?} и {b:?}");
        }
    }
    for images in [vec![], many_images()] {
        let atlas = pack(&images).expect("умещаются");
        for rect in &atlas.rects {
            for built_in in BUILT_IN_RECTS {
                assert!(!overlaps(*rect, built_in), "{rect:?} лёг на {built_in:?}");
            }
        }
        if !images.is_empty() {
            assert!(atlas.sheet_count >= 2, "{}", atlas.sheet_count);
            assert!(atlas.rects.iter().any(|rect| rect.sheet == 0));
        }
    }
}

/// Точка листа 0 `(x, y)` в пределах `rect`: цвет, умноженный на прозрачность, и прозрачность.
fn built_in_pixel(sheet: &[u8], rect: AtlasRect, x: u32, y: u32) -> [u8; 4] {
    assert!(x < rect.w && y < rect.h);
    let start = (((rect.y + y) * ATLAS_SIZE + rect.x + x) * 4) as usize;
    sheet[start..start + 4].try_into().unwrap()
}

fn first_sheet() -> Vec<u8> {
    let images = many_images();
    let atlas = pack(&images).expect("умещаются");
    let mut sheet = vec![0u8; SHEET_BYTES];
    fill_sheet(&atlas, &images, 0, &mut sheet);
    sheet
}

#[test]
fn the_smoke_puff_is_grey_and_nearly_opaque_in_the_middle_and_clear_in_the_corners() {
    let sheet = first_sheet();
    let rect = SMOKE_RECT;
    let [r, g, b, a] = built_in_pixel(&sheet, rect, rect.w / 2, rect.h / 2);
    assert!(a >= 200, "середина заметно непрозрачная: {a}");
    let straight = |channel: u8| channel as f32 * 255.0 / a as f32;
    for channel in [r, g, b] {
        assert!(
            (100.0..=200.0).contains(&straight(channel)),
            "серый: {r} {g} {b} {a}"
        );
    }
    assert!(
        r.abs_diff(b) <= 10 && r.abs_diff(g) <= 6,
        "серый: {r} {g} {b}"
    );
    for (x, y) in [
        (0, 0),
        (rect.w - 1, 0),
        (0, rect.h - 1),
        (rect.w - 1, rect.h - 1),
    ] {
        assert_eq!(built_in_pixel(&sheet, rect, x, y), [0; 4], "угол {x} {y}");
    }
}

#[test]
fn the_spark_is_brighter_in_the_middle_than_at_the_edge() {
    let sheet = first_sheet();
    let rect = SPARK_RECT;
    let brightness = |x: u32, y: u32| {
        let [r, g, b, a] = built_in_pixel(&sheet, rect, x, y);
        (r as u32 + g as u32 + b as u32, a)
    };
    let (middle, alpha) = brightness(rect.w / 2, rect.h / 2);
    assert_eq!(alpha, 255, "горячее ядро непрозрачное");
    let (edge, _) = brightness(rect.w - 3, rect.h / 2);
    let (between, _) = brightness(rect.w * 3 / 4, rect.h / 2);
    assert!(
        middle > between && between > edge,
        "{middle} {between} {edge}"
    );
    assert_eq!(brightness(0, 0), (0, 0));
}

#[test]
fn every_leaf_frame_is_opaque_in_the_middle_and_clear_in_the_corners() {
    let sheet = first_sheet();
    let side = LEAF_RECT.h;
    let mut colors = Vec::new();
    for frame in 0..4 {
        let frame_rect = AtlasRect {
            x: LEAF_RECT.x + frame * side,
            w: side,
            ..LEAF_RECT
        };
        let middle = built_in_pixel(&sheet, frame_rect, side / 2, side / 2);
        assert!(middle[3] > 200, "кадр {frame}: {middle:?}");
        for (x, y) in [(0, 0), (side - 1, 0), (0, side - 1), (side - 1, side - 1)] {
            assert_eq!(
                built_in_pixel(&sheet, frame_rect, x, y)[3],
                0,
                "кадр {frame}, угол {x} {y}"
            );
        }
        colors.push(middle);
    }
    colors.dedup();
    assert_eq!(colors.len(), 4, "у каждого кадра свой цвет: {colors:?}");
}

// -------------------------------------------------------------------------------------------
// Код игры
// -------------------------------------------------------------------------------------------

const CODE_KINDS: &str = r#"{
    "дым":{"image":"puff","rate":6,"lifetime":4,"size":1},
    "огонь":{"image":"spark","rate":6,"lifetime":4,"size":1}}"#;
const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;

fn load_code(code: &str) -> Game {
    let (game, ..) =
        load_flat(PLAIN, CODE_RULES, Some(CODE_KINDS), Some(code)).expect("должно загрузиться");
    game
}

#[test]
fn code_reads_and_writes_the_kind_of_a_source_by_name() {
    let mut game =
        load_code("function tick(obj)\n  print(obj.particles)\n  obj.particles = \"огонь\"\nend");
    step(&mut game, 2);
    assert_eq!(
        game.messages(),
        ["print: nil", "print: огонь"],
        "{:?}",
        game.messages()
    );
    assert_eq!(game.world.text(0, property::PARTICLES), Some("огонь"));
    assert!(game.code_error().is_none());
}

#[test]
fn code_writing_an_unknown_kind_is_a_code_error() {
    let mut game = load_code("function tick(obj)\n  obj.particles = \"туман\"\nend");
    step(&mut game, 1);
    let error = game.code_error().expect("неизвестный вид — ошибка кода");
    assert!(
        error.message.contains("неизвестный вид частиц: туман"),
        "{error:?}"
    );
    assert_eq!(game.world.text(0, property::PARTICLES), None);
}

#[test]
fn code_may_write_a_kind_the_live_game_was_just_given() {
    let code = "function tick(obj)\n  obj.particles = \"туман\"\nend";
    let (mut game, config, _, images) =
        load_flat(PLAIN, CODE_RULES, Some(CODE_KINDS), Some(code)).unwrap();
    let table = json!({"туман":{"image":"puff","rate":1,"lifetime":1,"size":1}});
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    assert_eq!(
        session::set_wind_particles(Some(&mut live), &mut game, &images, None, Some(&table)),
        Ok(())
    );
    advance(&mut live, &mut game, &config, &mut state);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.text(0, property::PARTICLES), Some("туман"));
}

// -------------------------------------------------------------------------------------------
// `set_wind_particles`
// -------------------------------------------------------------------------------------------

fn live_game() -> (Game, ScreensConfig, Vec<ImageDecl>) {
    let scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"particles":"дым"}],"wind":[1.5,0]}"#;
    let (game, screens, _warnings, images) =
        load_flat(scene, NO_RULES, Some(SMOKE), None).expect("должно загрузиться");
    (game, screens, images)
}

fn advance(
    session: &mut PlaySession,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.step_once(&mut queue, &mut mouse, game, config, state, VIEWPORT, &[]);
}

fn fire() -> serde_json::Value {
    json!({
        "дым": {"image": "puff", "rate": 6, "lifetime": 5, "size": 1, "gravity": -1},
        "огонь": {"image": "spark", "rate": 20, "lifetime": 1, "size": 0.5}
    })
}

#[test]
fn only_the_kinds_only_the_wind_or_both_can_be_set() {
    let (mut game, _config, images) = live_game();
    assert_eq!(
        session::set_wind_particles(None, &mut game, &images, None, Some(&fire())),
        Ok(())
    );
    assert_eq!(game.wind(), [1.5, 0.0], "ветер не тронут");
    assert_eq!(game.particles().find("дым").unwrap().gravity, -1.0);
    assert!(game.particles().find("огонь").is_some());

    assert_eq!(
        session::set_wind_particles(None, &mut game, &images, Some(&json!([-2, 0.5])), None),
        Ok(())
    );
    assert_eq!(game.wind(), [-2.0, 0.5]);
    assert!(game.particles().find("огонь").is_some(), "виды не тронуты");

    let both = json!({"дым": {"image": "puff", "rate": 1, "lifetime": 1, "size": 1}});
    assert_eq!(
        session::set_wind_particles(None, &mut game, &images, Some(&json!([0, 0])), Some(&both)),
        Ok(())
    );
    assert_eq!(game.wind(), [0.0, 0.0]);
    assert!(game.particles().find("огонь").is_none());
    assert_eq!(game.particles().find("дым").unwrap().rate, 1.0);

    assert_eq!(
        session::set_wind_particles(None, &mut game, &images, None, None),
        Ok(())
    );
}

#[test]
fn the_editor_may_send_a_kind_without_an_image() {
    let (mut game, _config, images) = live_game();
    let dots = json!({"дым": {"rate": 3, "lifetime": 2, "size": 1}});
    assert_eq!(
        session::set_wind_particles(None, &mut game, &images, None, Some(&dots)),
        Ok(())
    );
    assert_eq!(
        game.particles().find("дым").unwrap().look,
        ParticleLook::Shape(ParticleShape::Dot)
    );
}

#[test]
fn a_table_without_the_kind_an_object_names_is_refused_and_nothing_changes() {
    let (mut game, _config, images) = live_game();
    let without_smoke = json!({"огонь": {"image": "spark", "rate": 1, "lifetime": 1, "size": 1}});
    let error = session::set_wind_particles(
        None,
        &mut game,
        &images,
        Some(&json!([9, 9])),
        Some(&without_smoke),
    )
    .expect_err("объект называет дым");
    assert!(error.contains("дым"), "{error}");
    assert_eq!(game.wind(), [1.5, 0.0], "ветер тоже не поменялся");
    assert!(game.particles().find("дым").is_some());
    assert!(game.particles().find("огонь").is_none());
}

#[test]
fn a_broken_table_or_wind_is_refused_and_nothing_changes() {
    let (mut game, _config, images) = live_game();
    let broken = json!({"дым": {"image": "puff", "rate": 0, "lifetime": 1, "size": 1}});
    let error = session::set_wind_particles(
        None,
        &mut game,
        &images,
        Some(&json!([9, 9])),
        Some(&broken),
    )
    .expect_err("rate 0");
    assert!(error.contains("дым → rate"), "{error}");
    assert_eq!(game.wind(), [1.5, 0.0]);
    assert_eq!(game.particles().find("дым").unwrap().rate, 6.0);

    let error = session::set_wind_particles(
        None,
        &mut game,
        &images,
        Some(&json!("east")),
        Some(&fire()),
    )
    .expect_err("ветер не пара");
    assert!(error.contains("wind"), "{error}");
    assert!(
        game.particles().find("огонь").is_none(),
        "виды тоже не поставлены"
    );

    assert!(
        session::set_wind_particles(None, &mut game, &images, None, Some(&json!([1]))).is_err()
    );
}

#[test]
fn in_a_party_the_wind_is_recorded_and_the_kinds_are_not_and_a_stop_gives_the_file_back() {
    let (mut game, config, images) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    advance(&mut live, &mut game, &config, &mut state);
    assert_eq!(
        session::set_wind_particles(
            Some(&mut live),
            &mut game,
            &images,
            Some(&json!([3, 0])),
            Some(&fire())
        ),
        Ok(())
    );
    assert_eq!(game.wind(), [3.0, 0.0]);
    assert!(game.particles().find("огонь").is_some());
    let text = live.recording_text(&game);
    assert!(text.contains(r#""wind":[3.0,0.0]"#), "{text}");
    assert!(
        !text.contains("огонь") && !text.contains("particles"),
        "{text}"
    );

    live.end(&mut game);
    game.show_scene();
    assert!(
        game.particles().find("огонь").is_none(),
        "«Стоп» возвращает виды файла"
    );
    assert_eq!(game.particles().find("дым").unwrap().rate, 6.0);
}

#[test]
fn a_new_game_inside_a_party_keeps_the_kinds_the_party_was_given() {
    let (mut game, config, images) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    advance(&mut live, &mut game, &config, &mut state);
    assert_eq!(
        session::set_wind_particles(Some(&mut live), &mut game, &images, None, Some(&fire())),
        Ok(())
    );
    apply_command(
        ButtonCommand::NewGame(config.start_screen, None),
        &mut game,
        &config,
        &mut state,
    );
    assert!(game.particles().find("огонь").is_some());
    assert_eq!(game.particles().find("дым").unwrap().lifetime.to, 5.0);

    live.end(&mut game);
    game.show_scene();
    assert!(game.particles().find("огонь").is_none());
}

#[test]
fn in_a_replay_the_world_is_not_edited_and_in_a_three_dimensional_scene_there_are_no_particles() {
    let (mut game, config, images) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    advance(&mut live, &mut game, &config, &mut state);
    let text = live.recording_text(&game);
    let (mut replayed, config2, images2) = live_game();
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut replayed, &config2, &mut state2).unwrap();
    assert_eq!(
        session::set_wind_particles(
            Some(&mut replay),
            &mut replayed,
            &images2,
            None,
            Some(&fire())
        ),
        Err("в повторе мир не правится".to_string())
    );
    assert!(replayed.particles().find("огонь").is_none());

    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let (mut flat3d, ..) = load_with(&game_json(CAMERA_3D, ""), scene, NO_RULES, None, None)
        .expect("должно загрузиться");
    assert_eq!(
        session::set_wind_particles(None, &mut flat3d, &images, None, Some(&fire())),
        Err("частицы есть только в плоской сцене".to_string())
    );
    assert_eq!(
        session::set_wind_particles(None, &mut flat3d, &images, Some(&json!([1, 0])), None),
        Err("ветер есть только в плоской сцене".to_string())
    );
}

#[test]
fn an_edit_of_the_kinds_changes_the_particles_in_flight() {
    let mut rig = Rig::new(CHIMNEY, NO_RULES, SMOKE);
    rig.settle();
    let before = rig.paints();
    let heavy = json!({"дым": {"image": "puff", "rate": 6, "lifetime": [4, 6], "size": [0.6, 0.9],
        "grow": 1, "opacity": 0.5}});
    session::set_wind_particles(None, &mut rig.game, &rig.images, None, Some(&heavy))
        .expect("таблица верна");
    rig.settle();
    let after = rig.paints();
    assert_eq!(before.len(), after.len(), "вылетевшие не прерваны");
    assert!(after.iter().all(|paint| paint.color[3] == 0.5), "{after:?}");
    assert!(
        after.iter().all(|paint| paint.size[0] <= 0.9 + 1e-6),
        "рост 1 — частицы не больше своего размера"
    );
}

#[test]
fn a_party_edit_may_add_a_source_and_may_name_only_a_known_kind() {
    let (mut game, config, images) = live_game();
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    let added = live
        .add_object(
            &mut game,
            &images,
            &json!({"position": [5, 5], "size": [1, 1], "particles": "дым"}),
        )
        .expect("объект с известным видом");
    assert_eq!(game.world.text(added, property::PARTICLES), Some("дым"));
    let error = live
        .set_property(&mut game, &images, added, "particles", &json!("туман"))
        .expect_err("такого вида нет");
    assert!(error.contains("неизвестный вид частиц: туман"), "{error}");
    assert_eq!(game.world.text(added, property::PARTICLES), Some("дым"));
}

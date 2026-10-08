//! Фаза 32 — частицы по физике: девять свойств объекта вместо `particles` и `particles.json`, их
//! проверка перед запуском, дым, искры и листопад в списке рисования, листопад по непрозрачным
//! точкам картинки, вылет и прогрев, часы движения, свойства частиц в коде и правилах, `set_property`
//! и `remove_property` вне партии. Формулы полёта — в `render::particles`'s own `mod tests`; сама
//! видеокарта и браузер — только QA на стенде.

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::particles::ParticleShape;
use engine::core::property;
use engine::core::scene::LayerView;
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, load_rest_with_stamps, read_entry};
use engine::data::session::{self, PlaySession};
use engine::render::atlas::{
    ATLAS_SIZE, AtlasImage, AtlasRect, LEAF_RECT, RectPaint, SHEET_BYTES, SMOKE_RECT, SPARK_RECT,
    WHITE_LEAF_RECT, compose_world_paints, fill_sheet, opaque_masks, pack, particle_emitters,
    sway_objects,
};
use engine::render::wind::Motion;
use serde_json::json;

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"mark":"flag","hits":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;

const FILES: &str = r#","images":{
    "crown":{"path":"crown.png"},
    "small_crown":{"path":"crown.png","size":[4,2],"anchor":"bottom"},
    "plain":{"path":"plain.png"}}"#;

const CHIMNEY: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"smoke":0.5}]}"#;
const PLAIN: &str = r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true}]}"#;
const TREE: &str = r#"{"objects":[{"position":[10,5],"size":[16,8],"image":"crown"}]}"#;

fn game_json(scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

/// Левая половина картинки `crown` прозрачна, правая непрозрачна.
fn verdicts() -> Vec<(&'static str, ImageVerdict)> {
    let mut crown = vec![0u8; 8 * 4 * 4];
    for y in 0..4 {
        for x in 4..8 {
            crown[(y * 8 + x) * 4 + 3] = 255;
        }
    }
    let crown_verdict = ImageVerdict::Ok {
        width: 8,
        height: 4,
        pixels: crown,
    };
    vec![
        ("crown", crown_verdict.clone()),
        ("small_crown", crown_verdict),
        (
            "plain",
            ImageVerdict::Ok {
                width: 8,
                height: 8,
                pixels: vec![255u8; 8 * 8 * 4],
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
    load_rest_with_stamps(
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
    )
}

fn load_flat(scene: &str, rules: &str, code: Option<&str>) -> Result<Loaded, LoadFailure> {
    let files = if code.is_some() {
        format!(r#"{FILES},"code":"code.lua""#)
    } else {
        FILES.to_string()
    };
    load_with(&game_json("", &files), scene, rules, code)
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
// Старые вид, свойство и файл
// -------------------------------------------------------------------------------------------

#[test]
fn files_particles_is_an_unknown_key_and_a_game_without_it_loads() {
    let with_file = game_json("", r#","particles":"particles.json""#);
    let LoadFailure { errors, .. } = read_entry(&with_file).expect_err("ключа больше нет");
    assert_error(
        &errors,
        "game.json",
        "files → particles",
        "неизвестное поле",
    );
    assert!(read_entry(&game_json("", FILES)).is_ok());
}

#[test]
fn the_old_particles_property_is_an_unknown_property_everywhere() {
    let in_scene = r#"{"objects":[{"position":[1,1],"size":[1,1],"particles":"дым"}]}"#;
    let errors = errors_of(load_flat(in_scene, NO_RULES, None));
    assert_error(&errors, "scene.json", "particles", "неизвестное свойство");

    let rule =
        r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","particles","дым"]]}]}"#;
    let errors = errors_of(load_flat(PLAIN, rule, None));
    assert_error(&errors, "rules.json", "", "неизвестное свойство");
}

// -------------------------------------------------------------------------------------------
// Девять свойств: загрузка и проверка перед запуском
// -------------------------------------------------------------------------------------------

#[test]
fn the_nine_properties_load_as_numbers_and_colours_without_a_warning() {
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"smoke":0.5,"smoke_height":6,
        "smoke_color":"#bfbfbf","sparks":1,"sparks_reach":2.5,"sparks_direction":450,
        "sparks_spread":45,"leaf_fall":0.3,"leaf_color":"#ff8800"}]}"##;
    let (game, _screens, warnings, _images) =
        load_flat(scene, NO_RULES, None).expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("частиц")),
        "{warnings:?}"
    );
    let number = |prop| game.world.number_like(0, prop);
    assert_eq!(number(property::SMOKE), Some(0.5));
    assert_eq!(number(property::SMOKE_HEIGHT), Some(6.0));
    assert_eq!(number(property::SPARKS), Some(1.0));
    assert_eq!(number(property::SPARKS_REACH), Some(2.5));
    assert_eq!(number(property::SPARKS_DIRECTION), Some(450.0));
    assert_eq!(number(property::SPARKS_SPREAD), Some(45.0));
    assert_eq!(number(property::LEAF_FALL), Some(0.3));
    let rgb = |prop| game.world.color(0, prop).map(|[r, g, b, _]| [r, g, b]);
    assert_eq!(rgb(property::SMOKE_COLOR), Some([191.0 / 255.0; 3]));
    assert!(rgb(property::LEAF_COLOR).is_some());
}

#[test]
fn the_edges_of_every_range_are_allowed() {
    for object in [
        r#""smoke":0"#,
        r#""smoke":1"#,
        r#""sparks":0,"sparks_spread":0"#,
        r#""sparks":1,"sparks_spread":180"#,
        r#""leaf_fall":1,"sparks_direction":-90"#,
        r#""smoke_height":0.01,"sparks_reach":100"#,
    ] {
        let scene = format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],{object}}}]}}"#);
        load_flat(&scene, NO_RULES, None).unwrap_or_else(|e| panic!("{object}: {e:?}"));
    }
}

#[test]
fn every_check_before_the_start_names_the_property_and_the_place() {
    let cases = [
        (r#""smoke":1.5"#, "smoke", "smoke: нужно от 0 до 1"),
        (r#""smoke":-0.1"#, "smoke", "smoke: нужно от 0 до 1"),
        (r#""sparks":2"#, "sparks", "sparks: нужно от 0 до 1"),
        (
            r#""leaf_fall":-1"#,
            "leaf_fall",
            "leaf_fall: нужно от 0 до 1",
        ),
        (
            r#""smoke_height":0"#,
            "smoke_height",
            "smoke_height: нужно больше нуля",
        ),
        (r#""smoke_height":-2"#, "smoke_height", "больше нуля"),
        (
            r#""sparks_reach":0"#,
            "sparks_reach",
            "sparks_reach: нужно больше нуля",
        ),
        (r#""sparks_spread":181"#, "sparks_spread", "от 0 до 180"),
        (r#""sparks_spread":-1"#, "sparks_spread", "от 0 до 180"),
        (
            r#""smoke_color":"grey""#,
            "smoke_color",
            "цвет должен быть вида",
        ),
        (
            r##""leaf_color":"#12""##,
            "leaf_color",
            "цвет должен быть вида",
        ),
        (r#""leaf_color":5"#, "leaf_color", "ожидалась строка"),
        (r#""smoke":"много""#, "smoke", "ожидалось число"),
    ];
    for (field, path, message) in cases {
        let scene = format!(r#"{{"objects":[{{"position":[1,1],"size":[1,1],{field}}}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(&errors, "scene.json", path, message);
    }
}

#[test]
fn the_properties_need_a_position_and_a_size_and_do_not_repeat() {
    for object in [
        r#"{"size":[1,1],"smoke":0.5}"#,
        r#"{"position":[1,1],"sparks":0.5}"#,
        r#"{"position":[1,1],"leaf_fall":0.5}"#,
        r#"{"size":[1,1],"sparks_reach":3}"#,
        r##"{"size":[1,1],"leaf_color":"#ff0000"}"##,
    ] {
        let scene = format!(r#"{{"objects":[{object}]}}"#);
        let errors = errors_of(load_flat(&scene, NO_RULES, None));
        assert_error(
            &errors,
            "scene.json",
            "",
            "свойства частиц разрешены только объекту с position и size",
        );
    }
    let repeated = r##"{"objects":[{"position":[1,1],"size":[1,1],"smoke":0.5,"repeat_x":true,
        "color":"#ffffff"}]}"##;
    let errors = errors_of(load_flat(repeated, NO_RULES, None));
    assert_error(&errors, "scene.json", "", "вместе с repeat_x");

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"smoke":0.5}}]}"#;
    let errors = errors_of(load_flat(PLAIN, template, None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "свойства частиц разрешены только объекту с position и size",
    );
}

#[test]
fn a_rule_may_light_the_smoke_and_a_constant_outside_the_range_is_an_error() {
    let rule = |value: &str| {
        format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["mark"]}},"do":[["set","smoke",{value}]]}}]}}"#
        )
    };
    load_flat(PLAIN, &rule("0.8"), None).expect("0,8 годится");
    let errors = errors_of(load_flat(PLAIN, &rule("1.5"), None));
    assert_error(&errors, "rules.json", "", "smoke: нужно от 0 до 1");

    let template = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["hits"]}}},
        "where":{"at":[3,3]},"template":{"hits":0,"size":[1,1],"smoke":3}}]}"#;
    let errors = errors_of(load_flat(PLAIN, template, None));
    assert_error(&errors, "rules.json", "", "smoke: нужно от 0 до 1");
}

#[test]
fn the_properties_in_a_three_dimensional_scene_are_errors_everywhere() {
    let flat3d = game_json(CAMERA_3D, "");
    let plain3d = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    for field in [
        r#""smoke":0.5"#,
        r#""smoke_height":3"#,
        r##""smoke_color":"#ffffff""##,
        r#""sparks":0.5"#,
        r#""sparks_reach":2"#,
        r#""sparks_direction":90"#,
        r#""sparks_spread":10"#,
        r#""leaf_fall":0.5"#,
        r##""leaf_color":"#ffffff""##,
    ] {
        let scene = format!(
            r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff",{field}}}]}}"##
        );
        let errors = errors_of(load_with(&flat3d, &scene, NO_RULES, None));
        assert_error(&errors, "scene.json", "", "только в плоской сцене");
    }
    let rule = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","smoke",0.5]]}]}"#;
    let errors = errors_of(load_with(&flat3d, plain3d, rule, None));
    assert_error(
        &errors,
        "rules.json",
        "",
        "smoke есть только в плоской сцене",
    );
}

// -------------------------------------------------------------------------------------------
// Код игры
// -------------------------------------------------------------------------------------------

const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;

fn load_code(code: &str) -> Game {
    let (game, ..) = load_flat(PLAIN, CODE_RULES, Some(code)).expect("должно загрузиться");
    game
}

#[test]
fn code_reads_and_writes_the_properties_as_numbers_and_colours() {
    let mut game = load_code(
        "function tick(obj)\n  print(obj.smoke)\n  obj.smoke = 0.8\n  obj.smoke_color = \"#ff0000\"\n  print(obj.smoke)\n  print(obj.smoke_color)\nend",
    );
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.messages(),
        ["print: nil", "print: 0.8", "print: #ff0000"]
    );
    assert_eq!(game.world.number_like(0, property::SMOKE), Some(0.8));
}

#[test]
fn code_is_not_told_about_an_unknown_kind_any_more() {
    let mut game = load_code("function tick(obj)\n  obj.particles = \"дым\"\nend");
    step(&mut game, 1);
    let error = game.code_error().expect("свойства particles нет");
    assert!(error.message.contains("particles"), "{error:?}");
}

#[test]
fn code_may_not_light_the_smoke_in_a_three_dimensional_scene() {
    let rules = CODE_RULES;
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"collides":true,"mark":true}]}"##;
    let files = r#","code":"code.lua""#;
    let (mut game, ..) = load_with(
        &game_json(CAMERA_3D, files),
        scene,
        rules,
        Some("function tick(obj)\n  obj.smoke = 0.5\nend"),
    )
    .expect("должно загрузиться");
    step(&mut game, 1);
    let error = game.code_error().expect("ошибка кода");
    assert!(
        error.message.contains("только в плоской сцене"),
        "{error:?}"
    );
}

// -------------------------------------------------------------------------------------------
// `set_property` и `remove_property` вне партии
// -------------------------------------------------------------------------------------------

fn loaded(scene: &str) -> (Game, ScreensConfig, Vec<ImageDecl>) {
    let (game, screens, _warnings, images) =
        load_flat(scene, NO_RULES, None).expect("должно загрузиться");
    (game, screens, images)
}

#[test]
fn outside_a_party_set_property_changes_the_built_world_and_writes_no_recording() {
    let (mut game, config, images) = loaded(CHIMNEY);
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "smoke", &json!(0.9)),
        Ok(())
    );
    assert_eq!(game.world.number_like(0, property::SMOKE), Some(0.9));
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "leaf_fall", &json!(0.3)),
        Ok(()),
        "свойство, которого у объекта не было, дописывается"
    );
    assert_eq!(game.world.number_like(0, property::LEAF_FALL), Some(0.3));

    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    live.end(&mut game);
    assert_eq!(
        session::set_property(Some(&mut live), &mut game, &images, 0, "smoke", &json!(0.2)),
        Ok(())
    );
    assert_eq!(
        session::remove_property(Some(&mut live), &mut game, 0, "smoke"),
        Ok(())
    );
    let text = live.recording_text(&game);
    assert!(
        !text.contains("smoke"),
        "после «Стопа» правка не пишется: {text}"
    );
}

#[test]
fn a_failed_check_outside_a_party_changes_nothing_and_says_why() {
    let (mut game, _config, images) = loaded(CHIMNEY);
    for (name, value, part) in [
        ("smoke", json!(2), "от 0 до 1"),
        ("smoke_height", json!(0), "больше нуля"),
        ("sparks_spread", json!(200), "от 0 до 180"),
        ("smoke_color", json!("red"), "цвет"),
        ("particles", json!("дым"), "Новое свойство"),
    ] {
        let error =
            session::set_property(None, &mut game, &images, 0, name, &value).expect_err(name);
        assert!(error.contains(part), "{name}: {error}");
    }
    assert_eq!(game.world.number_like(0, property::SMOKE), Some(0.5));
    assert_eq!(game.world.number_like(0, property::SMOKE_HEIGHT), None);
    assert!(session::set_property(None, &mut game, &images, 7, "smoke", &json!(0.5)).is_err());
}

fn nine_valid() -> [(&'static str, serde_json::Value); 9] {
    [
        ("smoke", json!(0.5)),
        ("smoke_height", json!(2)),
        ("smoke_color", json!("#ff0000")),
        ("sparks", json!(0.5)),
        ("sparks_reach", json!(2)),
        ("sparks_direction", json!(10)),
        ("sparks_spread", json!(20)),
        ("leaf_fall", json!(0.5)),
        ("leaf_color", json!("#00ff00")),
    ]
}

/// Текст первой ошибки загрузки сцены с таким объектом.
fn load_message(object: &str) -> String {
    let scene = format!(r#"{{"objects":[{object}]}}"#);
    errors_of(load_flat(&scene, NO_RULES, None))
        .into_iter()
        .next()
        .expect("ошибка загрузки")
        .message
}

#[test]
fn any_particle_property_set_outside_a_party_on_an_object_without_position_or_size_is_refused() {
    for (object, lacks) in [
        (r#"{"position":[1,1],"mark":true}"#, "size"),
        (r#"{"size":[1,1],"mark":true}"#, "position"),
    ] {
        let (mut game, _config, images) = loaded(&format!(r#"{{"objects":[{object}]}}"#));
        for (name, value) in nine_valid() {
            let expected = load_message(&object.replace("}", &format!(r#","{name}":{value}}}"#)));
            let error = session::set_property(None, &mut game, &images, 0, name, &value)
                .expect_err(&format!("{name} без {lacks}"));
            assert_eq!(error, expected, "{name} без {lacks}");
            let prop = game.properties.resolve(name).expect("встроенное");
            assert!(!game.world.has(0, prop), "{name}: мир не меняется");
        }
    }
}

#[test]
fn any_particle_property_set_outside_a_party_on_a_repeating_object_is_refused() {
    let object = r##"{"position":[1,1],"size":[1,1],"repeat_x":true,"color":"#ffffff"}"##;
    let (mut game, _config, images) = loaded(&format!(r#"{{"objects":[{object}]}}"#));
    for (name, value) in nine_valid() {
        let expected = load_message(&object.replace("}", &format!(r#","{name}":{value}}}"#)));
        let error = session::set_property(None, &mut game, &images, 0, name, &value)
            .expect_err(&format!("{name} при repeat_x"));
        assert_eq!(error, expected, "{name}");
        let prop = game.properties.resolve(name).expect("встроенное");
        assert!(!game.world.has(0, prop), "{name}: мир не меняется");
    }
}

#[test]
fn repeat_x_set_outside_a_party_on_an_object_with_particles_is_refused_and_false_is_not() {
    let (mut game, _config, images) = loaded(CHIMNEY);
    let expected = load_message(r#"{"position":[10,10],"size":[2,1],"smoke":0.5,"repeat_x":true}"#);
    let error = session::set_property(None, &mut game, &images, 0, "repeat_x", &json!(true))
        .expect_err("repeat_x");
    assert_eq!(error, expected);
    assert!(!game.world.flag(0, property::REPEAT_X), "мир не меняется");
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "repeat_x", &json!(false)),
        Ok(())
    );
}

#[test]
fn position_or_size_removed_outside_a_party_from_an_object_with_particles_is_refused() {
    for (name, left, prop) in [
        ("position", r#""size":[2,1]"#, property::POSITION),
        ("size", r#""position":[10,10]"#, property::SIZE),
    ] {
        let (mut game, _config, _images) = loaded(CHIMNEY);
        let expected = load_message(&format!(r#"{{{left},"smoke":0.5}}"#));
        let error = session::remove_property(None, &mut game, 0, name).expect_err(name);
        assert_eq!(error, expected, "{name}");
        assert!(game.world.has(0, prop), "{name}: мир не меняется");
    }
    let (mut game, _config, _images) = loaded(PLAIN);
    assert_eq!(
        session::remove_property(None, &mut game, 0, "size"),
        Ok(()),
        "без частиц размер снимается"
    );
}

#[test]
fn remove_property_outside_a_party_takes_the_effect_off() {
    let (mut game, _config, images) = loaded(CHIMNEY);
    assert_eq!(
        session::remove_property(None, &mut game, 0, "smoke"),
        Ok(())
    );
    assert_eq!(game.world.number_like(0, property::SMOKE), None);
    assert!(session::remove_property(None, &mut game, 0, "туман").is_err());
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "smoke", &json!(0.4)),
        Ok(())
    );
}

#[test]
fn in_a_party_the_property_edit_is_recorded_and_in_a_replay_the_world_is_not_edited() {
    let (mut game, config, images) = loaded(CHIMNEY);
    let mut state = ScreenState::new(config.start_screen);
    let mut live = PlaySession::begin_live(&mut game, &config, &mut state);
    assert_eq!(
        session::set_property(Some(&mut live), &mut game, &images, 0, "smoke", &json!(0.8)),
        Ok(())
    );
    assert_eq!(
        session::set_property(
            Some(&mut live),
            &mut game,
            &images,
            0,
            "sparks",
            &json!(0.3)
        ),
        Ok(())
    );
    assert_eq!(
        session::remove_property(Some(&mut live), &mut game, 0, "sparks"),
        Ok(())
    );
    let text = live.recording_text(&game);
    assert!(text.contains("smoke") && text.contains("sparks"), "{text}");

    let (mut replayed, config2, images2) = loaded(CHIMNEY);
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut replayed, &config2, &mut state2).unwrap();
    let refusal = Err("в повторе мир не правится".to_string());
    assert_eq!(
        session::set_property(
            Some(&mut replay),
            &mut replayed,
            &images2,
            0,
            "smoke",
            &json!(0.1)
        ),
        refusal
    );
    assert_eq!(
        session::remove_property(Some(&mut replay), &mut replayed, 0, "smoke"),
        refusal
    );
    assert_eq!(replayed.world.number_like(0, property::SMOKE), Some(0.5));
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
    fn new(scene: &str) -> Rig {
        let (game, _screens, _warnings, images) =
            load_flat(scene, NO_RULES, None).expect("должно загрузиться");
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
        let mut motion = Motion::default();
        motion.set_opaque_masks(opaque_masks(&atlas_images, &images));
        Rig {
            game,
            images,
            rects,
            motion,
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
            particle_emitters(&self.game.world, &self.images),
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

    /// Живые частицы, что вылетели из объекта `id` сейчас.
    fn by_object(&self, id: u32) -> usize {
        self.motion
            .particles()
            .live_sprites(id, self.game.world.generation(id))
            .count()
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

#[test]
fn an_object_from_the_scene_is_warmed_up_on_the_first_assembly_and_its_smoke_reaches_the_draw_list()
{
    let mut rig = Rig::new(CHIMNEY);
    rig.settle();
    let live = rig.live();
    assert!((25..=70).contains(&live), "{live}");
    assert_eq!(rig.particle_paints().len(), live);
}

#[test]
fn an_object_that_gets_the_smoke_later_starts_from_nothing_and_a_zero_starts_it_again() {
    let mut rig = Rig::new(PLAIN);
    rig.settle();
    rig.game.world.set_number(0, property::SMOKE, 0.8);
    rig.settle();
    assert_eq!(rig.live(), 0, "получил дым — начинает с нуля");
    rig.frames(20, 0.1);
    let grown = rig.live();
    assert!(grown > 20, "{grown}");

    rig.game.world.set_number(0, property::SMOKE, 0.0);
    rig.frames(1, 0.1);
    assert_eq!(rig.by_object(0), 0, "ушли в доживающие");
    rig.game.world.set_number(0, property::SMOKE, 0.8);
    rig.settle();
    assert_eq!(rig.by_object(0), 0, "плотность вернули — как новый");
}

#[test]
fn the_smoke_written_by_a_rule_starts_from_nothing_while_the_scene_object_is_warm() {
    let scene = r#"{"objects":[
        {"position":[10,10],"size":[2,1],"smoke":0.5},
        {"position":[20,10],"size":[2,1],"smoke":0,"mark":true}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["set","smoke",0.8]]}]}"#;
    let (game, _screens, _warnings, images) =
        load_flat(scene, rules, None).expect("должно загрузиться");
    let mut rig = Rig::from_loaded(game, images);
    rig.settle();
    let warm = rig.live();
    assert!(warm >= 25, "{warm}");
    let chimney = rig.by_object(0);
    rig.game.step(StepInput::empty());
    rig.settle();
    assert_eq!(rig.game.world.number_like(1, property::SMOKE), Some(0.8));
    assert_eq!(rig.by_object(1), 0, "изба, получившая дым, начала с нуля");
    assert_eq!(
        rig.by_object(0),
        chimney,
        "правка плотности у идущего дыма его не перезапускает"
    );
}

#[test]
fn a_new_object_after_a_reload_starts_from_nothing() {
    let mut rig = Rig::new(CHIMNEY);
    rig.settle();
    let warm = rig.live();
    rig.game.show_scene();
    rig.motion.world_rebuilt();
    let dropped = rig.game.world.create();
    rig.game
        .world
        .set_vec2(dropped, property::POSITION, [20.0, 10.0]);
    rig.game.world.set_vec2(dropped, property::SIZE, [1.0, 1.0]);
    rig.game.world.set_number(dropped, property::SMOKE, 0.5);
    rig.settle();
    assert_eq!(rig.live(), warm, "прежний продолжает, новый — ноль");
}

#[test]
fn the_stop_and_the_party_start_warm_the_sources_up_again() {
    let mut rig = Rig::new(CHIMNEY);
    rig.settle();
    rig.frames(20, 0.1);
    rig.motion.reset();
    rig.settle();
    let live = rig.live();
    assert!((25..=70).contains(&live), "{live}");
}

#[test]
fn a_paused_world_freezes_the_particles_and_the_editor_clock_moves_them() {
    let mut rig = Rig::new(CHIMNEY);
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

    let mut editor = Rig::new(CHIMNEY);
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
        let mut rig = Rig::new(CHIMNEY);
        rig.motion.tick(Some(100), 0.0);
        rig.settle();
        rig.motion.tick(Some(130), 0.0);
        rig.settle();
        let before = rig.paints();
        rig.motion.tick(Some(jump_to), 0.0);
        rig.settle();
        let live = rig.live();
        assert!((25..=70).contains(&live), "прыжок на {jump_to}: {live}");
        assert_ne!(rig.paints(), before, "прыжок {jump_to}: заново");
    }
}

#[test]
fn an_object_that_lost_the_property_or_the_world_stops_emitting_and_its_particles_live_out() {
    let mut rig = Rig::new(CHIMNEY);
    rig.settle();
    let warm = rig.live();
    rig.game.world.clear_property(0, property::SMOKE);
    rig.frames(10, 0.1);
    assert!(rig.live() <= warm && rig.live() > 0);
    rig.frames(100, 0.1);
    assert_eq!(rig.live(), 0, "дожили и исчезли");

    let mut rig = Rig::new(CHIMNEY);
    rig.settle();
    let warm = rig.live();
    rig.game.world.delete(0);
    rig.frames(5, 0.1);
    assert!(rig.live() > 0 && rig.live() <= warm, "удалённый — то же");
}

#[test]
fn a_settings_edit_without_the_main_property_does_nothing() {
    let mut rig = Rig::new(PLAIN);
    rig.settle();
    rig.game.world.set_number(0, property::SMOKE_HEIGHT, 7.0);
    rig.game.world.set_number(0, property::SPARKS_REACH, 3.0);
    rig.frames(50, 0.1);
    assert_eq!(rig.live(), 0);
}

#[test]
fn the_particles_do_not_touch_the_course_of_the_party() {
    let code = r#"function tick(obj) print(math.random(1000)) end"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["mark"]},"do":[["run","tick"]]}]}"#;
    let with_particles = r#"{"objects":[{"position":[10,10],"size":[2,1],"mark":true,"smoke":0.5,"leaf_fall":0.5}]}"#;
    let run = |scene: &str| {
        let (mut game, _, _, images) =
            load_flat(scene, rules, Some(code)).expect("должно загрузиться");
        let mut motion = Motion::default();
        let mut snapshots = Vec::new();
        for index in 0..40_u64 {
            game.step(StepInput::empty());
            motion.tick(Some(game.step_count()), 0.0);
            motion.update(game.wind()[0], sway_objects(&game.world, &images));
            motion.update_particles(
                game.has_world(),
                game.wind(),
                particle_emitters(&game.world, &images),
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
    {"position":[10,10],"size":[2,1],"smoke":0.5},
    {"position":[4,2],"size":[1,1],"color":"#ffffff"},
    {"position":[6,2],"size":[1,1],"color":"#ffffff","layer":1}]}"##;

fn order_of(paints: &[RectPaint]) -> Vec<Option<u32>> {
    paints.iter().map(|paint| paint.object).collect()
}

#[test]
fn the_particles_of_a_live_object_follow_it_in_the_draw_order() {
    let mut rig = Rig::new(SCENE_ORDER);
    rig.settle();
    let count = rig.live();
    assert!(count > 0);
    let mut expected = vec![Some(0)];
    expected.extend(std::iter::repeat_n(None, count));
    expected.extend([Some(2), Some(3)]);
    assert_eq!(
        order_of(&rig.paints()),
        expected,
        "объект без картинки и цвета виден только своими частицами"
    );
}

#[test]
fn the_particles_of_a_vanished_effect_come_after_every_object_of_their_layer() {
    let mut rig = Rig::new(SCENE_ORDER);
    rig.settle();
    let count = rig.live();
    rig.game.world.clear_property(1, property::SMOKE);
    rig.settle();
    let mut expected = vec![Some(0), Some(2)];
    expected.extend(std::iter::repeat_n(None, count));
    expected.push(Some(3));
    assert_eq!(order_of(&rig.paints()), expected);
}

#[test]
fn an_object_that_lost_its_size_stops_emitting_but_its_particles_stay_drawn() {
    let mut rig = Rig::new(SCENE_ORDER);
    rig.settle();
    let count = rig.live();
    rig.game.world.clear_property(1, property::SIZE);
    rig.settle();
    assert!(rig.live() > 0 && rig.live() <= count);
    let mut expected = vec![Some(0), Some(2)];
    expected.extend(std::iter::repeat_n(None, rig.live()));
    expected.push(Some(3));
    assert_eq!(order_of(&rig.paints()), expected);
}

#[test]
fn a_particle_is_shifted_by_the_parallax_of_its_object_like_the_object_itself() {
    let scene = r##"{"objects":[{"position":[10,10],"size":[2,1],"smoke":0.5,"parallax":0.5,
        "color":"#ffffff"}]}"##;
    let mut rig = Rig::new(scene);
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
fn the_leaves_of_a_layer_object_are_half_as_wide_at_a_parallax_of_one_half() {
    let widths = |parallax: &str| {
        let scene =
            format!(r#"{{"objects":[{{"position":[10,2],"size":[6,12],"parallax":{parallax}}}]}}"#);
        let mut rig = Rig::new(&scene);
        rig.settle();
        rig.game.world.set_number(0, property::LEAF_FALL, 1.0);
        rig.frames(20, 0.1);
        let mut widths: Vec<f32> = rig.particle_paints().iter().map(|p| p.size[0]).collect();
        widths.sort_by(f32::total_cmp);
        widths
    };
    let (plain, far) = (widths("1"), widths("0.5"));
    assert!(plain.len() > 10, "{}", plain.len());
    assert_eq!(plain.len(), far.len());
    for (a, b) in plain.iter().zip(&far) {
        assert!((a * 0.5 - b).abs() < 1e-5, "{a} против {b}");
    }
}

#[test]
fn the_smoke_is_white_tinted_by_its_colour_and_the_default_colour_is_grey() {
    let scene = r##"{"objects":[
        {"position":[10,10],"size":[2,1],"smoke":0.5},
        {"position":[20,10],"size":[2,1],"smoke":0.5,"smoke_color":"#ff0000"}]}"##;
    let mut rig = Rig::new(scene);
    rig.settle();
    let paints = rig.particle_paints();
    let (grey, red): (Vec<&RectPaint>, Vec<&RectPaint>) =
        paints.iter().partition(|paint| paint.position[0] < 15.0);
    assert!(!grey.is_empty() && !red.is_empty());
    for paint in &grey {
        let [r, g, b, _] = paint.color;
        assert!((r - g).abs() < 0.01 && (b - r).abs() < 0.05 && r < 0.8 && r > 0.4);
    }
    assert!(red.iter().all(|paint| paint.color[..3] == [1.0, 0.0, 0.0]));
    assert!(
        paints
            .iter()
            .all(|paint| paint.atlas_rect == SMOKE_RECT && paint.smooth && !paint.glow)
    );

    rig.game
        .world
        .set_color(1, property::SMOKE_COLOR, [0.0, 0.0, 1.0, 1.0]);
    rig.settle();
    let recoloured = rig.particle_paints();
    assert!(
        recoloured
            .iter()
            .filter(|paint| paint.position[0] > 15.0)
            .all(|paint| paint.color[..3] == [0.0, 0.0, 1.0]),
        "правка цвета перекрашивает и вылетевшие клубы"
    );
}

#[test]
fn the_sparks_glow_fly_in_an_arc_and_cool() {
    let scene = r#"{"objects":[{"position":[10,10],"size":[1,1],"sparks":1,"sparks_direction":90,"sparks_spread":10}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    let paints = rig.particle_paints();
    assert!(!paints.is_empty());
    for paint in &paints {
        assert_eq!(paint.atlas_rect, SPARK_RECT);
        assert!(paint.glow && paint.smooth);
        assert_eq!(paint.angle, 0.0, "искры не поворачиваются");
        assert!(paint.size[0] >= 0.12 - 1e-6 && paint.size[0] <= 0.2 + 1e-6);
    }
    assert!(
        paints.iter().all(|paint| paint.position[0] > 10.0),
        "летят вправо"
    );
    let redness = |paint: &RectPaint| paint.color[1] / paint.color[0];
    let hottest = paints.iter().map(redness).fold(f32::MIN, f32::max);
    let coolest = paints.iter().map(redness).fold(f32::MAX, f32::min);
    assert!(hottest > coolest + 0.3, "остывают: {hottest} {coolest}");
}

#[test]
fn an_object_with_smoke_and_sparks_draws_both() {
    let scene = r#"{"objects":[{"position":[10,10],"size":[1,1],"smoke":0.5,"sparks":0.5}]}"#;
    let mut rig = Rig::new(scene);
    rig.settle();
    let rects: Vec<AtlasRect> = rig.particle_paints().iter().map(|p| p.atlas_rect).collect();
    assert!(rects.contains(&SMOKE_RECT) && rects.contains(&SPARK_RECT));
}

#[test]
fn autumn_leaves_are_drawn_by_their_own_frames_and_a_leaf_colour_by_the_white_ones() {
    let scene = r##"{"objects":[
        {"position":[10,2],"size":[2,12],"leaf_fall":1},
        {"position":[20,2],"size":[2,12],"leaf_fall":1,"leaf_color":"#ff8800"}]}"##;
    let mut rig = Rig::new(scene);
    rig.frames(30, 0.1);
    let paints = rig.particle_paints();
    let (autumn, tinted): (Vec<&RectPaint>, Vec<&RectPaint>) =
        paints.iter().partition(|paint| paint.position[0] < 16.0);
    assert!(autumn.len() > 20 && tinted.len() > 20);
    let frame_of =
        |paint: &RectPaint, rect: AtlasRect| (paint.atlas_rect.x - rect.x) / (rect.w / 4);
    let mut frames: Vec<u32> = autumn
        .iter()
        .map(|paint| {
            assert_eq!(paint.color[..3], [1.0, 1.0, 1.0], "без подкраски");
            assert_eq!(paint.atlas_rect.y, LEAF_RECT.y);
            assert_eq!(paint.atlas_rect.w, LEAF_RECT.w / 4);
            frame_of(paint, LEAF_RECT)
        })
        .collect();
    frames.sort_unstable();
    frames.dedup();
    assert_eq!(frames, [0, 1, 2, 3], "лист — кадр наугад из четырёх");
    for paint in &tinted {
        assert_eq!(paint.atlas_rect.w, WHITE_LEAF_RECT.w / 4);
        assert!(paint.atlas_rect.x >= WHITE_LEAF_RECT.x);
        assert!(paint.color[0] > paint.color[1] && paint.color[1] > paint.color[2]);
        assert!(paint.color[0] >= 0.8 - 1e-5 && paint.color[0] <= 1.15 + 1e-5);
    }
}

// -------------------------------------------------------------------------------------------
// Листопад по точкам картинки
// -------------------------------------------------------------------------------------------

/// Половина наибольшей ширины листа: насколько середина только что сорвавшегося листа может уйти от
/// точки срыва за шаг-другой.
const LEAF_HALF_WIDTH: f32 = 0.14;

/// Середины листьев в кадр, когда они сорвались, — за минуту после того, как объект получил листопад.
/// Новые листья идут в списке рисования последними: сколько частиц прибавилось за кадр в одну
/// сотку секунды, столько последних и сорвалось; смерть в тот же кадр лишь теряет отсчёт.
fn leaf_middles(scene: &str) -> Vec<[f32; 2]> {
    let mut rig = Rig::new(scene);
    rig.settle();
    rig.game.world.set_number(0, property::LEAF_FALL, 1.0);
    let mut middles = Vec::new();
    let mut before = 0;
    for _ in 0..3600 {
        rig.frames(1, 1.0 / 60.0);
        let paints = rig.particle_paints();
        if paints.len() > before {
            middles.extend(paints[before..].iter().map(|p| {
                [
                    p.position[0] + p.size[0] / 2.0,
                    p.position[1] + p.size[1] / 2.0,
                ]
            }));
        }
        before = paints.len();
    }
    assert!(middles.len() >= 10, "{}", middles.len());
    middles
}

#[test]
fn a_leaf_comes_only_from_the_opaque_half_of_the_picture_and_a_mirror_swaps_the_halves() {
    let plain = leaf_middles(TREE);
    assert!(
        plain.iter().all(|m| m[0] > 18.0 - LEAF_HALF_WIDTH),
        "прозрачна левая половина, 10..18: {plain:?}"
    );
    assert!(plain.iter().any(|m| m[0] > 22.0), "правая половина, 18..26");
    let mirrored = leaf_middles(
        r#"{"objects":[{"position":[10,5],"size":[16,8],"image":"crown","flip_x":true}]}"#,
    );
    assert!(
        mirrored.iter().all(|m| m[0] < 18.0 + LEAF_HALF_WIDTH),
        "отражённая — прозрачна правая, 18..26: {mirrored:?}"
    );
    assert!(mirrored.iter().any(|m| m[0] < 14.0));
}

#[test]
fn a_leaf_is_torn_where_the_picture_of_its_own_size_is_drawn() {
    let leaves =
        leaf_middles(r#"{"objects":[{"position":[10,5],"size":[16,8],"image":"small_crown"}]}"#);
    for [x, y] in leaves {
        assert!(
            (18.0 - LEAF_HALF_WIDTH..=20.0 + LEAF_HALF_WIDTH).contains(&x),
            "{x}: правая половина картинки, 18..20"
        );
        assert!(
            (11.0 - LEAF_HALF_WIDTH..=13.0 + LEAF_HALF_WIDTH).contains(&y),
            "{y}: картинка стоит на нижнем крае, 11..13"
        );
    }
}

#[test]
fn an_object_without_a_picture_drops_leaves_from_its_whole_rectangle() {
    let leaves =
        leaf_middles(r##"{"objects":[{"position":[10,5],"size":[16,8],"color":"#336633"}]}"##);
    let xs: Vec<f32> = leaves.iter().map(|m| m[0]).collect();
    assert!(
        xs.iter().any(|x| *x < 14.0) && xs.iter().any(|x| *x > 22.0),
        "{xs:?}"
    );
}

#[test]
fn a_leaf_ends_on_the_bottom_edge_of_the_object_and_the_tree_keeps_standing() {
    let mut rig = Rig::new(TREE);
    rig.settle();
    rig.game.world.set_number(0, property::LEAF_FALL, 1.0);
    for _ in 0..200 {
        rig.frames(1, 0.1);
        for paint in rig.particle_paints() {
            let bottom = paint.position[1] + paint.size[1] / 2.0;
            assert!(bottom <= 13.0 + 0.5, "лист ушёл ниже края: {paint:?}");
        }
    }
    assert!(rig.live() > 0);
}

// -------------------------------------------------------------------------------------------
// Атлас: встроенные рисунки
// -------------------------------------------------------------------------------------------

const BUILT_IN_RECTS: [AtlasRect; 4] = [SMOKE_RECT, SPARK_RECT, LEAF_RECT, WHITE_LEAF_RECT];

fn overlaps(a: AtlasRect, b: AtlasRect) -> bool {
    a.sheet == b.sheet && a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
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

fn corners_are_clear(sheet: &[u8], rect: AtlasRect) {
    for (x, y) in [
        (0, 0),
        (rect.w - 1, 0),
        (0, rect.h - 1),
        (rect.w - 1, rect.h - 1),
    ] {
        assert_eq!(built_in_pixel(sheet, rect, x, y), [0; 4], "угол {x} {y}");
    }
}

#[test]
fn the_smoke_puff_is_white_and_nearly_opaque_in_the_middle_and_clear_in_the_corners() {
    let sheet = first_sheet();
    let rect = SMOKE_RECT;
    let [r, g, b, a] = built_in_pixel(&sheet, rect, rect.w / 2, rect.h / 2);
    assert!(a >= 200, "середина заметно непрозрачная: {a}");
    assert!(r == g && g == b, "белый, чтобы окрашиваться: {r} {g} {b}");
    assert!(r >= 150, "светлый рисунок: {r}");
    corners_are_clear(&sheet, rect);
}

#[test]
fn the_spark_is_white_brighter_in_the_middle_than_at_the_edge() {
    let sheet = first_sheet();
    let rect = SPARK_RECT;
    let [r, g, b, alpha] = built_in_pixel(&sheet, rect, rect.w / 2, rect.h / 2);
    assert_eq!((r, g, b, alpha), (255, 255, 255, 255));
    let alpha_at = |x: u32| built_in_pixel(&sheet, rect, x, rect.h / 2)[3];
    assert!(alpha_at(rect.w / 2) > alpha_at(rect.w * 3 / 4));
    assert!(alpha_at(rect.w * 3 / 4) > alpha_at(rect.w - 3));
    corners_are_clear(&sheet, rect);
}

fn leaf_frames(sheet: &[u8], strip: AtlasRect) -> Vec<[u8; 4]> {
    let side = strip.h;
    (0..4)
        .map(|frame| {
            let frame_rect = AtlasRect {
                x: strip.x + frame * side,
                w: side,
                ..strip
            };
            let middle = built_in_pixel(sheet, frame_rect, side / 2, side / 2);
            assert!(middle[3] > 200, "кадр {frame}: {middle:?}");
            corners_are_clear(sheet, frame_rect);
            middle
        })
        .collect()
}

#[test]
fn every_autumn_leaf_frame_has_its_own_colour_and_every_white_one_has_none() {
    let sheet = first_sheet();
    let mut colours = leaf_frames(&sheet, LEAF_RECT);
    colours.dedup();
    assert_eq!(colours.len(), 4, "у каждого кадра свой цвет: {colours:?}");

    let side = WHITE_LEAF_RECT.h;
    for frame in 0..4 {
        let frame_rect = AtlasRect {
            x: WHITE_LEAF_RECT.x + frame * side,
            w: side,
            ..WHITE_LEAF_RECT
        };
        for y in 0..side {
            for x in 0..side {
                let [r, g, b, a] = built_in_pixel(&sheet, frame_rect, x, y);
                assert!(r == g && g == b, "белый лист без цвета: {r} {g} {b} {a}");
            }
        }
    }
    leaf_frames(&sheet, WHITE_LEAF_RECT);
}

#[test]
fn the_shapes_of_the_effects_are_the_four_built_in_pictures() {
    let shapes = [
        ParticleShape::Smoke,
        ParticleShape::Spark,
        ParticleShape::Leaf,
        ParticleShape::WhiteLeaf,
    ];
    assert_eq!(shapes.map(ParticleShape::frames), [1, 1, 4, 4]);
}

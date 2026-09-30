//! Фаза 15 — трёхмерная сцена из фигур. Математика камеры, попадание луча по форме, сетки фигур,
//! повёрнутые прямоугольники и поиск пути с ними проверены и в `mod tests` рядом с кодом; здесь —
//! сквозные проверки через загрузку игры из текстов: разбор и проверка данных, ход партии с
//! поворотом, щелчок лучом, запись с `eye`, список рисования, надписи над фигурами и шейдеры.

use engine::core::camera::Camera3d;
use engine::core::footprint::Footprint;
use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::property;
use engine::core::scene::{LightConfig, on_click_target_ray};
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::core::shapes::{Body, ray_through_ground, unit_mesh};
use engine::core::value::{Rotation, Shape};
use engine::core::world_elements::compute_world_draws_3d;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageVerdict, load_game_from_texts, load_game_from_texts_with_code, load_rest, read_entry,
};
use engine::data::recording::{self, ReplayEventKind};
use engine::data::session::PlaySession;
use engine::render::atlas::{self, AtlasImage};
use engine::render::scene3d::compose_frame3d;

const WINDOW: [f32; 2] = [1920.0, 1080.0];

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

/// `scene_extra` без камеры даёт плоскую сцену; с `CAMERA` — трёхмерную.
fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

const CAMERA: &str = r#","view_height":12,"camera":{"pitch":55}"#;
const PROPS: &str = r#"{"properties":{"wall":"flag","ball":"flag","clicked":"number","blocked":"flag","spawned":"flag"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;

type Loaded = Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_with(scene_extra: &str, props: &str, scene: &str, rules: &str) -> Loaded {
    load_game_from_texts(&game_json(scene_extra), props, scene, rules, SCREENS)
}

fn load3d(scene: &str, rules: &str) -> Game {
    let (game, _screens, _warnings) =
        load_with(CAMERA, PROPS, scene, rules).expect("трёхмерная игра должна загрузиться");
    game
}

fn errors_of(result: Loaded) -> Vec<String> {
    match result {
        Ok(_) => panic!("игра должна была не загрузиться"),
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
            .collect(),
    }
}

fn expect_error(result: Loaded, path: &str, fragment: &str) {
    let errors = errors_of(result);
    assert!(
        errors
            .iter()
            .any(|e| e.contains(path) && e.contains(fragment)),
        "нет ошибки с местом \"{path}\" и словом \"{fragment}\": {errors:#?}"
    );
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

fn position(game: &Game, id: u32) -> [f64; 2] {
    game.world.vec2(id, property::POSITION).expect("position")
}

fn scene_of(object: &str) -> String {
    format!(r#"{{"objects":[{object}]}}"#)
}

// -------------------------------------------------------------------------------------------
// Камера, конфигурация сцены и свет
// -------------------------------------------------------------------------------------------

#[test]
fn pitch_outside_thirty_to_ninety_or_not_a_number_is_an_error_with_its_own_place() {
    for pitch in ["29.9", "90.1", "0", "\"55\"", "null"] {
        let scene = format!(r#","view_height":12,"camera":{{"pitch":{pitch}}}"#);
        expect_error(
            load_with(&scene, PROPS, r#"{"objects":[]}"#, NO_RULES),
            "scene → camera → pitch",
            "pitch",
        );
    }
    for pitch in ["30", "55.5", "90"] {
        let scene = format!(r#","view_height":12,"camera":{{"pitch":{pitch}}}"#);
        load_with(&scene, PROPS, r#"{"objects":[]}"#, NO_RULES).expect("граница отрезка включена");
    }
}

#[test]
fn camera_with_an_unknown_key_or_without_view_height_or_without_pitch_is_an_error() {
    expect_error(
        load_with(
            r#","view_height":12,"camera":{"pitch":55,"roll":3}"#,
            PROPS,
            r#"{"objects":[]}"#,
            NO_RULES,
        ),
        "scene → camera → roll",
        "roll",
    );
    expect_error(
        load_with(
            r#","camera":{"pitch":55}"#,
            PROPS,
            r#"{"objects":[]}"#,
            NO_RULES,
        ),
        "scene → camera",
        "view_height",
    );
    expect_error(
        load_with(
            r#","view_height":12,"camera":{}"#,
            PROPS,
            r#"{"objects":[]}"#,
            NO_RULES,
        ),
        "scene → camera",
        "pitch",
    );
}

#[test]
fn light_needs_a_camera_and_keeps_every_field_inside_its_own_range() {
    expect_error(
        load_with(
            r#","light":{"shadow":0.5}"#,
            PROPS,
            r#"{"objects":[]}"#,
            NO_RULES,
        ),
        "scene → light",
        "camera",
    );
    let bad = [
        (r#"{"sun_from":361}"#, "sun_from"),
        (r#"{"sun_from":-1}"#, "sun_from"),
        (r#"{"sun_height":9.9}"#, "sun_height"),
        (r#"{"sun_height":90.1}"#, "sun_height"),
        (r#"{"shadow":1.01}"#, "shadow"),
        (r#"{"shadow":-0.1}"#, "shadow"),
        (r#"{"sunny":1}"#, "sunny"),
    ];
    for (light, key) in bad {
        let scene = format!(r#","view_height":12,"camera":{{"pitch":55}},"light":{light}"#);
        expect_error(
            load_with(&scene, PROPS, r#"{"objects":[]}"#, NO_RULES),
            "scene → light",
            key,
        );
    }
}

#[test]
fn light_defaults_and_explicit_values_reach_the_scene() {
    let (game, _s, _w) = load_with(CAMERA, PROPS, r#"{"objects":[]}"#, NO_RULES).unwrap();
    assert_eq!(game.scene.light, LightConfig::default());
    let light = game.scene.light;
    assert_eq!(
        (light.sun_from, light.sun_height, light.shadow),
        (135.0, 50.0, 0.4)
    );
    let scene = r#","view_height":12,"camera":{"pitch":40},"light":{"sun_from":270,"sun_height":10,"shadow":0}"#;
    let (game, _s, _w) = load_with(scene, PROPS, r#"{"objects":[]}"#, NO_RULES).unwrap();
    assert_eq!(game.scene.camera.map(|c| c.pitch), Some(40.0));
    let light = game.scene.light;
    assert_eq!(
        (light.sun_from, light.sun_height, light.shadow),
        (270.0, 10.0, 0.0)
    );
}

/// Требование 11: 0 — от игрока (к игроку, `+y`), 90 — слева, 180 — из глубины, 270 — справа.
#[test]
fn the_sun_comes_from_the_named_side_at_the_named_height() {
    let toward = |sun_from: f64, sun_height: f64| {
        LightConfig {
            sun_from,
            sun_height,
            shadow: 0.4,
        }
        .direction()
    };
    let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-12);
    let (h_sin, h_cos) = (50.0_f64.to_radians().sin(), 50.0_f64.to_radians().cos());
    assert!(close(toward(0.0, 90.0), [0.0, 0.0, 1.0]));
    assert!(close(toward(0.0, 50.0), [0.0, h_cos, h_sin]));
    assert!(close(toward(90.0, 50.0), [-h_cos, 0.0, h_sin]));
    assert!(close(toward(180.0, 50.0), [0.0, -h_cos, h_sin]));
    assert!(close(toward(270.0, 50.0), [h_cos, 0.0, h_sin]));
    let default = LightConfig::default().direction();
    assert!(
        default[0] < 0.0 && default[1] < 0.0,
        "по умолчанию — слева из глубины: {default:?}"
    );
    assert!((default[0] - default[1]).abs() < 1e-12);
    assert!((default.iter().map(|c| c * c).sum::<f64>() - 1.0).abs() < 1e-12);
}

#[test]
fn y_sort_together_with_a_camera_is_an_error() {
    let scene = r#","view_height":12,"camera":{"pitch":55},"y_sort":true"#;
    expect_error(
        load_with(scene, PROPS, r#"{"objects":[]}"#, NO_RULES),
        "scene → y_sort",
        "y_sort",
    );
}

// -------------------------------------------------------------------------------------------
// Свойства shape, height, rotation в данных
// -------------------------------------------------------------------------------------------

const SHAPE_OBJECT: &str = r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000"}"##;

#[test]
fn shape_and_height_in_a_flat_scene_are_errors_wherever_they_are_written() {
    let flat = |scene: &str, rules: &str| load_with("", PROPS, scene, rules);
    expect_error(
        flat(&scene_of(SHAPE_OBJECT), NO_RULES),
        "objects[0] → shape",
        "трёхмерной",
    );
    expect_error(
        flat(
            &scene_of(r#"{"position":[1,1],"size":[1,1],"height":2}"#),
            NO_RULES,
        ),
        "objects[0] → height",
        "трёхмерной",
    );
    let spawn = r##"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["spawned"]}}},
        "where":{"at":[3,3]},"template":{"spawned":true,"size":[1,1],"shape":"box","color":"#ff0000"}}]}"##;
    expect_error(
        flat(r#"{"objects":[]}"#, spawn),
        "rules[0] → template → shape",
        "трёхмерной",
    );
    let keys =
        scene_of(r#"{"position":[1,1],"size":[1,1],"keys":{"Space":{"press":[["height",3]]}}}"#);
    expect_error(flat(&keys, NO_RULES), "keys → Space → press", "height");
    let on_click = scene_of(r#"{"position":[1,1],"size":[1,1],"on_click":[["shape","box"]]}"#);
    expect_error(flat(&on_click, NO_RULES), "on_click", "shape");
    let wall = scene_of(r#"{"position":[1,1],"size":[1,1],"wall":true}"#);
    for action in [
        r#"["set","height",2]"#,
        r#"["add","height",1]"#,
        r#"["set","shape","box"]"#,
    ] {
        let rules =
            format!(r#"{{"rules":[{{"kind":"check","for":{{"has":["wall"]}},"do":[{action}]}}]}}"#);
        let errors = errors_of(flat(&wall, &rules));
        assert!(
            errors
                .iter()
                .any(|e| e.contains("rules[0]") && e.contains("трёхмерной")),
            "{action}: {errors:#?}"
        );
    }
}

fn load_with_image(scene: &str) -> Loaded {
    let game = game_json(CAMERA).replace(
        r#""fonts":{}"#,
        r#""fonts":{},"images":{"a":{"path":"a.png"}}"#,
    );
    let (config, _) = read_entry(&game).expect("game.json разбирается");
    let verdicts = vec![(
        "a".to_string(),
        ImageVerdict::Ok {
            width: 1,
            height: 1,
            pixels: vec![255; 4],
        },
    )];
    load_rest(
        &game,
        config,
        Some(PROPS),
        Some(scene),
        Some(NO_RULES),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &verdicts,
        None,
        false,
    )
    .map(|(game, screens, warnings, _)| (game, screens, warnings))
}

#[test]
fn every_error_of_a_shape_names_the_object_and_the_field() {
    let cases = [
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"cone","color":"#ff0000"}"##,
            "objects[0] → shape",
            "cone",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box"}"##,
            "objects[0]",
            "color",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","image":"a"}"##,
            "objects[0]",
            "image",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","opacity":0.5}"##,
            "objects[0]",
            "opacity",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","flip_x":true}"##,
            "objects[0]",
            "flip_x",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","height":0}"##,
            "objects[0] → height",
            "больше нуля",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","height":-2}"##,
            "objects[0] → height",
            "больше нуля",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","height":"tall"}"##,
            "objects[0] → height",
            "число",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"rotation":"left"}"##,
            "objects[0] → rotation",
            "число",
        ),
        (
            r##"{"size":[1,1],"shape":"box","color":"#ff0000"}"##,
            "objects[0]",
            "position и size",
        ),
        (
            r##"{"position":[1,1],"shape":"box","color":"#ff0000"}"##,
            "objects[0]",
            "position и size",
        ),
    ];
    for (object, path, fragment) in cases {
        expect_error(load_with_image(&scene_of(object)), path, fragment);
    }
    for (object, field) in [
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","opacity":0.5}"##,
            "opacity",
        ),
        (
            r##"{"position":[1,1],"size":[1,1],"shape":"box","color":"#ff0000","flip_x":true}"##,
            "flip_x",
        ),
    ] {
        let errors = errors_of(load_with_image(&scene_of(object)));
        assert_eq!(
            errors.len(),
            1,
            "одна ошибка данных — одно сообщение: {errors:#?}"
        );
        assert!(
            errors[0].contains("objects[0]") && errors[0].contains(field),
            "{errors:#?}"
        );
    }
}

#[test]
fn a_shape_template_in_a_spawn_rule_is_checked_like_a_scene_object() {
    let spawn = r##"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["spawned"]}}},
        "where":{"at":[3,3]},"template":{"spawned":true,"shape":"box"}}]}"##;
    let errors = errors_of(load_with(CAMERA, PROPS, r#"{"objects":[]}"#, spawn));
    assert!(
        errors
            .iter()
            .any(|e| e.contains("rules[0]") && e.contains("size")),
        "{errors:#?}"
    );
    assert!(
        errors
            .iter()
            .any(|e| e.contains("rules[0]") && e.contains("color")),
        "{errors:#?}"
    );
}

#[test]
fn rotation_takes_any_angle_in_a_three_dimensional_scene_and_only_quarters_in_a_flat_one() {
    let object = |degrees: &str| {
        scene_of(&format!(
            r#"{{"position":[3,3],"size":[2,1],"rotation":{degrees}}}"#
        ))
    };
    for degrees in ["30", "-90", "360", "0.5", "1080"] {
        let mut game = load_with(CAMERA, PROPS, &object(degrees), NO_RULES)
            .unwrap_or_else(|e| panic!("{degrees}: {e:?}"))
            .0;
        game.step(StepInput::empty());
        let rotation = game
            .world
            .rotation(0, property::ROTATION)
            .expect("rotation");
        assert_eq!(rotation.angle(), degrees.parse::<f64>().unwrap());
    }
    for degrees in ["30", "-90", "360"] {
        expect_error(
            load_with("", PROPS, &object(degrees), NO_RULES),
            "objects[0] → rotation",
            "0, 90, 180 или 270",
        );
    }
    for degrees in ["0", "90", "180", "270"] {
        load_with("", PROPS, &object(degrees), NO_RULES).expect("плоская сцена — как раньше");
    }
}

/// Требование 17: поворот, кратный 90, считается точно; `rotation: 360` и `-90` — то же, что 0 и 270.
#[test]
fn quarter_turns_are_exact_and_full_turns_match_their_smaller_twins() {
    for (degrees, expected) in [
        (0.0, (0.0, 1.0)),
        (90.0, (1.0, 0.0)),
        (180.0, (0.0, -1.0)),
        (270.0, (-1.0, 0.0)),
        (360.0, (0.0, 1.0)),
        (-90.0, (-1.0, 0.0)),
        (-450.0, (-1.0, 0.0)),
    ] {
        assert_eq!(
            Rotation::from_degrees(degrees).unwrap().sin_cos(),
            expected,
            "{degrees}"
        );
    }
    let (sin, cos) = Rotation::from_degrees(30.0).unwrap().sin_cos();
    assert!((sin - 0.5).abs() < 1e-15 && (cos - 3.0_f64.sqrt() / 2.0).abs() < 1e-15);
    assert_eq!(Rotation::from_degrees(f64::NAN), None);
    assert_eq!(Rotation::from_degrees(f64::INFINITY), None);
    let flat = Footprint::rotated([2.0, 1.0], [4.0, 1.0], Rotation::from_degrees(-90.0));
    assert_eq!(
        flat,
        Footprint::rotated([2.0, 1.0], [4.0, 1.0], Rotation::from_degrees(270.0))
    );
}

// -------------------------------------------------------------------------------------------
// Код игры читает и пишет shape, height, rotation
// -------------------------------------------------------------------------------------------

const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["wall"]},"do":[["run","go"]]}]}"#;

fn game_json_with_code(scene_extra: &str) -> String {
    game_json(scene_extra).replace(r#""fonts":{}"#, r#""fonts":{},"code":"code.lua""#)
}

fn code_game(scene_extra: &str, scene: &str, code: &str) -> Game {
    let (mut game, _s, _w) = load_game_from_texts_with_code(
        &game_json_with_code(scene_extra),
        PROPS,
        scene,
        CODE_RULES,
        SCREENS,
        Some(code),
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    game.step(StepInput::empty());
    game
}

#[test]
fn code_reads_and_writes_shape_as_text_and_height_and_rotation_as_numbers() {
    let scene = r##"{"objects":[
        {"position":[3,3],"size":[1,1],"wall":true,"shape":"box","color":"#ff0000","height":2,"rotation":30}
    ]}"##;
    let code = r#"
function go(obj)
    assert(obj.shape == "box")
    assert(obj.height == 2)
    assert(obj.rotation == 30)
    obj.shape = "capsule"
    obj.height = 3.5
    obj.rotation = 47.25
end
"#;
    let game = code_game(CAMERA, scene, code);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(game.world.shape(0, property::SHAPE), Some(Shape::Capsule));
    assert_eq!(game.world.number_like(0, property::HEIGHT), Some(3.5));
    assert_eq!(
        game.world
            .rotation(0, property::ROTATION)
            .map(Rotation::angle),
        Some(47.25)
    );
}

#[test]
fn code_errors_stop_the_game_and_name_the_offence() {
    let scene = r##"{"objects":[
        {"position":[3,3],"size":[1,1],"wall":true,"shape":"box","color":"#ff0000"}
    ]}"##;
    for (line, fragment) in [
        (r#"obj.shape = "cone""#, "cone"),
        ("obj.shape = 5", "фигур"),
        ("obj.height = 0", "больше нуля"),
        ("obj.height = -1", "больше нуля"),
        (r#"obj.rotation = "left""#, "число"),
        (r#"obj.image = "a""#, "shape"),
        ("obj.opacity = 0.5", "shape"),
        ("obj.flip_x = true", "shape"),
    ] {
        let code = format!("function go(obj)\n    {line}\nend\n");
        let game_text = game_json_with_code(CAMERA).replace(
            r#""fonts":{}"#,
            r#""fonts":{},"images":{"a":{"path":"a.png"}}"#,
        );
        let (config, _) = read_entry(&game_text).expect("game.json разбирается");
        let verdicts = vec![(
            "a".to_string(),
            ImageVerdict::Ok {
                width: 1,
                height: 1,
                pixels: vec![255; 4],
            },
        )];
        let (mut game, _s, _w, _i) = load_rest(
            &game_text,
            config,
            Some(PROPS),
            Some(scene),
            Some(CODE_RULES),
            Some(SCREENS),
            &[],
            &[],
            &[],
            &verdicts,
            Some(&code),
            false,
        )
        .unwrap_or_else(|e| panic!("{line}: {e:?}"));
        game.step(StepInput::empty());
        let error = game
            .code_error()
            .unwrap_or_else(|| panic!("{line}: ошибки кода нет"));
        assert!(
            error.message.contains(fragment),
            "{line}: {}",
            error.message
        );
        assert!(!game.is_running(), "{line}: игра должна остановиться");
    }
}

#[test]
fn code_may_not_use_shape_or_height_in_a_flat_scene_but_may_read_them() {
    let scene = r##"{"objects":[{"position":[3,3],"size":[1,1],"wall":true,"color":"#ff0000"}]}"##;
    for line in [r#"obj.shape = "box""#, "obj.height = 2"] {
        let game = code_game("", scene, &format!("function go(obj)\n    {line}\nend\n"));
        let error = game.code_error().expect("в плоской сцене — ошибка кода");
        assert!(error.message.contains("трёхмерной"), "{}", error.message);
    }
    let reads =
        "function go(obj)\n    assert(obj.shape == nil)\n    assert(obj.height == nil)\nend\n";
    let game = code_game("", scene, reads);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
}

// -------------------------------------------------------------------------------------------
// Ход партии: столкновения, ходьба, сдвиг, поворот группы, свободная клетка
// -------------------------------------------------------------------------------------------

const BOUNCE_RULES: &str = r##"{"rules":[
    {"kind":"move","for":{"has":["position","velocity"]}},
    {"kind":"collide","a":{"has":["ball"]},"b":{"has":["wall"]},"effects":{"a":[["bounce"]],"b":[]}}
]}"##;

fn velocity(game: &Game, id: u32) -> [f64; 2] {
    game.world.vec2(id, property::VELOCITY).expect("velocity")
}

/// Требование 13: квадрат под 45° падает на неповёрнутую стену — выталкивается по стороне меньшего
/// перекрытия, скорость отражается от неё.
#[test]
fn a_diamond_falling_on_an_upright_wall_is_pushed_out_and_reflected() {
    let scene = r##"{"objects":[
        {"position":[7.4,6],"size":[2,2],"ball":true,"collides":true,"velocity":[60,0],"rotation":45,
         "shape":"box","color":"#00ff00"},
        {"position":[10,2],"size":[1,10],"wall":true,"collides":true,"shape":"box","color":"#ff0000"}
    ]}"##;
    let mut game = load3d(scene, BOUNCE_RULES);
    step(&mut game, 1);
    // Ход на клетку: правый угол ромба вошёл в стену на 0,814 (10,814 − 10) — вытолкнут влево.
    let expected_x = 8.4 - (8.4 + 1.0 + 2.0_f64.sqrt() - 10.0);
    assert!(
        (position(&game, 0)[0] - expected_x).abs() < 1e-9,
        "{:?}",
        position(&game, 0)
    );
    assert!((position(&game, 0)[1] - 6.0).abs() < 1e-9);
    assert_eq!(velocity(&game, 0), [-60.0, 0.0]);
    step(&mut game, 2);
    assert!(
        position(&game, 0)[0] < expected_x,
        "отражённая скорость уводит ромб от стены"
    );
    assert_eq!(
        velocity(&game, 0),
        [-60.0, 0.0],
        "стоящий впритык ромб больше не «пересекает» стену"
    );
}

/// Требование 13: у повёрнутой стены отражение — от её собственной стороны, а не от осей.
#[test]
fn a_ball_hitting_a_turned_wall_keeps_the_tangent_speed_and_flips_the_normal_one() {
    let scene = r##"{"objects":[
        {"position":[9.5,3],"size":[0.5,0.5],"ball":true,"collides":true,"velocity":[0,60]},
        {"position":[4,9.75],"size":[12,0.5],"wall":true,"collides":true,"rotation":30,
         "shape":"box","color":"#ff0000"}
    ]}"##;
    let mut game = load3d(scene, BOUNCE_RULES);
    let before = velocity(&game, 0);
    for _ in 0..600 {
        game.step(StepInput::empty());
        if velocity(&game, 0) != before {
            break;
        }
    }
    let after = velocity(&game, 0);
    assert_ne!(after, before, "мяч должен был отразиться от стены");
    let direction = [30.0_f64.to_radians().cos(), 30.0_f64.to_radians().sin()];
    let normal = [-direction[1], direction[0]];
    let dot = |a: [f64; 2], b: [f64; 2]| a[0] * b[0] + a[1] * b[1];
    assert!(
        (dot(after, direction) - dot(before, direction)).abs() < 1e-9,
        "касательная не изменилась"
    );
    assert!(
        (dot(after, normal) + dot(before, normal)).abs() < 1e-9,
        "нормальная перевернулась"
    );
}

/// Неповёрнутые пары — как прежде: одна и та же партия в плоской и в трёхмерной сцене.
#[test]
fn unturned_pairs_behave_exactly_as_in_a_flat_scene() {
    let scene = r##"{"objects":[
        {"position":[1,3],"size":[1,1],"ball":true,"collides":true,"velocity":[45,20]},
        {"position":[8,2],"size":[1,10],"wall":true,"collides":true},
        {"position":[1,15],"size":[8,1],"wall":true,"collides":true}
    ]}"##;
    let mut flat = load_with("", PROPS, scene, BOUNCE_RULES).unwrap().0;
    let mut space = load_with(CAMERA, PROPS, scene, BOUNCE_RULES).unwrap().0;
    for i in 0..400 {
        flat.step(StepInput::empty());
        space.step(StepInput::empty());
        for id in 0..3 {
            assert_eq!(
                position(&flat, id),
                position(&space, id),
                "шаг {i}, объект {id}"
            );
            assert_eq!(
                flat.world.vec2(id, property::VELOCITY),
                space.world.vec2(id, property::VELOCITY)
            );
        }
    }
    assert_ne!(
        velocity(&flat, 0),
        [45.0, 20.0],
        "мяч за это время отражался"
    );
}

const WALK_RULES: &str = r##"{"rules":[
    {"kind":"walk","for":{"has":["ball"]},"avoid":{"has":["wall"]}}
]}"##;

/// Идёт к `target` (середине) до прихода; возвращает число шагов.
fn walk_to(game: &mut Game, id: u32, target: [f64; 2], max_steps: u32) -> u32 {
    game.world.set_vec2(id, property::WALK_TO, target);
    for taken in 1..=max_steps {
        game.step(StepInput::empty());
        if !game.world.has(id, property::WALK_TO) {
            return taken;
        }
    }
    max_steps
}

fn walker_scene(walls: &str, walker_position: [f64; 2], walker_extra: &str) -> String {
    format!(
        r##"{{"objects":[
        {{"position":[{},{}],"size":[1,1],"ball":true,"walk_speed":6{walker_extra}}},
        {walls}
    ]}}"##,
        walker_position[0], walker_position[1]
    )
}

/// Без поворотов путь в трёхмерной сцене — точка в точку тот же, что в плоской.
#[test]
fn walking_without_any_turned_object_takes_the_flat_scenes_own_path() {
    let walls = r##"{"position":[8,4],"size":[1,12],"wall":true},{"position":[3,3],"size":[2,1],"wall":true}"##;
    let scene = walker_scene(walls, [2.0, 10.0], "");
    let mut flat = load_with("", PROPS, &scene, WALK_RULES).unwrap().0;
    let mut space = load_with(CAMERA, PROPS, &scene, WALK_RULES).unwrap().0;
    flat.world.set_vec2(0, property::WALK_TO, [15.0, 10.0]);
    space.world.set_vec2(0, property::WALK_TO, [15.0, 10.0]);
    for i in 0..300 {
        flat.step(StepInput::empty());
        space.step(StepInput::empty());
        assert_eq!(position(&flat, 0), position(&space, 0), "шаг {i}");
    }
}

/// Требование 14: идущий обходит стену под 30° по концу и не проходит сквозь её тело.
#[test]
fn a_walker_goes_around_a_wall_turned_thirty_degrees_by_its_end() {
    let wall = r##"{"position":[5,9.75],"size":[10,0.5],"wall":true,"rotation":30,"shape":"box","color":"#ff0000"}"##;
    let mut game = load3d(&walker_scene(wall, [2.0, 10.0], ""), WALK_RULES);
    let footprint = Footprint::rotated([5.0, 9.75], [10.0, 0.5], Rotation::from_degrees(30.0));
    game.world.set_vec2(0, property::WALK_TO, [16.0, 10.0]);
    for _ in 0..600 {
        game.step(StepInput::empty());
        let at = position(&game, 0);
        assert!(
            !footprint.overlaps(&Footprint::flat(at, [1.0, 1.0])),
            "сквозь стену: {at:?}"
        );
        if !game.world.has(0, property::WALK_TO) {
            break;
        }
    }
    assert!(!game.world.has(0, property::WALK_TO), "дошёл");
    let at = position(&game, 0);
    assert!(
        (at[0] - 15.5).abs() < 1e-9 && (at[1] - 9.5).abs() < 1e-9,
        "{at:?}"
    );
}

/// Требование 14: щель в стене под 30° шире идущего — проходит, уже — идёт к ближайшему месту.
#[test]
fn a_gap_wider_than_the_walker_passes_and_a_narrower_one_stops_it_at_the_wall() {
    let (sin, cos) = (0.5_f64, 30.0_f64.to_radians().cos());
    let piece = |offset: f64| {
        let center = [10.0 + cos * offset, 10.0 + sin * offset];
        format!(
            r##"{{"position":[{},{}],"size":[40,0.5],"wall":true,"rotation":30,"shape":"box","color":"#ff0000"}}"##,
            center[0] - 20.0,
            center[1] - 0.25
        )
    };
    let start = [10.0 + sin * 5.0, 10.0 - cos * 5.0];
    let target = [10.0 - sin * 5.0, 10.0 + cos * 5.0];
    let run = |gap: f64| {
        let step = 20.0 + gap / 2.0;
        let walls = format!("{},{}", piece(-step), piece(step));
        let scene = walker_scene(&walls, [start[0] - 0.5, start[1] - 0.5], "");
        let mut game = load_with(CAMERA, PROPS, &scene, WALK_RULES).unwrap().0;
        walk_to(&mut game, 0, target, 2000);
        let at = position(&game, 0);
        (at[0] + 0.5 - target[0]).hypot(at[1] + 0.5 - target[1])
    };
    assert!(run(2.0) < 1e-6, "щель 2 при идущем 1");
    assert!(run(0.5) > 0.5, "щель 0,5 при идущем 1 закрыта");
}

/// Требование 14: идущий, повёрнутый на 45°, занимает по диагонали больше — в щель, куда влезал
/// квадрат по осям, ему не пройти.
#[test]
fn a_walker_turned_forty_five_degrees_does_not_fit_a_gap_its_square_form_fits() {
    let pillars = r##"{"position":[10,0],"size":[1,10],"wall":true},{"position":[10,11.4],"size":[1,8.6],"wall":true}"##;
    let run = |extra: &str| {
        let mut game = load3d(&walker_scene(pillars, [4.0, 10.2], extra), WALK_RULES);
        walk_to(&mut game, 0, [17.5, 10.7], 2000);
        position(&game, 0)[0]
    };
    assert!((run("") - 17.0).abs() < 1e-6, "по осям проходит в щель 1,4");
    assert!(
        run(r#","rotation":45"#) < 10.0,
        "квадрат на 45° (диагональ 1,41) в щель 1,4 не проходит"
    );
    assert!(
        (run(r#","rotation":90"#) - 17.0).abs() < 1e-6,
        "четверть оборота ничего не меняет"
    );
}

const SHIFT_RULES: &str = r##"{"rules":[
    {"kind":"check","for":{"has":["ball"]},
     "do":[["shift",{"group":{"has":["ball"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                     "if_blocked":[["give","blocked",{"has":["ball"]}]]}]]}
]}"##;

/// Требование 15: `blocked_by` считает повёрнутые прямоугольники — стена, повёрнутая на 90°, стоит
/// стеной по вертикали, а не лежит горизонтально мимо группы.
#[test]
fn blocked_by_counts_the_turned_rectangles() {
    let scene = |rotation: &str| {
        format!(
            r##"{{"objects":[
            {{"position":[3,8],"size":[2,2],"ball":true}},
            {{"position":[0,10],"size":[12,0.5],"wall":true{rotation}}}
        ]}}"##
        )
    };
    let mut turned = load_with(CAMERA, PROPS, &scene(r#","rotation":90"#), SHIFT_RULES)
        .unwrap()
        .0;
    let flag = turned.properties.resolve("blocked").unwrap();
    turned.step(StepInput::empty());
    assert!(
        turned.world.flag(0, flag),
        "повёрнутая стена преграждает путь"
    );
    assert_eq!(position(&turned, 0), [3.0, 8.0], "сдвиг отменён");
    let mut upright = load_with(CAMERA, PROPS, &scene(""), SHIFT_RULES).unwrap().0;
    upright.step(StepInput::empty());
    assert!(!upright.world.flag(0, flag), "неповёрнутая лежит мимо");
    assert_eq!(position(&upright, 0), [4.0, 8.0]);
}

/// Требование 16: `turn` в трёхмерной сцене — середины поворачиваются на четверть оборота, размеры
/// не меняются, `rotation` меняется на 90 по направлению поворота.
#[test]
fn turn_moves_the_middles_and_the_rotations_but_keeps_the_sizes() {
    let scene = r##"{"objects":[
        {"position":[9.5,9.5],"size":[1,1],"wall":true},
        {"position":[11,9.5],"size":[3,1],"ball":true,"rotation":30}
    ]}"##;
    let rules = r##"{"rules":[
        {"kind":"check","for":{"has":["wall"]},
         "do":[["turn",{"group":{"has":["ball"]},"around":{"has":["wall"]},"dir":1}]]}
    ]}"##;
    let mut game = load3d(scene, rules);
    game.step(StepInput::empty());
    // Середина (12,10) вокруг (10,10) на четверть по часовой: (10,12) — и тот же размер 3 × 1.
    assert_eq!(position(&game, 1), [8.5, 12.0]);
    assert_eq!(game.world.vec2(1, property::SIZE), Some([3.0, 1.0]));
    assert_eq!(
        game.world
            .rotation(1, property::ROTATION)
            .map(Rotation::angle),
        Some(120.0)
    );
    game.step(StepInput::empty());
    assert_eq!(
        game.world
            .rotation(1, property::ROTATION)
            .map(Rotation::angle),
        Some(210.0)
    );
    assert_eq!(game.world.vec2(1, property::SIZE), Some([3.0, 1.0]));
}

/// Требование 15: клетка, которую занимает повёрнутый объект, для `random_cell` занята.
#[test]
fn random_cell_avoids_the_cells_a_turned_object_covers() {
    let scene = |rotation: &str| {
        format!(
            r##"{{"objects":[
            {{"position":[-0.5,1.25],"size":[4,0.5],"wall":true,"collides":true{rotation}}}
        ]}}"##
        )
    };
    let rules = r##"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":8,"of":{"has":["spawned"]}}},
         "where":"random_cell","template":{"spawned":true,"size":[1,1],"collides":true}}
    ]}"##;
    let run = |rotation: &str| {
        let text =
            game_json(CAMERA).replace(r#""width":20,"height":20"#, r#""width":3,"height":3"#);
        let (mut game, _s, _w) =
            load_game_from_texts(&text, PROPS, &scene(rotation), rules, SCREENS).unwrap();
        step(&mut game, 12);
        let spawned = game.properties.resolve("spawned").unwrap();
        let cells: Vec<[f64; 2]> = game
            .world
            .ids()
            .filter(|&id| game.world.flag(id, spawned))
            .map(|id| position(&game, id))
            .collect();
        assert_eq!(cells.len(), 6, "свободных клеток 6: {cells:?}");
        cells
    };
    assert!(
        run("").iter().all(|c| c[1] != 1.0),
        "лежит горизонтально — занята средняя строка"
    );
    assert!(
        run(r#","rotation":90"#).iter().all(|c| c[0] != 1.0),
        "повёрнута на 90° — занят средний столбец"
    );
}

/// Требование 15: предупреждение «объект вне сцены» считает повёрнутый прямоугольник.
#[test]
fn the_outside_the_scene_warning_uses_the_turned_rectangle() {
    let object = |rotation: &str| {
        scene_of(&format!(
            r#"{{"position":[-1,2],"size":[1.5,0.5]{rotation}}}"#
        ))
    };
    let outside = |scene: String| {
        let (_g, _s, warnings) = load_with(CAMERA, PROPS, &scene, NO_RULES).unwrap();
        warnings
            .into_iter()
            .filter(|w| w.message.contains("за пределами сцены"))
            .count()
    };
    assert_eq!(outside(object("")), 0, "у неповёрнутого часть на сцене");
    assert_eq!(
        outside(object(r#","rotation":90"#)),
        1,
        "повёрнутый на 90° целиком слева от сцены"
    );

    // Квадрат 2 × 2 серединой в (−0,9; −0,9): охватывающий прямоугольник заходит на сцену, а сам
    // квадрат, повёрнутый на 45°, — нет.
    let square = |rotation: &str| {
        scene_of(&format!(
            r#"{{"position":[-1.9,-1.9],"size":[2,2]{rotation}}}"#
        ))
    };
    assert_eq!(outside(square("")), 0, "неповёрнутый частью на сцене");
    assert_eq!(
        outside(square(r#","rotation":45"#)),
        1,
        "ромб целиком вне сцены"
    );
    let (_g, _s, warnings) =
        load_with(CAMERA, PROPS, &square(r#","rotation":45"#), NO_RULES).unwrap();
    let message = &warnings
        .iter()
        .find(|w| w.message.contains("за пределами сцены"))
        .expect("предупреждение")
        .message;
    assert!(
        message.contains("[-1.9, -1.9]..[0.1") && message.contains("45°"),
        "текст — по месту объекта, а не по охватывающему прямоугольнику: {message}"
    );
}

/// Плоская сцена не меняется: объект нулевого размера внутри сцены — не «за пределами сцены»,
/// как и до фазы; в трёхмерной сцене неповёрнутый — так же.
#[test]
fn a_zero_size_object_inside_the_scene_gets_no_outside_warning() {
    for scene_extra in ["", CAMERA] {
        for size in ["[0,0]", "[0,1]"] {
            let scene = scene_of(&format!(r#"{{"position":[5,5],"size":{size}}}"#));
            let (_g, _s, warnings) = load_with(scene_extra, PROPS, &scene, NO_RULES).unwrap();
            assert!(
                warnings
                    .iter()
                    .all(|w| !w.message.contains("за пределами сцены")),
                "size {size}, сцена «{scene_extra}»: {warnings:?}"
            );
        }
    }
}

/// Требование 15: условие `outside_scene` в трёхмерной сцене считает повёрнутый прямоугольник;
/// у неповёрнутых — как прежде (за сценой дальше собственного размера).
#[test]
fn the_outside_scene_condition_uses_the_turned_rectangle() {
    let rules = r#"{"rules":[{"kind":"delete","for":{"has":["ball"]},"when":"outside_scene"}]}"#;
    // Ромб серединой в (−3,9; −3,9): охватывающий прямоугольник (ширина 2,83) не дальше своего
    // размера от сцены, сам ромб — дальше; второй ромб той же формы стоит серединой в (−2,4).
    let scene = r#"{"objects":[
        {"position":[-4.9,-4.9],"size":[2,2],"ball":true,"rotation":45},
        {"position":[-4.1,-4.1],"size":[2,2],"ball":true},
        {"position":[-3.0,-3.0],"size":[2,2],"ball":true},
        {"position":[-3.4,-3.4],"size":[2,2],"ball":true,"rotation":45}
    ]}"#;
    let mut game = load3d(scene, rules);
    step(&mut game, 1);
    assert!(
        !game.world.is_alive(0),
        "ромб дальше своего размера от сцены"
    );
    assert!(
        !game.world.is_alive(1),
        "неповёрнутый дальше своего размера — как прежде"
    );
    assert!(game.world.is_alive(2), "неповёрнутый ближе — остаётся");
    assert!(
        game.world.is_alive(3),
        "ромб ближе своего размера — остаётся"
    );
}

// -------------------------------------------------------------------------------------------
// Камера в партии, точка под курсором и щелчок лучом
// -------------------------------------------------------------------------------------------

const CLICK_SCENE: &str = r##"{"objects":[
    {"name":"hero","position":[9,9],"size":[1,1],"camera_follows":true,"shape":"capsule","height":1.8,"color":"#2f6fdb"}
]}"##;

/// Требование 5: камера держит середину объекта; луч из середины окна попадает в его фигуру.
#[test]
fn the_ray_from_the_middle_of_the_window_lands_in_the_followed_object() {
    let game = load3d(CLICK_SCENE, NO_RULES);
    let camera = game.camera_3d(WINDOW).expect("трёхмерная сцена");
    assert!((camera.target[0] - 9.5).abs() < 1e-9, "{camera:?}");
    let direction = camera.ray_direction([WINDOW[0] as f64 / 2.0, WINDOW[1] as f64 / 2.0]);
    let body = Body::of_object(&game.world, 0).expect("фигура");
    assert!(
        body.ray_hit(camera.eye, direction).is_some(),
        "середина окна — на герое"
    );
    let (flat, _s, _w) =
        load_with(r#","view_height":12"#, PROPS, r#"{"objects":[]}"#, NO_RULES).unwrap();
    assert!(
        flat.camera_3d(WINDOW).is_none(),
        "в плоской сцене камеры-луча нет"
    );
}

/// Требование 21: за краем сцены точка под курсором прижата к краю.
#[test]
fn the_point_under_the_cursor_beyond_the_scene_is_clamped_to_its_edge() {
    let game = load3d(CLICK_SCENE, NO_RULES);
    let camera = game.camera_3d(WINDOW).unwrap();
    let corner = camera.ground_point([0.0, 0.0]);
    assert!(
        corner[0] < 0.0 || corner[1] < 0.0 || corner[0] > 20.0 || corner[1] > 20.0,
        "верхний угол окна — за сценой: {corner:?}"
    );
    let clamped = game.scene.clamp_point(corner);
    assert!(
        (0.0..=20.0).contains(&clamped[0]) && (0.0..=20.0).contains(&clamped[1]),
        "{clamped:?}"
    );
    assert_eq!(game.scene.clamp_point([-3.0, 25.0]), [0.0, 20.0]);
    assert_eq!(game.scene.clamp_point([7.0, 8.0]), [7.0, 8.0]);
}

fn click_objects(shape: &str, extra: &str) -> String {
    format!(
        r##"{{"objects":[
        {{"name":"target","position":[10,10],"size":[2,1],"shape":"{shape}","height":3,"color":"#ff0000",
          "on_click":[["clicked",1]]{extra}}}
    ]}}"##
    )
}

/// Ground point under the pixel a world point projects to, and the camera's eye — what the mouse
/// reports for a click on that point of a body. The point is not clamped to the scene: the step
/// clamps it only where it goes in as a place of the scene.
fn ray_through(game: &Game, point: [f64; 3]) -> ([f64; 3], [f64; 2]) {
    let camera = game.camera_3d(WINDOW).unwrap();
    let window = camera.project(point).expect("перед камерой");
    (camera.eye, camera.ground_point(window))
}

fn clicked(game: &Game, point: [f64; 3]) -> Option<u32> {
    let (eye, cell) = ray_through(game, point);
    on_click_target_ray(&game.world, &game.scene, Some(eye), cell)
}

/// Требование 22: попадание в коробку, цилиндр, капсулу и шар — в том числе повёрнутые.
#[test]
fn a_click_ray_hits_each_shape_by_its_own_form_turned_or_not() {
    for (shape, extra) in [
        ("box", ""),
        ("box", r#","rotation":50"#),
        ("cylinder", ""),
        ("cylinder", r#","rotation":90"#),
        ("capsule", ""),
        ("capsule", r#","rotation":30"#),
        ("sphere", ""),
    ] {
        let game = load3d(&click_objects(shape, extra), NO_RULES);
        assert_eq!(
            clicked(&game, [11.0, 10.5, 1.5]),
            Some(0),
            "{shape}{extra}: середина тела"
        );
        assert_eq!(
            clicked(&game, [11.0, 10.5, 6.0]),
            None,
            "{shape}{extra}: выше фигуры луч мимо"
        );
    }
}

/// Требование 22: цилиндр растянут эллипсом — по углу описанного прямоугольника луч не попадает;
/// шар по углу тоже; прямо вниз через середину — попадает.
#[test]
fn a_stretched_cylinder_and_sphere_miss_at_the_corner_of_their_rectangle() {
    for shape in ["cylinder", "sphere"] {
        let game = load3d(&click_objects(shape, ""), NO_RULES);
        assert_eq!(
            on_click_target_ray(&game.world, &game.scene, None, [10.05, 10.02]),
            None,
            "{shape}: угол прямоугольника пуст"
        );
        assert_eq!(
            on_click_target_ray(&game.world, &game.scene, None, [11.0, 10.5]),
            Some(0),
            "{shape}: прямо вниз в середину"
        );
    }
}

/// Требование 22: луч мимо капсулы рядом с её верхом не попадает.
#[test]
fn a_ray_just_beside_the_top_of_a_capsule_does_not_hit() {
    let scene = r##"{"objects":[
        {"position":[10,10],"size":[0.6,0.6],"shape":"capsule","height":1.8,"color":"#ff0000","on_click":[["clicked",1]]}
    ]}"##;
    let game = load3d(scene, NO_RULES);
    let body = Body::of_object(&game.world, 0).unwrap();
    // Горизонтально на высоте 1,7 в 0,25 от оси: над плечом полушария (радиус там 0,22).
    assert_eq!(body.ray_hit([5.0, 10.55, 1.7], [1.0, 0.0, 0.0]), None);
    assert!(
        body.ray_hit([5.0, 10.55, 1.2], [1.0, 0.0, 0.0]).is_some(),
        "в теле тот же луч попадает"
    );
}

/// Требование 22: из двух объектов на луче срабатывает ближний к камере.
#[test]
fn the_nearest_object_on_the_ray_is_the_one_clicked() {
    let scene = r##"{"objects":[
        {"name":"far","position":[10,8],"size":[2,1],"shape":"box","height":6,"color":"#ff0000","on_click":[["clicked",1]]},
        {"name":"near","position":[10,9.5],"size":[2,1],"shape":"box","height":6,"color":"#00ff00","on_click":[["clicked",2]]}
    ]}"##;
    let game = load3d(scene, NO_RULES);
    // Низко через дальний ящик луч заходит и в ближний, высоко — проходит над ним.
    assert_eq!(
        clicked(&game, [11.0, 8.5, 2.0]),
        Some(1),
        "ближний ловит щелчок первым"
    );
    assert_eq!(
        clicked(&game, [11.0, 8.5, 5.5]),
        Some(0),
        "у высокой точки дальнего ящика ближний не на луче"
    );
}

/// Требование 22: объект без `on_click` луч не задерживает.
#[test]
fn an_object_without_on_click_does_not_stop_the_ray() {
    let scene = r##"{"objects":[
        {"name":"far","position":[10,8],"size":[2,1],"shape":"box","height":2,"color":"#ff0000","on_click":[["clicked",1]]},
        {"name":"screen","position":[10,10],"size":[2,1],"shape":"box","height":5,"color":"#0000ff"}
    ]}"##;
    let game = load3d(scene, NO_RULES);
    let (eye, cell) = ray_through(&game, [11.0, 8.5, 1.0]);
    let (origin, direction) = ray_through_ground(eye, cell);
    let screen = Body::of_object(&game.world, 1).unwrap();
    assert!(
        screen.ray_hit(origin, direction).is_some(),
        "экран действительно стоит на луче"
    );
    assert_eq!(
        on_click_target_ray(&game.world, &game.scene, Some(eye), cell),
        Some(0)
    );
}

/// Требование 22: плоский повёрнутый прямоугольник на земле ловит щелчок своим повёрнутым
/// прямоугольником; фигура над ним ближе к камере и срабатывает первой.
#[test]
fn a_turned_flat_rectangle_on_the_ground_catches_clicks_and_a_shape_over_it_wins() {
    let scene = r##"{"objects":[
        {"name":"puddle","position":[6,10],"size":[6,1],"color":"#3f7fd0","rotation":90,"on_click":[["clicked",1]]},
        {"name":"post","position":[8.8,10.2],"size":[0.4,0.4],"shape":"box","height":2,"color":"#5b3a1c","on_click":[["clicked",2]]}
    ]}"##;
    let game = load3d(scene, NO_RULES);
    let eye = game.camera_3d(WINDOW).unwrap().eye;
    // Лужа 6×1, повёрнутая на 90°, стоит вертикально: x ∈ [8.5, 9.5], y ∈ [7.5, 13.5].
    assert_eq!(
        on_click_target_ray(&game.world, &game.scene, Some(eye), [9.0, 8.0]),
        Some(0)
    );
    assert_eq!(
        on_click_target_ray(&game.world, &game.scene, Some(eye), [11.0, 10.5]),
        None
    );
    assert_eq!(
        on_click_target_ray(&game.world, &game.scene, Some(eye), [7.0, 10.5]),
        None
    );
    assert_eq!(
        clicked(&game, [9.0, 10.4, 1.0]),
        Some(1),
        "столб ближе к камере, чем лужа"
    );
}

/// Щелчок доходит до `on_click` через настоящий шаг: `MouseLeft` + указатель с `eye`.
#[test]
fn a_mouse_press_with_the_recorded_eye_fires_on_click_of_the_object_the_ray_hits() {
    let mut game = load3d(&click_objects("capsule", ""), NO_RULES);
    let (eye, cell) = ray_through(&game, [11.0, 10.5, 1.5]);
    game.set_cursor_ray(cell, eye);
    game.key_down("MouseLeft");
    let snapshot = game.take_input_snapshot();
    game.step(snapshot);
    let clicked = game.properties.resolve("clicked").unwrap();
    assert_eq!(game.world.number_like(0, clicked), Some(1.0));
    assert_eq!(game.cursor_eye(), Some(eye));
}

/// Высокая фигура у правого края сцены: за её верхом земля под курсором уже вне сцены.
const EDGE_SCENE: &str = r##"{"objects":[
    {"name":"tower","position":[18,10],"size":[1,1],"shape":"box","height":4,"color":"#ff0000",
     "on_click":[["clicked",1],["walk_to","cursor"]]}
]}"##;
const EDGE_TOP: [f64; 3] = [18.5, 10.5, 3.8];

/// Требования 21–22: щелчок по верху фигуры у края сцены, где земля под курсором за краем, попадает
/// в фигуру, а `walk_to "cursor"` получает точку, прижатую к краю.
#[test]
fn a_click_on_the_top_of_a_shape_at_the_scene_edge_hits_it_and_walks_to_the_clamped_point() {
    let mut game = load3d(EDGE_SCENE, NO_RULES);
    let (eye, ground) = ray_through(&game, EDGE_TOP);
    assert!(
        ground[0] > 20.0,
        "земля под курсором за краем сцены: {ground:?}"
    );
    let clamped = game.scene.clamp_point(ground);
    assert_eq!(
        on_click_target_ray(&game.world, &game.scene, Some(eye), clamped),
        None,
        "луч через прижатую точку прошёл бы мимо фигуры"
    );
    game.set_cursor_ray(ground, eye);
    game.key_down("MouseLeft");
    let snapshot = game.take_input_snapshot();
    game.step(snapshot);
    let clicked = game.properties.resolve("clicked").unwrap();
    assert_eq!(game.world.number_like(0, clicked), Some(1.0));
    assert_eq!(game.world.vec2(0, property::WALK_TO), Some(clamped));
    assert_eq!(clamped[0], 20.0);
}

/// То же через запись и повтор: живая партия пишет неприжатую точку с `eye`, повтор в окнах разной
/// формы даёт тот же щелчок и ту же прижатую точку в `walk_to`.
#[test]
fn the_unclamped_point_is_recorded_and_replays_the_same_edge_click() {
    let (mut game, config, _w) =
        load_game_from_texts(&game_json(CAMERA), PROPS, EDGE_SCENE, NO_RULES, SCREENS).unwrap();
    let (eye, ground) = ray_through(&game, EDGE_TOP);
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    game.set_cursor_ray(ground, eye);
    session.record_cursor(&game);
    let recorded = recording::parse(&session.recording_text(&game)).expect("запись разбирается");
    let ReplayEventKind::Cursor(cell, Some(recorded_eye)) = recorded.events[0].kind else {
        panic!("{:?}", recorded.events[0].kind);
    };
    assert!(cell[0] > 20.0, "записана неприжатая точка: {cell:?}");

    let text = format!(
        r#"{{"format":1,"steps":3,"events":[
            {{"step":0,"cursor":[{},{}],"eye":[{},{},{}]}},
            {{"step":1,"key_down":"MouseLeft"}}]}}"#,
        cell[0], cell[1], recorded_eye[0], recorded_eye[1], recorded_eye[2]
    );
    for viewport in [[800.0, 600.0], [400.0, 900.0], [1920.0, 1080.0]] {
        let replayed = replay(EDGE_SCENE, &text, viewport).expect("запись принимается");
        let clicked = replayed.properties.resolve("clicked").unwrap();
        assert_eq!(
            replayed.world.number_like(0, clicked),
            Some(1.0),
            "{viewport:?}"
        );
        assert_eq!(
            replayed.world.vec2(0, property::WALK_TO),
            Some(replayed.scene.clamp_point([cell[0], cell[1]])),
            "{viewport:?}"
        );
    }
}

// -------------------------------------------------------------------------------------------
// Запись партии: `eye` рядом с указателем
// -------------------------------------------------------------------------------------------

/// Указатель стоит на земле позади цели (её прямоугольник — x 10–12, y 10–11): луч от `eye`
/// (11; 26; 12) через точку тела (11; 10,5; 2) доходит до земли в (11; 7,4).
const RECORDING_EYE: &str = r#"{"format":1,"steps":3,"events":[
    {"step":0,"cursor":[11,7.4],"eye":[11,26,12]},
    {"step":1,"key_down":"MouseLeft"}
]}"#;

fn replay_click(recording: &str, viewport: [f32; 2]) -> Result<f64, String> {
    let game = replay(&click_objects("box", ""), recording, viewport)?;
    let clicked = game.properties.resolve("clicked").unwrap();
    Ok(game.world.number_like(0, clicked).unwrap_or(0.0))
}

fn replay(scene: &str, recording: &str, viewport: [f32; 2]) -> Result<Game, String> {
    let (mut game, config, _w) =
        load_game_from_texts(&game_json(CAMERA), PROPS, scene, NO_RULES, SCREENS).unwrap();
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_replay(recording, &mut game, &config, &mut state)?;
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    for _ in 0..3 {
        session.step_once(
            &mut queue,
            &mut mouse,
            &mut game,
            &config,
            &mut state,
            viewport,
            &[],
        );
    }
    Ok(game)
}

/// Требование 23: повтор берёт луч из записи — одна запись в окнах разной формы даёт одну партию.
/// Указатель записи стоит на земле позади цели, поэтому в цель попадает только луч от `eye`.
#[test]
fn a_recording_with_eye_replays_the_same_click_in_windows_of_any_shape() {
    for viewport in [
        [800.0, 600.0],
        [400.0, 900.0],
        [1920.0, 1080.0],
        [300.0, 300.0],
    ] {
        assert_eq!(
            replay_click(RECORDING_EYE, viewport),
            Ok(1.0),
            "{viewport:?}"
        );
    }
    let game = load3d(&click_objects("box", ""), NO_RULES);
    assert_eq!(
        on_click_target_ray(
            &game.world,
            &game.scene,
            Some([11.0, 26.0, 12.0]),
            [11.0, 7.4]
        ),
        Some(0),
        "луч от eye записи попадает в цель"
    );
    assert_eq!(
        on_click_target_ray(&game.world, &game.scene, None, [11.0, 7.4]),
        None,
        "та же точка без луча от eye в цель не попадает"
    );
}

/// Требование 23: трёхмерная запись без `eye` у указателя не принимается — с ошибкой.
#[test]
fn a_three_dimensional_recording_without_eye_is_rejected_with_an_error() {
    let without = r#"{"format":1,"steps":3,"events":[{"step":0,"cursor":[11,10.5]}]}"#;
    let error = replay_click(without, [800.0, 600.0]).unwrap_err();
    assert!(error.contains("eye"), "{error}");
    assert!(error.contains("Это не запись партии"), "{error}");
    // Плоской игре `eye` не нужен.
    let (mut game, config, _w) = load_game_from_texts(
        &game_json(""),
        PROPS,
        r#"{"objects":[]}"#,
        NO_RULES,
        SCREENS,
    )
    .unwrap();
    let mut state = ScreenState::new(config.start_screen);
    PlaySession::begin_replay(without, &mut game, &config, &mut state)
        .expect("плоская запись без eye принимается");
}

/// Живая партия пишет `eye` вместе с указателем — тем же событием.
#[test]
fn a_live_session_records_the_camera_position_next_to_the_pointer() {
    let (mut game, config, _w) = load_game_from_texts(
        &game_json(CAMERA),
        PROPS,
        &click_objects("box", ""),
        NO_RULES,
        SCREENS,
    )
    .unwrap();
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let (eye, cell) = ray_through(&game, [11.0, 10.5, 1.5]);
    game.set_cursor_ray(cell, eye);
    session.record_cursor(&game);
    let text = session.recording_text(&game);
    assert!(text.contains(r#""eye":["#), "{text}");
    let recorded = recording::parse(&text).expect("запись разбирается");
    let ReplayEventKind::Cursor(read_cell, Some(read_eye)) = recorded.events[0].kind else {
        panic!("{:?}", recorded.events[0].kind);
    };
    let close = |a: &[f64], b: &[f64]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-9);
    assert!(
        close(&read_cell[..2], &cell) && close(&read_eye, &eye),
        "{read_cell:?} {read_eye:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Надписи и полоски над фигурами
// -------------------------------------------------------------------------------------------

const LABEL_SCREENS: &str = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
 "world_elements":[
   {"kind":"bar","for":{"has":["wall"]},"anchor":"top","offset":[0,-0.3],"size":[1,0.12],
    "value":"clicked","max":10,"color":"#22c55e","back_color":"#00000080"},
   {"kind":"label","for":{"has":["wall"]},"anchor":"top","offset":[0,-0.6],"size":[1.4,0.35],
    "text":"{clicked}","font":"ui","align":"center","color":"#ffffff"}
 ]}"##;

fn labelled_game(scene: &str) -> (Game, ScreensConfig) {
    let text = game_json(CAMERA).replace(r#""fonts":{}"#, r#""fonts":{"ui":"ui.ttf"}"#);
    let (config, _) = read_entry(&text).expect("game.json разбирается");
    let (game, screens, _w, _i) = load_rest(
        &text,
        config,
        Some(PROPS),
        Some(scene),
        Some(NO_RULES),
        Some(LABEL_SCREENS),
        &[],
        &[],
        &[],
        &[],
        None,
        true,
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    (game, screens)
}

/// Требования 24–25: `top` — середина над верхней точкой капсулы на экране; размеры и сдвиг
/// переведены одной клеткой, поэтому у ближнего и дальнего объекта одинаковы.
#[test]
fn the_top_anchor_sits_above_the_capsules_top_point_and_sizes_do_not_depend_on_distance() {
    let scene = r##"{"objects":[
        {"position":[9,4],"size":[0.9,0.9],"wall":true,"shape":"capsule","height":1.3,"color":"#7dc243","clicked":5},
        {"position":[9,14],"size":[0.9,0.9],"wall":true,"shape":"capsule","height":1.3,"color":"#7dc243","clicked":5}
    ]}"##;
    let (game, screens) = labelled_game(scene);
    let camera = game.camera_3d(WINDOW).unwrap();
    let (bars, labels) = compute_world_draws_3d(
        &game.world,
        &game.scene,
        &game.properties,
        &screens.world_elements,
        &camera,
    );
    assert_eq!((bars.len(), labels.len()), (2, 2));
    let cell = camera.cell_points() as f32;
    for bar in &bars {
        assert!(
            (bar.fill.size[1] - 0.12 * cell).abs() < 1e-3,
            "высота полоски — 0,12 клетки"
        );
        assert!(
            (bar.fill.size[0] - 0.5 * cell).abs() < 1e-3,
            "5 из 10 — половина ширины"
        );
    }
    assert_eq!(
        bars[0].fill.size, bars[1].fill.size,
        "у ближнего и дальнего один размер"
    );
    assert_eq!(labels[0].size, labels[1].size);
    assert_eq!(labels[0].font_size, labels[1].font_size);
    for (index, bar) in bars.iter().enumerate() {
        let body = Body::of_object(&game.world, index as u32).unwrap();
        let [x0, y0, x1, _] = body.screen_rect(&camera).unwrap();
        let back = bar.back.expect("подложка");
        let middle = f64::from(back.position[0] + back.size[0] / 2.0);
        assert!(
            (middle - (x0 + x1) / 2.0).abs() < 1e-3,
            "над серединой фигуры по горизонтали"
        );
        let center_y = f64::from(bar.fill.position[1] + bar.fill.size[1] / 2.0);
        assert!(
            (center_y - (y0 - 0.3 * f64::from(cell))).abs() < 1e-3,
            "полоска на 0,3 клетки выше верха фигуры на экране: {center_y} против {y0}"
        );
    }
    assert!(
        bars[0].fill.position[1] < bars[1].fill.position[1],
        "дальняя (меньше y) — выше на экране"
    );
}

// -------------------------------------------------------------------------------------------
// Список рисования кадра
// -------------------------------------------------------------------------------------------

#[test]
fn the_frame_lists_shapes_with_their_place_and_flat_objects_by_layer_over_the_ground() {
    let text = game_json(CAMERA).replace(
        r#""fonts":{}"#,
        r#""fonts":{},"images":{"grass":{"path":"grass.png","frames":4,"columns":2}}"#,
    );
    let row = "[0,1,2,3,0,1,2,3,0,1,2,3,0,1,2,3,0,1,2,3]";
    let rows = vec![row; 20].join(",");
    let scene = format!(
        r##"{{"objects":[
        {{"name":"top","position":[4,4],"size":[3,1],"color":"#ff0000","layer":2,"rotation":90}},
        {{"name":"under","position":[4,4],"size":[3,1],"color":"#0000ff","layer":1}},
        {{"name":"tower","position":[8,6],"size":[2,1],"shape":"cylinder","height":3,"color":"#00ff00","rotation":30}},
        {{"name":"hero","position":[10,10],"size":[0.6,0.6],"shape":"capsule","height":1.8,"color":"#2f6fdb","camera_follows":true}}
    ],"ground":[{{"image":"grass","cells":[{rows}]}}]}}"##
    );
    let (config, _) = read_entry(&text).expect("game.json разбирается");
    let verdicts = vec![(
        "grass".to_string(),
        ImageVerdict::Ok {
            width: 4,
            height: 4,
            pixels: vec![255; 64],
        },
    )];
    let (game, _screens, warnings, images) = load_rest(
        &text,
        config,
        Some(PROPS),
        Some(&scene),
        Some(NO_RULES),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &verdicts,
        None,
        false,
    )
    .unwrap_or_else(|e| panic!("{e:?}"));
    let _ = warnings;
    let packed = atlas::pack(&[AtlasImage {
        width: 4,
        height: 4,
        pixels: vec![255; 64],
    }])
    .expect("влезает");
    let camera = game.camera_3d(WINDOW).expect("камера");
    let frame = compose_frame3d(&game, &camera, 0.0, &images, &packed.rects);

    // Фигуры: место, размер, поворот, цвет.
    assert_eq!(frame.shapes.len(), 2);
    let tower = frame
        .shapes
        .iter()
        .find(|s| s.shape == Shape::Cylinder)
        .expect("цилиндр");
    assert_eq!(tower.center, [9.0, 6.5]);
    assert_eq!(tower.size, [2.0, 1.0]);
    assert_eq!(tower.height, 3.0);
    assert!(
        (tower.sin - 0.5).abs() < 1e-6 && (tower.cos - 30.0_f32.to_radians().cos()).abs() < 1e-6
    );
    assert_eq!(tower.color, [0.0, 1.0, 0.0]);
    let hero = frame
        .shapes
        .iter()
        .find(|s| s.shape == Shape::Capsule)
        .expect("капсула");
    assert!((hero.cap_height - 0.3).abs() < 1e-6);

    // Земля на плоскости, затем плоские объекты по `layer`: `under` (слой 1) под `top` (слой 2).
    let tiles = frame.ground.len() - 2;
    assert!(tiles > 0 && tiles <= 400, "{tiles}");
    let (under, top) = (&frame.ground[tiles], &frame.ground[tiles + 1]);
    assert_eq!(under.color, [0.0, 0.0, 1.0, 1.0]);
    assert_eq!(top.color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(under.turn.sin, 0.0);
    assert_eq!(
        (top.turn.sin, top.turn.cos),
        (1.0, 0.0),
        "поворот целого прямоугольника на земле"
    );
    assert_eq!(top.turn.pivot, [5.5, 4.5], "вокруг середины объекта");
    assert!(
        frame.ground[..tiles]
            .iter()
            .all(|p| p.size == [1.0, 1.0] && p.turn.sin == 0.0)
    );
    // Камера и солнце.
    assert!(frame.view_proj.iter().flatten().all(|v| v.is_finite()));
    assert!((frame.sun[3] - 50.0_f32.to_radians().sin()).abs() < 1e-6);
    assert!((frame.shadow - 0.4).abs() < 1e-6);
    assert!(frame.depth_per_cell > 0.0);
}

// -------------------------------------------------------------------------------------------
// Сетки фигур и шейдеры
// -------------------------------------------------------------------------------------------

#[test]
fn every_mesh_stays_inside_the_unit_volume_touches_every_face_and_has_unit_normals() {
    for shape in Shape::ALL {
        let mesh = unit_mesh(shape);
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for vertex in &mesh.vertices {
            for axis in 0..3 {
                low[axis] = low[axis].min(vertex.position[axis]);
                high[axis] = high[axis].max(vertex.position[axis]);
            }
            let length = vertex.normal.iter().map(|c| c * c).sum::<f32>().sqrt();
            assert!((length - 1.0).abs() < 1e-5, "{shape:?}");
        }
        for axis in 0..2 {
            assert!(
                (low[axis] + 0.5).abs() < 1e-5 && (high[axis] - 0.5).abs() < 1e-5,
                "{shape:?}"
            );
        }
        assert!(
            low[2].abs() < 1e-6 && (high[2] - 1.0).abs() < 1e-6,
            "{shape:?}"
        );
    }
    assert_eq!(
        unit_mesh(Shape::Cylinder).vertices.len() % 24,
        0,
        "24 деления по кругу"
    );
}

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

/// Как `wgpu-hal` собирает GLSL для WebGL2: биндинг → слот из раскладки группы, `es 300`.
fn to_webgl2_glsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry_point: &str,
    bindings: &[u32],
) -> String {
    use naga::back::glsl;
    let mut binding_map = glsl::BindingMap::default();
    let mut slots = [0u8; 3];
    for &binding in bindings {
        let kind = match binding {
            0 => 0,
            1 | 3 => 1,
            _ => 2,
        };
        binding_map.insert(naga::ResourceBinding { group: 0, binding }, slots[kind]);
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
fn the_three_dimensional_shaders_validate_and_translate_to_glsl_es_300() {
    let (module, info) = parse_and_validate(include_str!("../shaders/scene3d.wgsl"));
    let all = [0, 1, 2, 3, 4];
    for (stage, entry, bindings) in [
        (naga::ShaderStage::Vertex, "vs_shadow", &[0][..]),
        (naga::ShaderStage::Vertex, "vs_shape", &[0][..]),
        (naga::ShaderStage::Fragment, "fs_shape", &all[..]),
        (naga::ShaderStage::Vertex, "vs_ground", &[0][..]),
        (naga::ShaderStage::Fragment, "fs_ground", &all[..]),
    ] {
        let glsl = to_webgl2_glsl(&module, &info, stage, entry, bindings);
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
    }
    // Одна текстура — одна выборка: тень читается только сравнением, атлас — массивом листов.
    let fragment = to_webgl2_glsl(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs_ground",
        &all,
    );
    assert!(
        fragment.contains("sampler2DShadow"),
        "карта теней — выборка сравнением"
    );
    assert!(fragment.contains("sampler2DArray"), "атлас — массив листов");
}

#[test]
fn the_flat_rectangle_shader_still_translates_to_glsl_es_300() {
    let (module, info) = parse_and_validate(include_str!("../shaders/rect.wgsl"));
    for (stage, entry) in [
        (naga::ShaderStage::Vertex, "vs_main"),
        (naga::ShaderStage::Fragment, "fs_main"),
    ] {
        to_webgl2_glsl(&module, &info, stage, entry, &[0, 1, 2]);
    }
}

#[test]
fn the_game_hands_its_camera_out_with_the_scenes_own_pitch_and_view_height() {
    let game = load3d(CLICK_SCENE, NO_RULES);
    let camera: Camera3d = game.camera_3d(WINDOW).unwrap();
    assert_eq!((camera.pitch, camera.view_height), (55.0, 12.0));
}

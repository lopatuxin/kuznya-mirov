//! Фаза 37 — бег и прыжок (требования 1–21). Каждый тест собирает свою маленькую плоскую игру в
//! коде, как `tests/phase11_camera_click_walk.rs`; клавиши героя ставит прямо признаками `hold_*`,
//! что и делают привязки на этапе 2 — привязки проверены отдельным тестом.

use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::property;
use engine::core::report::RuleFired;
use engine::core::run_jump::{self, JumpState, OutlineKind};
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{load_game_from_texts, load_game_from_texts_with_code};
use engine::data::session::PlaySession;

const HERO: u32 = 0;
const REST_Y: f64 = 8.1;
const VIEWPORT: [f32; 2] = [800.0, 600.0];

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"ground":"flag","ledge":"flag","hero":"flag","watcher":"flag","count":"number"}}"#;
const RUN_JUMP: &str = r#"{"kind":"run_jump","for":{"has":["hero"]},"solid":{"has":["ground"]},"one_way":{"has":["ledge"]}}"#;
const HERO_OBJECT: &str = r#"{"name":"hero","position":[3,8.1],"size":[0.8,1.9],"velocity":[0,0],"hero":true,"run_speed":7,"jump_height":3}"#;
const GROUND: &str = r#"{"name":"ground","position":[0,10],"size":[60,2],"ground":true}"#;

fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":60,"height":60,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

fn scene_of(objects: &[&str]) -> String {
    format!(r#"{{"objects":[{}]}}"#, objects.join(","))
}

fn rules_of(rules: &[&str]) -> String {
    format!(r#"{{"rules":[{}]}}"#, rules.join(","))
}

type Loaded = Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_texts(scene_extra: &str, props: &str, scene: &str, rules: &str) -> Loaded {
    load_game_from_texts(&game_json(scene_extra), props, scene, rules, SCREENS)
}

fn level(objects: &[&str]) -> Game {
    let (game, _screens, _warnings) =
        load_texts("", PROPS, &scene_of(objects), &rules_of(&[RUN_JUMP]))
            .expect("игра должна загрузиться");
    game
}

fn on_ground_level() -> Game {
    level(&[HERO_OBJECT, GROUND])
}

fn errors_of(result: Loaded) -> Vec<String> {
    match result {
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} ({})", e.message, e.path))
            .collect(),
        Ok(_) => Vec::new(),
    }
}

fn step(game: &mut Game) {
    game.step(StepInput::empty());
}

fn steps(game: &mut Game, count: usize) {
    for _ in 0..count {
        step(game);
    }
}

fn hold(game: &mut Game, prop: property::PropertyId, on: bool) {
    game.world.set_flag(HERO, prop, on);
}

fn pos(game: &Game) -> [f64; 2] {
    game.world.vec2(HERO, property::POSITION).expect("position")
}

fn vel(game: &Game) -> [f64; 2] {
    game.world.vec2(HERO, property::VELOCITY).expect("velocity")
}

fn grounded(game: &Game) -> bool {
    game.world.flag(HERO, property::ON_GROUND)
}

fn near(actual: f64, expected: f64, eps: f64) {
    assert!(
        (actual - expected).abs() <= eps,
        "ожидалось {expected} ± {eps}, вышло {actual}"
    );
}

/// Герой стоит: один пробный шаг, чтобы движок поставил `on_ground`.
fn settled(objects: &[&str]) -> Game {
    let mut game = level(objects);
    step(&mut game);
    assert!(grounded(&game));
    game
}

/// Прыжок с зажатой клавишей: высота верхней точки над землёй и число шагов до приземления.
fn full_jump() -> (f64, usize) {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    hold(&mut game, property::HOLD_JUMP, true);
    let mut top = REST_Y;
    for count in 1..=200 {
        step(&mut game);
        top = top.min(pos(&game)[1]);
        if count > 1 && grounded(&game) {
            return (REST_Y - top, count);
        }
    }
    panic!("герой не приземлился");
}

#[test]
fn an_object_on_solid_without_keys_stands_still_step_after_step() {
    let mut game = on_ground_level();
    for _ in 0..30 {
        step(&mut game);
        near(pos(&game)[1], REST_Y, 1e-9);
        near(pos(&game)[0], 3.0, 1e-9);
        assert!(grounded(&game));
        assert_eq!(vel(&game)[1], 0.0);
    }
}

#[test]
fn a_standing_object_is_not_reported_as_moved() {
    let mut game = on_ground_level();
    step(&mut game);
    game.begin_session();
    step(&mut game);
    let report = game.last_report().expect("отчёт шага");
    assert!(
        report
            .fired
            .iter()
            .all(|fired| !matches!(fired, RuleFired::RunJump { .. })),
        "{:?}",
        report.fired
    );
}

#[test]
fn running_speeds_up_in_six_steps_on_the_ground_and_stops_in_six() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 5);
    assert!(vel(&game)[0] < 7.0 - 0.5, "{:?}", vel(&game));
    step(&mut game);
    near(vel(&game)[0], 7.0, 1e-6);
    hold(&mut game, property::HOLD_RIGHT, false);
    steps(&mut game, 5);
    assert!(vel(&game)[0] > 0.5, "{:?}", vel(&game));
    step(&mut game);
    near(vel(&game)[0], 0.0, 1e-6);
}

#[test]
fn running_speeds_up_in_nine_steps_in_the_air() {
    let flying = r#"{"name":"hero","position":[3,2],"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":3}"#;
    let mut game = level(&[flying, GROUND]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 8);
    assert!(vel(&game)[0] < 7.0 - 0.3, "{:?}", vel(&game));
    assert!(!grounded(&game));
    step(&mut game);
    near(vel(&game)[0], 7.0, 1e-6);
}

#[test]
fn both_arrows_held_brake_the_object_to_zero() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 10);
    hold(&mut game, property::HOLD_LEFT, true);
    steps(&mut game, 10);
    near(vel(&game)[0], 0.0, 1e-6);
}

#[test]
fn a_run_speed_of_zero_only_jumps_in_place() {
    let still = r#"{"name":"hero","position":[3,8.1],"size":[0.8,1.9],"hero":true,"run_speed":0,"jump_height":3}"#;
    let mut game = settled(&[still, GROUND]);
    hold(&mut game, property::HOLD_RIGHT, true);
    hold(&mut game, property::HOLD_JUMP, true);
    steps(&mut game, 20);
    near(pos(&game)[0], 3.0, 1e-9);
    assert!(pos(&game)[1] < REST_Y - 1.0);
}

#[test]
fn a_held_jump_reaches_the_documented_height_and_lands_in_a_second() {
    let (height, length) = full_jump();
    near(height, 3.19, 0.15);
    assert!((58..=62).contains(&length), "{length}");
}

#[test]
fn a_jump_released_after_six_steps_is_low() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    hold(&mut game, property::HOLD_JUMP, true);
    steps(&mut game, 6);
    hold(&mut game, property::HOLD_JUMP, false);
    let mut top = pos(&game)[1];
    for _ in 0..100 {
        step(&mut game);
        top = top.min(pos(&game)[1]);
    }
    assert!(REST_Y - top < 1.8, "{}", REST_Y - top);
    assert!(REST_Y - top > 1.0, "{}", REST_Y - top);
}

#[test]
fn a_press_just_before_landing_jumps_on_touch_and_a_much_earlier_one_does_not() {
    let falling = r#"{"name":"hero","position":[3,3],"size":[0.8,1.9],"velocity":[0,0],"hero":true,"run_speed":7,"jump_height":3}"#;
    let mut probe = level(&[falling, GROUND]);
    let mut touch = 0;
    while !grounded(&probe) {
        step(&mut probe);
        touch += 1;
    }
    for (early, jumps) in [(3, true), (8, false)] {
        let mut game = level(&[falling, GROUND]);
        steps(&mut game, touch - early);
        hold(&mut game, property::HOLD_JUMP, true);
        steps(&mut game, early);
        near(pos(&game)[1], REST_Y, 1e-9);
        assert_eq!(
            vel(&game)[1] < 0.0,
            jumps,
            "нажатие за {early} шагов: прыжок в шаг касания {touch}"
        );
        assert_eq!(grounded(&game), !jumps, "оторвался — на земле не стоит");
        if !jumps {
            steps(&mut game, 10);
            assert!(grounded(&game));
        }
    }
}

/// Земля кончается на `x = 10`, герой сходит с края и через `after` шагов нажимает прыжок.
fn jump_after_leaving_the_edge(after: usize) -> bool {
    let ledge_end = r#"{"name":"ground","position":[0,10],"size":[10,2],"ground":true}"#;
    let on_the_edge = r#"{"name":"hero","position":[9.5,8.1],"size":[0.8,1.9],"velocity":[0,0],"hero":true,"run_speed":7,"jump_height":3}"#;
    let mut game = level(&[on_the_edge, ledge_end]);
    step(&mut game);
    hold(&mut game, property::HOLD_RIGHT, true);
    let mut leaving = 0;
    while grounded(&game) {
        step(&mut game);
        leaving += 1;
        assert!(leaving < 60, "герой не сошёл с края");
    }
    hold(&mut game, property::HOLD_RIGHT, false);
    steps(&mut game, after - 1);
    hold(&mut game, property::HOLD_JUMP, true);
    step(&mut game);
    vel(&game)[1] < 0.0
}

#[test]
fn a_jump_is_allowed_a_moment_after_leaving_the_edge_and_not_long_after() {
    assert!(jump_after_leaving_the_edge(5));
    assert!(!jump_after_leaving_the_edge(9));
}

#[test]
fn a_held_jump_does_not_repeat_after_landing() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    hold(&mut game, property::HOLD_JUMP, true);
    let mut landed = None;
    for count in 1..=200 {
        step(&mut game);
        if count > 1 && grounded(&game) {
            landed = Some(count);
            break;
        }
    }
    assert!(landed.is_some());
    for _ in 0..30 {
        step(&mut game);
        assert!(grounded(&game));
        near(pos(&game)[1], REST_Y, 1e-9);
    }
    hold(&mut game, property::HOLD_JUMP, false);
    step(&mut game);
    hold(&mut game, property::HOLD_JUMP, true);
    step(&mut game);
    assert!(vel(&game)[1] < 0.0, "новое нажатие прыгает");
}

#[test]
fn a_wall_stops_the_run_and_the_object_stands_flush_against_it() {
    let wall = r#"{"name":"wall","position":[6,4],"size":[1,6],"ground":true}"#;
    let mut game = settled(&[HERO_OBJECT, GROUND, wall]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 60);
    near(pos(&game)[0], 6.0 - 0.8, 1e-6);
    assert_eq!(vel(&game)[0], 0.0);
    assert!(grounded(&game));
}

#[test]
fn running_into_a_wall_in_a_jump_slides_down_it() {
    let wall = r#"{"name":"wall","position":[6,0],"size":[1,10],"ground":true}"#;
    let mut game = settled(&[HERO_OBJECT, GROUND, wall]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 30);
    hold(&mut game, property::HOLD_JUMP, true);
    steps(&mut game, 8);
    let rising = pos(&game)[1];
    assert!(rising < REST_Y - 0.5);
    near(pos(&game)[0], 6.0 - 0.8, 1e-6);
    assert_eq!(vel(&game)[0], 0.0);
    steps(&mut game, 80);
    near(pos(&game)[0], 6.0 - 0.8, 1e-6);
    near(pos(&game)[1], REST_Y, 1e-6);
}

#[test]
fn a_low_ceiling_stops_the_rise_and_the_object_falls() {
    let ceiling = r#"{"name":"ceiling","position":[0,5],"size":[60,1],"ground":true}"#;
    let mut game = settled(&[HERO_OBJECT, GROUND, ceiling]);
    hold(&mut game, property::HOLD_JUMP, true);
    let mut bumped = false;
    for _ in 0..40 {
        let before = pos(&game)[1];
        step(&mut game);
        if pos(&game)[1] <= 6.0 + 1e-6 && !bumped {
            bumped = true;
            assert_eq!(vel(&game)[1], 0.0);
            assert!(!grounded(&game));
        } else if bumped {
            assert!(pos(&game)[1] >= before - 1e-9, "после удара объект падает");
        }
        assert!(pos(&game)[1] >= 6.0 - 1e-6);
    }
    assert!(bumped);
}

#[test]
fn a_one_way_platform_is_passed_from_below_and_holds_from_above() {
    let ledge = r#"{"name":"ledge","position":[2,7.5],"size":[4,0.5],"ledge":true}"#;
    let mut game = settled(&[HERO_OBJECT, GROUND, ledge]);
    hold(&mut game, property::HOLD_JUMP, true);
    let mut passed_through = false;
    for _ in 0..120 {
        step(&mut game);
        let top = pos(&game)[1];
        passed_through |= top + 1.9 > 7.5 && top < 8.0 && !grounded(&game);
        if grounded(&game) && pos(&game)[1] < REST_Y - 1.0 {
            near(pos(&game)[1], 7.5 - 1.9, 1e-6);
            assert!(passed_through);
            return;
        }
    }
    panic!("герой не встал на площадку");
}

#[test]
fn a_one_way_platform_lets_the_object_pass_from_the_side() {
    let ledge = r#"{"name":"ledge","position":[6,8.5],"size":[2,0.5],"ledge":true}"#;
    let mut game = settled(&[HERO_OBJECT, GROUND, ledge]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 60);
    assert!(pos(&game)[0] > 8.0, "{:?}", pos(&game));
    near(pos(&game)[1], REST_Y, 1e-6);
}

#[test]
fn a_thin_solid_is_not_skipped_by_a_fast_fall() {
    let thin = r#"{"name":"thin","position":[0,10],"size":[60,0.1],"ground":true}"#;
    let fast = r#"{"name":"hero","position":[3,0],"size":[0.8,1.9],"velocity":[0,22.5],"hero":true,"run_speed":7,"jump_height":3}"#;
    let mut game = level(&[fast, thin]);
    steps(&mut game, 100);
    near(pos(&game)[1], 10.0 - 1.9, 1e-6);
    assert!(grounded(&game));
}

#[test]
fn an_object_placed_inside_a_solid_walks_out_and_is_not_stuck() {
    let inside = r#"{"name":"hero","position":[3,9.5],"size":[0.8,1.9],"velocity":[0,0],"hero":true,"run_speed":7,"jump_height":3}"#;
    let mut game = level(&[inside, GROUND]);
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 60);
    assert!(pos(&game)[0] > 8.0, "{:?}", pos(&game));
    assert!(pos(&game)[1] > 12.0 - 1e-6, "{:?}", pos(&game));
}

#[test]
fn a_solid_deleted_during_the_game_stops_holding_on_the_next_step() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    game.world.delete(1);
    step(&mut game);
    assert!(!grounded(&game));
    assert!(pos(&game)[1] > REST_Y);
}

#[test]
fn a_fresh_object_in_a_reused_slot_remembers_nothing() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    hold(&mut game, property::HOLD_JUMP, true);
    step(&mut game);
    assert_ne!(game.world.jump_state(HERO), JumpState::default());
    game.world.delete(HERO);
    let reused = game.world.create();
    assert_eq!(reused, HERO);
    assert_eq!(game.world.jump_state(reused), JumpState::default());
}

#[test]
fn after_move_of_sees_the_shift_of_run_jump() {
    let watcher = r#"{"name":"watcher","watcher":true,"count":0}"#;
    let watch = r#"{"kind":"check","for":{"has":["watcher"]},"when":{"after_move_of":{"has":["hero"]}},"do":[["add","count",1]]}"#;
    let (mut game, _screens, _warnings) = load_texts(
        "",
        PROPS,
        &scene_of(&[HERO_OBJECT, GROUND, watcher]),
        &rules_of(&[RUN_JUMP, watch]),
    )
    .expect("игра должна загрузиться");
    let count = |game: &Game| {
        game.world
            .number_like(2, game.properties.resolve("count").expect("count"))
            .expect("count задан")
    };
    steps(&mut game, 10);
    assert_eq!(count(&game), 0.0, "стоящий не сдвигается");
    hold(&mut game, property::HOLD_RIGHT, true);
    steps(&mut game, 10);
    assert_eq!(count(&game), 10.0);
}

#[test]
fn the_report_lists_the_objects_run_jump_moved() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    game.begin_session();
    hold(&mut game, property::HOLD_RIGHT, true);
    step(&mut game);
    let report = game.last_report().expect("отчёт шага");
    assert!(
        report.fired.iter().any(|fired| matches!(
            fired,
            RuleFired::RunJump { rule, objects } if rule == "rules[0]" && objects == &[HERO]
        )),
        "{:?}",
        report.fired
    );
}

#[test]
fn code_writing_velocity_pushes_the_object_and_the_rule_continues_from_it() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    game.world.set_vec2(HERO, property::VELOCITY, [0.0, -10.0]);
    step(&mut game);
    assert!(pos(&game)[1] < REST_Y);
    assert!(!grounded(&game));
}

#[test]
fn changed_jump_numbers_act_from_the_next_step() {
    let mut game = settled(&[HERO_OBJECT, GROUND]);
    game.world.set_number(HERO, property::JUMP_HEIGHT, 6.0);
    hold(&mut game, property::HOLD_JUMP, true);
    step(&mut game);
    near(vel(&game)[1], -2.0 * 6.0 / 0.4, 1e-9);
}

#[test]
fn a_game_with_run_jump_loads_without_warnings_and_on_ground_is_a_known_selector() {
    let probe = r#"{"kind":"check","for":{"has":["hero","on_ground"]},"do":[["add","count",1]]}"#;
    let counter = r#"{"name":"counter","position":[50,50],"size":[1,1],"hero":true,"count":0,"run_speed":1,"jump_height":1}"#;
    let (_game, _screens, warnings) = load_texts(
        "",
        r#"{"properties":{"ground":"flag","ledge":"flag","hero":"flag","count":"number"}}"#,
        &scene_of(&[HERO_OBJECT, GROUND, counter]),
        &rules_of(&[RUN_JUMP, probe]),
    )
    .expect("игра должна загрузиться");
    let texts: Vec<_> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(texts.iter().all(|t| !t.contains("on_ground")), "{texts:?}");
}

fn load_with_scene_objects(objects: &[&str]) -> Loaded {
    load_texts("", PROPS, &scene_of(objects), &rules_of(&[RUN_JUMP]))
}

fn assert_error(result: Loaded, fragment: &str) {
    let errors = errors_of(result);
    assert!(
        errors.iter().any(|e| e.contains(fragment)),
        "ожидалась ошибка с «{fragment}», вышло {errors:?}"
    );
}

#[test]
fn an_object_under_run_jump_needs_position_size_run_speed_and_jump_height() {
    for missing in ["position", "size", "run_speed", "jump_height"] {
        let hero = match missing {
            "position" => r#"{"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":3}"#,
            "size" => r#"{"position":[3,8],"hero":true,"run_speed":7,"jump_height":3}"#,
            "run_speed" => r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"jump_height":3}"#,
            _ => r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":7}"#,
        };
        assert_error(
            load_with_scene_objects(&[hero, GROUND]),
            &format!("не хватает свойства \"{missing}\""),
        );
    }
}

#[test]
fn run_speed_jump_height_and_jump_time_must_be_in_range() {
    let hero = |extra: &str| {
        format!(
            r#"{{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":3 {extra}}}"#
        )
    };
    let bad = [
        (
            r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":-1,"jump_height":3}"#
                .to_string(),
            "свойство \"run_speed\" не может быть меньше нуля",
        ),
        (
            r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":"fast","jump_height":3}"#
                .to_string(),
            "ожидалось число",
        ),
        (
            r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":0}"#
                .to_string(),
            "свойство \"jump_height\" должно быть больше нуля",
        ),
        (
            hero(r#","jump_time":0"#),
            "свойство \"jump_time\" должно быть больше нуля",
        ),
        (
            hero(r#","jump_time":-0.4"#),
            "свойство \"jump_time\" должно быть больше нуля",
        ),
        (hero(r#","jump_time":"slow""#), "ожидалось число"),
    ];
    for (object, fragment) in bad {
        assert_error(load_with_scene_objects(&[&object, GROUND]), fragment);
    }
    let ok = hero(r#","jump_time":0.5"#);
    assert!(load_with_scene_objects(&[&ok, GROUND]).is_ok());
}

#[test]
fn run_jump_in_a_3d_scene_is_an_error() {
    let hero = r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":3}"#;
    let result = load_texts(
        r#","view_height":12,"camera":{"pitch":55}"#,
        PROPS,
        &scene_of(&[hero, GROUND]),
        &rules_of(&[RUN_JUMP]),
    );
    assert_error(result, "run_jump есть только в плоской сцене");
}

#[test]
fn an_object_under_both_move_and_run_jump_is_an_error() {
    let result = load_texts(
        "",
        PROPS,
        &scene_of(&[HERO_OBJECT, GROUND]),
        &rules_of(&[
            RUN_JUMP,
            r#"{"kind":"move","for":{"has":["position","velocity"]}}"#,
        ]),
    );
    assert_error(
        result,
        "попал и под move (rules[1]), и под run_jump (rules[0])",
    );
    let apart = load_texts(
        "",
        PROPS,
        &scene_of(&[HERO_OBJECT, GROUND]),
        &rules_of(&[
            RUN_JUMP,
            r#"{"kind":"move","for":{"has":["velocity"],"without":["hero"]}}"#,
        ]),
    );
    assert!(apart.is_ok());
}

#[test]
fn on_ground_cannot_be_written_by_data() {
    let with_on_ground = r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":3,"on_ground":true}"#;
    assert_error(
        load_with_scene_objects(&[with_on_ground, GROUND]),
        "признак \"on_ground\" пишет только движок",
    );
    let keyed = r#"{"position":[3,8],"size":[0.8,1.9],"hero":true,"run_speed":7,"jump_height":3,
        "keys":{"KeyG":{"press":[["on_ground",true]]}}}"#;
    assert_error(
        load_with_scene_objects(&[keyed, GROUND]),
        "признак \"on_ground\" пишет только движок",
    );
    for action in [
        r#"["set","on_ground",true]"#,
        r#"["give","on_ground",{"has":["hero"]}]"#,
        r#"["take","on_ground"]"#,
    ] {
        let rule = format!(r#"{{"kind":"check","for":{{"has":["hero"]}},"do":[{action}]}}"#);
        let result = load_texts(
            "",
            PROPS,
            &scene_of(&[HERO_OBJECT, GROUND]),
            &rules_of(&[RUN_JUMP, &rule]),
        );
        assert_error(result, "признак \"on_ground\" пишет только движок");
    }
    let effect = r#"{"kind":"collide","a":{"has":["hero"]},"b":{"has":["ground"]},"effects":{"a":[["give","on_ground"]]}}"#;
    let result = load_texts(
        "",
        PROPS,
        &scene_of(&[HERO_OBJECT, GROUND]),
        &rules_of(&[RUN_JUMP, effect]),
    );
    assert_error(result, "признак \"on_ground\" пишет только движок");
}

#[test]
fn solid_and_one_way_must_be_selectors() {
    for rule in [
        r#"{"kind":"run_jump","for":{"has":["hero"]},"solid":5}"#,
        r#"{"kind":"run_jump","for":{"has":["hero"]},"solid":["ground"]}"#,
        r#"{"kind":"run_jump","for":{"has":["hero"]},"one_way":"ledge"}"#,
    ] {
        let result = load_texts(
            "",
            PROPS,
            &scene_of(&[HERO_OBJECT, GROUND]),
            &rules_of(&[rule]),
        );
        assert!(!errors_of(result).is_empty(), "{rule}: ожидалась ошибка");
    }
}

#[test]
fn run_jump_works_without_solid_and_one_way() {
    let rule = r#"{"kind":"run_jump","for":{"has":["hero"]}}"#;
    let (mut game, _screens, _warnings) = load_texts(
        "",
        PROPS,
        &scene_of(&[HERO_OBJECT, GROUND]),
        &rules_of(&[rule]),
    )
    .expect("игра должна загрузиться");
    steps(&mut game, 120);
    assert!(
        pos(&game)[1] > 12.0,
        "без твёрдого герой падает: {:?}",
        pos(&game)
    );
}

#[test]
fn outlines_list_solid_one_way_and_other_objects_without_picture_or_color() {
    let ledge = r#"{"name":"ledge","position":[2,7.5],"size":[4,0.5],"ledge":true}"#;
    let marker = r#"{"name":"marker","position":[20,2],"size":[1,1]}"#;
    let both = r#"{"name":"both","position":[30,2],"size":[1,1],"ground":true,"ledge":true}"#;
    let painted =
        r##"{"name":"painted","position":[40,2],"size":[1,1],"ground":true,"color":"#ff0000"}"##;
    let no_size = r#"{"name":"state","ground":true}"#;
    let game = level(&[HERO_OBJECT, GROUND, ledge, marker, both, painted, no_size]);
    let outlines = run_jump::outlines(&game.world, &game.rules);
    assert_eq!(
        outlines,
        vec![
            (0, OutlineKind::Other),
            (1, OutlineKind::Solid),
            (2, OutlineKind::OneWay),
            (3, OutlineKind::Other),
            (4, OutlineKind::Solid),
        ]
    );
}

#[test]
fn outlines_follow_the_current_world() {
    let mut game = level(&[HERO_OBJECT, GROUND]);
    assert_eq!(run_jump::outlines(&game.world, &game.rules).len(), 2);
    game.world.delete(1);
    assert_eq!(
        run_jump::outlines(&game.world, &game.rules),
        vec![(0, OutlineKind::Other)]
    );
}

#[test]
fn key_bindings_drive_the_hold_flags_and_the_run() {
    let keyed_hero = r#"{"name":"hero","position":[3,8.1],"size":[0.8,1.9],"velocity":[0,0],"hero":true,"run_speed":7,"jump_height":3,
        "keys":{"ArrowRight":{"press":[["hold_right",true]],"release":[["hold_right",false]]},
                "Space":{"press":[["hold_jump",true]],"release":[["hold_jump",false]]}}}"#;
    let mut game = level(&[keyed_hero, GROUND]);
    step(&mut game);
    game.key_down("ArrowRight");
    let input = game.take_input_snapshot();
    game.step(input);
    assert!(game.world.flag(HERO, property::HOLD_RIGHT));
    assert!(vel(&game)[0] > 0.0);
    game.key_up("ArrowRight");
    let input = game.take_input_snapshot();
    game.step(input);
    assert!(!game.world.flag(HERO, property::HOLD_RIGHT));
    game.key_down("Space");
    let input = game.take_input_snapshot();
    game.step(input);
    assert!(vel(&game)[1] < 0.0);
}

/// Партия с бегом и прыжками записывается и повторяется: положения совпадают шаг в шаг.
#[test]
fn a_recorded_run_and_jump_replays_step_for_step() {
    let keyed_hero = r#"{"name":"hero","position":[3,8.1],"size":[0.8,1.9],"velocity":[0,0],"hero":true,"run_speed":7,"jump_height":3,
        "keys":{"ArrowRight":{"press":[["hold_right",true]],"release":[["hold_right",false]]},
                "Space":{"press":[["hold_jump",true]],"release":[["hold_jump",false]]}}}"#;
    let wall = r#"{"name":"wall","position":[20,4],"size":[1,6],"ground":true}"#;
    let ledge = r#"{"name":"ledge","position":[8,7.5],"size":[4,0.5],"ledge":true}"#;
    let load = || {
        let (game, config, _warnings) = load_texts(
            "",
            PROPS,
            &scene_of(&[keyed_hero, GROUND, wall, ledge]),
            &rules_of(&[RUN_JUMP]),
        )
        .expect("игра должна загрузиться");
        (game, config)
    };
    let script: [(usize, &str, bool); 7] = [
        (5, "ArrowRight", true),
        (30, "Space", true),
        (36, "Space", false),
        (50, "Space", true),
        (60, "Space", false),
        (90, "ArrowRight", false),
        (110, "Space", true),
    ];

    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let mut recorded = Vec::new();
    for tick in 0..160 {
        let mut queue = UiQueue::new();
        for (at, code, down) in script {
            if at == tick {
                if down {
                    queue.push_key_down(code);
                } else {
                    queue.push_key_up(code);
                }
            }
        }
        let mut mouse = MouseState::default();
        session.step_once(
            &mut queue,
            &mut mouse,
            &mut game,
            &config,
            &mut state,
            VIEWPORT,
            &[],
        );
        recorded.push((pos(&game), vel(&game), grounded(&game)));
    }
    let text = session.recording_text(&game);
    assert!(recorded.iter().any(|(_, _, on)| *on));
    assert!(recorded.iter().any(|(_, _, on)| !*on));

    let (mut game2, config2) = load();
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2)
        .expect("запись должна разобраться");
    for (tick, expected) in recorded.iter().enumerate() {
        let mut queue = UiQueue::new();
        let mut mouse = MouseState::default();
        replay.step_once(
            &mut queue,
            &mut mouse,
            &mut game2,
            &config2,
            &mut state2,
            VIEWPORT,
            &[],
        );
        assert_eq!(
            &(pos(&game2), vel(&game2), grounded(&game2)),
            expected,
            "шаг {tick}"
        );
    }
}

/// Игра с кодом: правило `check` вызывает `act(obj)` для героя на каждом шаге после `run_jump`.
fn code_game(body: &str) -> Game {
    let game_json = game_json("").replace(r#""fonts":{}"#, r#""fonts":{},"code":"code.lua""#);
    let code = format!(
        "function act(obj)
{body}
end
"
    );
    let act = r#"{"kind":"check","for":{"has":["hero"]},"do":[["run","act"]]}"#;
    let (game, _screens, _warnings) = load_game_from_texts_with_code(
        &game_json,
        PROPS,
        &scene_of(&[HERO_OBJECT, GROUND]),
        &rules_of(&[RUN_JUMP, act]),
        SCREENS,
        Some(&code),
    )
    .expect("игра должна загрузиться");
    game
}

fn code_error_of(body: &str) -> String {
    let mut game = code_game(body);
    step(&mut game);
    game.code_error()
        .map(|error| error.message.clone())
        .unwrap_or_default()
}

#[test]
fn code_cannot_write_on_ground() {
    for body in [
        "obj.on_ground = true",
        "obj.on_ground = false",
        "obj.on_ground = nil",
    ] {
        let message = code_error_of(body);
        assert!(
            message.contains("признак \"on_ground\" пишет только движок"),
            "{body}: {message:?}"
        );
    }
}

#[test]
fn code_reads_on_ground_and_writes_the_other_run_and_jump_properties() {
    let mut game = code_game(
        "obj.run_speed = obj.on_ground and 4 or 5
         obj.jump_height = 2
         obj.jump_time = 0.3
         obj.hold_right = true
         obj.hold_left = false
         obj.hold_jump = false",
    );
    step(&mut game);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let number = |prop| game.world.number_like(HERO, prop);
    assert_eq!(number(property::RUN_SPEED), Some(4.0));
    assert_eq!(number(property::JUMP_HEIGHT), Some(2.0));
    assert_eq!(number(property::JUMP_TIME), Some(0.3));
    assert!(game.world.flag(HERO, property::HOLD_RIGHT));
}

#[test]
fn code_cannot_give_the_hero_numbers_the_rule_cannot_use() {
    for (body, fragment) in [
        ("obj.run_speed = -1", "run_speed"),
        ("obj.jump_height = 0", "jump_height"),
        ("obj.jump_time = 0", "jump_time"),
    ] {
        let message = code_error_of(body);
        assert!(message.contains(fragment), "{body}: {message:?}");
    }
}

#[test]
fn code_writing_velocity_knocks_the_object_and_the_rule_continues_from_it() {
    let mut game = code_game(
        "if not obj.on_ground then return end
obj.velocity = {x = 12, y = 0}",
    );
    steps(&mut game, 2);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    step(&mut game);
    assert!(vel(&game)[0] > 0.0, "{:?}", vel(&game));
    assert!(pos(&game)[0] > 3.0);
}

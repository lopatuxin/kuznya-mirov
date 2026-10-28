//! Фаза 39 — камера игрока (требования 1–20, 23–24). Страница и редактор шлют ядру колесо, кнопки и
//! клавиши вызовами `Game::camera_*`, так их здесь и зовут; каждый тест собирает свою игру в коде, как
//! `tests/phase11_camera_click_walk.rs`.

use engine::core::camera::{self, WHEEL_ZOOM_FACTOR};
use engine::core::game::{Game, PlayerView};
use engine::core::input::{MouseState, UiQueue};
use engine::core::property;
use engine::core::runner::Runner;
use engine::core::scene::{CameraConfig, Projection, pointer_hit};
use engine::core::screens::{self, ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::load_game_from_texts;
use engine::data::recording::{self, ReplayEventKind};
use engine::data::session::PlaySession;

const VIEWPORT: [f32; 2] = [1000.0, 600.0];
const CENTER_POINTER: [f32; 2] = [500.0, 300.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str =
    r#"{"properties":{"hero":"flag","picked":"flag","tile":"flag","index":"number"}}"#;

const HERO: &str = r##"{"name":"hero","position":[10,10],"size":[1,1],"color":"#ff0000","hero":true,"velocity":[0,0]}"##;
const FOLLOWED_HERO: &str = r##"{"name":"hero","position":[10,10],"size":[1,1],"color":"#ff0000","hero":true,"camera_follows":true,"velocity":[0,0]}"##;

fn game_json(width: u32, height: u32, scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

fn scene_of(objects: &[&str]) -> String {
    format!(r#"{{"objects":[{}]}}"#, objects.join(","))
}

type Loaded = Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_with(game: &str, scene: &str, rules: &str, screens: &str) -> Loaded {
    load_game_from_texts(game, PROPS, scene, rules, screens)
}

const MOVE_HERO: &str = r#"{"rules":[{"kind":"move","for":{"has":["velocity"]}}]}"#;

/// Плоская сцена 60 × 30: вид 10, камера игрока с пределами 4–30.
fn flat(start: &str, objects: &[&str]) -> Game {
    let extra = format!(r#","view_height":10,"player_camera":{{"zoom":[4,30]{start}}}"#);
    let (game, _screens, _warnings) = load_with(
        &game_json(60, 30, &extra),
        &scene_of(objects),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра должна загрузиться");
    game
}

fn flat_started(objects: &[&str]) -> Game {
    flat(r#","start":[30,15]"#, objects)
}

/// Трёхмерная сцена 80 × 60: вид 20, наклон 60.
fn space(start: &str, objects: &[&str]) -> Game {
    let extra = format!(
        r#","view_height":20,"camera":{{"pitch":60}},"player_camera":{{"zoom":[6,40]{start}}}"#
    );
    let (game, _screens, _warnings) = load_with(
        &game_json(80, 60, &extra),
        &scene_of(objects),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра должна загрузиться");
    game
}

fn space_started(objects: &[&str]) -> Game {
    space(r#","start":[40,30]"#, objects)
}

fn near(actual: f64, expected: f64, eps: f64) {
    assert!(
        (actual - expected).abs() <= eps,
        "ожидалось {expected} ± {eps}, вышло {actual}"
    );
}

fn near_pair(actual: [f64; 2], expected: [f64; 2], eps: f64) {
    near(actual[0], expected[0], eps);
    near(actual[1], expected[1], eps);
}

/// Точка плоской сцены под точкой окна.
fn flat_point_under(game: &Game, pointer: [f32; 2]) -> [f64; 2] {
    let (scale, offset) = game.camera_frame(VIEWPORT);
    game.scene.window_to_scene_frame(pointer, scale, offset)
}

/// Место на сцене с высотой под точкой окна в трёхмерной игре.
fn space_point_under(game: &Game, pointer: [f32; 2]) -> [f64; 3] {
    let camera = game.camera_3d(VIEWPORT).expect("трёхмерная камера");
    pointer_hit(
        &game.world,
        &game.scene,
        &camera,
        [pointer[0] as f64, pointer[1] as f64],
    )
    .expect("под указателем есть земля")
}

/// Середина окна плоской камеры в клетках сцены.
fn flat_center(game: &Game) -> [f64; 2] {
    flat_point_under(game, CENTER_POINTER)
}

fn space_center(game: &Game) -> [f64; 2] {
    game.camera_3d(VIEWPORT).expect("камера").target
}

fn step(game: &mut Game, count: usize) {
    for _ in 0..count {
        let input = game.take_input_snapshot();
        game.step(input);
    }
}

#[test]
fn the_view_starts_at_view_height_on_the_start_point() {
    let game = flat_started(&[HERO]);
    near(game.player_view_height().expect("камера игрока"), 10.0, 0.0);
    assert_eq!(game.player_camera_follows(), Some(false));
    near_pair(flat_center(&game), [30.0, 15.0], 1e-3);
}

#[test]
fn without_start_and_a_followed_object_the_camera_looks_at_the_middle_of_the_scene() {
    let game = flat("", &[HERO]);
    near_pair(flat_center(&game), [30.0, 15.0], 1e-3);
}

#[test]
fn a_followed_object_wins_over_the_start_point() {
    let game = flat_started(&[FOLLOWED_HERO]);
    assert_eq!(game.player_camera_follows(), Some(true));
    near_pair(flat_center(&game), [10.5, 10.5], 1e-3);
}

#[test]
fn a_game_without_the_field_has_no_player_camera() {
    let (mut game, _screens, _warnings) = load_with(
        &game_json(60, 30, r#","view_height":10"#),
        &scene_of(&[HERO]),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра должна загрузиться");
    assert_eq!(game.player_view_height(), None);
    assert!(!game.camera_wheel(-100.0, CENTER_POINTER, VIEWPORT));
    assert!(!game.camera_key_down("KeyW"));
    assert!(!game.camera_grab(CENTER_POINTER, VIEWPORT));
}

#[test]
fn a_notch_from_you_divides_the_view_by_one_point_fifteen_and_a_notch_to_you_multiplies() {
    let mut game = flat_started(&[HERO]);
    assert!(game.camera_wheel(-100.0, CENTER_POINTER, VIEWPORT));
    near(
        game.player_view_height().expect("вид"),
        10.0 / WHEEL_ZOOM_FACTOR,
        1e-9,
    );
    assert!(game.camera_wheel(200.0, CENTER_POINTER, VIEWPORT));
    near(
        game.player_view_height().expect("вид"),
        10.0 * WHEEL_ZOOM_FACTOR,
        1e-9,
    );
}

#[test]
fn a_small_wheel_turn_zooms_by_a_fraction_of_a_notch() {
    let mut game = flat_started(&[HERO]);
    assert!(game.camera_wheel(50.0, CENTER_POINTER, VIEWPORT));
    near(
        game.player_view_height().expect("вид"),
        10.0 * WHEEL_ZOOM_FACTOR.sqrt(),
        1e-9,
    );
}

#[test]
fn the_view_never_leaves_the_zoom_and_the_point_under_the_pointer_does_not_drift() {
    let mut game = flat_started(&[HERO]);
    let pointer = [800.0, 200.0];
    for _ in 0..40 {
        game.camera_wheel(-100.0, pointer, VIEWPORT);
    }
    near(game.player_view_height().expect("вид"), 4.0, 0.0);
    let held = flat_point_under(&game, pointer);
    assert!(!game.camera_wheel(-100.0, pointer, VIEWPORT));
    near_pair(flat_point_under(&game, pointer), held, 1e-9);
    for _ in 0..60 {
        game.camera_wheel(100.0, pointer, VIEWPORT);
    }
    near(game.player_view_height().expect("вид"), 30.0, 0.0);
}

#[test]
fn the_example_of_the_spec_keeps_the_point_under_the_pointer_and_moves_the_middle_away() {
    let scene = scene_of(&[HERO]);
    let extra = r#","view_height":100,"player_camera":{"zoom":[20,400],"start":[250,180]}"#;
    let (mut game, _screens, _warnings) =
        load_with(&game_json(600, 400, extra), &scene, MOVE_HERO, SCREENS).expect("игра");
    let pointer = [950.0, 300.0];
    let point = flat_point_under(&game, pointer);
    let before = flat_center(&game);
    assert!(game.camera_wheel(100.0, pointer, VIEWPORT));
    near(game.player_view_height().expect("вид"), 115.0, 1e-9);
    near_pair(flat_point_under(&game, pointer), point, 1e-3);
    let after = flat_center(&game);
    assert!(
        (point[0] - after[0]).abs() > (point[0] - before[0]).abs(),
        "середина окна должна отъехать от точки дальше: {before:?} -> {after:?}"
    );
}

#[test]
fn zooming_in_the_flat_scene_keeps_every_point_under_its_pointer() {
    for pointer in [
        [100.0, 100.0],
        [900.0, 500.0],
        [500.0, 300.0],
        [20.0, 580.0],
    ] {
        let mut game = flat_started(&[HERO]);
        let point = flat_point_under(&game, pointer);
        game.camera_wheel(-250.0, pointer, VIEWPORT);
        near_pair(flat_point_under(&game, pointer), point, 1e-3);
    }
}

#[test]
fn zooming_in_the_space_keeps_the_ground_under_the_pointer_on_its_height() {
    let deck = r##"{"name":"deck","position":[36,26],"size":[8,6],"deck":true,"shape":"box","height":1.2,"color":"#777777"}"##;
    for pointer in [[500.0, 300.0], [620.0, 380.0], [420.0, 250.0]] {
        let mut game = space_started(&[deck]);
        let point = space_point_under(&game, pointer);
        assert!(game.camera_wheel(-200.0, pointer, VIEWPORT));
        let after = space_point_under(&game, pointer);
        near(after[0], point[0], 1e-6);
        near(after[1], point[1], 1e-6);
        near(after[2], point[2], 1e-6);
    }
    let game = space_started(&[deck]);
    near(space_point_under(&game, CENTER_POINTER)[2], 1.2, 1e-6);
}

#[test]
fn the_wheel_while_following_changes_only_the_view_and_the_object_stays_in_the_middle() {
    let mut game = flat_started(&[FOLLOWED_HERO]);
    game.camera_wheel(-300.0, [900.0, 100.0], VIEWPORT);
    assert_eq!(game.player_camera_follows(), Some(true));
    near_pair(flat_center(&game), [10.5, 10.5], 1e-3);
    near(
        game.player_view_height().expect("вид"),
        10.0 / WHEEL_ZOOM_FACTOR.powi(3),
        1e-9,
    );
}

#[test]
fn dragging_keeps_the_grabbed_place_under_the_pointer_and_releases_the_follow() {
    let mut game = flat_started(&[FOLLOWED_HERO]);
    let start = [600.0, 300.0];
    let grabbed = flat_point_under(&game, start);
    assert!(game.camera_grab(start, VIEWPORT));
    for pointer in [[650.0, 320.0], [700.0, 340.0], [640.0, 260.0]] {
        assert!(game.camera_drag(pointer, VIEWPORT));
        near_pair(flat_point_under(&game, pointer), grabbed, 1e-3);
    }
    assert_eq!(game.player_camera_follows(), Some(false));
    game.camera_release();
    assert!(!game.camera_drag([100.0, 100.0], VIEWPORT));
}

#[test]
fn dragging_in_the_space_keeps_the_grabbed_place_under_the_pointer() {
    let mut game = space_started(&[]);
    let start = [600.0, 360.0];
    let grabbed = space_point_under(&game, start);
    assert!(game.camera_grab(start, VIEWPORT));
    for pointer in [[560.0, 330.0], [520.0, 400.0]] {
        assert!(game.camera_drag(pointer, VIEWPORT));
        let held = space_point_under(&game, pointer);
        near(held[0], grabbed[0], 1e-6);
        near(held[1], grabbed[1], 1e-6);
    }
}

#[test]
fn the_map_follows_a_drag_that_leaves_the_window() {
    let mut game = flat_started(&[HERO]);
    let start = [600.0, 300.0];
    let grabbed = flat_point_under(&game, start);
    game.camera_grab(start, VIEWPORT);
    assert!(game.camera_drag([1200.0, 700.0], VIEWPORT));
    near_pair(flat_point_under(&game, [1200.0, 700.0]), grabbed, 1e-2);
}

#[test]
fn a_key_moves_the_camera_by_point_six_of_the_view_a_second() {
    let mut game = flat_started(&[HERO]);
    assert!(game.camera_key_down("KeyW"));
    assert!(game.camera_pan(1.0, VIEWPORT));
    near_pair(flat_center(&game), [30.0, 15.0 - 6.0], 1e-3);
    assert!(game.camera_key_up("KeyW"));
    assert!(!game.camera_pan(1.0, VIEWPORT));
    near_pair(flat_center(&game), [30.0, 9.0], 1e-3);
}

#[test]
fn every_camera_key_goes_its_own_way_and_two_keys_make_a_diagonal() {
    for (code, shift) in [
        ("ArrowLeft", [-3.0, 0.0]),
        ("KeyD", [3.0, 0.0]),
        ("ArrowUp", [0.0, -3.0]),
        ("KeyS", [0.0, 3.0]),
    ] {
        let mut game = flat_started(&[HERO]);
        assert!(game.camera_key_down(code), "{code}");
        game.camera_pan(0.5, VIEWPORT);
        near_pair(flat_center(&game), [30.0 + shift[0], 15.0 + shift[1]], 1e-3);
    }
    let mut game = flat_started(&[HERO]);
    game.camera_key_down("KeyW");
    game.camera_key_down("KeyD");
    game.camera_pan(0.5, VIEWPORT);
    near_pair(flat_center(&game), [33.0, 12.0], 1e-3);
}

#[test]
fn a_view_closer_in_makes_the_keys_slower() {
    let mut game = flat_started(&[HERO]);
    game.camera_wheel(
        -100.0 * (10.0_f64 / 5.0).ln() / WHEEL_ZOOM_FACTOR.ln(),
        CENTER_POINTER,
        VIEWPORT,
    );
    near(game.player_view_height().expect("вид"), 5.0, 1e-9);
    game.camera_key_down("KeyA");
    game.camera_pan(1.0, VIEWPORT);
    near_pair(flat_center(&game), [27.0, 15.0], 1e-3);
}

#[test]
fn a_key_named_in_an_object_is_left_to_its_binding() {
    let keyed = r##"{"name":"hero","position":[10,10],"size":[1,1],"color":"#ff0000","hero":true,"picked":false,
        "keys":{"KeyW":{"press":[["picked",true]]}}}"##;
    let mut game = flat_started(&[keyed]);
    assert!(!game.camera_takes_key("KeyW"));
    assert!(!game.camera_key_down("KeyW"));
    assert!(game.camera_takes_key("KeyA"));
    game.key_down("KeyW");
    step(&mut game, 1);
    let picked = game.properties.resolve("picked").expect("picked");
    assert!(game.world.flag(0, picked));
    assert!(!game.camera_pan(1.0, VIEWPORT));
}

#[test]
fn a_key_that_the_camera_did_not_take_is_not_let_go_by_it() {
    let mut game = flat_started(&[HERO]);
    assert!(!game.camera_key_up("KeyW"));
    game.camera_key_down("KeyW");
    game.camera_release_input();
    assert!(!game.camera_pan(1.0, VIEWPORT));
}

#[test]
fn on_a_screen_where_the_world_does_not_run_the_camera_moves_and_the_world_stands() {
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"keys":{"Escape":["show_screen","pause"]},"elements":[]},
        {"name":"pause","world_runs":false,"keys":{"Escape":["resume"]},"elements":[]}]}"#;
    let extra = r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[30,15]}"#;
    let (mut game, config, _warnings) = load_with(
        &game_json(60, 30, extra),
        &scene_of(&[HERO]),
        MOVE_HERO,
        screens,
    )
    .expect("игра");
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    queue.push_key_down("Escape");
    queue.push_key_up("Escape");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        0.1,
    );
    assert!(!state.is_live(&config));
    let steps_before = game.step_count();
    game.world.set_vec2(0, property::VELOCITY, [60.0, 0.0]);
    game.camera_key_down("KeyD");
    for _ in 0..10 {
        screens::engine_call(
            &mut UiQueue::new(),
            &mut mouse,
            &mut runner,
            &mut game,
            &config,
            &mut state,
            VIEWPORT,
            0.1,
        );
        game.camera_pan(0.1, VIEWPORT);
    }
    assert_eq!(game.step_count(), steps_before, "мир на паузе стоит");
    near(
        game.world.vec2(0, property::POSITION).expect("position")[0],
        10.0,
        0.0,
    );
    near_pair(flat_center(&game), [30.0 + 0.6 * 10.0 * 1.0, 15.0], 1e-3);
    game.camera_wheel(-100.0, CENTER_POINTER, VIEWPORT);
    assert!(game.player_view_height().expect("вид") < 10.0);
}

#[test]
fn without_a_world_the_camera_stands() {
    let mut game = flat_started(&[HERO]);
    game.quit();
    assert!(!game.camera_wheel(-100.0, CENTER_POINTER, VIEWPORT));
    assert!(!game.camera_takes_key("KeyW"));
    assert!(!game.camera_grab(CENTER_POINTER, VIEWPORT));
    assert!(!game.camera_pan(1.0, VIEWPORT));
}

#[test]
fn the_flat_camera_never_looks_past_the_edge() {
    let mut game = flat_started(&[HERO]);
    game.camera_key_down("KeyW");
    game.camera_key_down("KeyA");
    game.camera_pan(100.0, VIEWPORT);
    let visible_width = 1000.0 * 10.0 / 600.0;
    near_pair(flat_center(&game), [visible_width / 2.0, 5.0], 1e-3);
    game.camera_grab(CENTER_POINTER, VIEWPORT);
    game.camera_drag([900.0, 550.0], VIEWPORT);
    let center = flat_center(&game);
    assert!(center[0] >= visible_width / 2.0 - 1e-3 && center[1] >= 5.0 - 1e-3);
}

#[test]
fn a_scene_smaller_than_the_view_stands_in_the_middle() {
    let mut game = flat_started(&[HERO]);
    for _ in 0..30 {
        game.camera_wheel(100.0, CENTER_POINTER, VIEWPORT);
    }
    near_pair(flat_center(&game), [30.0, 15.0], 1e-3);
    game.camera_key_down("KeyW");
    game.camera_pan(1.0, VIEWPORT);
    near_pair(flat_center(&game), [30.0, 15.0], 1e-3);
}

#[test]
fn the_space_camera_never_looks_past_the_edge() {
    let mut game = space_started(&[]);
    game.camera_key_down("KeyW");
    game.camera_key_down("KeyD");
    game.camera_pan(100.0, VIEWPORT);
    let space = CameraConfig {
        pitch: 60.0,
        projection: Projection::Perspective,
    };
    let limit = camera::clamp_center_3d(&game.scene, space, 20.0, [1000.0, 600.0], [1.0e6, -1.0e6]);
    near_pair(space_center(&game), limit, 1e-6);
    let corner = space_point_under(&game, [500.0, 300.0]);
    assert!(corner[0] <= 80.0 + 1e-6 && corner[1] >= -1e-6);
}

#[test]
fn a_shift_by_the_player_lets_go_of_the_follow_and_the_object_may_walk_away() {
    let mut game = flat_started(&[FOLLOWED_HERO]);
    step(&mut game, 1);
    near_pair(flat_center(&game), [10.5, 10.5], 1e-3);
    game.camera_key_down("KeyD");
    game.camera_pan(0.5, VIEWPORT);
    assert_eq!(game.player_camera_follows(), Some(false));
    let stood = flat_center(&game);
    game.world.set_vec2(0, property::VELOCITY, [60.0, 0.0]);
    step(&mut game, 30);
    near_pair(flat_center(&game), stood, 1e-3);
}

#[test]
fn writing_camera_follows_again_brings_the_camera_back_even_to_the_same_object() {
    let mut game = flat_started(&[FOLLOWED_HERO]);
    game.camera_key_down("KeyD");
    game.camera_pan(0.5, VIEWPORT);
    game.camera_key_up("KeyD");
    assert_eq!(game.player_camera_follows(), Some(false));
    step(&mut game, 1);
    assert_eq!(game.player_camera_follows(), Some(false));
    game.world.set_flag(0, property::CAMERA_FOLLOWS, true);
    step(&mut game, 1);
    assert_eq!(game.player_camera_follows(), Some(true));
    near_pair(flat_center(&game), [10.5, 10.5], 1e-3);
}

#[test]
fn a_binding_that_writes_camera_follows_brings_the_camera_back() {
    let bound = r##"{"name":"hero","position":[10,10],"size":[1,1],"color":"#ff0000","hero":true,
        "keys":{"KeyF":{"press":[["camera_follows",true]]}}}"##;
    let mut game = flat_started(&[bound]);
    assert_eq!(game.player_camera_follows(), Some(false));
    game.key_down("KeyF");
    step(&mut game, 1);
    assert_eq!(game.player_camera_follows(), Some(true));
    near_pair(flat_center(&game), [10.5, 10.5], 1e-3);
}

#[test]
fn a_followed_object_that_is_gone_leaves_the_camera_where_it_stood() {
    let mut game = flat_started(&[FOLLOWED_HERO]);
    step(&mut game, 1);
    let stood = flat_center(&game);
    game.world.delete(0);
    step(&mut game, 5);
    near_pair(flat_center(&game), stood, 1e-3);
}

#[test]
fn a_new_world_returns_the_camera_to_the_start() {
    let mut game = flat_started(&[HERO]);
    game.camera_wheel(-300.0, [800.0, 100.0], VIEWPORT);
    game.camera_key_down("KeyW");
    game.camera_pan(1.0, VIEWPORT);
    game.new_game();
    near(game.player_view_height().expect("вид"), 10.0, 0.0);
    near_pair(flat_center(&game), [30.0, 15.0], 1e-3);

    let mut followed = flat_started(&[FOLLOWED_HERO]);
    followed.camera_key_down("KeyD");
    followed.camera_pan(1.0, VIEWPORT);
    followed.new_game();
    assert_eq!(followed.player_camera_follows(), Some(true));
    near_pair(flat_center(&followed), [10.5, 10.5], 1e-3);

    let mut plain = flat("", &[HERO]);
    plain.camera_key_down("KeyD");
    plain.camera_pan(1.0, VIEWPORT);
    plain.new_game();
    near_pair(flat_center(&plain), [30.0, 15.0], 1e-3);
}

#[test]
fn labels_keep_their_size_on_the_screen_at_every_view() {
    let mut game = flat_started(&[HERO]);
    let pixels = |game: &Game| {
        let (scale, _) = game.camera_frame(VIEWPORT);
        game.scene.label_cell(scale, VIEWPORT) * scale as f64
    };
    let first = pixels(&game);
    near(first, 600.0 / 10.0, 1e-3);
    game.camera_wheel(-400.0, CENTER_POINTER, VIEWPORT);
    near(pixels(&game), first, 1e-3);
    game.camera_wheel(900.0, CENTER_POINTER, VIEWPORT);
    near(pixels(&game), first, 1e-3);

    let mut deep = space_started(&[]);
    let cell = |game: &Game| game.camera_3d(VIEWPORT).expect("камера").cell_points();
    let first = cell(&deep);
    near(first, 600.0 / 20.0, 1e-9);
    deep.camera_wheel(-300.0, CENTER_POINTER, VIEWPORT);
    near(cell(&deep), first, 1e-9);
}

#[test]
fn without_a_player_camera_labels_grow_with_the_world() {
    let (game, _screens, _warnings) = load_with(
        &game_json(60, 30, r#","view_height":10"#),
        &scene_of(&[HERO]),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра");
    let (scale, _) = game.camera_frame(VIEWPORT);
    near(game.scene.label_cell(scale, VIEWPORT), 1.0, 0.0);
}

fn errors_of(result: Loaded) -> Vec<String> {
    match result {
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
            .collect(),
        Ok(_) => Vec::new(),
    }
}

fn scene_error(extra: &str, fragment: &str) {
    let result = load_with(
        &game_json(60, 30, extra),
        &scene_of(&[HERO]),
        MOVE_HERO,
        SCREENS,
    );
    let errors = errors_of(result);
    assert!(
        errors.iter().any(|e| e.contains(fragment)),
        "нет ошибки со словами \"{fragment}\" для {extra}: {errors:#?}"
    );
}

#[test]
fn each_mistake_in_the_player_camera_field_is_an_error() {
    scene_error(
        r#","player_camera":{"zoom":[4,30]}"#,
        "player_camera без view_height",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[4,30],"speed":1}"#,
        "speed",
    );
    scene_error(r#","view_height":10,"player_camera":{}"#, "zoom");
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[4]}"#,
        "пара чисел",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":"4-30"}"#,
        "массив",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[0,30]}"#,
        "больше нуля",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[-1,30]}"#,
        "больше нуля",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[30,4]}"#,
        "больше самого дальнего",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[12,30]}"#,
        "не вмещает",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[4,8]}"#,
        "не вмещает",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[10]}"#,
        "пара чисел",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[61,5]}"#,
        "вне сцены",
    );
    scene_error(
        r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[5,-1]}"#,
        "вне сцены",
    );
}

#[test]
fn the_ends_of_the_zoom_and_the_edges_of_the_scene_are_allowed() {
    for extra in [
        r#","view_height":10,"player_camera":{"zoom":[10,10]}"#,
        r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[0,0]}"#,
        r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[60,30]}"#,
    ] {
        load_with(
            &game_json(60, 30, extra),
            &scene_of(&[HERO]),
            MOVE_HERO,
            SCREENS,
        )
        .expect("игра должна загрузиться");
    }
}

#[test]
fn a_camera_key_taken_by_a_binding_is_a_warning() {
    let keyed = r##"{"name":"герой","position":[10,10],"size":[1,1],"color":"#ff0000",
        "keys":{"KeyW":{"press":[["picked",true]]},"Space":{"press":[["picked",true]]}}}"##;
    let extra = r#","view_height":10,"player_camera":{"zoom":[4,30]}"#;
    let (_game, _screens, warnings) = load_with(
        &game_json(60, 30, extra),
        &scene_of(&[keyed]),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра должна загрузиться");
    let messages: Vec<&str> = warnings.iter().map(|w| w.message.as_str()).collect();
    assert!(
        messages
            .iter()
            .any(|m| m.contains("клавиша KeyW у объекта „герой“: камера её не берёт")),
        "{messages:#?}"
    );
    assert!(!messages.iter().any(|m| m.contains("Space")));
}

#[test]
fn a_camera_key_taken_by_a_screen_is_a_warning_and_without_the_field_there_is_none() {
    let screens = r#"{"screens":[{"name":"main","world_runs":true,"keys":{"ArrowUp":["show_screen","main"]},"elements":[]}]}"#;
    let extra = r#","view_height":10,"player_camera":{"zoom":[4,30]}"#;
    let (_game, _screens, warnings) = load_with(
        &game_json(60, 30, extra),
        &scene_of(&[HERO]),
        MOVE_HERO,
        screens,
    )
    .expect("игра должна загрузиться");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("клавиша ArrowUp у экрана „main“")),
        "{warnings:#?}"
    );
    let (_game, _screens, warnings) = load_with(
        &game_json(60, 30, r#","view_height":10"#),
        &scene_of(&[HERO]),
        MOVE_HERO,
        screens,
    )
    .expect("игра должна загрузиться");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("камера её не берёт"))
    );
}

/// Запись пишется текстом, и последняя цифра числа может разойтись на единицу последнего разряда.
fn assert_same_view(actual: &PlayerView, expected: &PlayerView, step: usize) {
    let message = format!("вид разошёлся на шаге {step}: {actual:?} против {expected:?}");
    assert_eq!(actual.follow, expected.follow, "{message}");
    assert_eq!(actual.z.is_some(), expected.z.is_some(), "{message}");
    for (a, b) in [
        (actual.center[0], expected.center[0]),
        (actual.center[1], expected.center[1]),
        (actual.view_height, expected.view_height),
        (actual.z.unwrap_or(0.0), expected.z.unwrap_or(0.0)),
    ] {
        assert!((a - b).abs() < 1e-9, "{message}");
    }
}

fn clickable(name: &str, x: f64) -> String {
    format!(
        r##"{{"name":"{name}","position":[{x},14],"size":[2,2],"color":"#00ff00","on_click":[["picked",true]]}}"##
    )
}

fn live_session(game: &mut Game, config: &ScreensConfig) -> (PlaySession, ScreenState) {
    let mut state = ScreenState::new(config.start_screen);
    let session = PlaySession::begin_live(game, config, &mut state);
    (session, state)
}

fn step_session(
    session: &mut PlaySession,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    queue: &mut UiQueue,
) {
    session.step_once(
        queue,
        &mut MouseState::default(),
        game,
        config,
        state,
        VIEWPORT,
        &[],
    );
}

#[test]
fn the_view_of_the_player_camera_goes_into_the_recording_and_replays_on_the_same_steps() {
    let extra = r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[30,15]}"#;
    let left = clickable("left", 26.0);
    let right = clickable("right", 33.0);
    let objects = [HERO, left.as_str(), right.as_str()];
    let (mut game, config, _warnings) = load_with(
        &game_json(60, 30, extra),
        &scene_of(&objects),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра");
    let (mut session, mut state) = live_session(&mut game, &config);
    let mut queue = UiQueue::new();
    let mut live_views: Vec<PlayerView> = Vec::new();
    for count in 0..40 {
        if count == 5 {
            game.camera_wheel(-200.0, [700.0, 250.0], VIEWPORT);
            session.record_camera(&game);
        }
        if count == 10 {
            game.camera_key_down("KeyD");
        }
        if (10..20).contains(&count) {
            game.camera_pan(1.0 / 60.0, VIEWPORT);
            session.record_camera(&game);
        }
        if count == 20 {
            game.camera_key_up("KeyD");
            game.camera_grab([300.0, 300.0], VIEWPORT);
        }
        if (20..25).contains(&count) {
            game.camera_drag([300.0 + 8.0 * (count - 19) as f32, 300.0], VIEWPORT);
            session.record_camera(&game);
        }
        if count == 30 {
            game.camera_release();
            let (scale, offset) = game.camera_frame(VIEWPORT);
            let over_right = [offset[0] + scale * 34.0, offset[1] + scale * 15.0];
            let point = flat_point_under(&game, over_right);
            near_pair(point, [34.0, 15.0], 1e-3);
            game.set_cursor_cell(point);
            session.record_cursor(&game);
            queue.push_key_down("MouseLeft");
        }
        step_session(&mut session, &mut game, &config, &mut state, &mut queue);
        live_views.push(game.player_view().expect("камера игрока"));
    }
    let picked = game.properties.resolve("picked").expect("picked");
    let clicked: Vec<bool> = (1..=2).map(|id| game.world.flag(id, picked)).collect();
    assert_eq!(
        clicked,
        vec![false, true],
        "щелчок должен попасть в правый объект"
    );

    let text = session.recording_text(&game);
    let parsed = recording::parse(&text).expect("запись читается");
    assert!(
        parsed
            .events
            .iter()
            .any(|e| matches!(e.kind, ReplayEventKind::Camera { follow: false, .. }))
    );
    let steps_of_cameras: Vec<u64> = parsed
        .events
        .iter()
        .filter(|e| matches!(e.kind, ReplayEventKind::Camera { .. }))
        .map(|e| e.step)
        .collect();
    let mut sorted = steps_of_cameras.clone();
    sorted.dedup();
    assert_eq!(
        sorted, steps_of_cameras,
        "на одном шаге одно событие камеры"
    );

    let (mut again, config2, _warnings) = load_with(
        &game_json(60, 30, extra),
        &scene_of(&objects),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut again, &config2, &mut state2).expect("запись");
    let mut idle = UiQueue::new();
    for (count, expected) in live_views.iter().enumerate() {
        step_session(&mut replay, &mut again, &config2, &mut state2, &mut idle);
        let view = again.player_view().expect("камера игрока");
        assert_same_view(&view, expected, count);
    }
    let clicked_again: Vec<bool> = (1..=2).map(|id| again.world.flag(id, picked)).collect();
    assert_eq!(clicked_again, clicked, "щелчок попал в тот же объект");
}

#[test]
fn a_recorded_space_camera_replays_with_its_height() {
    let extra = r#","view_height":20,"camera":{"pitch":60},"player_camera":{"zoom":[6,40],"start":[40,30]}"#;
    let (mut game, config, _warnings) = load_with(
        &game_json(80, 60, extra),
        &scene_of(&[HERO]),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра");
    let (mut session, mut state) = live_session(&mut game, &config);
    game.camera_wheel(-300.0, [600.0, 360.0], VIEWPORT);
    session.record_camera(&game);
    let mut queue = UiQueue::new();
    step_session(&mut session, &mut game, &config, &mut state, &mut queue);
    let expected = game.player_view().expect("камера");
    assert!(expected.z.is_some());
    let text = session.recording_text(&game);

    let (mut again, config2, _warnings) = load_with(
        &game_json(80, 60, extra),
        &scene_of(&[HERO]),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut again, &config2, &mut state2).expect("запись");
    step_session(
        &mut replay,
        &mut again,
        &config2,
        &mut state2,
        &mut UiQueue::new(),
    );
    assert_same_view(&again.player_view().expect("камера"), &expected, 0);
}

#[test]
fn a_view_that_did_not_change_is_not_recorded() {
    let extra = r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[30,15]}"#;
    let (mut game, config, _warnings) = load_with(
        &game_json(60, 30, extra),
        &scene_of(&[HERO]),
        MOVE_HERO,
        SCREENS,
    )
    .expect("игра");
    let (mut session, mut state) = live_session(&mut game, &config);
    session.record_camera(&game);
    let mut queue = UiQueue::new();
    step_session(&mut session, &mut game, &config, &mut state, &mut queue);
    session.record_camera(&game);
    let parsed = recording::parse(&session.recording_text(&game)).expect("запись");
    assert!(parsed.events.is_empty(), "{:?}", parsed.events);
}

//! Фаза 29 — камера редактора плоской сцены (требования 1, 3, 7–10, 13), вызовы `scene_point` и
//! `screen_point` в плоской сцене и заготовка платформера (требование 34). Игры для тестов
//! собираются в коде, как в `tests/phase28_depth_layers.rs`.

use std::fs;
use std::path::PathBuf;

use engine::core::camera::FlatEditorCamera;
use engine::core::game::Game;
use engine::core::scene::{
    LayerView, letterbox, object_at_frame, object_rect_frame, recorded_place, window_point,
};
use engine::data::load::load_game_from_texts;
use engine::render::atlas::compose_world_paints;
use engine::render::wind::Motion;
use serde_json::Value;

const WINDOW: [f32; 2] = [1200.0, 600.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const PROPS: &str = r#"{"properties":{"wall":"flag"}}"#;
const HILLS: &str = r##"{"position":[90,11.5],"size":[24,8],"color":"#8ea7b8","parallax":0.25}"##;
const SKY: &str = r##"{"position":[70,6],"size":[20,4],"color":"#b4d2ec","parallax":0}"##;
const CRATE: &str = r##"{"position":[10,10],"size":[0.5,0.5],"color":"#aa5522"}"##;
const BARE: &str = r#"{"wall":true}"#;
const VIEWER: &str = r#"{"position":[99.5,17.5],"size":[1,1],"camera_follows":true}"#;

fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":160,"height":24,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

fn load(scene_extra: &str, objects: &[&str]) -> Game {
    let scene = format!(r#"{{"objects":[{}]}}"#, objects.join(","));
    let (game, _screens, warnings) =
        load_game_from_texts(&game_json(scene_extra), PROPS, &scene, NO_RULES, SCREENS)
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game
}

/// Сцена 160 × 24, `view_height` 12: объекты — холмы (1), небо (2), ящик (3), без места (4).
fn editor_game() -> Game {
    load(r#","view_height":12"#, &[BARE, HILLS, SKY, CRATE, BARE])
}

fn assert_close(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-3,
        "{what}: {actual} != {expected}"
    );
}

fn assert_camera(actual: FlatEditorCamera, center: [f64; 2], view_height: f64, what: &str) {
    assert_close(actual.center[0], center[0], &format!("{what}: center x"));
    assert_close(actual.center[1], center[1], &format!("{what}: center y"));
    assert_close(
        actual.view_height,
        view_height,
        &format!("{what}: view_height"),
    );
}

fn send(game: &mut Game, center: [f64; 2], view_height: f64) {
    game.set_flat_editor_camera(FlatEditorCamera {
        center,
        view_height,
    });
}

#[test]
fn the_editor_frame_keeps_the_cell_under_the_wheel_point_on_the_same_pixel() {
    let mut game = editor_game();
    send(&mut game, [76.0, 12.0], 72.0);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    assert_close(scale as f64, 600.0 / 72.0, "масштаб");
    let point = window_point(scale, offset, [40.0, 12.0]);
    assert_close(point[0], 300.0, "x клетки (40, 12)");
    assert_close(point[1], 300.0, "y клетки (40, 12)");
}

#[test]
fn the_editor_camera_is_not_pressed_to_the_scene_edges() {
    let mut game = editor_game();
    send(&mut game, [-50.0, 100.0], 10.0);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    let point = window_point(scale, offset, [-50.0, 100.0]);
    assert_close(point[0], 600.0, "середина по x");
    assert_close(point[1], 300.0, "середина по y");
}

#[test]
fn without_a_sent_camera_the_whole_scene_is_shown_as_before() {
    let game = editor_game();
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    let (whole_scale, whole_offset) = letterbox(WINDOW, [160.0, 24.0]);
    assert_eq!(scale, whole_scale);
    assert_eq!(offset, whole_offset);
}

#[test]
fn fit_camera_without_an_object_sees_the_whole_scene() {
    let game = editor_game();
    let camera = game.fit_flat_camera(None, WINDOW).expect("плоская сцена");
    assert_camera(camera, [80.0, 12.0], 80.0, "холст 1200 × 600");
    let tall = game
        .fit_flat_camera(None, [600.0, 1200.0])
        .expect("плоская сцена");
    assert_camera(tall, [80.0, 12.0], 320.0, "узкий холст");
}

#[test]
fn fit_camera_of_a_whole_scene_view_matches_the_plain_letterbox_frame() {
    let mut game = editor_game();
    let camera = game.fit_flat_camera(None, WINDOW).expect("плоская сцена");
    let (whole_scale, whole_offset) = game.editor_flat_frame(WINDOW);
    game.set_flat_editor_camera(camera);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    assert_close(scale as f64, whole_scale as f64, "масштаб");
    assert_close(offset[0] as f64, whole_offset[0] as f64, "сдвиг x");
    assert_close(offset[1] as f64, whole_offset[1] as f64, "сдвиг y");
}

#[test]
fn fit_camera_of_the_hills_centres_where_they_are_drawn() {
    let mut game = editor_game();
    let camera = game
        .fit_flat_camera(Some(1), WINDOW)
        .expect("у холмов есть место и размер");
    assert_camera(camera, [168.0, 26.0], 14.4, "холмы");

    game.set_flat_editor_camera(camera);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    let rect = object_rect_frame(&game.world, &game.scene, 1, scale, offset, WINDOW)
        .expect("у холмов есть место и размер");
    assert_close(
        (rect.x + rect.width / 2.0) as f64,
        600.0,
        "середина холмов по x",
    );
    assert_close(
        (rect.y + rect.height / 2.0) as f64,
        300.0,
        "середина холмов по y",
    );
}

#[test]
fn fit_camera_of_a_parallax_zero_object_keeps_the_centre() {
    let mut game = editor_game();
    let from_the_scene_middle = game
        .fit_flat_camera(Some(2), WINDOW)
        .expect("у неба есть место и размер");
    assert_camera(
        from_the_scene_middle,
        [80.0, 12.0],
        14.4,
        "небо, камера не слали",
    );

    send(&mut game, [100.0, 18.0], 30.0);
    let moved = game
        .fit_flat_camera(Some(2), WINDOW)
        .expect("у неба есть место и размер");
    assert_camera(moved, [100.0, 18.0], 14.4, "небо, камера ушла");
}

#[test]
fn fit_camera_of_a_small_object_does_not_get_closer_than_two_cells() {
    let game = editor_game();
    let camera = game
        .fit_flat_camera(Some(3), WINDOW)
        .expect("у ящика есть место и размер");
    assert_camera(camera, [10.25, 10.25], 2.0, "ящик");
}

#[test]
fn fit_camera_without_a_usable_object_is_undefined() {
    let game = editor_game();
    assert_eq!(
        game.fit_flat_camera(Some(0), WINDOW),
        None,
        "без place и size"
    );
    assert_eq!(
        game.fit_flat_camera(Some(4), WINDOW),
        None,
        "без place и size"
    );
    assert_eq!(
        game.fit_flat_camera(Some(99), WINDOW),
        None,
        "нет такого объекта"
    );
}

#[test]
fn fit_camera_of_a_three_dimensional_scene_is_not_a_flat_one() {
    let game = load(r#","view_height":12,"camera":{"pitch":55}"#, &[CRATE, BARE]);
    assert_eq!(game.fit_flat_camera(None, WINDOW), None);
    assert_eq!(game.fit_flat_camera(Some(0), WINDOW), None);
}

#[test]
fn depth_layers_follow_the_editor_camera() {
    let mut game = editor_game();
    send(&mut game, [100.0, 18.0], 12.0);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    let layers = LayerView::of_frame(&game.scene, scale, offset, WINDOW);
    let paints = compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        &Motion::default(),
        &[],
        &[],
        &layers,
    );
    let hills = paints
        .iter()
        .find(|paint| paint.object == Some(1))
        .expect("холмы нарисованы");
    assert_close(hills.position[0] as f64, 105.0, "холмы по x");
    assert_close(hills.position[1] as f64, 16.0, "холмы по y");

    let rect = object_rect_frame(&game.world, &game.scene, 1, scale, offset, WINDOW)
        .expect("у холмов есть место и размер");
    let drawn = window_point(scale, offset, [105.0, 16.0]);
    assert_close(rect.x as f64, drawn[0], "рамка холмов по x");
    assert_close(rect.y as f64, drawn[1], "рамка холмов по y");

    let inside = [(drawn[0] + 5.0) as f32, (drawn[1] + 5.0) as f32];
    assert_eq!(
        object_at_frame(&game.world, &game.scene, inside, scale, offset, WINDOW),
        Some(1),
        "щелчок по нарисованным холмам"
    );
    let recorded = window_point(scale, offset, [90.0, 11.5]);
    let at_recorded = [(recorded[0] + 5.0) as f32, (recorded[1] + 5.0) as f32];
    assert_eq!(
        object_at_frame(&game.world, &game.scene, at_recorded, scale, offset, WINDOW),
        None,
        "на записанном месте холмов их нет"
    );
}

#[test]
fn scene_point_gives_the_recorded_place_of_a_layer_object() {
    let mut game = editor_game();
    send(&mut game, [60.0, 18.0], 12.0);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    let window = window_point(scale, offset, [40.5, 15.2]);
    let place = recorded_place(&game.scene, scale, offset, WINDOW, window, 0.6);
    assert_close(place[0], 48.5, "x для parallax 0,6");
    assert_close(place[1], 12.8, "y для parallax 0,6");

    let on_screen = recorded_place(&game.scene, scale, offset, WINDOW, window, 1.0);
    assert_close(on_screen[0], 40.5, "x для parallax 1");
    assert_close(on_screen[1], 15.2, "y для parallax 1");

    let negative = recorded_place(&game.scene, scale, offset, WINDOW, window, -3.0);
    let zero = recorded_place(&game.scene, scale, offset, WINDOW, window, 0.0);
    assert_eq!(negative, zero, "parallax меньше нуля — как 0");
    assert_close(zero[0], 60.5, "x для parallax 0");
    assert_close(zero[1], 9.2, "y для parallax 0");
}

#[test]
fn scene_point_is_not_pressed_to_the_scene_edges() {
    let mut game = editor_game();
    send(&mut game, [0.0, 0.0], 12.0);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    let place = recorded_place(&game.scene, scale, offset, WINDOW, [0.0, 0.0], 1.0);
    assert!(place[0] < -5.0 && place[1] < -5.0, "{place:?}");
}

#[test]
fn scene_point_on_pause_follows_the_game_camera() {
    let game = load(r#","view_height":12"#, &[VIEWER, BARE]);
    let (scale, offset) = game.camera_frame(WINDOW);
    let middle = [WINDOW[0] as f64 / 2.0, WINDOW[1] as f64 / 2.0];
    let place = recorded_place(&game.scene, scale, offset, WINDOW, middle, 0.0);
    assert_close(
        place[0],
        80.0,
        "середина экрана для parallax 0 — середина сцены по x",
    );
    assert_close(
        place[1],
        12.0,
        "середина экрана для parallax 0 — середина сцены по y",
    );
    let seen = recorded_place(&game.scene, scale, offset, WINDOW, middle, 1.0);
    assert_close(
        seen[0],
        100.0,
        "середина экрана для parallax 1 — объект камеры по x",
    );
    assert_close(
        seen[1],
        18.0,
        "середина экрана для parallax 1 — объект камеры по y",
    );
}

#[test]
fn screen_point_is_the_inverse_of_scene_point_for_parallax_one() {
    let mut game = editor_game();
    send(&mut game, [33.0, 7.0], 9.0);
    let (scale, offset) = game.editor_flat_frame(WINDOW);
    for cell in [[0.0, 0.0], [40.5, 15.2], [-12.0, 40.0], [160.0, 24.0]] {
        let window = window_point(scale, offset, cell);
        let back = recorded_place(&game.scene, scale, offset, WINDOW, window, 1.0);
        assert_close(back[0], cell[0], "x туда и обратно");
        assert_close(back[1], cell[1], "y туда и обратно");
    }
}

#[test]
fn the_platformer_blank_declares_ninety_six_pixels_per_cell() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("platformer")
        .join("game.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("не прочитан {path:?}: {e}"));
    let game: Value = serde_json::from_str(&text).expect("game.json разбирается");
    assert_eq!(game["scene"]["cell_pixels"], 96);
}

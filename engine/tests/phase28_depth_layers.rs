//! Фаза 28 — слои глубины (требования 1–25) и заготовка платформера (требования 26–32). Игры для
//! тестов собираются в коде, как в `tests/phase14_smooth_flip_atlas_sheets.rs`; записанные партии
//! платформера лежат в `tests/replays/platformer-*.json`.

use std::fs;
use std::path::PathBuf;

use engine::core::camera::followed_center;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::scene::{
    LayerView, letterbox, object_at, object_at_frame, object_rect_frame, on_click_target,
};
use engine::core::step::apply_follow_mouse;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::load_game_from_texts_with_code;
use engine::render::atlas::{RectPaint, compose_world_paints};

const WINDOW: [f32; 2] = [1280.0, 720.0];
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const NO_PROPS: &str = r#"{"properties":{}}"#;
const PROPS_CLICKED: &str = r#"{"properties":{"clicked":"number"}}"#;
const PROPS: &str = r#"{"properties":{"wall":"flag","a_side":"flag","b_side":"flag","slow":"flag","hits":"number","clicked":"number"}}"#;
const CAMERA_3D: &str = r#","view_height":12,"camera":{"pitch":55}"#;
const VIEWER: &str = r#"{"position":[99.5,17.5],"size":[1,1],"camera_follows":true}"#;

fn game_json(width: u32, height: u32, scene_extra: &str, files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

type Loaded = Result<(Game, Vec<GameError>), LoadFailure>;

fn load_with_props(
    props: &str,
    game: &str,
    scene: &str,
    rules: &str,
    code: Option<&str>,
) -> Loaded {
    load_game_from_texts_with_code(game, props, scene, rules, SCREENS, code)
        .map(|(game, _screens, warnings)| (game, warnings))
}

fn load_with(game: &str, scene: &str, rules: &str, code: Option<&str>) -> Loaded {
    load_with_props(PROPS, game, scene, rules, code)
}

fn scene_of(objects: &[&str]) -> String {
    format!(r#"{{"objects":[{}]}}"#, objects.join(","))
}

fn camera_scene(objects: &[&str]) -> String {
    let mut all = vec![VIEWER];
    all.extend_from_slice(objects);
    scene_of(&all)
}

/// Сцена 160 × 24, `view_height` 12 — как в примере требования 6; объект камеры идёт первым.
fn camera_game(objects: &[&str]) -> Game {
    let (game, warnings) = load_with_props(
        NO_PROPS,
        &game_json(160, 24, r#","view_height":12"#, ""),
        &camera_scene(objects),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game
}

fn layers_of(game: &Game, viewport: [f32; 2]) -> LayerView {
    let (scale, offset) = game.camera_frame(viewport);
    LayerView::of_frame(&game.scene, scale, offset, viewport)
}

fn paints_of(game: &Game, viewport: [f32; 2]) -> Vec<RectPaint> {
    compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        0.0,
        &[],
        &[],
        &layers_of(game, viewport),
    )
}

fn paints_of_object(game: &Game, viewport: [f32; 2], id: u32) -> Vec<RectPaint> {
    paints_of(game, viewport)
        .into_iter()
        .filter(|paint| paint.object == Some(id))
        .collect()
}

fn assert_near(actual: [f32; 2], expected: [f32; 2], what: &str) {
    assert!(
        (actual[0] - expected[0]).abs() < 1e-3 && (actual[1] - expected[1]).abs() < 1e-3,
        "{what}: {actual:?} != {expected:?}"
    );
}

fn hill(extra: &str) -> String {
    format!(r##"{{"position":[90,11.5],"size":[24,8],"color":"#8ea7b8"{extra}}}"##)
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

// -------------------------------------------------------------------------------------------
// Место объекта слоя, требования 3–9
// -------------------------------------------------------------------------------------------

#[test]
fn shift_follows_the_worked_example_for_four_parallax_values() {
    for (parallax, shift) in [
        (0.0, [20.0, 6.0]),
        (0.25, [15.0, 4.5]),
        (0.6, [8.0, 2.4]),
        (1.4, [-8.0, -2.4]),
    ] {
        let game = camera_game(&[&hill(&format!(r#","parallax":{parallax}"#))]);
        let drawn = paints_of_object(&game, WINDOW, 1);
        assert_eq!(drawn.len(), 1, "parallax {parallax}");
        assert_near(
            drawn[0].position,
            [90.0 + shift[0], 11.5 + shift[1]],
            &format!("parallax {parallax}"),
        );
    }
    let game = camera_game(&[&hill(r#","parallax":0.25"#)]);
    let drawn = paints_of_object(&game, WINDOW, 1);
    assert_near(drawn[0].position, [105.0, 16.0], "холм по примеру плана");
    assert_near(drawn[0].size, [24.0, 8.0], "размер не меняется");
}

#[test]
fn recorded_place_is_unshifted_without_view_height_and_outside_a_battle() {
    let (game, _warnings) = load_with(
        &game_json(160, 24, "", ""),
        &camera_scene(&[&hill(r#","parallax":0"#)]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    let drawn = paints_of_object(&game, WINDOW, 1);
    assert_near(drawn[0].position, [90.0, 11.5], "без view_height");

    let game = camera_game(&[&hill(r#","parallax":0"#)]);
    let (scale, offset) = letterbox(WINDOW, [160.0, 24.0]);
    let whole = LayerView::of_frame(&game.scene, scale, offset, WINDOW);
    assert_near(
        [whole.shift[0] as f32, whole.shift[1] as f32],
        [0.0, 0.0],
        "вне партии — сцена целиком",
    );
}

#[test]
fn a_scene_narrower_than_the_window_shifts_only_vertically() {
    let (game, _warnings) = load_with(
        &game_json(10, 24, r#","view_height":12"#, ""),
        &scene_of(&[
            r#"{"position":[4.5,19.5],"size":[1,1],"camera_follows":true}"#,
            r##"{"position":[2,10],"size":[3,3],"color":"#112233","parallax":0}"##,
        ]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    let shift = layers_of(&game, WINDOW).shift;
    assert!(shift[0].abs() < 1e-3, "по x сдвига нет: {shift:?}");
    assert!(
        (shift[1] - 6.0).abs() < 1e-3,
        "по y камера у низа: {shift:?}"
    );
}

#[test]
fn a_camera_pressed_to_the_edge_shifts_from_the_pressed_middle() {
    let (game, _warnings) = load_with(
        &game_json(160, 24, r#","view_height":12"#, ""),
        &scene_of(&[
            r#"{"position":[0.5,17.5],"size":[1,1],"camera_follows":true}"#,
            &hill(r#","parallax":0"#),
        ]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    let half_window = 1280.0 / 60.0 / 2.0;
    let drawn = paints_of_object(&game, WINDOW, 1);
    assert_near(
        drawn[0].position,
        [90.0 + (half_window - 80.0), 11.5 + 6.0],
        "середина камеры прижата к левому краю",
    );
}

#[test]
fn parallax_below_zero_during_the_game_draws_as_zero() {
    let pinned = camera_game(&[&hill(r#","parallax":0"#)]);
    let expected = paints_of_object(&pinned, WINDOW, 1)[0].position;
    let mut game = camera_game(&[&hill(r#","parallax":0.5"#)]);
    game.world.set_number(1, property::PARALLAX, -3.0);
    let drawn = paints_of_object(&game, WINDOW, 1);
    assert_near(drawn[0].position, expected, "меньше нуля рисуется как 0");
}

#[test]
fn the_shift_changes_only_the_picture_not_the_recorded_place() {
    let game = camera_game(&[&hill(r#","parallax":0.25"#)]);
    let before = game.world.vec2(1, property::POSITION);
    let _ = paints_of(&game, WINDOW);
    assert_eq!(game.world.vec2(1, property::POSITION), before);
    assert_eq!(before, Some([90.0, 11.5]));
}

// -------------------------------------------------------------------------------------------
// Повтор по ширине, требования 10–14
// -------------------------------------------------------------------------------------------

#[test]
fn repeat_x_draws_the_two_visible_copies_of_the_worked_example() {
    let game = camera_game(&[&hill(r#","parallax":0.25,"repeat_x":true"#)]);
    let drawn = paints_of_object(&game, WINDOW, 1);
    let mut xs: Vec<f32> = drawn.iter().map(|paint| paint.position[0]).collect();
    xs.sort_by(f32::total_cmp);
    assert_eq!(xs, vec![81.0, 105.0], "{drawn:?}");
    assert!(
        drawn.iter().all(|paint| paint.position[1] == 16.0),
        "{drawn:?}"
    );
}

#[test]
fn repeat_x_with_zero_width_draws_once() {
    let game = camera_game(&[
        r##"{"position":[90,11.5],"size":[0,8],"color":"#8ea7b8","parallax":0.25,"repeat_x":true}"##,
    ]);
    assert_eq!(paints_of_object(&game, WINDOW, 1).len(), 1);
}

#[test]
fn repeat_x_in_a_wide_window_without_view_height_reaches_the_margins() {
    let (game, _warnings) = load_with(
        &game_json(100, 100, "", ""),
        &scene_of(&[r##"{"position":[40,10],"size":[10,10],"color":"#336699","repeat_x":true}"##]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    let drawn = paints_of_object(&game, [2000.0, 600.0], 0);
    let mut xs: Vec<f32> = drawn.iter().map(|paint| paint.position[0]).collect();
    xs.sort_by(f32::total_cmp);
    assert_eq!(xs.len(), 34, "{xs:?}");
    assert_eq!(xs.first(), Some(&-120.0), "поле слева от сцены закрыто");
    assert_eq!(xs.last(), Some(&210.0), "поле справа от сцены закрыто");
}

// -------------------------------------------------------------------------------------------
// Порядок рисования, требование 15
// -------------------------------------------------------------------------------------------

fn order_of(game: &Game) -> Vec<u32> {
    paints_of(game, WINDOW)
        .iter()
        .filter_map(|paint| paint.object)
        .filter(|&id| id != 0)
        .collect()
}

#[test]
fn layer_decides_who_covers_whom_not_parallax() {
    let game = camera_game(&[
        r##"{"position":[0,0],"size":[2,2],"color":"#ff0000","layer":1,"parallax":0}"##,
        r##"{"position":[0,0],"size":[2,2],"color":"#0000ff","layer":0,"parallax":2}"##,
    ]);
    assert_eq!(order_of(&game), vec![2, 1]);
}

#[test]
fn y_sort_compares_the_recorded_bottom_edges() {
    let (game, _warnings) = load_with(
        &game_json(160, 24, r#","view_height":12,"y_sort":true"#, ""),
        &camera_scene(&[
            r##"{"position":[0,8],"size":[2,2],"color":"#ff0000","parallax":0}"##,
            r##"{"position":[0,10],"size":[2,2],"color":"#0000ff"}"##,
        ]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    let paints = paints_of(&game, WINDOW);
    let red = paints.iter().find(|paint| paint.object == Some(1)).unwrap();
    let blue = paints.iter().find(|paint| paint.object == Some(2)).unwrap();
    assert!(
        red.position[1] + 2.0 > blue.position[1] + 2.0,
        "красный нарисован ниже"
    );
    assert_eq!(
        order_of(&game),
        vec![1, 2],
        "а поверх — синий, у него записанный край ниже"
    );
}

// -------------------------------------------------------------------------------------------
// Выбор и рамка, требования 19–20
// -------------------------------------------------------------------------------------------

#[test]
fn object_at_and_object_rect_follow_the_place_where_the_hill_is_drawn() {
    let game = camera_game(&[&hill(r#","parallax":0.25,"repeat_x":true"#)]);
    let (scale, offset) = game.camera_frame(WINDOW);
    let picked = object_at_frame(
        &game.world,
        &game.scene,
        [300.0, 600.0],
        scale,
        offset,
        WINDOW,
    );
    assert_eq!(picked, Some(1), "клетка (94,33; 22) — копия от 81 до 105");
    let recorded = game.world.vec2(1, property::POSITION).unwrap();
    assert!(
        recorded[1] + 8.0 < 22.0,
        "записанный прямоугольник эту клетку не накрывает"
    );
    let above = object_at_frame(
        &game.world,
        &game.scene,
        [300.0, 100.0],
        scale,
        offset,
        WINDOW,
    );
    assert_eq!(above, None, "выше нарисованного холма — пусто");

    let rect = object_rect_frame(&game.world, &game.scene, 1, scale, offset, WINDOW)
        .expect("у холма есть место и размер");
    assert!(
        (rect.x - 940.0).abs() < 1e-2 && (rect.y - 240.0).abs() < 1e-2,
        "{rect:?}"
    );
    assert!(
        (rect.width - 1440.0).abs() < 1e-2 && (rect.height - 480.0).abs() < 1e-2,
        "{rect:?}"
    );
}

#[test]
fn outside_a_battle_the_hill_is_picked_by_its_recorded_height_across_the_window() {
    let game = camera_game(&[&hill(r#","parallax":0.25,"repeat_x":true"#)]);
    // Вся сцена в окне: 8 точек на клетку, сверху поле в 264 точки.
    for x in [5.0, 640.0, 1275.0] {
        assert_eq!(
            object_at(&game.world, &game.scene, [x, 264.0 + 15.0 * 8.0], WINDOW),
            Some(1),
            "строка 15, x {x}"
        );
        assert_eq!(
            object_at(&game.world, &game.scene, [x, 264.0 + 22.0 * 8.0], WINDOW),
            None,
            "строка 22, x {x}"
        );
    }
    let still = camera_game(&[&hill(r#","parallax":0.25"#)]);
    assert_eq!(
        object_at(
            &still.world,
            &still.scene,
            [5.0, 264.0 + 15.0 * 8.0],
            WINDOW
        ),
        None,
        "без repeat_x вне записанной ширины выбрать нечего"
    );
}

// -------------------------------------------------------------------------------------------
// Объект слоя только рисуется, требование 17
// -------------------------------------------------------------------------------------------

const COLLIDE_RULES: &str = r#"{"rules":[
    {"kind":"collide","a":{"has":["a_side"]},"b":{"has":["b_side"]},"effects":{"a":[["add","hits",1]],"b":[]}}
]}"#;
const COLLIDE_AFTER_PARALLAX_RULE: &str = r#"{"rules":[
    {"kind":"check","for":{"has":["slow"]},"do":[["add","parallax",-0.5]]},
    {"kind":"collide","a":{"has":["a_side"]},"b":{"has":["b_side"]},"effects":{"a":[["add","hits",1]],"b":[]}}
]}"#;

fn colliding_pair(b_extra: &str, rules: &str) -> Game {
    let a = r##"{"position":[0,0],"size":[2,2],"collides":true,"a_side":true,"hits":0,"color":"#ff0000"}"##;
    let b = format!(
        r##"{{"position":[1,1],"size":[2,2],"collides":true,"b_side":true,"color":"#0000ff"{b_extra}}}"##
    );
    let (game, _warnings) = load_with(&game_json(20, 20, "", ""), &scene_of(&[a, &b]), rules, None)
        .expect("должно загрузиться");
    game
}

fn hits(game: &Game) -> f64 {
    let hits = game.properties.resolve("hits").expect("hits объявлен");
    game.world.number_like(0, hits).unwrap_or(0.0)
}

#[test]
fn a_depth_layer_does_not_collide_until_parallax_returns_to_one() {
    let mut game = colliding_pair("", COLLIDE_RULES);
    step(&mut game, 1);
    assert_eq!(hits(&game), 1.0, "обычный объект сталкивается");

    let mut game = colliding_pair("", COLLIDE_RULES);
    game.world.set_number(1, property::PARALLAX, 0.5);
    step(&mut game, 3);
    assert_eq!(hits(&game), 0.0, "объект слоя не сталкивается");
    game.world.set_number(1, property::PARALLAX, 1.0);
    step(&mut game, 1);
    assert_eq!(hits(&game), 1.0, "с parallax 1 столкновения вернулись");
}

#[test]
fn parallax_set_by_a_rule_during_the_game_also_stops_collisions() {
    let mut game = colliding_pair(r#","parallax":1,"slow":true"#, COLLIDE_AFTER_PARALLAX_RULE);
    step(&mut game, 3);
    assert_eq!(hits(&game), 0.0);
    assert_eq!(game.world.number_like(1, property::PARALLAX), Some(-0.5));
}

#[test]
fn a_depth_layer_takes_no_click_leads_no_camera_and_follows_no_mouse() {
    let (mut game, _warnings) = load_with(
        &game_json(20, 20, r#","view_height":10"#, ""),
        &scene_of(&[
            r#"{"position":[0,0],"size":[2,2],"camera_follows":true,"on_click":[["clicked",1]],"follow_mouse":"x"}"#,
            r#"{"position":[10,10],"size":[2,2],"camera_follows":true}"#,
        ]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");

    assert_eq!(
        on_click_target(&game.world, &game.scene, [1.0, 1.0]),
        Some(0)
    );
    assert_eq!(followed_center(&game.world), Some([1.0, 1.0]));
    game.world.set_number(0, property::PARALLAX, 0.5);
    assert_eq!(on_click_target(&game.world, &game.scene, [1.0, 1.0]), None);
    assert_eq!(
        followed_center(&game.world),
        Some([11.0, 11.0]),
        "камеру берёт следующий объект"
    );
    apply_follow_mouse(&mut game.world, Some([8.0, 8.0, 0.0]), &game.scene);
    assert_eq!(
        game.world.vec2(0, property::POSITION),
        Some([0.0, 0.0]),
        "за курсором объект слоя не идёт"
    );
    game.world.set_number(0, property::PARALLAX, 1.0);
    assert_eq!(
        on_click_target(&game.world, &game.scene, [1.0, 1.0]),
        Some(0)
    );
    apply_follow_mouse(&mut game.world, Some([8.0, 8.0, 0.0]), &game.scene);
    assert_eq!(game.world.vec2(0, property::POSITION), Some([7.0, 0.0]));
}

#[test]
fn with_only_a_depth_layer_to_follow_the_camera_does_not_move() {
    let (mut game, _warnings) = load_with(
        &game_json(20, 20, r#","view_height":10"#, ""),
        &scene_of(&[r#"{"position":[3,3],"size":[2,2],"camera_follows":true}"#]),
        NO_RULES,
        None,
    )
    .expect("должно загрузиться");
    game.world.set_number(0, property::PARALLAX, 0.5);
    assert_eq!(followed_center(&game.world), None);
}

// -------------------------------------------------------------------------------------------
// Проверка перед запуском, требования 23–25
// -------------------------------------------------------------------------------------------

fn flat(scene: &str) -> Loaded {
    load_with_props(
        PROPS_CLICKED,
        &game_json(20, 20, "", ""),
        scene,
        NO_RULES,
        None,
    )
}

fn flat_bare(scene: &str) -> Loaded {
    load_with_props(NO_PROPS, &game_json(20, 20, "", ""), scene, NO_RULES, None)
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

fn object(extra: &str) -> String {
    scene_of(&[&format!(
        r##"{{"position":[1,1],"size":[2,2],"color":"#336699"{extra}}}"##
    )])
}

#[test]
fn parallax_that_is_not_a_number_is_an_error() {
    expect_error(flat(&object(r#","parallax":"far""#)), "parallax", "число");
}

#[test]
fn parallax_below_zero_in_a_file_is_an_error() {
    expect_error(
        flat(&object(r#","parallax":-0.5"#)),
        "parallax",
        "меньше нуля",
    );
}

#[test]
fn parallax_below_zero_in_a_rule_constant_is_an_error() {
    for set in [r#"-1"#, r#"{"table":[0.5,-0.25],"by":"clicked"}"#] {
        let rules = format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["position"]}},"do":[["set","parallax",{set}]]}}]}}"#
        );
        let result = load_with_props(
            PROPS_CLICKED,
            &game_json(20, 20, "", ""),
            &object(""),
            &rules,
            None,
        );
        expect_error(result, "do", "меньше нуля");
    }
    let rules =
        r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["set","parallax",0.5]]}]}"#;
    let loaded = load_with_props(
        PROPS_CLICKED,
        &game_json(20, 20, "", ""),
        &object(""),
        rules,
        None,
    );
    assert!(loaded.is_ok(), "{:?}", loaded.err().map(|f| f.errors));
}

#[test]
fn repeat_x_that_is_not_a_flag_is_an_error() {
    expect_error(flat(&object(r#","repeat_x":"yes""#)), "repeat_x", "признак");
}

fn three_d(scene: &str, rules: &str) -> Loaded {
    load_with(&game_json(20, 20, CAMERA_3D, ""), scene, rules, None)
}

#[test]
fn parallax_in_a_three_dimensional_scene_is_an_error() {
    expect_error(
        three_d(&object(r#","parallax":0.5"#), NO_RULES),
        "parallax",
        "плоской",
    );
}

#[test]
fn repeat_x_in_a_three_dimensional_scene_is_an_error() {
    expect_error(
        three_d(&object(r#","repeat_x":true"#), NO_RULES),
        "repeat_x",
        "плоской",
    );
}

#[test]
fn depth_layer_properties_in_keys_clicks_and_rules_of_a_three_dimensional_scene_are_errors() {
    let keys = object(r#","keys":{"KeyA":{"press":[["repeat_x",true]]}}"#);
    expect_error(three_d(&keys, NO_RULES), "repeat_x", "плоской");
    let click = object(r#","on_click":[["parallax",0.5]]"#);
    expect_error(three_d(&click, NO_RULES), "parallax", "плоской");
    let plain = object("");
    for rules in [
        r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["add","parallax",1]]}]}"#,
        r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["set","parallax",1]]}]}"#,
        r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["set","repeat_x",true]]}]}"#,
        r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["give","repeat_x",{"has":["size"]}]]}]}"#,
        r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["take","repeat_x"]]}]}"#,
    ] {
        let errors = errors_of(three_d(&plain, rules));
        assert!(
            errors.iter().any(|e| e.contains("плоской")),
            "{rules}: {errors:#?}"
        );
    }
}

#[test]
fn a_depth_layer_with_collides_is_an_error() {
    expect_error(
        flat(&object(r#","parallax":0.5,"collides":true"#)),
        "objects[0]",
        "collides",
    );
}

#[test]
fn a_depth_layer_with_on_click_is_an_error() {
    expect_error(
        flat(&object(r#","parallax":0.5,"on_click":[["clicked",1]]"#)),
        "objects[0]",
        "on_click",
    );
}

#[test]
fn a_depth_layer_with_camera_follows_is_an_error() {
    expect_error(
        flat(&object(r#","parallax":0.5,"camera_follows":true"#)),
        "objects[0]",
        "camera_follows",
    );
}

#[test]
fn a_depth_layer_with_follow_mouse_is_an_error() {
    expect_error(
        flat(&object(r#","parallax":0.5,"follow_mouse":"x""#)),
        "objects[0]",
        "follow_mouse",
    );
}

#[test]
fn parallax_one_and_a_false_flag_do_not_clash_with_the_pointer_properties() {
    let (_game, warnings) = flat(&object(
        r#","parallax":1,"collides":true,"on_click":[["clicked",1]]"#,
    ))
    .expect("parallax 1 — обычный объект");
    assert_eq!(warnings, Vec::new());
    let (_game, warnings) = flat_bare(&object(r#","parallax":0.5,"collides":false"#))
        .expect("collides: false объекту слоя не мешает");
    assert_eq!(warnings, Vec::new());
}

#[test]
fn a_spawn_template_with_a_depth_layer_and_collides_is_an_error() {
    let rules = r##"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["slow"]}}},
        "where":{"at":[3,3]},"template":{"slow":true,"size":[1,1],"color":"#ff0000","parallax":0.5,"collides":true}}]}"##;
    let result = load_with(&game_json(20, 20, "", ""), &object(""), rules, None);
    expect_error(result, "template", "collides");
}

#[test]
fn repeat_x_without_any_fill_is_an_error() {
    let result = flat(&scene_of(&[
        r#"{"position":[1,1],"size":[2,2],"repeat_x":true}"#,
    ]));
    expect_error(result, "objects[0]", "repeat_x");
}

#[test]
fn repeat_x_gets_no_error_when_a_key_or_a_rule_can_give_it_a_color() {
    let by_key = scene_of(&[
        r##"{"position":[1,1],"size":[2,2],"repeat_x":true,"keys":{"KeyA":{"press":[["color","#ff0000"]]}}}"##,
    ]);
    flat(&by_key).expect("цвет от клавиши — не ошибка");
    let by_rule = r##"{"rules":[{"kind":"collide","a":{"has":["repeat_x"]},"b":{"has":["wall"]},
        "effects":{"a":[["set","color","#ff0000"]],"b":[]}}]}"##;
    load_with(
        &game_json(20, 20, "", ""),
        &scene_of(&[
            r#"{"position":[1,1],"size":[2,2],"collides":true,"repeat_x":true}"#,
            r##"{"position":[5,5],"size":[2,2],"collides":true,"wall":true,"color":"#00ff00"}"##,
        ]),
        by_rule,
        None,
    )
    .expect("цвет от правила — не ошибка");
}

#[test]
fn the_words_color_or_image_in_code_silence_the_repeat_x_fill_check() {
    let scene = scene_of(&[r#"{"position":[1,1],"size":[2,2],"repeat_x":true}"#]);
    for code in ["-- color\n", "-- image\n"] {
        load_with(
            &game_json(20, 20, "", r#","code":"code.lua""#),
            &scene,
            NO_RULES,
            Some(code),
        )
        .unwrap_or_else(|e| panic!("{code}: {e:?}"));
    }
    let silent = load_with(
        &game_json(20, 20, "", r#","code":"code.lua""#),
        &scene,
        NO_RULES,
        Some("-- nothing\n"),
    );
    expect_error(silent, "objects[0]", "repeat_x");
}

#[test]
fn the_outside_scene_warning_skips_depth_layers_and_checks_only_the_height_of_repeats() {
    let far_foreground = r##"{"position":[-30,40],"size":[2,2],"color":"#112233","parallax":1.4}"##;
    let (_game, warnings) = flat_bare(&scene_of(&[far_foreground])).expect("загружается");
    assert_eq!(warnings, Vec::new(), "parallax 1,4 за краем — молчание");

    let below = r##"{"position":[1,30],"size":[24,4],"color":"#112233","repeat_x":true}"##;
    let (_game, warnings) = flat_bare(&scene_of(&[below])).expect("загружается");
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].message.contains("за пределами"), "{warnings:?}");

    let beside = r##"{"position":[-90,5],"size":[24,4],"color":"#112233","repeat_x":true}"##;
    let (_game, warnings) = flat_bare(&scene_of(&[beside])).expect("загружается");
    assert_eq!(warnings, Vec::new(), "по ширине повтор закрывает окно");
}

// -------------------------------------------------------------------------------------------
// Код игры, требование 24
// -------------------------------------------------------------------------------------------

const CODE_RULES: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["wall"]},"do":[["run","go"]]}]}"#;

fn code_error_of(scene_extra: &str, line: &str) -> Option<String> {
    let scene = scene_of(&[
        r##"{"position":[3,3],"size":[1,1],"wall":true,"color":"#ff0000","parallax":0.5}"##,
    ]);
    let (mut game, _warnings) = load_with(
        &game_json(20, 20, scene_extra, r#","code":"code.lua""#),
        &scene,
        CODE_RULES,
        Some(&format!("function go(obj)\n    {line}\nend\n")),
    )
    .unwrap_or_else(|e| panic!("{line}: {e:?}"));
    step(&mut game, 1);
    game.code_error().map(|error| error.message.clone())
}

#[test]
fn code_writing_parallax_below_zero_is_a_code_error() {
    let message = code_error_of("", "obj.parallax = -1").expect("ошибка кода");
    assert!(message.contains("меньше нуля"), "{message}");
}

#[test]
fn code_writing_parallax_as_not_a_number_is_a_code_error() {
    let message = code_error_of("", r#"obj.parallax = "far""#).expect("ошибка кода");
    assert!(message.contains("число"), "{message}");
}

#[test]
fn code_writing_repeat_x_as_not_a_flag_is_a_code_error() {
    let message = code_error_of("", "obj.repeat_x = 1").expect("ошибка кода");
    assert!(message.contains("признак"), "{message}");
}

#[test]
fn code_writing_depth_layer_properties_in_a_three_dimensional_scene_is_a_code_error() {
    let scene = scene_of(&[r##"{"position":[3,3],"size":[1,1],"wall":true,"color":"#ff0000"}"##]);
    for line in ["obj.parallax = 0.5", "obj.repeat_x = true"] {
        let (mut game, _warnings) = load_with(
            &game_json(20, 20, CAMERA_3D, r#","code":"code.lua""#),
            &scene,
            CODE_RULES,
            Some(&format!("function go(obj)\n    {line}\nend\n")),
        )
        .unwrap_or_else(|e| panic!("{line}: {e:?}"));
        step(&mut game, 1);
        let message = game
            .code_error()
            .map(|error| error.message.clone())
            .unwrap_or_else(|| panic!("{line}: ошибки кода нет"));
        assert!(message.contains("плоской сцене"), "{line}: {message}");
    }
}

#[test]
fn code_reads_and_writes_the_depth_layer_properties_in_a_flat_scene() {
    let line = "assert(obj.parallax == 0.5)\n    obj.parallax = 0.25\n    obj.repeat_x = true\n    obj.repeat_x = false";
    assert_eq!(code_error_of("", line), None);
}

// -------------------------------------------------------------------------------------------
// Заготовка платформера, требования 26–32
// -------------------------------------------------------------------------------------------

fn platformer_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("platformer")
}

fn platformer() -> (Game, Vec<GameError>) {
    let read = |name: &str| {
        fs::read_to_string(platformer_dir().join(name))
            .unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"))
    };
    load_game_from_texts_with_code(
        &read("game.json"),
        &read("properties.json"),
        &read("scene.json"),
        &read("rules.json"),
        &read("screens.json"),
        Some(&read("code.lua")),
    )
    .map(|(game, _screens, warnings)| (game, warnings))
    .unwrap_or_else(|e| panic!("платформер не загрузился: {e:?}"))
}

#[test]
fn the_platformer_blank_loads_without_errors_or_warnings() {
    let (game, warnings) = platformer();
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert_eq!(
        game.world.text(0, property::NAME),
        Some("viewer"),
        "точка осмотра — первый объект сцены"
    );
    assert_eq!(game.world.alive_count(), 1 + 3 + 1 + 17 + 13 + 6 + 15);
}

#[test]
fn the_platformer_starts_with_the_camera_at_the_bottom_of_the_scene() {
    let (game, _warnings) = platformer();
    let (scale, offset) = game.camera_frame(WINDOW);
    let layers = LayerView::of_frame(&game.scene, scale, offset, WINDOW);
    assert!(
        (scale - 60.0).abs() < 1e-3,
        "12 клеток по высоте окна: {scale}"
    );
    assert!(
        (layers.shift[1] - 6.0).abs() < 1e-3,
        "камера у низа: {layers:?}"
    );
}

#[test]
fn sky_and_hills_cover_a_window_of_any_width() {
    let (game, _warnings) = platformer();
    for width in [400.0_f32, 1280.0, 3000.0] {
        let viewport = [width, 720.0];
        let layers = layers_of(&game, viewport);
        let middle = 80.0 + layers.shift[0];
        let (left, right) = (
            middle - layers.window_cells / 2.0,
            middle + layers.window_cells / 2.0,
        );
        for (what, ids) in [("небо", 1..=3), ("холмы", 4..=4)] {
            for id in ids {
                let mut drawn: Vec<(f32, f32)> = paints_of_object(&game, viewport, id)
                    .iter()
                    .map(|paint| (paint.position[0], paint.position[0] + paint.size[0]))
                    .collect();
                drawn.sort_by(|a, b| a.0.total_cmp(&b.0));
                assert!(
                    drawn.first().unwrap().0 <= left as f32,
                    "{what} {id}, окно {width}"
                );
                assert!(
                    drawn.last().unwrap().1 >= right as f32,
                    "{what} {id}, окно {width}"
                );
                assert!(
                    drawn
                        .windows(2)
                        .all(|pair| (pair[0].1 - pair[1].0).abs() < 1e-3),
                    "{what} {id}: копии встык, окно {width}: {drawn:?}"
                );
            }
        }
    }
}

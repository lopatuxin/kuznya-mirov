//! Фаза 11 — камера, щелчок в мире и ходьба (требования 1–47). Игр в `games/` не заводит —
//! каждый тест собирает свою маленькую игру в коде, как `tests/phase6_engine.rs`.

use std::time::Instant;

use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::property;
use engine::core::report::RuleFired;
use engine::core::runner::STEP_SECONDS;
use engine::core::scene::LayerView;
use engine::core::screens::{self, ScreenState};
use engine::data::load::{
    LoadFailure, load_game_from_texts, load_game_from_texts_with_code, load_rest, read_entry,
};
use engine::data::session::PlaySession;
use engine::render::wind::Motion;

const VIEWPORT: [f32; 2] = [800.0, 600.0];

fn err_text(result: &Result<impl std::fmt::Debug, LoadFailure>) -> String {
    match result {
        Err(f) => f
            .errors
            .iter()
            .map(|e| format!("{} ({})", e.message, e.path))
            .collect::<Vec<_>>()
            .join(" | "),
        Ok(_) => String::new(),
    }
}

fn game_json(width: u32, height: u32, extra_scene: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"{extra_scene}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{{}}}}}}"##
    )
}

/// Same as `game_json`, plus a `"ui"` font declared — `load_world`'s own tests drive `screens.json`
/// buttons, which need one to resolve their own `font` field.
fn game_json_with_font(width: u32, height: u32) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{{"ui":"ui.ttf"}}}}}}"##
    )
}

fn game_json_with_code(width: u32, height: u32) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","code":"code.lua","fonts":{{}}}}}}"##
    )
}

const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

// ------------------------------------------------------------------------------------------
// Камера — требования 1–7, 41
// ------------------------------------------------------------------------------------------

/// «Камера», требование 3: окно 1600×900 при view_height 12 — 75 пикселей в клетке, 21⅓ клетки
/// по ширине.
#[test]
fn camera_frame_uses_view_height_for_the_cell_size() {
    let game = game_json(100, 100, r#","view_height":12"#);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[50,50],"size":[2,2],"camera_follows":true}]}"#;
    let (game, _s, warnings) =
        load_game_from_texts(&game, props, scene, r#"{"rules":[]}"#, SCREENS)
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let (scale, _offset) = game.camera_frame([1600.0, 900.0]);
    assert!((scale - 75.0).abs() < 1e-3, "{scale}");
}

/// «Камера», требование 5–6: камера стоит на объекте `camera_follows` сразу в собранном мире и
/// без отставания идёт за ним после каждого шага.
#[test]
fn camera_follows_its_object_immediately_and_without_lag_after_a_step() {
    let game = game_json(100, 100, r#","view_height":10"#);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[
        {"position":[9,9],"size":[2,2],"camera_follows":true,"velocity":[60,0]}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    let viewport = [1000.0, 1000.0]; // scale 100
    let (_, offset_before) = game.camera_frame(viewport);
    game.step(StepInput::empty());
    let (_, offset_after) = game.camera_frame(viewport);
    assert_ne!(
        offset_before, offset_after,
        "камера должна была сдвинуться вместе с объектом на этом же шаге"
    );
}

/// «Камера», требование 5: без объекта `camera_follows` в только что собранном мире — середина
/// сцены.
#[test]
fn camera_is_scene_center_in_a_fresh_world_without_a_camera_object() {
    let game = game_json(40, 40, r#","view_height":10"#);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1]}]}"#;
    let (game, _s, _w) = load_game_from_texts(&game, props, scene, r#"{"rules":[]}"#, SCREENS)
        .expect("должно загрузиться");
    let viewport = [1000.0, 1000.0];
    let (scale, offset) = game.camera_frame(viewport);
    let cell = game
        .scene
        .window_to_scene_frame([500.0, 500.0], scale, offset);
    assert!((cell[0] - 20.0).abs() < 1e-3, "{cell:?}");
    assert!((cell[1] - 20.0).abs() < 1e-3, "{cell:?}");
}

/// «Камера», требование 2: без `view_height` — прежнее вписывание сцены целиком, независимо от
/// `camera_follows`.
#[test]
fn without_view_height_the_frame_is_the_plain_letterbox_regardless_of_camera_follows() {
    let game = game_json(10, 20, "");
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[9,19],"size":[1,1],"camera_follows":true}]}"#;
    let (game, _s, warnings) =
        load_game_from_texts(&game, props, scene, r#"{"rules":[]}"#, SCREENS)
            .expect("должно загрузиться");
    assert_eq!(
        warnings.len(),
        1,
        "camera_follows без view_height — предупреждение: {warnings:?}"
    );
    assert!(warnings[0].message.contains("camera_follows"));
    let viewport = [800.0, 600.0];
    assert_eq!(
        game.camera_frame(viewport),
        engine::core::scene::letterbox(viewport, [10.0, 20.0])
    );
}

// ------------------------------------------------------------------------------------------
// Порядок рисования — требования 8–10
// ------------------------------------------------------------------------------------------

/// «Порядок рисования», требование 9: при равных `layer` и `y_sort` поверх тот, чей нижний край
/// ниже — доведено до отрисовки (`compose_world_paints`) и до выбора щелчком (`on_click_target`).
#[test]
fn y_sort_orders_by_the_lower_bottom_edge_when_layers_tie() {
    let game = game_json(20, 20, r#","y_sort":true"#);
    let props = r#"{"properties":{}}"#;
    // Both rectangles overlap at, say, (1, 2) — object 1's bottom edge (y=4) is lower than
    // object 0's (y=3), so it draws on top and wins the click there.
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[3,3],"color":"#ff0000","on_click":[["color","#00ff00"]]},
        {"position":[0,1],"size":[3,3],"color":"#0000ff","on_click":[["color","#ffff00"]]}
    ]}"##;
    let (game, _s, _w) = load_game_from_texts(&game, props, scene, r#"{"rules":[]}"#, SCREENS)
        .expect("должно загрузиться");
    let images = Vec::new();
    let atlas_rects = Vec::new();
    let paints = engine::render::atlas::compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        &Motion::default(),
        &images,
        &atlas_rects,
        &LayerView::default(),
    );
    assert_eq!(paints[0].color, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(paints[1].color, [0.0, 0.0, 1.0, 1.0]);

    let target = engine::core::scene::on_click_target(&game.world, &game.scene, [1.0, 2.0]);
    assert_eq!(
        target,
        Some(1),
        "id 1 накрывает ту же точку и рисуется выше"
    );
}

/// «Порядок рисования», требование 9: `layer` сильнее `y_sort` — объект со слоем 1 всегда поверх
/// слоя 0, даже если его нижний край выше.
#[test]
fn layer_wins_over_y_sort() {
    let game = game_json(20, 20, r#","y_sort":true"#);
    let props = r#"{"properties":{}}"#;
    let scene = r##"{"objects":[
        {"position":[0,5],"size":[2,2],"color":"#0000ff","layer":0},
        {"position":[0,0],"size":[2,2],"color":"#ff0000","layer":1}
    ]}"##;
    let (game, _s, _w) = load_game_from_texts(&game, props, scene, r#"{"rules":[]}"#, SCREENS)
        .expect("должно загрузиться");
    let paints = engine::render::atlas::compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        &Motion::default(),
        &[],
        &[],
        &LayerView::default(),
    );
    // Layer 1 (id 1) always draws last, on top, regardless of its own bottom edge.
    assert_eq!(paints.last().unwrap().color, [1.0, 0.0, 0.0, 1.0]);
}

// ------------------------------------------------------------------------------------------
// Мышь в мире — требования 11–21, 36
// ------------------------------------------------------------------------------------------

fn load_world(
    game: &str,
    props: &str,
    scene: &str,
    rules: &str,
    screens_json: &str,
) -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _w) = read_entry(game).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))];
    let (game, screens, warnings, _images) = load_rest(
        game,
        config,
        Some(props),
        Some(scene),
        Some(rules),
        Some(screens_json),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
        false,
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    (game, screens)
}

/// «Мышь в мире», требование 13: нажатие над кнопкой активного экрана до мира не доходит.
#[test]
fn a_press_over_a_button_never_reaches_the_world_as_mouseleft() {
    let game = game_json_with_font(10, 10);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],
        "keys":{"MouseLeft":{"press":[["walk_to","cursor"]]}}}]}"#;
    let screens_json = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"button","anchor":"top_left","offset":[0,0],"size":[50,50],"text":"",
         "font":"ui","color":"#ffffff","on_click":["resume"]}
    ]}]}"##;
    let (mut game, config) = load_world(&game, props, scene, r#"{"rules":[]}"#, screens_json);
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = engine::core::runner::Runner::new();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    queue.push_mouse_move(25.0, 25.0);
    queue.push_mouse_down();
    queue.push_mouse_up();
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        STEP_SECONDS,
    );
    let walk_to = engine::core::property::WALK_TO;
    assert_eq!(
        game.world.vec2(0, walk_to),
        None,
        "клик по кнопке не должен дойти до мира"
    );
}

/// «Мышь в мире», требования 14, 17–18: щелчок по земле пишет точку в `walk_to` через
/// `"cursor"`, а в `position` — под середину, с удержанием в сцене.
#[test]
fn a_click_on_the_ground_reaches_the_world_and_cursor_resolves_in_walk_to_and_position() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],
         "keys":{"MouseLeft":{"press":[["walk_to","cursor"]]}}},
        {"position":[0,0],"size":[2,2],
         "keys":{"MouseLeft":{"press":[["position","cursor"]]}}}
    ]}"#;
    let (mut game, _config) = load_world(&game, props, scene, r#"{"rules":[]}"#, SCREENS);
    // No UI elements on this screen, so the pixel→scene mapping the real mouse event would go
    // through is beside the point here — set the world cursor directly, in scene cells, and
    // drive the world-bound `MouseLeft` press the same way a screen-absorbed key never would.
    game.set_cursor_cell([1.0, 1.0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    let walk_to = property::WALK_TO;
    assert_eq!(game.world.vec2(0, walk_to), Some([1.0, 1.0]));
    // Object 1 is 2x2: its position must keep it fully in scene, midpoint under the cursor.
    assert_eq!(game.world.vec2(1, property::POSITION), Some([0.0, 0.0]));
}

/// «Мышь в мире», требования 19–20: `on_click` срабатывает у верхнего объекта, а `keys`
/// применяются первыми — обе записи на одно и то же свойство остаются со значением `on_click`.
#[test]
fn on_click_fires_on_the_topmost_object_after_keys_edits_of_the_same_press() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{"tag":"number"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[10,10],"tag":0,
         "keys":{"MouseLeft":{"press":[["tag",1]]}},"on_click":[["tag",2]]}
    ]}"#;
    let (mut game, _config) = load_world(&game, props, scene, r#"{"rules":[]}"#, SCREENS);
    game.set_cursor_cell([5.0, 5.0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    let tag = game.properties.resolve("tag").unwrap();
    assert_eq!(
        game.world.number_like(0, tag),
        Some(2.0),
        "on_click перезаписал keys"
    );
}

/// «Мышь в мире», требование 19: объект без `on_click` щелчок не перехватывает — невидимая зона
/// щелчка (без цвета/картинки) с `on_click` всё равно ловит его.
#[test]
fn on_click_fires_on_an_invisible_object_but_never_on_one_without_on_click() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{"hit":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[10,10]},
        {"position":[0,0],"size":[10,10],"on_click":[["hit",true]]}
    ]}"#;
    let (mut game, _config) = load_world(&game, props, scene, r#"{"rules":[]}"#, SCREENS);
    game.set_cursor_cell([5.0, 5.0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    let hit = game.properties.resolve("hit").unwrap();
    assert!(
        game.world.flag(1, hit),
        "невидимая зона щелчка должна была сработать"
    );
}

/// «Крайние случаи»: точка под курсором ещё неизвестна — мышь ни разу не двигалась; запись с
/// `"cursor"` пропускается, `on_click` не срабатывает, а остальные записи применяются.
#[test]
fn without_a_known_cursor_point_the_cursor_record_is_skipped_and_on_click_never_fires() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{"tag":"number","hit":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[10,10],"tag":0,
         "keys":{"MouseLeft":{"press":[["walk_to","cursor"],["tag",7]]}},
         "on_click":[["hit",true]]}
    ]}"#;
    let (mut game, _config) = load_world(&game, props, scene, r#"{"rules":[]}"#, SCREENS);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    let tag = game.properties.resolve("tag").unwrap();
    let hit = game.properties.resolve("hit").unwrap();
    assert_eq!(
        game.world.vec2(0, property::WALK_TO),
        None,
        "cursor неизвестен — запись пропущена"
    );
    assert_eq!(
        game.world.number_like(0, tag),
        Some(7.0),
        "остальные записи применяются"
    );
    assert!(
        !game.world.flag(0, hit),
        "on_click без известной точки не срабатывает"
    );
}

/// «Мышь в мире», требование 15: отпускание идёт туда, куда пошло нажатие — нажали над миром,
/// отпустили над кнопкой, кнопка не срабатывает, а мир получает отпускание.
#[test]
fn release_goes_wherever_the_press_went_even_over_a_button_later() {
    let game = game_json_with_font(10, 10);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],
        "keys":{"MouseLeft":{"press":[["velocity",[1,0]]],"release":[["velocity",[0,0]]]}}}]}"#;
    let screens_json = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"button","anchor":"top_left","offset":[0,0],"size":[50,50],"text":"",
         "font":"ui","color":"#ffffff","on_click":["resume"]}
    ]}]}"##;
    let (mut game, config) = load_world(&game, props, scene, r#"{"rules":[]}"#, screens_json);
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = engine::core::runner::Runner::new();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    // Press over open ground (not the button, which sits at top-left 50x50).
    queue.push_mouse_move(400.0, 400.0);
    queue.push_mouse_down();
    // Move over the button, then release there.
    queue.push_mouse_move(10.0, 10.0);
    queue.push_mouse_up();
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        STEP_SECONDS,
    );
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "мир должен был получить и отпускание — иначе объект остался бы с [1,0]"
    );
}

/// «Мышь в мире», требование 13: панель без кнопки под курсором забирает нажатие и никому его не
/// отдаёт.
#[test]
fn a_press_over_a_bare_panel_is_absorbed_and_never_reaches_the_world() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],
        "keys":{"MouseLeft":{"press":[["velocity",[1,0]]]}}}]}"#;
    let screens_json = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[50,50],"color":"#ffffff"}
    ]}]}"##;
    let (mut game, config) = load_world(&game, props, scene, r#"{"rules":[]}"#, screens_json);
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = engine::core::runner::Runner::new();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    queue.push_mouse_move(10.0, 10.0);
    queue.push_mouse_down();
    queue.push_mouse_up();
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        STEP_SECONDS,
    );
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        None,
        "панель должна была забрать нажатие"
    );
}

// ------------------------------------------------------------------------------------------
// Ходьба — требования 22–32
// ------------------------------------------------------------------------------------------

fn step_until_walk_stops(
    game: &mut engine::core::game::Game,
    walk_to: engine::core::property::PropertyId,
    max_steps: u32,
) {
    for _ in 0..max_steps {
        if game.world.vec2(0, walk_to).is_none() {
            return;
        }
        game.step(StepInput::empty());
    }
}

/// «Ходьба», требования 22–23: без `avoid` — по прямой, `walk_speed/60` клеток за шаг.
#[test]
fn walks_straight_at_the_declared_speed_without_avoid() {
    let game = game_json(20, 20, "");
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"walk_speed":60,"walk_to":[5,0]}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    game.step(StepInput::empty());
    // walk_speed 60/s = 1 cell/step; center starts at [0.5,0.5], moves to [1.5,0.5] -> position [1,0].
    assert_eq!(game.world.vec2(0, property::POSITION), Some([1.0, 0.0]));
}

/// «Ходьба», требования 24, 27, 26, 31: обходит препятствие, доходит, снимает `walk_to`, и
/// `after_move_of` видит сдвиг; отчёт шага несёт вид `"walk"`.
#[test]
fn walks_around_an_obstacle_arrives_clears_walk_to_and_reports_walk() {
    let game = game_json(30, 30, "");
    let props = r#"{"properties":{"obstacle":"flag","chased":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,9],"size":[2,2],"walk_speed":300,"walk_to":[15,9]},
        {"position":[9,0],"size":[1,20],"obstacle":true,"collides":true},
        {"position":[0,0],"size":[1,1]}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"walk","for":{"has":["walk_speed"]},"avoid":{"has":["obstacle"]}},
        {"kind":"check","for":{"has":["position"]},"when":{"after_move_of":{"has":["walk_speed"]}},
         "do":[["give","chased",{"has":["position"]}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    let walk_to = property::WALK_TO;
    game.begin_session();
    step_until_walk_stops(&mut game, walk_to, 600);
    assert_eq!(
        game.world.vec2(0, walk_to),
        None,
        "должен был дойти и снять walk_to"
    );
    let pos = game.world.vec2(0, property::POSITION).unwrap();
    let center = [pos[0] + 1.0, pos[1] + 1.0]; // size [2,2]
    assert!((center[0] - 15.0).abs() < 1e-3, "{center:?}");
    assert!((center[1] - 9.0).abs() < 1e-3, "{center:?}");
    let chased = game.properties.resolve("chased").unwrap();
    assert!(
        game.world.flag(2, chased),
        "after_move_of должен был увидеть ходьбу"
    );
    let report = game.last_report().expect("сессия должна собирать отчёт");
    assert!(
        report
            .fired
            .iter()
            .any(|f| matches!(f, RuleFired::Walk { .. })),
        "{report:?}"
    );
}

/// «Крайние случаи»: `walk_speed` 0 или меньше — объект стоит, `walk_to` остаётся.
#[test]
fn zero_or_negative_walk_speed_leaves_the_object_standing() {
    let game = game_json(20, 20, "");
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"walk_speed":0,"walk_to":[5,0]}]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([0.0, 0.0]));
    assert_eq!(game.world.vec2(0, property::WALK_TO), Some([5.0, 0.0]));
}

/// «Крайние случаи»: цель внутри препятствия — идущий подходит вплотную к ближайшему краю.
#[test]
fn a_target_inside_an_obstacle_stops_flush_against_its_edge() {
    let game = game_json(30, 30, "");
    let props = r#"{"properties":{"obstacle":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,9],"size":[2,2],"walk_speed":600,"walk_to":[10,10]},
        {"position":[8,8],"size":[4,4],"obstacle":true}
    ]}"#;
    let rules =
        r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]},"avoid":{"has":["obstacle"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    step_until_walk_stops(&mut game, property::WALK_TO, 200);
    let pos = game.world.vec2(0, property::POSITION).unwrap();
    // Inflated obstacle's left edge sits at 8 - 1 (half the walker's own width) = 7.
    assert!((pos[0] + 1.0 - 7.0).abs() < 1e-3, "{pos:?}");
}

/// «Крайние случаи»: край сцены — тоже стена, идущий за него не выходит.
#[test]
fn the_scene_edge_is_a_wall_the_walker_never_crosses() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{}}"#;
    let scene =
        r#"{"objects":[{"position":[4,4],"size":[1,1],"walk_speed":600,"walk_to":[-50,4.5]}]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    step_until_walk_stops(&mut game, property::WALK_TO, 200);
    let pos = game.world.vec2(0, property::POSITION).unwrap();
    assert!((pos[0] - 0.0).abs() < 1e-3, "{pos:?}");
}

/// «Ходьба», требование 28: препятствие появилось на пути посреди хода — путь считается заново.
#[test]
fn a_new_obstacle_mid_path_forces_a_detour() {
    let game = game_json(30, 30, "");
    let props = r#"{"properties":{"obstacle":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,9],"size":[2,2],"walk_speed":120,"walk_to":[15,9]},
        {"position":[9,0],"size":[1,20],"obstacle":true}
    ]}"#;
    let rules =
        r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]},"avoid":{"has":["obstacle"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    // The wall starts absent from `avoid` — give it the flag only after a couple of steps, once
    // the walker has already committed to a straight-line path through where it will stand.
    let obstacle = game.properties.resolve("obstacle").unwrap();
    game.world.set_flag(1, obstacle, false);
    game.step(StepInput::empty());
    game.step(StepInput::empty());
    game.world.set_flag(1, obstacle, true);
    step_until_walk_stops(&mut game, property::WALK_TO, 200);
    let pos = game.world.vec2(0, property::POSITION).unwrap();
    let center = [pos[0] + 1.0, pos[1] + 1.0]; // size [2,2]
    assert!((center[0] - 15.0).abs() < 1e-3, "{center:?}");
    assert!(
        (center[1] - 9.0).abs() < 1e-3,
        "должен был обойти появившуюся стену: {center:?}"
    );
}

/// «Ходьба», нефункциональное требование: с двумястами препятствиями поиск пути с нуля не
/// дольше 4 мс в `cargo test --release`.
#[test]
fn pathfinding_with_two_hundred_obstacles_is_fast_in_release() {
    // 20 of the 200 actually stand in the walker's own way — a wall with a one-cell gap between
    // it and its target, exactly like the smaller "walks around a wall" tests above; the other
    // 180 are scattered well off to the side, on the rest of a scene big enough to hold a level
    // with two hundred obstacles in it without every one of them bearing on this one walk.
    let mut props_objects = String::new();
    for y in 0..20 {
        if y == 10 {
            continue; // the gap
        }
        props_objects.push_str(&format!(
            r#",{{"position":[20,{y}],"size":[1,1],"obstacle":true}}"#
        ));
    }
    for i in 0..180 {
        let x = 200 + (i % 20) * 4;
        let y = 200 + (i / 20) * 4;
        props_objects.push_str(&format!(
            r#",{{"position":[{x},{y}],"size":[2,2],"obstacle":true}}"#
        ));
    }
    let game = game_json(300, 300, "");
    let props = r#"{"properties":{"obstacle":"flag"}}"#;
    let scene = format!(
        r#"{{"objects":[{{"position":[10,9],"size":[1,1],"walk_speed":300,"walk_to":[40,10]}}{props_objects}]}}"#
    );
    let rules =
        r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]},"avoid":{"has":["obstacle"]}}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(&game, props, &scene, rules, SCREENS).expect("должно загрузиться");
    let started = Instant::now();
    game.step(StepInput::empty());
    let elapsed = started.elapsed();
    println!("поиск пути среди 200 препятствий занял {elapsed:?}");
    let pos = game.world.vec2(0, property::POSITION).unwrap();
    assert_ne!(pos, [10.0, 9.0], "должен был сдвинуться на первом шаге");
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_secs_f64() < 0.004,
        "поиск пути занял {elapsed:?} — дольше 4 мс"
    );
}

// ------------------------------------------------------------------------------------------
// Код игры — требование 34
// ------------------------------------------------------------------------------------------

/// «Код игры», требование 34: `camera_follows` (признак), `walk_speed` (число), `walk_to`
/// (пара) читаются и пишутся кодом; `nil` в `walk_to` останавливает объект.
#[test]
fn code_reads_and_writes_the_new_engine_properties() {
    let game_text = game_json_with_code(20, 20);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"camera_follows":false,"walk_speed":2,"walk_to":[3,4]}
    ]}"#;
    let code = r##"
function touch(obj)
    assert(obj.camera_follows == false)
    obj.camera_follows = true
    assert(obj.walk_speed == 2)
    obj.walk_speed = 5
    assert(obj.walk_to.x == 3 and obj.walk_to.y == 4)
    obj.walk_to = nil
end
"##;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["position"]},"do":[["run","touch"]]}
    ]}"#;
    let (mut game, _s, warnings) =
        load_game_from_texts_with_code(&game_text, props, scene, rules, SCREENS, Some(code))
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert!(game.world.flag(0, property::CAMERA_FOLLOWS));
    assert_eq!(game.world.number_like(0, property::WALK_SPEED), Some(5.0));
    assert_eq!(game.world.vec2(0, property::WALK_TO), None);
}

/// «Код игры», требование 34: `on_click` коду недоступен, как `keys`.
#[test]
fn code_cannot_access_on_click() {
    let game = game_json_with_code(20, 20);
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"on_click":[["position",[1,1]]]}]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["position"]},"do":[["run","touch"]]}
    ]}"#;
    let code = r#"
function touch(obj)
    local ok, err = pcall(function() return obj.on_click end)
    assert(not ok)
end
"#;
    let (mut game, _s, _w) =
        load_game_from_texts_with_code(&game, props, scene, rules, SCREENS, Some(code))
            .expect("должно загрузиться");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
}

// ------------------------------------------------------------------------------------------
// Проверка перед запуском — требования 35–37
// ------------------------------------------------------------------------------------------

#[test]
fn view_height_zero_or_negative_is_a_prestart_error() {
    let game = game_json(10, 10, r#","view_height":0"#);
    let result = load_game_from_texts(
        &game,
        r#"{"properties":{}}"#,
        r#"{"objects":[]}"#,
        r#"{"rules":[]}"#,
        SCREENS,
    );
    assert!(result.is_err());
    assert!(
        err_text(&result).contains("view_height"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn y_sort_not_a_boolean_is_a_prestart_error() {
    let game = game_json(10, 10, r#","y_sort":"yes""#);
    let result = load_game_from_texts(
        &game,
        r#"{"properties":{}}"#,
        r#"{"objects":[]}"#,
        r#"{"rules":[]}"#,
        SCREENS,
    );
    assert!(result.is_err());
    assert!(
        err_text(&result).contains("y_sort"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn cursor_on_a_non_pair_property_is_a_prestart_error() {
    let game = game_json(10, 10, "");
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],
        "keys":{"MouseLeft":{"press":[["layer","cursor"]]}}}]}"#;
    let result = load_game_from_texts(
        &game,
        r#"{"properties":{}}"#,
        scene,
        r#"{"rules":[]}"#,
        SCREENS,
    );
    assert!(result.is_err());
    assert!(
        err_text(&result).contains("cursor"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn on_click_naming_an_unknown_property_is_a_prestart_error() {
    let game = game_json(10, 10, "");
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],
        "on_click":[["nope",1]]}]}"#;
    let result = load_game_from_texts(
        &game,
        r#"{"properties":{}}"#,
        scene,
        r#"{"rules":[]}"#,
        SCREENS,
    );
    assert!(result.is_err());
    assert!(err_text(&result).contains("nope"), "{}", err_text(&result));
}

#[test]
fn camera_follows_without_position_or_size_is_a_prestart_error() {
    let game = game_json(10, 10, "");
    let scene = r#"{"objects":[{"camera_follows":true}]}"#;
    let result = load_game_from_texts(
        &game,
        r#"{"properties":{}}"#,
        scene,
        r#"{"rules":[]}"#,
        SCREENS,
    );
    assert!(result.is_err());
    assert!(
        err_text(&result).contains("camera_follows"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn walk_without_walk_speed_is_a_prestart_error() {
    let game = game_json(10, 10, "");
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1]}]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["position"]}}]}"#;
    let result = load_game_from_texts(&game, r#"{"properties":{}}"#, scene, rules, SCREENS);
    assert!(result.is_err());
    assert!(
        err_text(&result).contains("walk_speed"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn an_unknown_key_in_a_walk_rule_is_a_prestart_error() {
    let game = game_json(10, 10, "");
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["position"]},"bogus":1}]}"#;
    let result = load_game_from_texts(&game, r#"{"properties":{}}"#, scene, rules, SCREENS);
    assert!(result.is_err());
    assert!(err_text(&result).contains("bogus"), "{}", err_text(&result));
}

#[test]
fn mouseleft_as_a_screen_key_is_a_prestart_error() {
    let game = game_json(10, 10, "");
    let screens_json = r#"{"screens":[{"name":"main","world_runs":true,"keys":{"MouseLeft":["resume"]},"elements":[]}]}"#;
    let result = load_game_from_texts(
        &game,
        r#"{"properties":{}}"#,
        r#"{"objects":[]}"#,
        r#"{"rules":[]}"#,
        screens_json,
    );
    assert!(result.is_err());
    assert!(
        err_text(&result).contains("MouseLeft"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn an_empty_avoid_selector_is_a_prestart_warning() {
    let game = game_json(10, 10, "");
    let props = r#"{"properties":{"nonexistent":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"walk_speed":1}]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]},"avoid":{"has":["nonexistent"]}}]}"#;
    let (_game, _s, warnings) =
        load_game_from_texts(&game, props, scene, rules, SCREENS).expect("должно загрузиться");
    assert!(
        warnings.iter().any(|w| w.path.contains("avoid")),
        "{warnings:?}"
    );
}

// ------------------------------------------------------------------------------------------
// Запись и повтор — требования 38–40
// ------------------------------------------------------------------------------------------

/// «Запись и повтор», требования 38–40: партия с `MouseLeft`, `"cursor"` и ходьбой проигрывается
/// шаг в шаг в окне другой формы — «в повторе точку задаёт только запись».
#[test]
fn a_partiya_with_mouseleft_cursor_and_walk_replays_step_for_step_at_a_different_canvas_size() {
    let game_json = game_json(20, 20, "");
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"walk_speed":600,
         "keys":{"MouseLeft":{"press":[["walk_to","cursor"]]}}}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"walk","for":{"has":["walk_speed"]}}]}"#;
    let (config, _w) = read_entry(&game_json).expect("game.json должен разбираться");
    let (mut game, screens, warnings, _images) = load_rest(
        &game_json,
        config,
        Some(props),
        Some(scene),
        Some(rules),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &[],
        None,
        false,
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    let mut state = ScreenState::new(screens.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &screens, &mut state);
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();

    // 20x20 scene in an 800x600 window: scale 30, click at (300,300) -> scene cell (10,10).
    queue.push_mouse_move(300.0, 300.0);
    queue.push_mouse_down();
    for _ in 0..40 {
        session.step_once(
            &mut queue,
            &mut mouse,
            &mut game,
            &screens,
            &mut state,
            VIEWPORT,
            &[],
        );
    }
    let live_pos = game.world.vec2(0, property::POSITION);
    let recording = session.recording_text(&game);

    let (config2, _w2) = read_entry(&game_json).expect("game.json должен разбираться");
    let (mut game2, screens2, _w3, _images2) = load_rest(
        &game_json,
        config2,
        Some(props),
        Some(scene),
        Some(rules),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &[],
        None,
        false,
    )
    .expect("должно загрузиться");
    let mut state2 = ScreenState::new(screens2.start_screen);
    let mut session2 = PlaySession::begin_replay(&recording, &mut game2, &screens2, &mut state2)
        .expect("своя же запись должна открыться");
    let other_viewport = [400.0, 300.0]; // different canvas shape — must not affect the replay
    let mut queue2 = UiQueue::new();
    let mut mouse2 = MouseState::default();
    for _ in 0..40 {
        session2.step_once(
            &mut queue2,
            &mut mouse2,
            &mut game2,
            &screens2,
            &mut state2,
            other_viewport,
            &[],
        );
    }
    assert_eq!(game2.world.vec2(0, property::POSITION), live_pos);
}

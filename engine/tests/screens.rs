//! Runtime behavior of the screen state machine — «Экраны и состояние». Prestart validation for
//! `screens.json` lives in `screens_validation.rs`; this file drives a loaded game through
//! `core::screens`' free functions the way `wasm::Engine` would.

use engine::core::input::{MouseState, UiQueue};
use engine::core::property;
use engine::core::runner::Runner;
use engine::core::screens::{self, ScreenState};
use engine::data::load::{load_rest, read_entry};

const VIEWPORT: [f32; 2] = [800.0, 600.0];

const GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"menu","loss_screen":"loss","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
/// A four-byte TrueType signature — enough to pass the font-file sniff without being a real,
/// fully parseable font; these tests never render text on the GPU.
const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];
const PROPS: &str = r##"{"properties":{"score":"number"}}"##;
// `head` never moves and carries the score a label would read; `trigger` walks off the scene to
// end the game without deleting `head`; `paddle` reacts to a key the way arkanoid's does.
const SCENE: &str = r##"{"objects":[
    {"name":"head","position":[2,2],"size":[1,1],"score":7},
    {"position":[2,2],"size":[1,1],"velocity":[120,0]},
    {"position":[0,0],"size":[1,1],"velocity":[0,0],
     "keys":{"KeyA":{"press":[["velocity",[5,0]]],"release":[["velocity",[0,0]]]}}}
]}"##;
const RULES: &str = r##"{"rules":[
    {"kind":"move","for":{"has":["position","velocity"]}},
    {"kind":"delete","for":{"has":["velocity"]},"when":"outside_scene",
     "do":[["end_game","loss"]]}
]}"##;
const SCREENS_JSON: &str = r##"{"screens":[
    {"name":"menu","world_runs":false,"elements":[
        {"kind":"button","anchor":"top_left","offset":[0,0],"size":[40,20],"text":"Играть",
         "font":"ui","color":"#ffffff","on_click":["new_game","game"]}
    ]},
    {"name":"game","world_runs":true,"elements":[
        {"kind":"button","anchor":"top_left","offset":[0,0],"size":[40,20],"text":"II",
         "font":"ui","color":"#ffffff","on_click":["show_screen","pause"]}
    ]},
    {"name":"pause","world_runs":false,"elements":[]},
    {"name":"loss","world_runs":false,"elements":[
        {"kind":"label","anchor":"center","size":[200,20],"text":"{head.score}","font":"ui"}
    ]}
]}"##;

fn load() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) = read_entry(GAME).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let (game, screens, warnings) = load_rest(
        GAME,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(RULES),
        Some(SCREENS_JSON),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    (game, screens)
}

fn label_text_of(
    screen: &engine::core::screens::Screen,
    game: &engine::core::game::Game,
) -> String {
    let engine::core::screens::Element::Label { text, .. } = &screen.elements[0] else {
        panic!("первый элемент должен быть надписью");
    };
    screens::format_text(text, &game.world, &game.properties)
}

#[test]
fn non_live_screen_does_not_step_resets_the_accumulator_and_drops_keyboard() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    assert!(!state.is_live(&config), "menu не должен быть live");

    // A key pressed on a non-live screen must never reach the queue at all.
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    queue.push_key_down("KeyA");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert!(game.take_input_snapshot().is_empty());

    // A full second of real time would be 60 steps on a live screen; here it must step zero.
    screens::tick(&mut runner, &mut game, &config, &mut state, 1.0);
    assert_eq!(
        game.step_count(),
        0,
        "шаг не должен выполняться на неигровом экране"
    );

    // The accumulator was dropped, not carried — a following small delta still doesn't step.
    screens::tick(&mut runner, &mut game, &config, &mut state, 1.0 / 120.0);
    assert_eq!(game.step_count(), 0);
}

/// «Исполнение игры» → «Шаг и кадр»: the queue is drained before this call's own steps — the
/// whole reason for the reorder — so a key queued before the call reaches `Game`'s own input
/// queue in time for those same steps, not only a later call's. Goes through
/// `screens::engine_call`, the same entry point `wasm::Engine::tick` uses, rather than a
/// hand-rolled process-then-tick pair.
#[test]
fn a_key_queued_before_the_call_is_applied_by_this_calls_own_steps() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    screens::apply_command(
        screens::ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );

    // KeyA queued before the call — the way a real key event landing between two frames would.
    queue.push_key_down("KeyA");

    // One engine call: the queue drains first, then the step runs.
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );

    let paddle = 2u32;
    assert_eq!(
        game.world.vec2(paddle, property::VELOCITY),
        Some([5.0, 0.0]),
        "нажатие, положенное в очередь до вызова, должно было примениться шагами этого же вызова"
    );
}

#[test]
fn end_game_switches_to_the_loss_screen_with_the_world_still_alive() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    screens::apply_command(
        screens::ButtonCommand::NewGame(
            config
                .screens
                .iter()
                .position(|s| s.name == "game")
                .unwrap(),
        ),
        &mut game,
        &config,
        &mut state,
    );
    assert!(state.is_live(&config));

    // The trigger object starts on top of a wall-less scene and needs several steps at its
    // velocity to cross the "outside_scene" threshold; run enough real time to guarantee it.
    for _ in 0..120 {
        screens::tick(&mut runner, &mut game, &config, &mut state, 1.0 / 60.0);
    }

    let loss_id = config
        .screens
        .iter()
        .position(|s| s.name == "loss")
        .unwrap();
    assert_eq!(
        state.active(),
        loss_id,
        "экран должен переключиться на loss"
    );
    assert!(!state.is_live(&config));
    assert!(
        game.world.has(0, property::NAME),
        "мир не уничтожен: head всё ещё там"
    );
    assert_eq!(
        label_text_of(&config.screens[loss_id], &game),
        "7",
        "надпись на экране исхода читает score"
    );
}

#[test]
fn quit_from_the_outcome_screen_clears_the_sticky_mark_so_the_menu_stays() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    let loss_id = config
        .screens
        .iter()
        .position(|s| s.name == "loss")
        .unwrap();

    screens::apply_command(
        screens::ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );
    for _ in 0..120 {
        screens::tick(&mut runner, &mut game, &config, &mut state, 1.0 / 60.0);
    }
    assert_eq!(state.active(), loss_id);

    // "В меню" on the loss screen: apply_command(Quit) takes the player back to start_screen.
    screens::apply_command(screens::ButtonCommand::Quit, &mut game, &config, &mut state);
    assert_eq!(state.active(), config.start_screen);

    // A tick on the menu must not drag the player back to loss — `quit` has to clear the
    // sticky outcome mark, since `handle_outcome` runs every tick regardless of the screen.
    screens::tick(&mut runner, &mut game, &config, &mut state, 1.0 / 60.0);
    assert_eq!(
        state.active(),
        config.start_screen,
        "quit должен снимать липкую метку исхода, а не только очищать мир"
    );
}

#[test]
fn new_game_resets_the_outcome_mark_and_the_second_run_does_not_end_instantly() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    let loss_id = config
        .screens
        .iter()
        .position(|s| s.name == "loss")
        .unwrap();

    screens::apply_command(
        screens::ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );
    for _ in 0..120 {
        screens::tick(&mut runner, &mut game, &config, &mut state, 1.0 / 60.0);
    }
    assert_eq!(state.active(), loss_id);

    // "Ещё раз": new_game again, from the loss screen.
    screens::apply_command(
        screens::ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(state.active(), game_id);
    assert!(
        game.is_running(),
        "новая партия начинается со сброшенной меткой исхода"
    );

    // One single step must not already have re-triggered the loss — the trigger object is back
    // at its starting position, same as at the very first load.
    screens::tick(&mut runner, &mut game, &config, &mut state, 1.0 / 60.0);
    assert_eq!(
        state.active(),
        game_id,
        "вторая партия не должна кончаться мгновенно"
    );
}

#[test]
fn leaving_a_live_screen_releases_held_keys_and_a_returning_player_must_press_again() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    let pause_id = config
        .screens
        .iter()
        .position(|s| s.name == "pause")
        .unwrap();
    screens::apply_command(
        screens::ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );

    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    queue.push_key_down("KeyA");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    let paddle = 2u32;
    assert_eq!(
        game.world.vec2(paddle, property::VELOCITY),
        Some([5.0, 0.0]),
        "нажатие должно было задать скорость"
    );

    // Leave for the pause screen while the key is still (physically) held down.
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(pause_id),
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(
        game.world.vec2(paddle, property::VELOCITY),
        Some([0.0, 0.0]),
        "уход с живого экрана обязан снять зажатую привязку"
    );

    // Resume: the key was never released by the player, but the engine already did — a step
    // now, before any new press, must not see the old velocity come back.
    screens::apply_command(
        screens::ButtonCommand::Resume,
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(state.active(), game_id);
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(paddle, property::VELOCITY),
        Some([0.0, 0.0]),
        "зажатая клавиша не действует до нового нажатия"
    );
}

#[test]
fn button_press_release_inside_fires_and_release_outside_does_not() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    let pause_id = config
        .screens
        .iter()
        .position(|s| s.name == "pause")
        .unwrap();
    screens::apply_command(
        screens::ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );

    let mut mouse = MouseState::default();

    // Press and release inside the "II" button (top_left, [0,0]..[40,20]) fires show_screen.
    let mut queue = UiQueue::new();
    queue.push_mouse_move(10.0, 10.0);
    queue.push_mouse_down();
    queue.push_mouse_up();
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert_eq!(
        state.active(),
        pause_id,
        "отпускание внутри границ должно сработать"
    );

    // Back to the game screen, then press inside the button but release far outside it.
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(game_id),
        &mut game,
        &config,
        &mut state,
    );
    mouse = MouseState::default();
    let mut queue = UiQueue::new();
    queue.push_mouse_move(10.0, 10.0);
    queue.push_mouse_down();
    queue.push_mouse_move(500.0, 500.0);
    queue.push_mouse_up();
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert_eq!(
        state.active(),
        game_id,
        "отпускание за границами не должно сработать"
    );
}

// «Экраны и состояние» → «Клавиши экрана»: a self-contained fixture, separate from the shared
// one above — one live screen ("arena") whose sole object binds `Space` the same way arkanoid's
// paddle does, so a test can tell "reached the world" from "did not" by the velocity it left.
const KEY_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"arena","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const KEY_PROPS: &str = r##"{"properties":{}}"##;
const KEY_SCENE: &str = r##"{"objects":[
    {"position":[0,0],"size":[1,1],"velocity":[0,0],
     "keys":{"Space":{"press":[["velocity",[9,9]]],"release":[["velocity",[0,0]]]}}}
]}"##;
const KEY_RULES: &str = r##"{"rules":[]}"##;

/// `arena` declares `Space`; releasing it inside the screen's own `keys` table shows up on
/// `paused`, the only other screen — nothing else references it.
const KEY_SCREENS_DECLARED: &str = r##"{"screens":[
    {"name":"arena","world_runs":true,
     "keys":{"Space":["show_screen","paused"]},
     "elements":[]},
    {"name":"paused","world_runs":false,"elements":[]}
]}"##;

/// Same `Space` binding on the scene object, but no screen declares it at all.
const KEY_SCREENS_UNDECLARED: &str = r##"{"screens":[
    {"name":"arena","world_runs":true,"elements":[]}
]}"##;

/// `paused` (non-live) declares `Space`; `arena` declares `Escape` back, purely so `paused`
/// counts as reachable and the loader's zero-warnings check stays meaningful.
const KEY_SCREENS_NONLIVE_WITH_KEY: &str = r##"{"screens":[
    {"name":"arena","world_runs":true,
     "keys":{"Escape":["show_screen","paused"]},
     "elements":[]},
    {"name":"paused","world_runs":false,
     "keys":{"Space":["show_screen","arena"]},
     "elements":[]}
]}"##;

/// Two LIVE screens: `arena` doesn't declare `Space` at all, `arena2` does. `Escape` on `arena`
/// exists only to keep `arena2` reachable for the loader's zero-warnings check.
const KEY_SCREENS_TWO_LIVE: &str = r##"{"screens":[
    {"name":"arena","world_runs":true,
     "keys":{"Escape":["show_screen","arena2"]},
     "elements":[]},
    {"name":"arena2","world_runs":true,
     "keys":{"Space":["show_screen","arena"]},
     "elements":[]}
]}"##;

fn load_key_fixture(
    screens_json: &str,
) -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) = read_entry(KEY_GAME).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let (game, screens, warnings) = load_rest(
        KEY_GAME,
        config,
        Some(KEY_PROPS),
        Some(KEY_SCENE),
        Some(KEY_RULES),
        Some(screens_json),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
    )
    .expect("должно загрузиться");
    // Every `KEY_SCREENS_*` fixture in this file pairs a live screen's own `Space` binding with
    // `KEY_SCENE`'s object doing the same, on purpose — «Экраны и состояние» → «Клавиши экрана»:
    // that's the exact shape the loader now warns about, so it's expected here rather than a sign
    // something broke; any *other* warning still fails the fixture.
    let unexpected: Vec<_> = warnings
        .iter()
        .filter(|w| !w.message.contains("совпадает с клавишей"))
        .collect();
    assert!(unexpected.is_empty(), "{warnings:?}");
    (game, screens)
}

#[test]
fn released_declared_key_switches_screen_but_pressed_does_not() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_DECLARED);
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();

    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert_eq!(
        state.active(),
        config.start_screen,
        "нажатие не должно переключать экран"
    );

    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert_eq!(
        state.active(),
        paused_id,
        "отпускание объявленной клавиши должно было переключить экран"
    );
}

#[test]
fn declared_screen_key_never_reaches_the_world() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_DECLARED);
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();

    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    assert!(
        step_input.is_empty(),
        "объявленная клавиша не должна попасть в очередь ввода мира"
    );
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "свойство объекта, привязанное к той же клавише, не должно измениться"
    );
}

#[test]
fn same_key_reaches_the_world_when_the_active_screen_does_not_declare_it() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_UNDECLARED);
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();

    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([9.0, 9.0]),
        "та же клавиша, необъявленная на активном экране, должна дойти до мира как раньше"
    );
}

#[test]
fn non_live_screen_lets_its_declared_key_through_but_drops_everything_else() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_NONLIVE_WITH_KEY);
    let arena_id = config
        .screens
        .iter()
        .position(|s| s.name == "arena")
        .unwrap();
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();
    let mut state = ScreenState::new(paused_id);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();

    // Undeclared key on a non-live screen: dropped outright, as before.
    queue.push_key_down("KeyZ");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert!(game.take_input_snapshot().is_empty());

    // Declared key: absorbed on press, fires its command once released and drained.
    queue.push_key_down("Space");
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert_eq!(
        state.active(),
        arena_id,
        "клавиша, объявленная у неигрового экрана, должна была сработать"
    );
}

/// Реальный сценарий бага, который второе ревью нашло у `Game::release_key`: нажатие на экране,
/// не объявляющем клавишу, кладёт её в очередь мира, но между нажатием и переключением экрана не
/// делается ни одного шага — нажатие остаётся непотреблённым. Отпускание, поглощённое новым
/// экраном, не должно трогать мир: мир этого нажатия не видел вовсе. Отдельная зеркальная
/// фикстура (`MIRROR_SCENE`), а не `KEY_SCENE`, потому что `release` там пишет значение, которого
/// `press` не пишет никогда, — ошибочно применённое отпускание видно, а не прячется за тем, что
/// `velocity` и так уже была нулевой.
#[test]
fn absorbed_release_also_drops_a_press_of_the_same_key_still_waiting_in_the_queue() {
    let (mut game, config) = load_mirror_two_live_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let declares2_id = config
        .screens
        .iter()
        .position(|s| s.name == "declares2")
        .unwrap();

    // Space is pressed on `undeclared`, which doesn't declare it — queued for the world, but no
    // step runs before the switch below, so it sits there unconsumed.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    screens::apply_command(
        screens::ButtonCommand::ShowScreen(declares2_id),
        &mut game,
        &config,
        &mut state,
    );

    // `declares2` declares Space: its release is absorbed. The press never reached the world (no
    // step ever consumed it), so `Game::release_key` must leave the world untouched.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    let step_input = game.take_input_snapshot();
    assert!(
        step_input.is_empty(),
        "мир не должен был увидеть ни нажатия, ни отпускания клавиши, которую он не видел"
    );
    game.step(step_input);
    assert_eq!(
        game.world
            .number_like(0, resolve(&game.properties, "score")),
        Some(0.0),
        "release не должен сработать для клавиши, чьё нажатие не дошло до мира"
    );
}

/// Сценарий 1 третьего ревью: автоповтор браузера кладёт в очередь ещё один `keydown` уже
/// зажатой клавиши. `InputQueue::press` больше не ставит его в очередь вовсе, но даже если бы
/// поставил, суждение обязано опираться на собственное состояние мира (`Game::is_key_held`), а
/// не на то, что ещё лежит в очереди страницы: шаг уже применил первое нажатие, значит
/// отпускание, поглощённое другим экраном, обязано сработать.
#[test]
fn a_repeated_press_after_a_step_still_lets_the_absorbed_release_fire() {
    let (mut game, config) = load_mirror_two_live_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let declares2_id = config
        .screens
        .iter()
        .position(|s| s.name == "declares2")
        .unwrap();

    // Press on `undeclared`, one engine call so a step actually applies it to the world.
    queue.push_key_down("Space");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );
    assert_eq!(
        game.world
            .number_like(0, resolve(&game.properties, "score")),
        Some(1.0),
        "нажатие должно было примениться шагом"
    );

    // Browser auto-repeat: another `keydown` for the same physical key, still held down.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    screens::apply_command(
        screens::ButtonCommand::ShowScreen(declares2_id),
        &mut game,
        &config,
        &mut state,
    );

    // `declares2` declares Space: absorbed release. The world DOES hold Space — a step already
    // applied its press — so the release must reach it now, whatever a repeat left in the queue.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world
            .number_like(0, resolve(&game.properties, "score")),
        Some(99.0),
        "мир держал клавишу — отпускание обязано было сработать несмотря на автоповтор"
    );
}

/// Сценарий 2 третьего ревью: отпускание и новое нажатие одной и той же клавиши в одном зазоре
/// после того, как шаг уже применил первое нажатие — итог тот же: мир держит клавишу, и
/// поглощённое отпускание обязано сработать.
#[test]
fn a_release_then_press_in_one_gap_after_a_step_still_lets_the_absorbed_release_fire() {
    let (mut game, config) = load_mirror_two_live_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let declares2_id = config
        .screens
        .iter()
        .position(|s| s.name == "declares2")
        .unwrap();

    // Press on `undeclared`, one engine call so a step actually applies it to the world.
    queue.push_key_down("Space");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );
    assert_eq!(
        game.world
            .number_like(0, resolve(&game.properties, "score")),
        Some(1.0),
        "нажатие должно было примениться шагом"
    );

    // Release then press of the same key in the same gap, while `undeclared` is still active —
    // the way a very fast tap-and-hold could land between two engine calls. No step happens in
    // between, so neither reaches the world yet.
    queue.push_key_up("Space");
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    screens::apply_command(
        screens::ButtonCommand::ShowScreen(declares2_id),
        &mut game,
        &config,
        &mut state,
    );

    // `declares2` declares Space: absorbed release. The world still holds Space — nothing since
    // the first step ever ran — so the release must reach it now.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world
            .number_like(0, resolve(&game.properties, "score")),
        Some(99.0),
        "мир держал клавишу — отпускание обязано было сработать"
    );
}

/// «Экраны и состояние» → «Клавиши экрана»: a key held on a live screen that doesn't declare it,
/// released on a *different* live screen that does — the release is absorbed (its command runs),
/// but the world must still feel it, or the property the press set stays on forever. See
/// `Game::release_key`.
#[test]
fn switching_between_two_live_screens_still_releases_a_key_absorbed_on_the_new_one() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_TWO_LIVE);
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let arena2_id = config
        .screens
        .iter()
        .position(|s| s.name == "arena2")
        .unwrap();

    // Space is pressed on `arena`, which does not declare it — reaches the world as usual.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([9.0, 9.0]),
        "нажатие на экране, где клавиша не объявлена, должно дойти до мира"
    );

    // Move to `arena2`, another live screen — both keep the world running, so leaving `arena`
    // does not trigger the engine's release-held-keys.
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(arena2_id),
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(state.active(), arena2_id);

    // `arena2` declares Space, so the release is absorbed — its command runs instead of the key
    // reaching the world through the normal path — but the world must still see the release, or
    // the key stays stuck held forever.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "клавиша не должна остаться зажатой после поглощения отпускания другим живым экраном"
    );

    // The absorbed release's own command still fires, same as any screen key.
    assert_eq!(state.active(), config.start_screen);
}

/// Same two live screens as `KEY_SCREENS_TWO_LIVE`, but `arena2`'s `Space` leads to `paused`, a
/// screen *without* `world_runs` — the world must feel the release immediately, before that
/// command even runs, not only once `switch_to`'s `release_held_keys` gets around to it.
const KEY_SCREENS_TWO_LIVE_TO_PAUSE: &str = r##"{"screens":[
    {"name":"arena","world_runs":true,
     "keys":{"Escape":["show_screen","arena2"]},
     "elements":[]},
    {"name":"arena2","world_runs":true,
     "keys":{"Space":["show_screen","paused"]},
     "elements":[]},
    {"name":"paused","world_runs":false,"elements":[]}
]}"##;

#[test]
fn absorbed_release_on_a_new_live_screen_survives_a_switch_before_the_next_step() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_TWO_LIVE_TO_PAUSE);
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let arena2_id = config
        .screens
        .iter()
        .position(|s| s.name == "arena2")
        .unwrap();

    // Space is pressed on `arena`, which does not declare it — reaches the world as usual.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(game.world.vec2(0, property::VELOCITY), Some([9.0, 9.0]));

    // Move to `arena2`, another live screen — both keep the world running, so leaving `arena`
    // does not trigger `release_held_keys` yet.
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(arena2_id),
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(state.active(), arena2_id);

    // `arena2` declares Space: the release is applied to the world immediately, before its own
    // command (`show_screen("paused")`, a non-live screen) even runs — no step happens in between,
    // exactly the window in which the regression lost the release.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();
    assert_eq!(state.active(), paused_id);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "отпускание должно было снять привязку сразу, не дожидаясь шага или переключения на неигровой экран"
    );
}

#[test]
fn holding_a_declared_key_without_releasing_never_switches_the_screen() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_DECLARED);
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();

    // Browser auto-repeat: many presses queued in a row, no release in between.
    for _ in 0..10 {
        queue.push_key_down("Space");
    }
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert_eq!(
        state.active(),
        config.start_screen,
        "автоповтор нажатия не должен породить ни одного переключения"
    );
}

// «Экраны и состояние» → «Клавиши экрана»: bug #3's own fixture — `release` writes a `score` no
// `press` touches, so a release that reaches the world despite the key never having been pressed
// on the current screen shows up unmistakably, instead of hiding behind a property `press` would
// have set to the same value anyway.
const MIRROR_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"declares","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const MIRROR_PROPS: &str = r##"{"properties":{"score":"number"}}"##;
const MIRROR_SCENE: &str = r##"{"objects":[
    {"position":[0,0],"size":[1,1],"score":0,
     "keys":{"Space":{"press":[["score",1]],"release":[["score",99]]}}}
]}"##;
const MIRROR_RULES: &str = r##"{"rules":[]}"##;
/// `declares` absorbs `Space` on press, never forwarding it to the world; `plain`, the other
/// live screen, does not name `Space` at all.
const MIRROR_SCREENS: &str = r##"{"screens":[
    {"name":"declares","world_runs":true,
     "keys":{"Space":["show_screen","plain"],"Escape":["show_screen","plain"]},
     "elements":[]},
    {"name":"plain","world_runs":true,"elements":[]}
]}"##;

fn load_mirror_fixture() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) = read_entry(MIRROR_GAME).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let (game, screens, warnings) = load_rest(
        MIRROR_GAME,
        config,
        Some(MIRROR_PROPS),
        Some(MIRROR_SCENE),
        Some(MIRROR_RULES),
        Some(MIRROR_SCREENS),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
    )
    .expect("должно загрузиться");
    // `declares` deliberately absorbs the same `Space` `MIRROR_SCENE`'s object binds — «Экраны и состояние» → «Клавиши экрана»: expected here, any *other* warning still fails the fixture.
    let unexpected: Vec<_> = warnings
        .iter()
        .filter(|w| !w.message.contains("совпадает с клавишей"))
        .collect();
    assert!(unexpected.is_empty(), "{warnings:?}");
    (game, screens)
}

/// Same `MIRROR_SCENE`, but with two LIVE screens instead of one screen absorbing on press:
/// `undeclared` doesn't name Space at all, so a press there reaches the world's own queue;
/// `declares2` does, so a release there is absorbed.
const MIRROR_TWO_LIVE_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"undeclared","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const MIRROR_TWO_LIVE_SCREENS: &str = r##"{"screens":[
    {"name":"undeclared","world_runs":true,
     "keys":{"Escape":["show_screen","declares2"]},
     "elements":[]},
    {"name":"declares2","world_runs":true,
     "keys":{"Space":["show_screen","undeclared"]},
     "elements":[]}
]}"##;

fn load_mirror_two_live_fixture() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) =
        read_entry(MIRROR_TWO_LIVE_GAME).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let (game, screens, warnings) = load_rest(
        MIRROR_TWO_LIVE_GAME,
        config,
        Some(MIRROR_PROPS),
        Some(MIRROR_SCENE),
        Some(MIRROR_RULES),
        Some(MIRROR_TWO_LIVE_SCREENS),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
    )
    .expect("должно загрузиться");
    // `declares2` deliberately absorbs the same `Space` `MIRROR_SCENE`'s object binds — expected
    // here, same as `load_mirror_fixture`.
    let unexpected: Vec<_> = warnings
        .iter()
        .filter(|w| !w.message.contains("совпадает с клавишей"))
        .collect();
    assert!(unexpected.is_empty(), "{warnings:?}");
    (game, screens)
}

#[test]
fn release_on_a_screen_that_never_declared_the_key_does_not_fire_if_press_was_absorbed_elsewhere() {
    let (mut game, config) = load_mirror_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let plain_id = config
        .screens
        .iter()
        .position(|s| s.name == "plain")
        .unwrap();

    // `declares` names Space in its own table: the press is absorbed, never reaching the world.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert!(game.take_input_snapshot().is_empty());

    // Switch to `plain`, another live screen, without ever releasing Space on `declares` — the
    // way a player moving the mouse to a "Пауза" button while still holding the key down would.
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(plain_id),
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(state.active(), plain_id);

    // `plain` does not name Space at all — the mirror-image branch to the one above.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world
            .number_like(0, resolve(&game.properties, "score")),
        Some(0.0),
        "release не должен сработать для клавиши, которую игрок не нажимал на этом экране"
    );
}

fn resolve(properties: &engine::core::property::PropertyTable, name: &str) -> property::PropertyId {
    properties.resolve(name).unwrap()
}

/// Same mirror scene, but a single live screen that doesn't declare Space at all, and a
/// non-live screen alongside it — for testing `Game::release_held_keys` on its own: leaving the
/// live screen must release only what the world still holds, never what a step already released.
const MIRROR_LIVE_TO_PAUSE_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"live","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const MIRROR_LIVE_TO_PAUSE_SCREENS: &str = r##"{"screens":[
    {"name":"live","world_runs":true,"elements":[]},
    {"name":"paused","world_runs":false,"elements":[]}
]}"##;

fn load_mirror_live_to_pause_fixture() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) =
        read_entry(MIRROR_LIVE_TO_PAUSE_GAME).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let (game, screens, warnings) = load_rest(
        MIRROR_LIVE_TO_PAUSE_GAME,
        config,
        Some(MIRROR_PROPS),
        Some(MIRROR_SCENE),
        Some(MIRROR_RULES),
        Some(MIRROR_LIVE_TO_PAUSE_SCREENS),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
    )
    .expect("должно загрузиться");
    // `paused` is only ever reached programmatically (`ShowScreen`) in the test below, not
    // through any button — the loader's own "unreachable" warning is expected here, same as
    // `load_key_fixture` expects its own key-collision warning.
    let unexpected: Vec<_> = warnings
        .iter()
        .filter(|w| !w.message.contains("не ведёт ни одна кнопка"))
        .collect();
    assert!(unexpected.is_empty(), "{warnings:?}");
    (game, screens)
}

/// Третье ревью, пункт 3: ветка `KeyAction::Release` в `Game::apply_to_world` обязана снимать
/// клавишу с `world_held_keys`, когда обычное (не поглощённое ни одним экраном) отпускание
/// применяется шагом — иначе уход с живого экрана применит `release` ещё раз и перезапишет
/// всё, что случилось со свойством после.
#[test]
fn a_normal_step_applied_release_is_not_reapplied_when_leaving_the_live_screen() {
    let (mut game, config) = load_mirror_live_to_pause_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();
    let score_prop = resolve(&game.properties, "score");

    queue.push_key_down("Space");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );
    assert_eq!(
        game.world.number_like(0, score_prop),
        Some(1.0),
        "нажатие должно было примениться шагом"
    );

    // A normal release — not absorbed by any screen's `keys` table — consumed by the next step.
    queue.push_key_up("Space");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );
    assert_eq!(
        game.world.number_like(0, score_prop),
        Some(99.0),
        "отпускание должно было примениться шагом"
    );

    // Something else changed the property afterwards — if the world still thought the key held,
    // leaving the live screen would stomp this value back to 99.
    game.world.set_number(0, score_prop, 42.0);

    screens::apply_command(
        screens::ButtonCommand::ShowScreen(paused_id),
        &mut game,
        &config,
        &mut state,
    );

    assert_eq!(
        game.world.number_like(0, score_prop),
        Some(42.0),
        "отпускание, уже применённое шагом, не должно применяться повторно при уходе с экрана"
    );
}

/// Третье ревью, пункт 3: `Game::new_game` обязан чистить `world_held_keys` вместе с миром —
/// иначе «новая партия» наследует набор зажатых клавиш от прошлой, а уничтоженный мир того не
/// спрашивал.
#[test]
fn new_game_clears_world_held_keys_so_a_later_absorbed_release_does_not_touch_the_new_world() {
    let (mut game, config) = load_mirror_two_live_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let undeclared_id = config
        .screens
        .iter()
        .position(|s| s.name == "undeclared")
        .unwrap();
    let declares2_id = config
        .screens
        .iter()
        .position(|s| s.name == "declares2")
        .unwrap();
    let score_prop = resolve(&game.properties, "score");

    // Press on `undeclared`, one engine call so a step applies it to the first world.
    queue.push_key_down("Space");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );
    assert_eq!(game.world.number_like(0, score_prop), Some(1.0));

    // "New game" rebuilds the world from scratch — `world_held_keys` must go with it.
    screens::apply_command(
        screens::ButtonCommand::NewGame(undeclared_id),
        &mut game,
        &config,
        &mut state,
    );
    assert_eq!(
        game.world.number_like(0, score_prop),
        Some(0.0),
        "новый мир начинается с исходных значений"
    );

    // Switch to `declares2`, another live screen — both live, so `switch_to` never calls
    // `release_held_keys` on the way, leaving a stale `world_held_keys` entry undisturbed.
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(declares2_id),
        &mut game,
        &config,
        &mut state,
    );

    // The physical key was never released in the previous game — its release now lands on
    // `declares2`, which declares Space, and is absorbed.
    queue.push_key_up("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );

    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.number_like(0, score_prop),
        Some(0.0),
        "новый мир не должен получить отпускание клавиши, нажатой в прошлой партии"
    );
}

/// Третье ревью, пункт 1: `Game::quit` обязан чистить очередь ввода вместе с миром, той же
/// парой строк, что и `new_game`. `start_screen` здесь сам живой, а `quit` всегда возвращает
/// именно на него — значит `switch_to` после `quit` не зовёт `release_held_keys` (уход с живого
/// экрана на живой её не вызывает), и без собственной очистки `InputQueue::held` пережил бы
/// `quit`: совершенно новое нажатие той же клавиши в следующей партии схлопнулось бы как повтор
/// зажатой в прошлой партии.
#[test]
fn quit_clears_the_input_queue_so_a_later_press_is_not_swallowed_as_a_repeat() {
    let (mut game, config) = load_mirror_two_live_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();

    // Space is pressed on `undeclared`, which doesn't declare it — queued for the world.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert!(
        !game.take_input_snapshot().is_empty(),
        "первое нажатие должно было попасть в очередь"
    );

    screens::apply_command(screens::ButtonCommand::Quit, &mut game, &config, &mut state);
    assert_eq!(
        state.active(),
        config.start_screen,
        "quit всегда возвращает на start_screen"
    );

    // A brand new press of the same physical key, in the next game — must not be swallowed as a
    // repeat of the one held before `quit`.
    queue.push_key_down("Space");
    screens::process_ui_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, VIEWPORT,
    );
    assert!(
        !game.take_input_snapshot().is_empty(),
        "нажатие после quit не должно быть схлопнуто как повтор зажатой до quit клавиши"
    );
}

/// `menu` starts the party through the mouse queue, exactly the way clicking «Играть» does;
/// `arena` declares `Space`, mirroring the manually reported loss on snake's `game` screen.
const NEWGAME_KEY_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const NEWGAME_KEY_SCREENS: &str = r##"{"screens":[
    {"name":"menu","world_runs":false,"elements":[
        {"kind":"button","anchor":"top_left","offset":[0,0],"size":[40,20],"text":"Играть",
         "font":"ui","color":"#ffffff","on_click":["new_game","arena"]}
    ]},
    {"name":"arena","world_runs":true,
     "keys":{"Space":["show_screen","paused"]},
     "elements":[]},
    {"name":"paused","world_runs":false,"elements":[]}
]}"##;

fn load_newgame_key_fixture() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) =
        read_entry(NEWGAME_KEY_GAME).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let (game, screens, warnings) = load_rest(
        NEWGAME_KEY_GAME,
        config,
        Some(KEY_PROPS),
        Some(KEY_SCENE),
        Some(KEY_RULES),
        Some(NEWGAME_KEY_SCREENS),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
    )
    .expect("должно загрузиться");
    // `arena`'s own `Space` (screen key) intentionally shadows `KEY_SCENE`'s object binding of the
    // same code — «Экраны и состояние» → «Клавиши экрана»: expected here, any *other* warning
    // still fails the fixture, same as `load_key_fixture`.
    let unexpected: Vec<_> = warnings
        .iter()
        .filter(|w| !w.message.contains("совпадает с клавишей"))
        .collect();
    assert!(unexpected.is_empty(), "{warnings:?}");
    (game, screens)
}

/// Reduction of a defect once reproduced by hand in the browser: a screen key arriving BEFORE the
/// engine call that drains the click that changed the screen. The key and the click now share one
/// queue, in arrival order, and both are judged only once that queue is drained — so the click
/// (queued first) switches to `arena` before the key events (queued right behind it) are judged,
/// and `arena`'s table claims Space. Goes through `screens::engine_call`, not a hand-rolled
/// process-then-tick pair, so a future change to that order is exercised the same way here as in
/// `wasm::Engine::tick`.
#[test]
fn a_screen_key_arriving_before_the_click_is_drained_still_fires() {
    let (mut game, config) = load_newgame_key_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();

    // The click on «Играть» — move, down, up — lands in the queue and waits there for the next
    // engine call. With a throttled frame loop that wait lasts seconds, not milliseconds.
    queue.push_mouse_move(10.0, 10.0);
    queue.push_mouse_down();
    queue.push_mouse_up();

    // The key arrives during that wait, queued right behind the click — the same order the
    // browser's own `Engine::key_down`/`key_up` would append them in.
    queue.push_key_down("Space");
    queue.push_key_up("Space");

    // One engine call drains the whole queue first, in order: the click switches to `arena`
    // before the key events right behind it are judged, so they see `arena`'s table, which
    // declares Space — «Исполнение игры» → «Шаг и кадр».
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        1.0 / 60.0,
    );

    assert_eq!(
        state.active(),
        paused_id,
        "клавиша, пришедшая до разбора щелчка, не должна теряться"
    );
}

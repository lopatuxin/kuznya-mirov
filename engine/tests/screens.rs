//! Runtime behavior of the screen state machine — «Экраны и состояние». Prestart validation for
//! `screens.json` lives in `screens_validation.rs`; this file drives a loaded game through
//! `core::screens`' free functions the way `wasm::Engine` would.

use engine::core::input::{KeyQueue, MouseQueue, MouseState};
use engine::core::property;
use engine::core::runner::Runner;
use engine::core::screens::{self, ScreenState};
use engine::data::load::{load_rest, read_entry};

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
        config,
        Some(PROPS),
        Some(SCENE),
        Some(RULES),
        Some(SCREENS_JSON),
        &font_bytes,
        &[],
        &[],
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
    screens::key_down(&mut game, &config, &state, "KeyA");
    assert_eq!(game.take_input_snapshot().pressed, Vec::<String>::new());

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

    screens::key_down(&mut game, &config, &state, "KeyA");
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

    let viewport = [800.0, 600.0];
    let mut mouse = MouseState::default();

    // Press and release inside the "II" button (top_left, [0,0]..[40,20]) fires show_screen.
    let mut queue = MouseQueue::new();
    queue.push_move(10.0, 10.0);
    queue.push_down();
    queue.push_up();
    screens::process_mouse_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, viewport,
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
    let mut queue = MouseQueue::new();
    queue.push_move(10.0, 10.0);
    queue.push_down();
    queue.push_move(500.0, 500.0);
    queue.push_up();
    screens::process_mouse_queue(
        &mut queue, &mut mouse, &mut game, &config, &mut state, viewport,
    );
    assert_eq!(
        state.active(),
        game_id,
        "отпускание за границами не должно сработать"
    );
}

// «Экраны и состояние» → «Клавиша экрана»: a self-contained fixture, separate from the shared
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
        config,
        Some(KEY_PROPS),
        Some(KEY_SCENE),
        Some(KEY_RULES),
        Some(screens_json),
        &font_bytes,
        &[],
        &[],
    )
    .expect("должно загрузиться");
    // Every `KEY_SCREENS_*` fixture in this file pairs a live screen's own `Space` binding with
    // `KEY_SCENE`'s object doing the same, on purpose — «Экраны и состояние» → «Клавиша экрана»:
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
    let mut queue = KeyQueue::new();
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();

    screens::key_down(&mut game, &config, &state, "Space");
    assert_eq!(
        state.active(),
        config.start_screen,
        "нажатие не должно переключать экран"
    );

    screens::key_up(&mut game, &config, &state, &mut queue, "Space");
    assert_eq!(
        state.active(),
        config.start_screen,
        "отпускание ждёт границы шага, а не срабатывает внутри key_up"
    );

    screens::process_key_queue(&mut queue, &mut game, &config, &mut state);
    assert_eq!(
        state.active(),
        paused_id,
        "отпускание объявленной клавиши должно было переключить экран"
    );
}

#[test]
fn declared_screen_key_never_reaches_the_world() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_DECLARED);
    let state = ScreenState::new(config.start_screen);

    screens::key_down(&mut game, &config, &state, "Space");
    let step_input = game.take_input_snapshot();
    assert!(
        step_input.pressed.is_empty(),
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
    let state = ScreenState::new(config.start_screen);

    screens::key_down(&mut game, &config, &state, "Space");
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
    let mut queue = KeyQueue::new();

    // Undeclared key on a non-live screen: dropped outright, as before.
    screens::key_down(&mut game, &config, &state, "KeyZ");
    assert!(game.take_input_snapshot().pressed.is_empty());

    // Declared key: absorbed on press, fires its command once released and drained.
    screens::key_down(&mut game, &config, &state, "Space");
    screens::key_up(&mut game, &config, &state, &mut queue, "Space");
    screens::process_key_queue(&mut queue, &mut game, &config, &mut state);
    assert_eq!(
        state.active(),
        arena_id,
        "клавиша, объявленная у неигрового экрана, должна была сработать"
    );
}

#[test]
fn switching_between_two_live_screens_still_releases_a_key_absorbed_on_the_new_one() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_TWO_LIVE);
    let mut state = ScreenState::new(config.start_screen);
    let mut queue = KeyQueue::new();
    let arena2_id = config
        .screens
        .iter()
        .position(|s| s.name == "arena2")
        .unwrap();

    // Space is pressed on `arena`, which does not declare it — reaches the world as usual.
    screens::key_down(&mut game, &config, &state, "Space");
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

    // `arena2` declares Space, so the release is absorbed into the screen-key queue — but the
    // world must still see it, or the key stays stuck held forever.
    screens::key_up(&mut game, &config, &state, &mut queue, "Space");
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "клавиша не должна остаться зажатой после поглощения отпускания другим живым экраном"
    );

    // The absorbed release still fires its own command once drained, same as any screen key.
    screens::process_key_queue(&mut queue, &mut game, &config, &mut state);
    assert_eq!(state.active(), config.start_screen);
}

#[test]
fn absorbed_release_also_drops_a_press_of_the_same_key_still_waiting_in_the_queue() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_TWO_LIVE);
    let mut state = ScreenState::new(config.start_screen);
    let mut queue = KeyQueue::new();
    let arena2_id = config
        .screens
        .iter()
        .position(|s| s.name == "arena2")
        .unwrap();

    // Press and release land inside the same gap between two steps: the press is queued on
    // `arena`, which does not declare Space, and never reaches the world before the switch.
    screens::key_down(&mut game, &config, &state, "Space");
    screens::apply_command(
        screens::ButtonCommand::ShowScreen(arena2_id),
        &mut game,
        &config,
        &mut state,
    );
    screens::key_up(&mut game, &config, &state, &mut queue, "Space");

    // `arena2` absorbs the release and applies it to the world at once. The queued press must go
    // with it: folded in a step later it would switch the binding on with nothing left in the
    // held set to ever switch it off.
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "нажатие, оставшееся в очереди, не должно пережить поглощённое отпускание"
    );
    assert!(!game.is_key_held("Space"));
}

#[test]
fn holding_a_declared_key_without_releasing_never_switches_the_screen() {
    let (mut game, config) = load_key_fixture(KEY_SCREENS_DECLARED);
    let state = ScreenState::new(config.start_screen);

    // Browser auto-repeat: many `key_down` calls in a row, no `key_up` in between.
    for _ in 0..10 {
        screens::key_down(&mut game, &config, &state, "Space");
    }
    assert_eq!(
        state.active(),
        config.start_screen,
        "автоповтор нажатия не должен породить ни одного переключения"
    );
}

/// Same two live screens as `KEY_SCREENS_TWO_LIVE`, but `arena2`'s `Space` leads to `paused`, a
/// screen *without* `world_runs` — the switch that follows the absorbed release must run
/// `release_held_keys`, the same servicing action a live-to-non-live transition always triggers.
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
    let mut queue = KeyQueue::new();
    let arena2_id = config
        .screens
        .iter()
        .position(|s| s.name == "arena2")
        .unwrap();

    // Space is pressed on `arena`, which does not declare it — reaches the world as usual.
    screens::key_down(&mut game, &config, &state, "Space");
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

    // `arena2` declares Space, so its release is absorbed and queued as a screen command. No
    // step runs in between — the runner's копилка has not reached `STEP_SECONDS` yet, exactly
    // the window in which the regression lost the release.
    screens::key_up(&mut game, &config, &state, &mut queue, "Space");
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "отпускание должно было снять привязку сразу, не дожидаясь шага"
    );

    // Draining the queue fires `show_screen("paused")`, a non-live screen — `switch_to` calls
    // `release_held_keys`, which used to throw the still-queued release away together with the
    // rest of the input queue.
    screens::process_key_queue(&mut queue, &mut game, &config, &mut state);
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();
    assert_eq!(state.active(), paused_id);
    assert_eq!(
        game.world.vec2(0, property::VELOCITY),
        Some([0.0, 0.0]),
        "переключение на неигровой экран не должно возвращать снятую привязку"
    );
}

// «Экраны и состояние» → «Клавиша экрана»: bug #3's own fixture — `release` writes a `score` no
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
        config,
        Some(MIRROR_PROPS),
        Some(MIRROR_SCENE),
        Some(MIRROR_RULES),
        Some(MIRROR_SCREENS),
        &font_bytes,
        &[],
        &[],
    )
    .expect("должно загрузиться");
    // `declares` deliberately absorbs the same `Space` `MIRROR_SCENE`'s object binds — «Экраны и
    // состояние» → «Клавиша экрана»: expected here, any *other* warning still fails the fixture.
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
    let mut queue = KeyQueue::new();
    let plain_id = config
        .screens
        .iter()
        .position(|s| s.name == "plain")
        .unwrap();

    // `declares` names Space in its own table: the press is absorbed, never reaching the world.
    screens::key_down(&mut game, &config, &state, "Space");
    assert!(!game.is_key_held("Space"));

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
    screens::key_up(&mut game, &config, &state, &mut queue, "Space");
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
        config,
        Some(KEY_PROPS),
        Some(KEY_SCENE),
        Some(KEY_RULES),
        Some(NEWGAME_KEY_SCREENS),
        &font_bytes,
        &[],
        &[],
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    (game, screens)
}

/// Reduction of the defect reproduced by hand in the browser: a screen key that arrives BEFORE
/// the frame which drains the click that changed the screen. `key_up` judges the key against the
/// screen active at event time — still `menu`, which does not declare it and does not run the
/// world — so the event is dropped; by the time the click is drained and `arena` is active,
/// nothing is left to fire.
///
/// Ignored on purpose, and failing while it is: this is the open seam between judging a key the
/// moment it arrives and switching screens only on a step boundary. A live player cannot reach
/// it — the gap between the click and the next animation frame is a single frame — it showed up
/// only against a throttled frame loop in a hidden browser pane. Closing it means queueing the
/// raw key event and judging it at drain time, ordered against the mouse queue; this test starts
/// passing then and the `is_key_held` machinery becomes unnecessary.
#[test]
#[ignore = "открытый шов: клавиша судится в момент прихода, а экран переключается на границе шага"]
fn a_screen_key_arriving_before_the_click_is_drained_is_lost() {
    let (mut game, config) = load_newgame_key_fixture();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut mouse_queue = MouseQueue::new();
    let mut key_queue = KeyQueue::new();
    let viewport = [800.0, 600.0];
    let paused_id = config
        .screens
        .iter()
        .position(|s| s.name == "paused")
        .unwrap();

    let mut frame = |game: &mut engine::core::game::Game,
                     state: &mut ScreenState,
                     dt: f64,
                     mouse_queue: &mut engine::core::input::MouseQueue,
                     key_queue: &mut KeyQueue| {
        screens::tick(&mut runner, game, &config, state, dt);
        screens::process_mouse_queue(mouse_queue, &mut mouse, game, &config, state, viewport);
        screens::process_key_queue(key_queue, game, &config, state);
    };

    // The click on «Играть» — move, down, up — lands in the mouse queue and waits there for the
    // next frame. With a throttled frame loop that wait lasts seconds, not milliseconds.
    mouse_queue.push_move(10.0, 10.0);
    mouse_queue.push_down();
    mouse_queue.push_up();

    // The key arrives during that wait. `key_down`/`key_up` fire synchronously, outside of
    // `tick`, the way the browser's own listeners call `Engine::key_down`/`key_up` — and the
    // screen they see is still `menu`.
    screens::key_down(&mut game, &config, &state, "Space");
    screens::key_up(&mut game, &config, &state, &mut key_queue, "Space");

    // One frame drains both queues: the click switches to `arena`, whose table declares Space,
    // so the key belongs to that screen and must still fire.
    frame(
        &mut game,
        &mut state,
        1.0 / 60.0,
        &mut mouse_queue,
        &mut key_queue,
    );
    assert_eq!(
        state.active(),
        paused_id,
        "клавиша, пришедшая до разбора щелчка, не должна теряться"
    );
}

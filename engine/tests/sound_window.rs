//! Runtime sound behavior — «Звук» → «Окно чисел»: the step raises
//! marks, the circle clears and writes them, and a step never reads either back. Prestart
//! validation for sound data lives in `sound_validation.rs`; this file drives a loaded game
//! through `core::sound`/`core::screens` the way `wasm::Engine` would.

use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::rules::Outcome;
use engine::core::runner::Runner;
use engine::core::screens::{self, ButtonCommand, ScreenState};
use engine::data::load::{MusicVerdict, load_rest, read_entry};

/// Minimal valid 16-bit mono PCM WAV, one frame — same fixture shape `sound_validation.rs` uses;
/// only the header is ever read, so the single frame of silence is never touched.
fn wav_bytes() -> Vec<u8> {
    let sample_rate = 44_100u32;
    let channels = 1u16;
    let bits_per_sample = 16u16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * u32::from(block_align);
    let data_size = u32::from(block_align);
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // WAVE_FORMAT_PCM
    out.extend_from_slice(&channels.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&block_align.to_le_bytes());
    out.extend_from_slice(&bits_per_sample.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_size.to_le_bytes());
    out.resize(out.len() + data_size as usize, 0);
    out
}

#[allow(clippy::too_many_arguments)]
fn load(
    game_json: &str,
    props: &str,
    scene: &str,
    rules: &str,
    screens_json: &str,
    sounds: &[&str],
    music: &[&str],
) -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let (config, _entry_warnings) = read_entry(game_json).expect("game.json должен разбираться");
    let sound_bytes: Vec<(String, Option<Vec<u8>>)> = sounds
        .iter()
        .map(|name| (name.to_string(), Some(wav_bytes())))
        .collect();
    let music_verdicts: Vec<(String, MusicVerdict)> = music
        .iter()
        .map(|name| (name.to_string(), MusicVerdict::Ok))
        .collect();
    let (game, screens, _warnings) = load_rest(
        game_json,
        config,
        Some(props),
        Some(scene),
        Some(rules),
        Some(screens_json),
        &[],
        &sound_bytes,
        &music_verdicts,
        &[],
        None,
    )
    .expect("должно загрузиться");
    (game, screens)
}

const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const SCENE_EMPTY: &str = r#"{"objects":[]}"#;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_MAIN: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

// -------------------------------------------------------------------------------------------
// «Внутри шага правило поднимает отметку»: десять срабатываний за шаг — одна отметка, «add» тем
// временем честно срабатывает все десять раз («Звук» → «Звук — не счётчик»).
// -------------------------------------------------------------------------------------------

const GAME_TEN: &str = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},"sounds":{"hit":"sounds/hit.wav"}}}"##;
const PROPS_TEN: &str = r#"{"properties":{"score":"number","mover":"flag","target":"flag"}}"#;
const RULES_TEN: &str = r#"{"rules":[
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["target"]},
     "do":[["add","score",1],["play_sound","hit"]]}
]}"#;

fn scene_ten() -> String {
    let mut targets = String::new();
    for _ in 0..10 {
        targets.push_str(r#",{"position":[0,0],"size":[1,1],"collides":true,"target":true}"#);
    }
    format!(
        r#"{{"objects":[
            {{"position":[5,5],"size":[1,1],"score":0}},
            {{"position":[0,0],"size":[1,1],"collides":true,"mover":true}}
            {targets}
        ]}}"#
    )
}

#[test]
fn ten_collisions_in_one_step_raise_one_mark_but_add_still_fires_ten_times() {
    let scene = scene_ten();
    let (mut game, _config) = load(
        GAME_TEN,
        PROPS_TEN,
        &scene,
        RULES_TEN,
        SCREENS_MAIN,
        &["hit"],
        &[],
    );

    game.step(StepInput::empty());

    let score = game.properties.resolve("score").unwrap();
    let counter = game
        .world
        .ids()
        .find(|&id| game.world.number_like(id, score).is_some())
        .expect("счётчик должен существовать");
    assert_eq!(
        game.world.number_like(counter, score),
        Some(10.0),
        "do должен сработать по разу на каждую из десяти пар"
    );
    assert!(
        game.sound_window().mark(0),
        "звук hit должен быть отмечен хотя бы одной из десяти пар"
    );
}

// -------------------------------------------------------------------------------------------
// «Звук события не смотрит на активный экран вовсе»: удар обязан прозвучать, даже если тот же
// шаг увёл на экран поражения.
// -------------------------------------------------------------------------------------------

const GAME_ENDGAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"main","loss_screen":"loss","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},"sounds":{"hit":"sounds/hit.wav"}}}"##;
const PROPS_ENDGAME: &str = r#"{"properties":{"gone":"flag"}}"#;
const SCENE_ENDGAME: &str = r#"{"objects":[{"position":[-100,-100],"size":[1,1],"gone":true}]}"#;
const RULES_ENDGAME: &str = r#"{"rules":[
    {"kind":"delete","for":{"has":["gone"]},"when":"outside_scene",
     "do":[["end_game","loss"],["play_sound","hit"]]}
]}"#;
const SCREENS_ENDGAME: &str = r#"{"screens":[
    {"name":"main","world_runs":true,"elements":[]},
    {"name":"loss","world_runs":false,"elements":[]}
]}"#;

#[test]
fn end_game_and_play_sound_in_the_same_step_both_take_effect() {
    let (mut game, _config) = load(
        GAME_ENDGAME,
        PROPS_ENDGAME,
        SCENE_ENDGAME,
        RULES_ENDGAME,
        SCREENS_ENDGAME,
        &["hit"],
        &[],
    );

    game.step(StepInput::empty());

    assert_eq!(
        game.outcome().map(|(o, _)| o),
        Some(Outcome::Loss),
        "правило должно было закончить партию"
    );
    assert!(
        game.sound_window().mark(0),
        "удар обязан прозвучать, даже если тот же шаг увёл на экран поражения"
    );
}

// -------------------------------------------------------------------------------------------
// «Пять шагов за вызов собирают отметки всех пяти; отметки очищаются в начале вызова» — a mover
// on a one-step grid hop crosses five stationary zones, one per catch-up step of a single burst.
// -------------------------------------------------------------------------------------------

const GAME_BURST: &str = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},
         "sounds":{"s1":"sounds/s1.wav","s2":"sounds/s2.wav","s3":"sounds/s3.wav",
                   "s4":"sounds/s4.wav","s5":"sounds/s5.wav"}}}"##;
const PROPS_BURST: &str = r#"{"properties":{
    "mover":"flag","zone1":"flag","zone2":"flag","zone3":"flag","zone4":"flag","zone5":"flag"
}}"#;
const SCENE_BURST: &str = r#"{"objects":[
    {"position":[6,0],"size":[1,1],"collides":true,"mover":true,"velocity":[1,0],
     "grid":{"interval":0.001}},
    {"position":[7,0],"size":[1,1],"collides":true,"zone1":true},
    {"position":[8,0],"size":[1,1],"collides":true,"zone2":true},
    {"position":[9,0],"size":[1,1],"collides":true,"zone3":true},
    {"position":[10,0],"size":[1,1],"collides":true,"zone4":true},
    {"position":[11,0],"size":[1,1],"collides":true,"zone5":true}
]}"#;
const RULES_BURST: &str = r#"{"rules":[
    {"kind":"move","for":{"has":["mover"]}},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone1"]},"do":[["play_sound","s1"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone2"]},"do":[["play_sound","s2"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone3"]},"do":[["play_sound","s3"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone4"]},"do":[["play_sound","s4"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone5"]},"do":[["play_sound","s5"]]}
]}"#;

fn load_burst() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    load(
        GAME_BURST,
        PROPS_BURST,
        SCENE_BURST,
        RULES_BURST,
        SCREENS_MAIN,
        &["s1", "s2", "s3", "s4", "s5"],
        &[],
    )
}

#[test]
fn five_catchup_steps_in_one_call_collect_marks_from_every_step() {
    let (mut game, config) = load_burst();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();

    screens::tick(&mut runner, &mut game, &config, &mut state, 5.0 / 60.0);

    assert_eq!(
        game.step_count(),
        5,
        "весь остаток копилки должен уйти в пять шагов"
    );
    for i in 0..5 {
        assert!(
            game.sound_window().mark(i),
            "s{} должен был прозвучать на одном из пяти шагов",
            i + 1
        );
    }
}

#[test]
fn marks_are_cleared_at_the_start_of_the_next_call_not_the_end_of_this_one() {
    let (mut game, config) = load_burst();
    let mut state = ScreenState::new(config.start_screen);
    let mut runner = Runner::new();

    screens::tick(&mut runner, &mut game, &config, &mut state, 5.0 / 60.0);
    assert!(
        game.sound_window().mark(0),
        "первая отметка должна была подняться"
    );

    // Копилка уже пуста — этот вызов не делает ни одного шага, — но начало вызова обязано
    // стереть прошлые отметки: страница уже успела их прочитать.
    screens::tick(&mut runner, &mut game, &config, &mut state, 0.0);
    assert_eq!(game.step_count(), 5, "второй вызов не должен был шагнуть");
    assert!(
        !game.sound_window().mark(0),
        "отметки должны очищаться в начале вызова, а не жить до следующего срабатывания"
    );
}

// -------------------------------------------------------------------------------------------
// Музыка экрана и переключатель звука — «Звук».
// -------------------------------------------------------------------------------------------

const GAME_MUSIC: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},
         "music":{"battle":"music/battle.mp3","theme":"music/theme.mp3"}}}"##;
const SCREENS_MUSIC: &str = r#"{"screens":[
    {"name":"menu","world_runs":false,"music":"theme","elements":[]},
    {"name":"game","world_runs":true,"music":"battle","elements":[]}
]}"#;

#[test]
fn menu_click_in_the_same_call_writes_the_new_screens_music() {
    let (mut game, config) = load(
        GAME_MUSIC,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MUSIC,
        &[],
        &["battle", "theme"],
    );
    let mut state = ScreenState::new(config.start_screen);
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();

    screens::write_sound_frame(&mut game, &config, &state);
    let menu_music = game.sound_window().music();
    assert!(menu_music.is_some(), "у меню объявлена музыка");

    // Тот же вызов: щелчок меняет экран, а запись в окно происходит уже после этого — «Звук
    // снаружи движка»: должное пишется после мыши и клавиш экрана.
    screens::apply_command(
        ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );
    screens::write_sound_frame(&mut game, &config, &state);
    assert_ne!(
        game.sound_window().music(),
        menu_music,
        "в окне должна оказаться музыка нового экрана, а не покинутого"
    );
}

const SCREENS_TOGGLE: &str = r#"{"screens":[
    {"name":"menu","world_runs":false,"music":"theme","elements":[]},
    {"name":"game","world_runs":true,"music":"battle","elements":[]},
    {"name":"pause","world_runs":false,"elements":[]}
]}"#;

#[test]
fn toggle_sound_flips_the_flag_new_game_and_quit_leave_it_alone_and_pause_is_silent() {
    let (mut game, config) = load(
        GAME_MUSIC,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_TOGGLE,
        &[],
        &["battle", "theme"],
    );
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
    assert!(state.sound_enabled(), "звук включён при загрузке");

    screens::apply_command(ButtonCommand::ToggleSound, &mut game, &config, &mut state);
    assert!(!state.sound_enabled());

    screens::apply_command(
        ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );
    assert!(
        !state.sound_enabled(),
        "new_game не должен трогать переключатель звука"
    );

    screens::apply_command(ButtonCommand::Quit, &mut game, &config, &mut state);
    assert!(
        !state.sound_enabled(),
        "quit не должен трогать переключатель звука"
    );

    screens::apply_command(
        ButtonCommand::ShowScreen(pause_id),
        &mut game,
        &config,
        &mut state,
    );
    screens::write_sound_frame(&mut game, &config, &state);
    assert_eq!(
        game.sound_window().music(),
        None,
        "у паузы нет музыки — в окне должна быть тишина"
    );
    assert!(
        !game.sound_window().sound_enabled(),
        "признак звука тоже должен был записаться выключенным"
    );
}

// -------------------------------------------------------------------------------------------
// Клавиша экрана через реальный путь очередь → process_ui_queue, а не через прямой вызов
// apply_command — «Экраны и состояние» → «Клавиши экрана» покрыт этим путём и для переключения
// экрана (Escape), и для toggle_sound (KeyM), чтобы регрессия в одном из них не пряталась за
// тестом, вызывающим apply_command напрямую.
// -------------------------------------------------------------------------------------------

const SCREENS_KEYM: &str = r#"{"screens":[
    {"name":"menu","world_runs":false,"music":"theme","elements":[]},
    {"name":"game","world_runs":true,"music":"battle",
     "keys":{"Space":["show_screen","pause"],"Escape":["show_screen","pause"],
             "KeyM":["toggle_sound"]},"elements":[]},
    {"name":"pause","world_runs":false,
     "keys":{"Space":["resume"],"Escape":["resume"]},"elements":[]}
]}"#;

fn load_keym_game() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
    ScreenState,
) {
    let (mut game, config) = load(
        GAME_MUSIC,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_KEYM,
        &[],
        &["battle", "theme"],
    );
    let mut state = ScreenState::new(config.start_screen);
    let game_id = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    screens::apply_command(
        ButtonCommand::NewGame(game_id),
        &mut game,
        &config,
        &mut state,
    );
    (game, config, state)
}

#[test]
fn key_m_press_and_release_on_the_game_screen_toggles_sound_through_the_real_key_path() {
    let (mut game, config, mut state) = load_keym_game();
    assert!(state.sound_enabled(), "звук включён при загрузке");

    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    for i in 0..4 {
        queue.push_key_down("KeyM");
        queue.push_key_up("KeyM");
        screens::process_ui_queue(
            &mut queue,
            &mut mouse,
            &mut game,
            &config,
            &mut state,
            [800.0, 600.0],
        );
        assert_eq!(
            state.sound_enabled(),
            i % 2 == 1,
            "нажатие #{i} должно было перевернуть флаг"
        );
    }
}

#[test]
fn escape_press_and_release_on_the_game_screen_switches_to_pause_through_the_real_key_path() {
    let (mut game, config, mut state) = load_keym_game();
    let game_id = state.active();

    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    queue.push_key_down("Escape");
    queue.push_key_up("Escape");
    screens::process_ui_queue(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        [800.0, 600.0],
    );

    assert_ne!(
        state.active(),
        game_id,
        "Escape через реальный путь клавиши должен был увести с игрового экрана"
    );
}

/// «Звук» → «Один вызов движка»: очередь разбирается до шагов, а должное пишется после и
/// разбора очереди, и шагов — так что переключение экрана реальным, очередным вводом (не прямым
/// `apply_command`) уже видно к моменту `write_sound_frame`, ровно тот порядок, которому следует
/// `wasm::Engine::tick`.
#[test]
fn a_real_queued_switch_writes_the_new_screens_music_in_the_same_call() {
    let (mut game, config, mut state) = load_keym_game();
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();

    screens::write_sound_frame(&mut game, &config, &state);
    assert!(
        game.sound_window().music().is_some(),
        "у game объявлена музыка"
    );

    // Escape — собственная клавиша экрана `game`, ведёт на `pause` — тот же путь, каким прошло бы
    // настоящее событие браузера, а не прямой вызов `apply_command`. `engine_call` — та же точка
    // входа, что и `wasm::Engine::tick`, а не рукописная пара process_ui_queue+tick.
    queue.push_key_down("Escape");
    queue.push_key_up("Escape");
    screens::engine_call(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        [800.0, 600.0],
        1.0 / 60.0,
    );

    assert_eq!(
        game.sound_window().music(),
        None,
        "у pause нет музыки — в окне должна быть тишина, а не музыка покинутого game"
    );
}

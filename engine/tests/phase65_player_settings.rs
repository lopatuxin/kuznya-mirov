//! Фаза 65 — настройки игрока: цепочка возвратов у `resume`, ползунки громкости, переключатели звука и полного
//! экрана, переназначение клавиш с обменом, сохранение настроек страницей и проверки перед запуском. Игра собрана
//! из текстов, её ведёт `ui_support::Ui` теми же вызовами, что `wasm::Engine`. Вид на видеокарте и браузер — только
//! QA на стенде.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::runner::Runner;
use engine::core::screens::{
    self, ButtonCommand, Element, RecordedEvent, ScreenState, ScreensConfig,
};
use engine::core::settings::Volume;
use engine::data::error::GameError;
use engine::data::load::{load_rest, read_entry};
use support::{Files, Ui, VIEWPORT};

const PROPS: &str = r#"{"properties":{"dx":"number","hit":"number"}}"#;
const SCENE: &str = r#"{"objects":[
    {"name":"hero","position":[1,1],"size":[1,1],"dx":0,"hit":0,
     "keys":{"KeyA":{"press":[["dx",-1]],"release":[["dx",0]]},
             "KeyJ":{"press":[["hit",1]],"release":[["hit",0]]}}}
]}"#;

fn button(name: &str, offset: [i32; 2], command: &str) -> String {
    format!(
        r##"{{"kind":"button","anchor":"top_left","offset":[{},{}],"size":[100,40],"name":"{name}",
        "text":"{name}","font":"ui","color":"#ffffff","on_click":{command}}}"##,
        offset[0], offset[1]
    )
}

fn slider(name: &str, y: i32, setting: &str) -> String {
    format!(
        r##"{{"kind":"slider","anchor":"top_left","offset":[100,{y}],"size":[400,40],"name":"{name}",
        "setting":"{setting}","color":"#444444","fill_color":"#00ff00","knob_color":"#ffffff","knob_size":[40,40]}}"##
    )
}

fn switch(name: &str, x: i32, setting: &str) -> String {
    format!(
        r##"{{"kind":"switch","anchor":"top_left","offset":[{x},140],"size":[40,40],"name":"{name}",
        "setting":"{setting}","color_on":"#00ff00","color_off":"#ff0000"}}"##
    )
}

fn key(game_key: &str, y: i32) -> String {
    format!(
        r##"{{"kind":"key","anchor":"top_left","offset":[100,{y}],"size":[200,40],"name":"key_{game_key}",
        "key":"{game_key}","font":"ui","font_size":20,"text_color":"#ffffff","waiting_text":"Нажмите клавишу",
        "waiting_color":"#ff0000","color":"#222222"}}"##
    )
}

fn label(offset: [i32; 2], text: &str) -> String {
    format!(
        r##"{{"kind":"label","anchor":"top_left","offset":[{},{}],"size":[100,40],"text":"{text}","font":"ui"}}"##,
        offset[0], offset[1]
    )
}

/// Экран настроек: на нём всё, что меняет настройки игрока. Элементы по порядку: 0 и 1 — ползунки, 2 и 3 —
/// переключатели, 4 и 5 — клавиши, 6 и 7 — надписи громкости, 8 и 9 — кнопки.
fn settings_screen() -> String {
    let elements = [
        slider("music", 20, "music_volume"),
        slider("sound", 80, "sound_volume"),
        switch("sound_switch", 100, "sound"),
        switch("fullscreen_switch", 200, "fullscreen"),
        key("KeyA", 200),
        key("KeyJ", 260),
        label([520, 20], "{setting:music_volume}"),
        label([520, 80], "{setting:sound_volume}"),
        button("reset", [100, 320], r#"["reset_keys"]"#),
        button("back", [220, 320], r#"["resume"]"#),
    ]
    .join(",");
    format!(
        r#"{{"name":"settings","world_runs":false,"keys":{{"Escape":["resume"]}},"elements":[{elements}]}}"#
    )
}

fn screens_json() -> String {
    let main = [
        button("open_settings", [0, 0], r#"["show_screen","settings"]"#),
        button("play", [0, 60], r#"["new_game","game"]"#),
        button("open_picker", [0, 120], r#"["show_screen","picker"]"#),
    ]
    .join(",");
    let picker = button("to_main", [0, 0], r#"["show_screen","main"]"#);
    let pause = [
        button("pause_settings", [0, 0], r#"["show_screen","settings"]"#),
        button("pause_resume", [0, 60], r#"["resume"]"#),
        button("pause_quit", [0, 120], r#"["quit"]"#),
    ]
    .join(",");
    let settings = settings_screen();
    format!(
        r#"{{"screens":[
    {{"name":"main","world_runs":false,"elements":[{main}]}},
    {{"name":"picker","world_runs":false,"elements":[{picker}]}},
    {{"name":"game","world_runs":true,"keys":{{"Escape":["show_screen","pause"],"KeyP":["toggle_sound"]}},"elements":[]}},
    {{"name":"pause","world_runs":false,"keys":{{"Escape":["resume"]}},"elements":[{pause}]}},
    {settings}
]}}"#
    )
}

fn files(screens: &str) -> Files<'_> {
    Files::new(PROPS, SCENE, screens)
}

fn ui() -> Ui {
    let screens = screens_json();
    files(&screens).load()
}

fn screen_name(ui: &Ui) -> &str {
    &ui.config.screens[ui.state.active()].name
}

fn chain_names(ui: &Ui) -> Vec<&str> {
    ui.state
        .return_chain()
        .iter()
        .map(|&id| ui.config.screens[id].name.as_str())
        .collect()
}

/// Середина элемента `name` текущего экрана, в точках окна.
fn middle(ui: &Ui, name: &str) -> [f32; 2] {
    let screen = &ui.config.screens[ui.state.active()];
    let element = screen
        .elements
        .iter()
        .find(|element| element.place_name() == Some(name))
        .unwrap_or_else(|| panic!("нет места {name}"));
    let [x, y] = element.placement().top_left(VIEWPORT);
    let [width, height] = element.placement().size;
    [x + width / 2.0, y + height / 2.0]
}

fn click_on(ui: &mut Ui, name: &str) {
    let [x, y] = middle(ui, name);
    ui.click(x, y);
}

fn open_settings(ui: &mut Ui) {
    ui.show("settings");
}

fn play(ui: &mut Ui) {
    click_on(ui, "play");
    assert_eq!(screen_name(ui), "game");
}

fn dx(ui: &Ui) -> f64 {
    ui.number(ui.id_of("hero"), "dx").expect("dx")
}

fn hit(ui: &Ui) -> f64 {
    ui.number(ui.id_of("hero"), "hit").expect("hit")
}

fn text_of(ui: &Ui, element: usize) -> String {
    let screen = &ui.config.screens[ui.screen_id("settings")];
    let Element::Label { text, .. } = &screen.elements[element] else {
        panic!("элемент {element} — не надпись");
    };
    screens::format_text_with(
        text,
        &ui.game.world,
        &ui.game.properties,
        None,
        Some(ui.state.choices(&ui.config)),
    )
}

// -------------------------------------------------------------------------------------------
// Цепочка возвратов
// -------------------------------------------------------------------------------------------

#[test]
fn back_from_settings_returns_to_the_screen_that_opened_them() {
    let mut ui = ui();
    click_on(&mut ui, "open_settings");
    assert_eq!(screen_name(&ui), "settings");
    assert_eq!(chain_names(&ui), ["main"]);
    click_on(&mut ui, "back");
    assert_eq!(screen_name(&ui), "main");
    assert!(ui.state.return_chain().is_empty());
}

#[test]
fn resume_from_pause_after_settings_returns_to_the_game_not_to_the_settings() {
    let mut ui = ui();
    play(&mut ui);
    ui.key("Escape");
    assert_eq!(screen_name(&ui), "pause");
    assert_eq!(chain_names(&ui), ["game"]);
    click_on(&mut ui, "pause_settings");
    assert_eq!(chain_names(&ui), ["game", "pause"]);
    click_on(&mut ui, "back");
    assert_eq!(screen_name(&ui), "pause");
    assert_eq!(chain_names(&ui), ["game"]);
    click_on(&mut ui, "pause_resume");
    assert_eq!(screen_name(&ui), "game");
    assert!(ui.state.return_chain().is_empty());
}

#[test]
fn escape_walks_the_chain_back_one_screen_at_a_time() {
    let mut ui = ui();
    play(&mut ui);
    ui.key("Escape");
    click_on(&mut ui, "pause_settings");
    ui.key("Escape");
    assert_eq!(screen_name(&ui), "pause");
    ui.key("Escape");
    assert_eq!(screen_name(&ui), "game");
    assert!(ui.state.return_chain().is_empty());
}

#[test]
fn going_to_a_screen_already_in_the_chain_cuts_the_chain_to_it() {
    let mut ui = ui();
    click_on(&mut ui, "open_picker");
    assert_eq!(chain_names(&ui), ["main"]);
    click_on(&mut ui, "to_main");
    assert_eq!(screen_name(&ui), "main");
    assert!(ui.state.return_chain().is_empty());
}

#[test]
fn resume_with_an_empty_chain_does_nothing() {
    let mut ui = ui();
    screens::apply_command(
        ButtonCommand::Resume,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
    );
    assert_eq!(screen_name(&ui), "main");
}

#[test]
fn new_game_and_quit_clear_the_chain() {
    let mut ui = ui();
    click_on(&mut ui, "open_picker");
    assert_eq!(chain_names(&ui), ["main"]);
    ui.show("main");
    click_on(&mut ui, "play");
    assert!(ui.state.return_chain().is_empty());
    ui.key("Escape");
    assert_eq!(chain_names(&ui), ["game"]);
    click_on(&mut ui, "pause_quit");
    assert_eq!(screen_name(&ui), "main");
    assert!(ui.state.return_chain().is_empty());
    screens::apply_command(
        ButtonCommand::Resume,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
    );
    assert_eq!(screen_name(&ui), "main");
}

const OUTCOME_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"menu","loss_screen":"loss","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const OUTCOME_SCREENS: &str = r##"{"screens":[
    {"name":"menu","world_runs":false,"elements":[]},
    {"name":"game","world_runs":true,"elements":[]},
    {"name":"arena","world_runs":true,"elements":[]},
    {"name":"loss","world_runs":false,"keys":{"Escape":["resume"]},"elements":[]}
]}"##;

#[test]
fn the_end_of_a_partie_clears_the_chain() {
    let (config, _) = read_entry(OUTCOME_GAME).expect("game.json");
    let fonts = vec![("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))];
    let (mut game, screens_config, _, _): (Game, ScreensConfig, _, _) = load_rest(
        OUTCOME_GAME,
        config,
        Some(r#"{"properties":{}}"#),
        Some(r#"{"objects":[{"position":[2,2],"size":[1,1],"velocity":[120,0]}]}"#),
        Some(
            r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}},
            {"kind":"delete","for":{"has":["velocity"]},"when":"outside_scene","do":[["end_game","loss"]]}]}"#,
        ),
        Some(OUTCOME_SCREENS),
        &fonts,
        &[],
        &[],
        &[],
        None,
        false,
    )
    .expect("должно загрузиться");
    let mut state = ScreenState::new(screens_config.start_screen);
    let mut runner = Runner::new();
    let mut mouse = MouseState::default();
    let mut queue = UiQueue::new();
    let id = |name: &str| {
        screens_config
            .screens
            .iter()
            .position(|screen| screen.name == name)
            .expect("экран")
    };
    screens::apply_command(
        ButtonCommand::NewGame(id("game"), None),
        &mut game,
        &screens_config,
        &mut state,
    );
    screens::apply_command(
        ButtonCommand::ShowScreen(id("arena")),
        &mut game,
        &screens_config,
        &mut state,
    );
    assert_eq!(state.return_chain(), [id("game")]);
    for _ in 0..10 {
        screens::engine_call(
            &mut queue,
            &mut mouse,
            &mut runner,
            &mut game,
            &screens_config,
            &mut state,
            VIEWPORT,
            1.0 / 60.0,
        );
    }
    assert_eq!(state.active(), id("loss"));
    assert!(state.return_chain().is_empty());
}

// -------------------------------------------------------------------------------------------
// Ползунки
// -------------------------------------------------------------------------------------------

#[test]
fn a_press_on_a_slider_sets_the_volume_under_the_cursor() {
    let mut ui = ui();
    open_settings(&mut ui);
    assert_eq!(ui.state.volume(Volume::Music), 100);
    ui.press(300.0, 40.0);
    assert_eq!(ui.state.volume(Volume::Music), 50);
    assert_eq!(ui.state.volume(Volume::Sound), 100);
    ui.release();
    assert_eq!(text_of(&ui, 6), "50");
    assert_eq!(text_of(&ui, 7), "100");
}

#[test]
fn a_slider_follows_the_cursor_beyond_its_edges_until_release() {
    let mut ui = ui();
    open_settings(&mut ui);
    ui.press(120.0, 100.0);
    assert_eq!(ui.state.volume(Volume::Sound), 0);
    ui.move_to(480.0, 300.0);
    assert_eq!(ui.state.volume(Volume::Sound), 100);
    ui.move_to(-500.0, 590.0);
    assert_eq!(ui.state.volume(Volume::Sound), 0);
    ui.move_to(120.0 + 3.6 * 75.0, 5.0);
    assert_eq!(ui.state.volume(Volume::Sound), 75);
    ui.release();
    ui.move_to(120.0, 100.0);
    assert_eq!(ui.state.volume(Volume::Sound), 75);
}

#[test]
fn the_volumes_reach_the_sound_window_on_the_next_call() {
    let mut ui = ui();
    open_settings(&mut ui);
    ui.press(300.0, 40.0);
    ui.release();
    ui.press(120.0, 100.0);
    ui.release();
    ui.tick(1.0 / 60.0);
    let window = ui.game.sound_window();
    assert_eq!(window.music_volume(), 50);
    assert_eq!(window.sound_volume(), 0);
}

#[test]
fn a_volume_change_is_reported_once_and_only_when_it_changes() {
    let mut ui = ui();
    open_settings(&mut ui);
    assert!(!ui.state.take_settings_changed());
    ui.press(300.0, 40.0);
    assert!(ui.state.take_settings_changed());
    assert!(!ui.state.take_settings_changed());
    ui.move_to(300.0, 90.0);
    assert!(!ui.state.take_settings_changed(), "та же громкость");
    ui.move_to(310.0, 90.0);
    assert!(ui.state.take_settings_changed());
}

#[test]
fn volume_survives_new_game_and_quit() {
    let mut ui = ui();
    open_settings(&mut ui);
    ui.press(300.0, 40.0);
    ui.release();
    ui.show("main");
    play(&mut ui);
    assert_eq!(ui.state.volume(Volume::Music), 50);
    ui.key("Escape");
    click_on(&mut ui, "pause_quit");
    assert_eq!(ui.state.volume(Volume::Music), 50);
}

// -------------------------------------------------------------------------------------------
// Переключатели
// -------------------------------------------------------------------------------------------

#[test]
fn the_sound_switch_toggles_sound_when_released_over_itself() {
    let mut ui = ui();
    open_settings(&mut ui);
    assert!(ui.state.sound_enabled());
    click_on(&mut ui, "sound_switch");
    assert!(!ui.state.sound_enabled());
    ui.tick(1.0 / 60.0);
    assert!(!ui.game.sound_window().sound_enabled());
    click_on(&mut ui, "sound_switch");
    assert!(ui.state.sound_enabled());
}

#[test]
fn a_switch_released_elsewhere_does_nothing() {
    let mut ui = ui();
    open_settings(&mut ui);
    let [x, y] = middle(&ui, "sound_switch");
    ui.press(x, y);
    ui.move_to(700.0, 500.0);
    ui.release();
    assert!(ui.state.sound_enabled());
}

#[test]
fn the_fullscreen_switch_only_asks_and_shows_what_the_page_reports() {
    let mut ui = ui();
    open_settings(&mut ui);
    assert!(!ui.state.take_fullscreen_request());
    click_on(&mut ui, "fullscreen_switch");
    assert!(ui.state.take_fullscreen_request());
    assert!(!ui.state.take_fullscreen_request());
    assert!(
        !ui.state
            .toggle_is_on(engine::core::settings::Toggle::Fullscreen)
    );
    ui.state.set_fullscreen(true);
    assert!(
        ui.state
            .toggle_is_on(engine::core::settings::Toggle::Fullscreen)
    );
    ui.state.set_fullscreen(false);
    assert!(
        !ui.state
            .toggle_is_on(engine::core::settings::Toggle::Fullscreen)
    );
}

// -------------------------------------------------------------------------------------------
// Клавиши
// -------------------------------------------------------------------------------------------

#[test]
fn a_click_on_a_key_waits_and_the_next_key_is_assigned() {
    let mut ui = ui();
    open_settings(&mut ui);
    assert_eq!(ui.state.player_key_name("KeyJ"), "J");
    click_on(&mut ui, "key_KeyJ");
    assert_eq!(ui.state.awaiting_key(), Some("KeyJ"));
    ui.key("KeyU");
    assert_eq!(ui.state.awaiting_key(), None);
    assert_eq!(ui.state.player_key_name("KeyJ"), "U");
    assert_eq!(ui.state.player_key_name("KeyA"), "A");
    assert!(ui.state.take_settings_changed());
    assert_eq!(screen_name(&ui), "settings");
}

#[test]
fn assigning_a_taken_key_swaps_the_two_game_keys() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key("KeyA");
    assert_eq!(ui.state.player_key_name("KeyJ"), "A");
    assert_eq!(ui.state.player_key_name("KeyA"), "J");
}

#[test]
fn escape_cancels_the_wait_without_leaving_the_screen() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key("Escape");
    assert_eq!(ui.state.awaiting_key(), None);
    assert_eq!(screen_name(&ui), "settings");
    assert_eq!(ui.state.player_key_name("KeyJ"), "J");
    assert!(!ui.state.take_settings_changed());
    ui.key("Escape");
    assert_eq!(
        screen_name(&ui),
        "main",
        "без ожидания Escape — обычная клавиша экрана"
    );
}

#[test]
fn a_held_escape_that_cancelled_the_wait_does_not_leave_the_screen_on_repeat() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key_down("Escape");
    ui.key_down("Escape");
    ui.key_down("Escape");
    ui.key_up("Escape");
    assert_eq!(ui.state.awaiting_key(), None);
    assert_eq!(
        screen_name(&ui),
        "settings",
        "повтор зажатого Escape — то же нажатие"
    );
    ui.key("Escape");
    assert_eq!(screen_name(&ui), "main");
}

#[test]
fn a_screen_key_is_not_assigned_and_the_wait_goes_on() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key("KeyP");
    assert_eq!(ui.state.awaiting_key(), Some("KeyJ"));
    assert_eq!(ui.state.player_key_name("KeyJ"), "J");
    ui.key("KeyU");
    assert_eq!(ui.state.player_key_name("KeyJ"), "U");
}

#[test]
fn a_click_elsewhere_or_leaving_the_screen_cancels_the_wait() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.click(700.0, 500.0);
    assert_eq!(ui.state.awaiting_key(), None);
    ui.key("KeyU");
    assert_eq!(ui.state.player_key_name("KeyJ"), "J");

    click_on(&mut ui, "key_KeyJ");
    ui.show("main");
    assert_eq!(ui.state.awaiting_key(), None);

    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    click_on(&mut ui, "music");
    assert_eq!(ui.state.awaiting_key(), None);
}

#[test]
fn only_one_key_waits_at_a_time() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    click_on(&mut ui, "key_KeyA");
    assert_eq!(ui.state.awaiting_key(), Some("KeyA"));
    ui.key("KeyU");
    assert_eq!(ui.state.player_key_name("KeyA"), "U");
    assert_eq!(ui.state.player_key_name("KeyJ"), "J");
}

#[test]
fn reset_keys_removes_every_replacement() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key("KeyA");
    assert!(ui.state.take_settings_changed());
    click_on(&mut ui, "reset");
    assert_eq!(ui.state.player_key_name("KeyJ"), "J");
    assert_eq!(ui.state.player_key_name("KeyA"), "A");
    assert!(ui.state.take_settings_changed());
    click_on(&mut ui, "reset");
    assert!(!ui.state.take_settings_changed(), "сбрасывать нечего");
}

#[test]
fn a_replaced_key_drives_the_hero_and_the_old_one_does_nothing() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyA");
    ui.key("KeyU");
    ui.show("main");
    play(&mut ui);

    ui.key_down("KeyA");
    ui.tick(0.05);
    assert_eq!(dx(&ui), 0.0, "прежняя клавиша ничего не делает");
    ui.queue.push_key_up("KeyA");

    ui.key_down("KeyU");
    ui.tick(1.0 / 60.0);
    assert_eq!(dx(&ui), -1.0);
    ui.queue.push_key_up("KeyU");
    ui.tick(1.0 / 60.0);
    assert_eq!(dx(&ui), 0.0, "отпускание тоже переведено");
}

#[test]
fn swapped_keys_trade_their_actions() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key("KeyA");
    ui.show("main");
    play(&mut ui);

    ui.key_down("KeyA");
    ui.tick(1.0 / 60.0);
    assert_eq!(hit(&ui), 1.0);
    assert_eq!(dx(&ui), 0.0);
    ui.key_down("KeyJ");
    ui.tick(1.0 / 60.0);
    assert_eq!(dx(&ui), -1.0);
}

#[test]
fn the_recording_keeps_the_game_key_the_world_saw() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyA");
    ui.key("KeyU");
    ui.show("main");
    play(&mut ui);
    let mut events = Vec::new();
    ui.queue.push_key_down("KeyU");
    ui.queue.push_key_up("KeyU");
    screens::process_ui_queue_recording(
        &mut ui.queue,
        &mut ui.mouse,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
        VIEWPORT,
        Some(&mut events),
        true,
    );
    assert_eq!(
        events,
        [
            RecordedEvent::WorldKeyDown("KeyA".to_string()),
            RecordedEvent::WorldKeyUp("KeyA".to_string()),
        ]
    );
}

#[test]
fn a_key_nobody_took_reaches_the_world_as_it_is() {
    let mut ui = ui();
    open_settings(&mut ui);
    click_on(&mut ui, "key_KeyJ");
    ui.key("KeyU");
    ui.show("main");
    play(&mut ui);
    ui.key_down("KeyA");
    ui.tick(1.0 / 60.0);
    assert_eq!(dx(&ui), -1.0);
    assert_eq!(hit(&ui), 0.0);
}

// -------------------------------------------------------------------------------------------
// Хранение страницей
// -------------------------------------------------------------------------------------------

#[test]
fn settings_survive_a_round_trip_through_the_text_the_page_stores() {
    let mut first = ui();
    open_settings(&mut first);
    first.press(300.0, 40.0);
    first.release();
    click_on(&mut first, "key_KeyJ");
    first.key("KeyA");
    let saved = first.state.settings_json();

    let mut second = ui();
    second.state.load_settings_json(&second.config, &saved);
    assert_eq!(second.state.volume(Volume::Music), 50);
    assert_eq!(second.state.player_key_name("KeyJ"), "A");
    assert_eq!(second.state.player_key_name("KeyA"), "J");
    assert!(
        !second.state.take_settings_changed(),
        "загруженное сохранять заново не нужно"
    );
    assert_eq!(second.state.settings_json(), saved);
}

#[test]
fn saved_settings_the_game_does_not_fit_are_dropped_silently() {
    let mut ui = ui();
    ui.state.load_settings_json(
        &ui.config,
        r#"{"music_volume": 140, "sound_volume": 30,
            "keys": {"KeyZ": "KeyQ", "KeyA": "KeyP", "KeyJ": "KeyU"}}"#,
    );
    assert_eq!(ui.state.volume(Volume::Music), 100, "вне 0–100");
    assert_eq!(ui.state.volume(Volume::Sound), 30);
    assert_eq!(ui.state.player_key_name("KeyJ"), "U");
    assert_eq!(
        ui.state.player_key_name("KeyA"),
        "A",
        "KeyP — клавиша экрана"
    );
    ui.state.load_settings_json(&ui.config, "это не JSON");
    assert_eq!(ui.state.volume(Volume::Sound), 30);
}

#[test]
fn the_player_key_is_named_in_russian() {
    let mut ui = ui();
    ui.state
        .load_settings_json(&ui.config, r#"{"keys": {"KeyA": "ArrowLeft"}}"#);
    assert_eq!(ui.state.player_key_name("KeyA"), "Стрелка влево");
}

// -------------------------------------------------------------------------------------------
// Проверка данных перед запуском
// -------------------------------------------------------------------------------------------

fn screen_with(elements: &str) -> String {
    format!(r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[{elements}]}}]}}"#)
}

fn errors_of(screens: &str) -> Vec<String> {
    match files(screens).try_load() {
        Ok(_) => Vec::new(),
        Err(failure) => failure.errors.into_iter().map(|e| e.message).collect(),
    }
}

fn expect_error(elements: &str, fragment: &str) {
    let errors = errors_of(&screen_with(elements));
    assert!(
        errors.iter().any(|message| message.contains(fragment)),
        "ожидалась ошибка с «{fragment}», есть {errors:?}"
    );
}

const GOOD_SLIDER: &str = r##"{"kind":"slider","anchor":"top_left","size":[400,40],"setting":"music_volume","knob_color":"#ffffff","knob_size":[40,40]}"##;

#[test]
fn a_bare_slider_with_only_a_knob_is_fine() {
    assert_eq!(errors_of(&screen_with(GOOD_SLIDER)), Vec::<String>::new());
}

#[test]
fn the_setting_has_to_be_one_the_kind_changes() {
    expect_error(
        &GOOD_SLIDER.replace("music_volume", "fullscreen"),
        "ползунок меняет music_volume или sound_volume",
    );
    expect_error(
        r##"{"kind":"switch","anchor":"top_left","size":[40,40],"setting":"music_volume","color_on":"#ffffff","color_off":"#000000"}"##,
        "переключатель меняет sound или fullscreen",
    );
}

#[test]
fn a_slider_needs_a_knob_and_its_size_and_never_two_of_a_part() {
    expect_error(
        r##"{"kind":"slider","anchor":"top_left","size":[400,40],"setting":"music_volume","knob_color":"#ffffff"}"##,
        "у ползунка нет knob_size",
    );
    expect_error(
        r##"{"kind":"slider","anchor":"top_left","size":[400,40],"setting":"music_volume","knob_size":[40,40]}"##,
        "у ползунка нет бегунка",
    );
    expect_error(
        r##"{"kind":"slider","anchor":"top_left","size":[400,40],"setting":"music_volume","knob_color":"#ffffff","knob_image":"x","knob_size":[40,40]}"##,
        "у бегунка ползунка заданы и \"knob_color\", и \"knob_image\"",
    );
    expect_error(
        &GOOD_SLIDER.replace(
            r#""setting""#,
            r##""color":"#000000","image":"x","setting""##,
        ),
        "у полосы ползунка заданы и \"color\", и \"image\"",
    );
    expect_error(
        &GOOD_SLIDER.replace(
            r#""setting""#,
            r##""fill_color":"#000000","fill_image":"x","setting""##,
        ),
        "у заливки ползунка заданы и \"fill_color\", и \"fill_image\"",
    );
    expect_error(
        &GOOD_SLIDER.replace(r#""knob_size":[40,40]"#, r#""knob_size":[0,40]"#),
        "размер бегунка должен быть больше нуля",
    );
}

#[test]
fn a_switch_needs_exactly_one_pair() {
    let none = r##"{"kind":"switch","anchor":"top_left","size":[40,40],"setting":"sound"}"##;
    expect_error(none, "у переключателя нет пары");
    let half = r##"{"kind":"switch","anchor":"top_left","size":[40,40],"setting":"sound","color_on":"#ffffff"}"##;
    expect_error(half, "у переключателя ровно одна пара");
    let both = r##"{"kind":"switch","anchor":"top_left","size":[40,40],"setting":"sound",
        "color_on":"#ffffff","color_off":"#000000","image_on":"x","image_off":"y"}"##;
    expect_error(both, "у переключателя ровно одна пара");
}

#[test]
fn a_key_names_a_key_the_scene_binds() {
    let key = |code: &str| {
        format!(
            r##"{{"kind":"key","anchor":"top_left","size":[200,40],"key":"{code}","font":"ui"}}"##
        )
    };
    assert_eq!(errors_of(&screen_with(&key("KeyA"))), Vec::<String>::new());
    expect_error(&key("KeyZ"), "нет ни в одной привязке keys");
}

#[test]
fn the_three_kinds_stand_on_the_screen_by_themselves() {
    for (kind, element) in [
        ("slider", GOOD_SLIDER.to_string()),
        (
            "switch",
            r##"{"kind":"switch","anchor":"top_left","size":[40,40],"setting":"sound","color_on":"#ffffff","color_off":"#000000"}"##.to_string(),
        ),
        (
            "key",
            r##"{"kind":"key","anchor":"top_left","size":[200,40],"key":"KeyA","font":"ui"}"##.to_string(),
        ),
    ] {
        let with_for = element.replacen('{', r#"{"for":{"has":["hero"]},"#, 1);
        expect_error(&with_for, &format!("у \"{kind}\" нет \"for\""));
        let in_row = format!(
            r##"{{"kind":"list","anchor":"top_left","size":[300,300],"for":{{"has":["hero"]}},"row_height":64,
            "row":[{}]}}"##,
            element.replace(r#""anchor":"top_left","#, r#""offset":[0,0],"#)
        );
        expect_error(&in_row, &format!("в строке списка нет \"{kind}\""));
    }
}

#[test]
fn a_setting_substitution_names_a_volume() {
    let text = |text: &str| {
        format!(
            r##"{{"kind":"label","anchor":"top_left","size":[100,40],"text":"{text}","font":"ui"}}"##
        )
    };
    assert_eq!(
        errors_of(&screen_with(&text("{setting:sound_volume}"))),
        Vec::<String>::new()
    );
    expect_error(&text("{setting:fullscreen}"), "называет не громкость");
    expect_error(&text("{setting:}"), "называет не громкость");
}

#[test]
fn a_setting_substitution_is_not_allowed_in_a_tooltip() {
    let screens = r##"{"tooltip":{"font":"ui","color":"#000000"},"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"{setting:music_volume}"}]}]}"##;
    let errors = errors_of(screens);
    assert!(
        errors
            .iter()
            .any(|message| message.contains("{setting:…} есть в тексте надписи и кнопки")),
        "{errors:?}"
    );
}

#[test]
fn a_place_no_element_answers_is_warned_about_and_a_repeated_name_is_an_error() {
    let screens = screen_with(&format!(
        r##"{GOOD_SLIDER},{}"##,
        button("reset", [0, 100], r#"["reset_keys"]"#)
    ));
    let screens = screens.replace(
        r#""setting":"music_volume""#,
        r#""setting":"music_volume","name":"music""#,
    );
    let (_, _, warnings) = files(&screens).try_load().expect("грузится");
    assert!(
        warnings
            .iter()
            .any(|warning: &GameError| warning.message.contains("место наведения \"music\"")),
        "{warnings:?}"
    );
    let second_music = format!(
        "{},{}",
        GOOD_SLIDER.replace(
            r#""setting":"music_volume""#,
            r#""setting":"music_volume","name":"same""#
        ),
        GOOD_SLIDER.replace(
            r#""setting":"music_volume""#,
            r#""setting":"sound_volume","name":"same""#
        ),
    );
    expect_error(&second_music, "имя места наведения \"same\" повторяется");
}

#[test]
fn reset_keys_takes_no_arguments() {
    expect_error(
        &button("reset", [0, 0], r#"["reset_keys","all"]"#),
        "у команды \"reset_keys\"",
    );
}

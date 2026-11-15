//! Начальный экран `games/platformer`: статичная картина, переход в книгу героев и выход к списку игр; экран
//! настроек, который открывается из меню и из паузы.

use std::fs;

#[path = "ui_support/mod.rs"]
mod support;

#[path = "platformer_support/mod.rs"]
mod platformer;

use engine::core::interface;
use engine::core::screens::{self, ButtonCommand, Element, TextPart};
use engine::core::settings::Volume;
use engine::data::load::{ImageDecl, load_rest_with_stamps, read_entry};
use platformer::{platformer_dir, read};
use support::{Ui, VIEWPORT};

fn load() -> (Ui, Vec<engine::data::error::GameError>, Vec<ImageDecl>) {
    let game_json = read("game.json");
    let (config, mut warnings) =
        read_entry(&game_json).unwrap_or_else(|e| panic!("game.json не разобран: {e:?}"));
    let images = platformer::image_verdicts(&config.files.images);
    let tables = platformer::table_texts(&config.files.tables);
    let fonts: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(platformer_dir().join(path)).ok()))
        .collect();
    let (game, screens, rest_warnings, image_decls) = load_rest_with_stamps(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &fonts,
        &[],
        &[],
        &images,
        Some(&read("code.lua")),
        false,
        &tables,
        None,
        &[],
        &[],
        &[],
    )
    .unwrap_or_else(|e| panic!("платформер не загрузился: {e:?}"));
    warnings.extend(rest_warnings);
    (Ui::new(game, screens), warnings, image_decls)
}

fn ui() -> Ui {
    load().0
}

fn active_screen(ui: &Ui) -> &str {
    &ui.config.screens[ui.state.active()].name
}

/// Середина элемента экрана, в точках окна.
fn middle_of(ui: &Ui, matches: impl Fn(&Element) -> bool) -> [f32; 2] {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(screen_id, VIEWPORT);
    interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport)
        .iter()
        .find_map(|part| match part {
            interface::Part::Element(placed) if matches(placed.element) => Some([
                (placed.rect[0] + placed.rect[2] / 2.0) * frame.scale[0],
                (placed.rect[1] + placed.rect[3] / 2.0) * frame.scale[1],
            ]),
            _ => None,
        })
        .expect("элемент на экране")
}

fn button_command(element: &Element) -> Option<ButtonCommand> {
    match element {
        Element::Button { on_click, .. } => Some(*on_click),
        _ => None,
    }
}

fn place_at(ui: &Ui, point: [f32; 2]) -> Option<String> {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(screen_id, VIEWPORT);
    let parts = interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport);
    screens::hover_place(&parts, Some(frame.point(point))).map(|index| {
        screen.elements[index]
            .place_name()
            .expect("имя")
            .to_string()
    })
}

#[test]
fn the_platformer_loads_without_errors_or_warnings() {
    let (_, warnings, _) = load();
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
}

#[test]
fn the_game_starts_on_the_menu_screen_without_a_world() {
    let ui = ui();
    assert_eq!(active_screen(&ui), "menu");
    assert!(!ui.config.screens[ui.state.active()].world_runs);
    assert!(!ui.game.has_world());
}

fn new_game_button(ui: &Ui) -> [f32; 2] {
    middle_of(ui, |element| element.place_name() == Some("new_game"))
}

fn go_button(ui: &Ui) -> [f32; 2] {
    middle_of(ui, |element| element.place_name() == Some("go"))
}

fn choose_hero(ui: &mut Ui, hero: &str) {
    let at = middle_of(ui, |element| element.place_name() == Some(hero));
    ui.click(at[0], at[1]);
}

#[test]
fn new_game_opens_the_book_without_creating_a_world() {
    let mut ui = ui();
    let new_game = new_game_button(&ui);
    assert_eq!(place_at(&ui, new_game).as_deref(), Some("new_game"));
    ui.click(new_game[0], new_game[1]);
    assert_eq!(active_screen(&ui), "hero_select");
    assert!(!ui.game.has_world());
}

#[test]
fn heroes_on_the_start_screen_do_not_react_to_the_mouse() {
    let mut ui = ui();
    let frame = ui.config.ui_frame(ui.state.active(), VIEWPORT);
    for (hero, [x, y]) in [
        ("ilya", [-630.0, 61.0]),
        ("alyosha", [-333.0, -160.0]),
        ("vasilisa", [-111.0, 84.0]),
    ] {
        let at = [
            VIEWPORT[0] / 2.0 + x * frame.scale[0],
            VIEWPORT[1] / 2.0 + y * frame.scale[1],
        ];
        assert_eq!(place_at(&ui, at), None, "{hero}");
        ui.click(at[0], at[1]);
        assert_eq!(active_screen(&ui), "menu", "{hero}");
        assert_eq!(ui.state.chosen("heroes"), None, "{hero}");
    }
}

#[test]
fn exit_asks_the_page_to_leave_to_the_list_of_games() {
    let mut ui = ui();
    let exit = middle_of(&ui, |element| {
        matches!(button_command(element), Some(ButtonCommand::Exit))
    });
    ui.click(exit[0], exit[1]);
    assert!(ui.state.exit_requested());
}

#[test]
fn the_menu_has_only_settings_new_game_and_exit_buttons() {
    let ui = ui();
    let buttons: Vec<(&str, ButtonCommand)> = ui.config.screens[ui.state.active()]
        .elements
        .iter()
        .filter_map(|element| Some((element.place_name()?, button_command(element)?)))
        .collect();
    assert!(
        matches!(
            buttons.as_slice(),
            [
                ("settings", ButtonCommand::ShowScreen(_)),
                ("new_game", ButtonCommand::ShowScreen(_)),
                ("exit", ButtonCommand::Exit)
            ]
        ),
        "{buttons:?}"
    );
}

#[test]
fn continue_does_not_press() {
    let mut ui = ui();
    let at = middle_of(&ui, |element| {
        matches!(element, Element::Label { text, .. }
            if matches!(text.as_slice(), [TextPart::Literal(text)] if text == "Продолжить"))
    });
    ui.click(at[0], at[1]);
    assert_eq!(active_screen(&ui), "menu");
    assert!(!ui.state.exit_requested());
}

#[test]
fn settings_opens_the_settings_screen_and_back_returns_to_the_menu() {
    let mut ui = ui();
    let settings = middle_of(&ui, |element| element.place_name() == Some("settings"));
    ui.click(settings[0], settings[1]);
    assert_eq!(active_screen(&ui), "settings");
    let back = middle_of(&ui, |element| element.place_name() == Some("back"));
    ui.click(back[0], back[1]);
    assert_eq!(active_screen(&ui), "menu");
    assert!(ui.state.return_chain().is_empty());
}

#[test]
fn the_menu_uses_the_approved_static_picture() {
    let (ui, _, images) = load();
    let menu = &ui.config.screens[ui.state.active()];
    let pictures: Vec<_> = menu
        .elements
        .iter()
        .filter_map(|element| match element {
            Element::Panel {
                fill: screens::PanelFill::Fixed(screens::Fill::Image { image, .. }),
                show_on_hover: None,
                ..
            } => Some(images[*image].name.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(pictures, ["start_wide"]);
    assert!(
        menu.elements
            .iter()
            .all(|element| element.play_on_hover().is_none())
    );
    assert!(
        menu.elements
            .iter()
            .all(|element| element.particles().is_none())
    );
}

#[test]
fn the_pause_returns_to_the_start_screen_with_the_same_hero_chosen() {
    let mut ui = ui();
    let new_game = new_game_button(&ui);
    ui.click(new_game[0], new_game[1]);
    choose_hero(&mut ui, "alyosha");
    let go = go_button(&ui);
    ui.click(go[0], go[1]);
    ui.key("Escape");
    assert_eq!(active_screen(&ui), "pause");
    let to_menu = middle_of(&ui, |element| {
        matches!(button_command(element), Some(ButtonCommand::Quit))
    });
    ui.click(to_menu[0], to_menu[1]);
    assert_eq!(active_screen(&ui), "menu");
    assert_eq!(ui.state.chosen("heroes"), Some("alyosha"));
}

// Настройки игрока — фаза 65: экран `settings` открывается из меню и из паузы.

fn click_place(ui: &mut Ui, name: &str) {
    let at = middle_of(ui, |element| element.place_name() == Some(name));
    ui.click(at[0], at[1]);
}

fn start_game(ui: &mut Ui) {
    let game = ui.screen_id("game");
    screens::apply_command(
        ButtonCommand::NewGame(game, None),
        &mut ui.game,
        &ui.config,
        &mut ui.state,
    );
}

#[test]
fn settings_from_the_pause_go_back_to_the_pause_and_then_to_the_game() {
    let mut ui = ui();
    start_game(&mut ui);
    ui.key("Escape");
    click_place(&mut ui, "settings");
    assert_eq!(active_screen(&ui), "settings");
    click_place(&mut ui, "back");
    assert_eq!(active_screen(&ui), "pause");
    click_place(&mut ui, "resume");
    assert_eq!(active_screen(&ui), "game");
    assert!(ui.state.return_chain().is_empty());
}

#[test]
fn a_slider_of_the_settings_screen_takes_the_volume_from_the_press() {
    let mut ui = ui();
    click_place(&mut ui, "settings");
    for name in ["music_volume", "sound_volume"] {
        let at = middle_of(&ui, |element| element.place_name() == Some(name));
        ui.click(at[0], at[1]);
    }
    assert_eq!(ui.state.volume(Volume::Music), 50);
    assert_eq!(ui.state.volume(Volume::Sound), 50);
    click_place(&mut ui, "fullscreen");
    assert!(ui.state.take_fullscreen_request());
}

#[test]
fn every_hero_key_has_a_row_on_the_settings_screen_and_a_new_one_moves_the_hero() {
    let mut ui = ui();
    click_place(&mut ui, "settings");
    let screen = &ui.config.screens[ui.state.active()];
    let mut rows: Vec<&str> = screen
        .elements
        .iter()
        .filter_map(|element| match element {
            Element::Key { key, .. } => Some(key.as_str()),
            _ => None,
        })
        .collect();
    rows.sort_unstable();
    assert_eq!(rows, ui.config.game_keys);
    assert_eq!(ui.state.player_key_name("KeyD"), "D");

    click_place(&mut ui, "key_KeyD");
    ui.key("KeyL");
    assert_eq!(ui.state.player_key_name("KeyD"), "L");
    click_place(&mut ui, "back");
    start_game(&mut ui);
    let hero = ui.id_of("Герой");
    ui.key_down("KeyD");
    ui.tick(0.05);
    assert!(
        !ui.flag(hero, "hold_right"),
        "прежняя клавиша ничего не делает"
    );
    ui.key_down("KeyL");
    ui.tick(0.05);
    assert!(ui.flag(hero, "hold_right"));
}

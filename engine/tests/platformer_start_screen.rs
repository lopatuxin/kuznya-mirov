//! Фаза 07 платформера — начальный экран `games/platformer`: игра грузится без ошибок и предупреждений,
//! стартует с экрана `menu`, «Новая игра» ведёт на уровень, «Выход» просит страницу уйти к списку игр, три
//! героя — места наведения с видео и горящей строкой. Видео и курсор в браузере — только QA на стенде.

use std::fs;

#[path = "ui_support/mod.rs"]
mod support;

#[path = "platformer_support/mod.rs"]
mod platformer;

use engine::core::interface;
use engine::core::screens::{self, ButtonCommand, Element, TextPart};
use engine::data::load::{ImageDecl, load_rest_with_stamps, read_entry};
use platformer::{platformer_dir, read};
use support::{Ui, VIEWPORT};

const HEROES: [&str; 3] = ["ilya", "alyosha", "vasilisa"];

fn load() -> (Ui, Vec<engine::data::error::GameError>, Vec<ImageDecl>) {
    let game_json = read("game.json");
    let (config, mut warnings) =
        read_entry(&game_json).unwrap_or_else(|e| panic!("game.json не разобран: {e:?}"));
    let images = platformer::image_verdicts(&config.files.images);
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
        &[],
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
    let frame = ui.config.ui_frame(VIEWPORT);
    interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport)
        .iter()
        .find_map(|part| match part {
            interface::Part::Element(placed) if matches(placed.element) => Some([
                (placed.rect[0] + placed.rect[2] / 2.0) * frame.scale,
                (placed.rect[1] + placed.rect[3] / 2.0) * frame.scale,
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
    let frame = ui.config.ui_frame(VIEWPORT);
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

#[test]
fn new_game_leads_to_the_level() {
    let mut ui = ui();
    let new_game = middle_of(&ui, |element| {
        matches!(button_command(element), Some(ButtonCommand::NewGame(..)))
    });
    ui.click(new_game[0], new_game[1]);
    assert_eq!(active_screen(&ui), "game");
    assert!(ui.game.has_world());
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
fn only_new_game_and_exit_are_buttons_on_the_menu() {
    let ui = ui();
    let commands: Vec<ButtonCommand> = ui.config.screens[ui.state.active()]
        .elements
        .iter()
        .filter_map(button_command)
        .collect();
    assert_eq!(commands.len(), 2, "{commands:?}");
    assert!(matches!(commands[0], ButtonCommand::NewGame(..)));
    assert_eq!(commands[1], ButtonCommand::Exit);
}

#[test]
fn continue_and_settings_do_not_press() {
    let mut ui = ui();
    for caption in ["Продолжить", "Настройки"] {
        let at = middle_of(&ui, |element| {
            matches!(element, Element::Label { text, .. }
                if matches!(text.as_slice(), [TextPart::Literal(text)] if text == caption))
        });
        ui.click(at[0], at[1]);
        assert_eq!(active_screen(&ui), "menu", "{caption}");
        assert!(!ui.state.exit_requested(), "{caption}");
    }
}

#[test]
fn each_hero_is_a_place_with_a_video_and_a_glowing_line() {
    let (ui, _, images) = load();
    let elements = &ui.config.screens[ui.state.active()].elements;
    for hero in HEROES {
        let place = elements
            .iter()
            .position(|element| element.place_name() == Some(hero))
            .unwrap_or_else(|| panic!("нет места {hero}"));
        let plays = elements
            .iter()
            .filter(|element| element.play_on_hover() == Some(place))
            .count();
        let glows = elements
            .iter()
            .filter(|element| element.show_on_hover() == Some(place))
            .count();
        assert_eq!(plays, 1, "{hero}: видео");
        assert_eq!(glows, 1, "{hero}: высеченная строка горит золотом");
    }
    let videos = ui.config.panel_videos();
    assert_eq!(videos.len(), 3);
    for video in videos {
        assert!(images[video].reverse.is_some(), "{}", images[video].name);
    }
}

#[test]
fn the_cursor_over_a_hero_is_over_his_place() {
    let ui = ui();
    for hero in HEROES {
        let at = middle_of(&ui, |element| element.place_name() == Some(hero));
        assert_eq!(place_at(&ui, at).as_deref(), Some(hero));
    }
    assert_eq!(place_at(&ui, [5.0, 5.0]), None);
}

#[test]
fn the_pause_returns_to_the_start_screen() {
    let mut ui = ui();
    let new_game = middle_of(&ui, |element| {
        matches!(button_command(element), Some(ButtonCommand::NewGame(..)))
    });
    ui.click(new_game[0], new_game[1]);
    ui.key("Escape");
    assert_eq!(active_screen(&ui), "pause");
    let to_menu = middle_of(&ui, |element| {
        matches!(button_command(element), Some(ButtonCommand::Quit))
    });
    ui.click(to_menu[0], to_menu[1]);
    assert_eq!(active_screen(&ui), "menu");
}

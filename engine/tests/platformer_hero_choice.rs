//! Фаза 10 платформера — экран героя `games/platformer`: выбранный на «Распутье» герой, «Новая игра» открывает его
//! экран с портретом и числами таблицы `heroes`, «Назад» возвращает к выбору, «В путь» начинает партию, в которой
//! герой получает лист кадров, дальность удара и числа своей строки. Вид экрана — только QA на стенде.

use std::fs;

#[path = "ui_support/mod.rs"]
mod support;

#[path = "platformer_support/mod.rs"]
mod platformer;

use engine::core::interface;
use engine::core::screens::{self, ButtonCommand, Element, PanelFill};
use engine::data::load::{ImageDecl, load_rest_with_stamps, read_entry};
use platformer::{platformer_dir, read};
use support::{Ui, VIEWPORT};

/// Герой, его имя, прозвище и числа Силы богатырской, Сноровки, Прыти и Мудрости — как в `heroes.json`.
const HEROES: [(&str, &str, &str, [f64; 4]); 3] = [
    ("ilya", "Илья Муромец", "громила", [8.0, 4.0, 3.0, 2.0]),
    ("alyosha", "Алёша Попович", "ловкач", [4.0, 6.0, 7.0, 3.0]),
    (
        "vasilisa",
        "Василиса Премудрая",
        "чародейка",
        [3.0, 4.0, 4.0, 8.0],
    ),
];

/// Лист кадров героя и дальность его удара в клетках — как в `heroes.json`: докуда достаёт нарисованное оружие.
const SHEETS: [(&str, &str, f64); 3] = [
    ("ilya", "hero_ilya", 1.5),
    ("alyosha", "hero_alyosha", 1.3),
    ("vasilisa", "hero_vasilisa", 1.6),
];

fn load() -> (Ui, Vec<ImageDecl>) {
    let game_json = read("game.json");
    let (config, _) =
        read_entry(&game_json).unwrap_or_else(|e| panic!("game.json не разобран: {e:?}"));
    let images = platformer::image_verdicts(&config.files.images);
    let tables = platformer::table_texts(&config.files.tables);
    let fonts: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(platformer_dir().join(path)).ok()))
        .collect();
    let (game, screens, _, image_decls) = load_rest_with_stamps(
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
    (Ui::new(game, screens), image_decls)
}

fn active_screen(ui: &Ui) -> &str {
    &ui.config.screens[ui.state.active()].name
}

/// Середина элемента активного экрана, в точках окна.
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

fn click_place(ui: &mut Ui, name: &str) {
    let at = middle_of(ui, |element| element.place_name() == Some(name));
    ui.click(at[0], at[1]);
}

/// Выбор героя на «Распутье» и «Новая игра».
fn hero_screen_of(hero: &str) -> (Ui, Vec<ImageDecl>) {
    let (mut ui, images) = load();
    click_place(&mut ui, hero);
    click_place(&mut ui, "new_game");
    assert_eq!(active_screen(&ui), "hero", "{hero}");
    (ui, images)
}

/// Подписи надписей активного экрана после подстановки.
fn label_texts(ui: &Ui) -> Vec<String> {
    ui.config.screens[ui.state.active()]
        .elements
        .iter()
        .filter_map(|element| match element {
            Element::Label { text, .. } => Some(screens::format_text_with(
                text,
                &ui.game.world,
                &ui.game.properties,
                None,
                Some(ui.state.choices(&ui.config)),
            )),
            _ => None,
        })
        .collect()
}

#[test]
fn the_hero_screen_shows_the_chosen_hero_with_the_numbers_of_his_row() {
    for (hero, name, epithet, numbers) in HEROES {
        let (ui, _) = hero_screen_of(hero);
        let texts = label_texts(&ui);
        for expected in [
            name,
            epithet,
            "Сила богатырская",
            "Сноровка",
            "Прыть",
            "Мудрость",
        ] {
            assert!(
                texts.iter().any(|t| t == expected),
                "{hero}: нет «{expected}» в {texts:?}"
            );
        }
        for number in numbers {
            let shown = format!("{number}");
            assert!(
                texts.contains(&shown),
                "{hero}: нет числа {shown} в {texts:?}"
            );
        }
        assert!(
            texts.iter().all(|t| !t.contains('{')),
            "{hero}: подстановка не раскрыта: {texts:?}"
        );
    }
}

#[test]
fn the_hero_screen_shows_the_portrait_of_the_chosen_hero() {
    for (hero, ..) in HEROES {
        let (ui, images) = hero_screen_of(hero);
        let (choice, names) = ui.config.screens[ui.state.active()]
            .elements
            .iter()
            .find_map(|element| match element {
                Element::Panel {
                    fill: PanelFill::Choice { choice, names, .. },
                    ..
                } => Some((choice, names)),
                _ => None,
            })
            .expect("панель портрета");
        let image = ui
            .state
            .choices(&ui.config)
            .image(choice, names)
            .expect("картинка портрета");
        assert_eq!(images[image].name, format!("{hero}_portrait"));
    }
}

#[test]
fn back_returns_to_the_crossroads_with_the_same_hero_chosen() {
    let (mut ui, _) = hero_screen_of("alyosha");
    click_place(&mut ui, "back");
    assert_eq!(active_screen(&ui), "menu");
    assert_eq!(ui.state.chosen("heroes"), Some("alyosha"));
    assert!(!ui.game.has_world());
}

#[test]
fn on_the_road_the_hero_gets_the_numbers_of_his_row() {
    for (hero, _, _, numbers) in HEROES {
        let (mut ui, _) = hero_screen_of(hero);
        click_place(&mut ui, "go");
        assert_eq!(active_screen(&ui), "game", "{hero}");
        ui.ticks(2, 1.0 / 60.0);
        let id = ui.id_of("Герой");
        assert_eq!(ui.text(id, "hero_id").as_deref(), Some(hero));
        let got = ["strength", "skill", "agility", "wisdom"].map(|p| ui.number(id, p));
        assert_eq!(got, numbers.map(Some), "{hero}");
    }
}

#[test]
fn on_the_road_the_hero_gets_the_sheet_and_reach_of_his_row() {
    for (hero, sheet, reach) in SHEETS {
        let (mut ui, images) = hero_screen_of(hero);
        click_place(&mut ui, "go");
        ui.ticks(2, 1.0 / 60.0);
        let id = ui.id_of("Герой");
        let image = ui
            .game
            .world
            .image(id, engine::core::property::IMAGE)
            .expect("картинка героя");
        assert_eq!(images[image].name, sheet, "{hero}");
        assert_eq!(ui.number(id, "reach"), Some(reach), "{hero}");
    }
}

#[test]
fn starting_over_from_the_pause_keeps_the_chosen_hero() {
    let (mut ui, _) = hero_screen_of("alyosha");
    click_place(&mut ui, "go");
    ui.key("Escape");
    assert_eq!(active_screen(&ui), "pause");
    let again = middle_of(&ui, |element| {
        matches!(
            element,
            Element::Button {
                on_click: ButtonCommand::NewGame(..),
                ..
            }
        )
    });
    ui.click(again[0], again[1]);
    assert_eq!(active_screen(&ui), "game");
    let id = ui.id_of("Герой");
    assert_eq!(ui.text(id, "hero_id").as_deref(), Some("alyosha"));
}

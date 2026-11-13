//! Книга героев «Распутья»: закладки, числа из `heroes` и запуск партии с разворота.

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

const HEROES: [(&str, &str, [f64; 4]); 3] = [
    ("ilya", "Илья Муромец", [8.0, 4.0, 3.0, 2.0]),
    ("alyosha", "Алёша Попович", [4.0, 6.0, 7.0, 3.0]),
    ("vasilisa", "Василиса Премудрая", [3.0, 4.0, 4.0, 8.0]),
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

fn click_place(ui: &mut Ui, name: &str) {
    let at = middle_of(ui, |element| element.place_name() == Some(name));
    ui.click(at[0], at[1]);
}

fn book_page_of(hero: &str) -> (Ui, Vec<ImageDecl>) {
    let (mut ui, images) = load();
    click_place(&mut ui, "new_game");
    assert_eq!(active_screen(&ui), "hero_select");
    assert_eq!(ui.state.chosen("heroes"), Some("ilya"));
    if hero != "ilya" {
        click_place(&mut ui, hero);
    }
    assert_eq!(active_screen(&ui), "hero_select", "{hero}");
    assert_eq!(ui.state.chosen("heroes"), Some(hero));
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
fn the_book_shows_the_chosen_hero_with_the_numbers_of_their_row() {
    for (hero, name, numbers) in HEROES {
        let (ui, _) = book_page_of(hero);
        let texts = label_texts(&ui);
        let expected = [
            name.to_string(),
            "Сила богатырская".to_string(),
            numbers[0].to_string(),
            "Сноровка".to_string(),
            numbers[1].to_string(),
            "Прыть".to_string(),
            numbers[2].to_string(),
            "Мудрость".to_string(),
            numbers[3].to_string(),
        ];
        assert!(texts.starts_with(&expected), "{hero}: {texts:?}");
        assert!(
            texts.iter().all(|t| !t.contains('{')),
            "{hero}: подстановка не раскрыта: {texts:?}"
        );
    }
}

#[test]
fn the_chosen_heros_spread_fills_the_window() {
    for (hero, ..) in HEROES {
        let (ui, images) = book_page_of(hero);
        let elements = &ui.config.screens[ui.state.active()].elements;
        let (spread, choice, names) = elements
            .iter()
            .find_map(|element| match element {
                Element::Panel {
                    placement,
                    fill: PanelFill::Choice { choice, names, .. },
                    ..
                } => Some((placement, choice, names)),
                _ => None,
            })
            .expect("разворот выбранного героя");
        let image = ui
            .state
            .choices(&ui.config)
            .image(choice, names)
            .expect("иллюстрация разворота");
        assert_eq!(images[image].name, format!("hero_book_{hero}"));
        assert!(matches!(spread.anchor, screens::Anchor::Center));
        assert_eq!(spread.offset, [0.0, 0.0]);
        for window in [[1920.0, 911.0], [1920.0, 1080.0], [1280.0, 960.0]] {
            let frame = ui.config.ui_frame(ui.state.active(), window);
            assert_eq!(spread.top_left(frame.viewport), [0.0, 0.0]);
            assert_eq!(
                [
                    spread.size[0] * frame.scale[0],
                    spread.size[1] * frame.scale[1]
                ],
                window
            );
        }
    }
}

#[test]
fn bookmark_colors_follow_the_chosen_hero_while_hovering_another() {
    let (mut ui, images) = load();
    click_place(&mut ui, "new_game");

    for (hero, hovered, expected) in [
        ("vasilisa", "ilya", Some("hero_bookmarks_vasilisa")),
        ("alyosha", "vasilisa", Some("hero_bookmarks_alyosha")),
        ("ilya", "alyosha", None),
    ] {
        click_place(&mut ui, hero);
        let [x, y] = middle_of(&ui, |element| element.place_name() == Some(hovered));
        ui.queue.push_mouse_move(x, y);
        screens::process_ui_queue(
            &mut ui.queue,
            &mut ui.mouse,
            &mut ui.game,
            &ui.config,
            &mut ui.state,
            VIEWPORT,
        );
        assert_eq!(ui.state.chosen("heroes"), Some(hero));

        let elements = &ui.config.screens[ui.state.active()].elements;
        let (choice, names) = elements
            .iter()
            .find_map(|element| match element {
                Element::Panel {
                    fill: PanelFill::Choice { choice, names, .. },
                    show_on_hover: None,
                    ..
                } if choice.field.as_deref() == Some("bookmarks") => Some((choice, names)),
                _ => None,
            })
            .expect("слой закладок зависит от выбора героя");
        let image = ui.state.choices(&ui.config).image(choice, names);
        assert_eq!(image.map(|id| images[id].name.as_str()), expected, "{hero}");
    }
}

#[test]
fn the_diamond_follows_the_pointer_over_equal_bookmark_areas() {
    let (ui, _) = book_page_of("vasilisa");
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(screen_id, VIEWPORT);
    let parts = interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport);
    let bookmarks = screen
        .elements
        .iter()
        .filter(|element| element.choose() == Some("heroes"));
    for bookmark in bookmarks {
        assert_eq!(bookmark.placement().offset[0], 809.0);
        assert_eq!(bookmark.placement().size, [226.0, 194.0]);
    }

    let visible_diamonds = |pointer: Option<[f32; 2]>| {
        let hover = screens::hover_place(&parts, pointer.map(|point| frame.point(point)));
        let lit = ui.state.lit(screen, hover);
        screen
            .elements
            .iter()
            .filter_map(|element| match element {
                Element::Label { text, placement, .. }
                    if element.visible_over(&lit)
                        && matches!(text.as_slice(), [screens::TextPart::Literal(glyph)] if glyph == "◆") =>
                {
                    Some(placement.offset[1])
                }
                _ => None,
            })
            .collect::<Vec<_>>()
    };
    assert!(visible_diamonds(None).is_empty());
    assert!(visible_diamonds(Some([400.0, 300.0])).is_empty());
    for (hero, row) in [("ilya", -285.0), ("vasilisa", -55.0), ("alyosha", 178.0)] {
        let point = middle_of(&ui, |element| element.place_name() == Some(hero));
        assert_eq!(visible_diamonds(Some(point)), [row], "{hero}");
    }
}

#[test]
fn the_right_edge_of_the_bookmark_selects_the_hero_in_a_wide_window() {
    let (mut ui, _) = book_page_of("ilya");
    ui.queue.push_mouse_move(1880.0, 408.0);
    ui.queue.push_mouse_down();
    ui.queue.push_mouse_up();
    screens::process_ui_queue(
        &mut ui.queue,
        &mut ui.mouse,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
        [1920.0, 911.0],
    );
    assert_eq!(ui.state.chosen("heroes"), Some("vasilisa"));
}

#[test]
fn back_returns_to_the_menu_with_the_same_hero_chosen() {
    for (hero, ..) in HEROES {
        let (mut ui, _) = book_page_of(hero);
        click_place(&mut ui, "back_select");
        assert_eq!(active_screen(&ui), "menu");
        assert_eq!(ui.state.chosen("heroes"), Some(hero));
        assert!(!ui.game.has_world());
    }
}

#[test]
fn bookmarks_switch_the_page_and_keep_the_choice_when_the_book_reopens() {
    let (mut ui, _) = load();
    click_place(&mut ui, "new_game");
    assert_eq!(active_screen(&ui), "hero_select");
    assert_eq!(ui.state.chosen("heroes"), Some("ilya"));
    assert!(!ui.game.has_world());

    for (hero, name, _) in HEROES {
        click_place(&mut ui, hero);
        assert_eq!(active_screen(&ui), "hero_select");
        assert_eq!(label_texts(&ui).first().map(String::as_str), Some(name));
        assert_eq!(ui.state.chosen("heroes"), Some(hero));
        assert!(!ui.game.has_world());
    }

    click_place(&mut ui, "back_select");
    click_place(&mut ui, "new_game");
    assert_eq!(ui.state.chosen("heroes"), Some("vasilisa"));
    assert_eq!(
        label_texts(&ui).first().map(String::as_str),
        Some("Василиса Премудрая")
    );
}

#[test]
fn on_the_road_the_hero_gets_the_numbers_of_his_row() {
    for (hero, _, numbers) in HEROES {
        let (mut ui, _) = book_page_of(hero);
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
        let (mut ui, images) = book_page_of(hero);
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
    let (mut ui, _) = book_page_of("alyosha");
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

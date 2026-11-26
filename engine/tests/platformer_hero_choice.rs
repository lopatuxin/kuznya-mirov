//! Книга героев «Распутья»: закладки, числа из `heroes` и запуск партии с разворота.

use std::fs;

#[path = "ui_support/mod.rs"]
mod support;

#[path = "platformer_support/mod.rs"]
mod platformer;

use engine::core::interface;
use engine::core::screens::{self, ButtonCommand, Element};
use engine::data::load::{ImageDecl, load_rest_with_stamps, read_entry};
use platformer::{platformer_dir, read};
use serde_json::{Value, json};
use support::{Ui, VIEWPORT};

const HEROES: [(&str, &str, [f64; 4]); 3] = [
    ("ilya", "Илья Муромец", [8.0, 4.0, 3.0, 2.0]),
    ("alyosha", "Алёша Попович", [4.0, 6.0, 7.0, 3.0]),
    ("vasilisa", "Василиса Премудрая", [3.0, 4.0, 4.0, 8.0]),
];

/// Лист кадров героя и дальность его удара в клетках — как в `heroes.json`: докуда достаёт нарисованное оружие,
/// а у Василисы — сколько летит сгусток.
const SHEETS: [(&str, &str, f64); 3] = [
    ("ilya", "hero_ilya", 1.5),
    ("alyosha", "hero_alyosha", 1.3),
    ("vasilisa", "hero_vasilisa", 3.0),
];

fn load() -> (Ui, Vec<ImageDecl>) {
    load_scene(&read("scene.json"))
}

fn load_scene(scene: &str) -> (Ui, Vec<ImageDecl>) {
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
        Some(scene),
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
    middle_of_placed(ui, |placed| matches(placed.element))
}

fn middle_of_placed(ui: &Ui, matches: impl Fn(&interface::Placed<'_>) -> bool) -> [f32; 2] {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(screen_id, VIEWPORT);
    interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport)
        .iter()
        .find_map(|part| match part {
            interface::Part::Element(placed) if matches(placed) => Some([
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
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(screen_id, VIEWPORT);
    interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport)
        .iter()
        .filter_map(|part| {
            let interface::Part::Element(placed) = part else {
                return None;
            };
            let Element::Label { text, .. } = placed.element else {
                return None;
            };
            Some(screens::format_text_with(
                text,
                &ui.game.world,
                &ui.game.properties,
                placed.object,
                Some(ui.state.choices(&ui.config)),
            ))
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
            "Силушка".to_string(),
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
        let screen = &ui.config.screens[ui.state.active()];
        let book = screen.book.as_ref().expect("книга выбора героя");
        let page = book.selected(&ui.state).expect("разворот выбранного героя");
        assert_eq!(
            images[book.bookmarks[page].spread].name,
            format!("hero_book_{hero}")
        );
        for window in [[1920.0, 911.0], [1920.0, 1080.0], [1280.0, 960.0]] {
            let frame = ui.config.ui_frame(ui.state.active(), window);
            assert_eq!(frame.viewport, [1920.0, 1080.0]);
            assert_eq!(
                [
                    frame.viewport[0] * frame.scale[0],
                    frame.viewport[1] * frame.scale[1]
                ],
                window
            );
        }
    }
}

#[test]
fn bookmarks_for_preceding_pages_move_left_and_still_accept_clicks() {
    let (mut ui, _) = load();
    click_place(&mut ui, "new_game");
    let heroes = ["ilya", "vasilisa", "alyosha"];
    for (hero, on_left) in [
        ("ilya", 0),
        ("vasilisa", 1),
        ("alyosha", 2),
        ("vasilisa", 1),
        ("ilya", 0),
        ("alyosha", 2),
        ("ilya", 0),
    ] {
        click_place(&mut ui, hero);
        assert_eq!(ui.state.chosen("heroes"), Some(hero));
        for (index, name) in heroes.iter().enumerate() {
            let [x, y] = middle_of(&ui, |element| element.place_name() == Some(name));
            assert_eq!(x < VIEWPORT[0] / 2.0, index < on_left, "{hero}: {name}");
            ui.click(VIEWPORT[0] - x, y);
            assert_eq!(ui.state.chosen("heroes"), Some(hero), "пустое место {name}");
        }
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
        assert_eq!(bookmark.placement().offset[0], 811.0);
        assert_eq!(bookmark.placement().size, [244.0, 202.0]);
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
    for (hero, row) in [("ilya", -293.0), ("vasilisa", -60.0), ("alyosha", 175.0)] {
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

#[test]
fn the_hero_stats_show_current_health_and_the_heros_own_combat_numbers() {
    let profiles = [
        ("116", "14", "2,33 уд/с", "16 %", "6 %"),
        ("108", "7", "3,67 уд/с", "24 %", "14 %"),
        ("106", "10", "2,54 уд/с", "16 %", "8 %"),
    ];
    for ((hero, name, _), (health, damage, speed, parry, dodge)) in HEROES.into_iter().zip(profiles)
    {
        let (mut ui, _) = book_page_of(hero);
        click_place(&mut ui, "go");
        ui.ticks(2, 1.0 / 60.0);
        let id = ui.id_of("Герой");
        let health_prop = ui.game.properties.resolve("health").expect("здоровье");
        ui.game.world.set_number(id, health_prop, 41.0);
        if hero == "vasilisa" {
            let spirit = ui.game.properties.resolve("spirit").expect("чары");
            ui.game.world.set_number(id, spirit, 1.0);
        }

        ui.key("KeyC");
        assert_eq!(active_screen(&ui), "hero_stats", "{hero}");
        assert_eq!(ui.text(id, "book_art"), Some(format!("book_{hero}")));
        let texts = label_texts(&ui);
        for expected in [
            name,
            health,
            damage,
            "Скорость атаки",
            speed,
            parry,
            dodge,
            "Шанс двойного урона",
            "Заряды чар",
        ] {
            assert!(
                texts.iter().any(|text| text == expected),
                "{hero}: {texts:?}"
            );
        }
        assert_eq!(ui.number(id, "health"), Some(41.0));
        if hero == "vasilisa" {
            assert_eq!(ui.number(id, "spirit"), Some(1.0));
        }
    }
}

#[test]
fn closing_the_hero_book_returns_to_its_source_without_advancing_the_world() {
    let (mut ui, _) = book_page_of("ilya");
    ui.key("KeyC");
    assert_eq!(active_screen(&ui), "hero_select");
    click_place(&mut ui, "go");
    ui.ticks(2, 1.0 / 60.0);
    let epoch = ui.game.world_epoch();
    let step = ui.game.step_count();

    ui.key("KeyC");
    assert_eq!(active_screen(&ui), "hero_stats");
    ui.ticks(60, 1.0 / 60.0);
    assert_eq!(ui.game.step_count(), step);
    ui.key("KeyC");
    assert_eq!(active_screen(&ui), "game");

    let portrait = middle_of(&ui, |element| {
        matches!(
            element,
            Element::Button { on_click: ButtonCommand::ShowScreen(screen), .. }
                if ui.config.screens[*screen].name == "hero_stats"
        )
    });
    ui.click(portrait[0], portrait[1]);
    assert_eq!(active_screen(&ui), "hero_stats");
    ui.key("Escape");
    assert_eq!(active_screen(&ui), "game");

    ui.key("Escape");
    assert_eq!(active_screen(&ui), "pause");
    click_place(&mut ui, "open_hero_stats");
    assert_eq!(active_screen(&ui), "hero_stats");
    click_place(&mut ui, "back_hero_stats");
    assert_eq!(active_screen(&ui), "pause");
    ui.key("Escape");
    assert_eq!(active_screen(&ui), "game");

    assert_eq!(ui.game.world_epoch(), epoch);
    assert_eq!(ui.game.step_count(), step);
    ui.ticks(2, 1.0 / 60.0);
    assert!(ui.game.step_count() > step);
}

#[test]
fn opening_the_hero_book_before_the_first_step_still_shows_the_hero_and_base_stats() {
    let (mut ui, _) = book_page_of("vasilisa");
    click_place(&mut ui, "go");
    ui.key("KeyC");
    assert_eq!(active_screen(&ui), "hero_stats");
    assert_eq!(ui.game.step_count(), 0);
    let texts = label_texts(&ui);
    assert!(texts.iter().any(|text| text == "Василиса Премудрая"));
    let id = ui.id_of("Герой");
    for (key, value) in [
        ("strength", "3"),
        ("skill", "4"),
        ("agility", "4"),
        ("wisdom", "8"),
    ] {
        assert_eq!(ui.text(id, &format!("book_{key}")).as_deref(), Some(value));
    }
    assert_eq!(ui.text(id, "book_art").as_deref(), Some("book_vasilisa"));
    assert_eq!(ui.number(id, "level"), Some(1.0));
    assert_eq!(ui.number(id, "xp"), Some(0.0));
    assert_eq!(ui.number(id, "xp_needed"), Some(100.0));
    assert_eq!(ui.number(id, "strong_water"), Some(0.0));
    assert_eq!(ui.count("bag_item"), 0);
}

const BAG_ITEMS: [&str; 12] = [
    "cap",
    "armor",
    "gloves",
    "belt",
    "boots",
    "shield",
    "amulet",
    "ring_strength",
    "ring_skill",
    "ring_wisdom",
    "great_axe",
    "worn_axe",
];

fn equipped_book(hero: &str) -> Ui {
    let mut scene: Value = serde_json::from_str(&read("scene.json")).unwrap();
    let objects = scene["objects"].as_array_mut().unwrap();
    let player = objects.iter_mut().find(|o| o["hero"] == true).unwrap();
    player["strong_water"] = 2.into();
    player["health"] = 41.into();
    player["spirit"] = 1.into();
    player["level"] = 3.into();
    player["xp"] = 47.into();
    player["xp_needed"] = 300.into();
    for (index, item) in BAG_ITEMS.iter().enumerate() {
        objects.push(json!({
            "bag_item": true, "item_id": item, "bag_order": index + 1,
        }));
    }
    let (mut ui, _) = load_scene(&scene.to_string());
    click_place(&mut ui, "new_game");
    if hero != "ilya" {
        click_place(&mut ui, hero);
    }
    click_place(&mut ui, "go");
    ui.key("KeyC");
    assert_eq!(active_screen(&ui), "hero_stats");
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
    ui
}

fn bag_item(ui: &Ui, item: &str) -> u32 {
    let id_prop = ui.game.properties.resolve("item_id").unwrap();
    let bag_prop = ui.game.properties.resolve("bag_item").unwrap();
    ui.game
        .world
        .ids()
        .find(|&id| {
            ui.game.world.has(id, bag_prop) && ui.game.world.text(id, id_prop) == Some(item)
        })
        .unwrap_or_else(|| panic!("нет вещи {item}"))
}

fn run_book(ui: &mut Ui, function: &str, item: Option<u32>) {
    ui.game
        .run_screen_function(function, item, "hero book test");
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
}

fn wear(ui: &mut Ui, item: &str) {
    let id = bag_item(ui, item);
    run_book(ui, "equip_item", Some(id));
}

fn set_number(ui: &mut Ui, id: u32, name: &str, value: f64) {
    let prop = ui.game.properties.resolve(name).unwrap();
    ui.game.world.set_number(id, prop, value);
}

#[test]
fn the_book_initializes_scene_items_and_displays_scene_progress_before_the_world_starts() {
    let ui = equipped_book("ilya");
    let hero = ui.id_of("Герой");
    assert_eq!(ui.game.step_count(), 0);
    assert_eq!(ui.count("bag_item"), 12);
    assert_eq!(ui.number(hero, "level"), Some(3.0));
    assert_eq!(ui.number(hero, "xp"), Some(47.0));
    assert_eq!(ui.number(hero, "xp_needed"), Some(300.0));
    assert_eq!(ui.number(hero, "max_health"), Some(136.0));
    assert_eq!(ui.number(hero, "health"), Some(41.0));
    for item in BAG_ITEMS {
        let id = bag_item(&ui, item);
        assert!(ui.text(id, "item_name").is_some());
        assert!(ui.text(id, "item_icon").unwrap().starts_with("item_"));
    }
    assert_eq!(ui.text(hero, "gear_weapon").as_deref(), Some("item_axe"));
    assert!(ui.text(hero, "gear_head").is_none());
    assert!(ui.flag(hero, "can_add_water"));
    assert!(!ui.flag(hero, "can_undo_strength"));
}

#[test]
fn fitting_better_and_worse_items_never_changes_equipment_health_or_wards() {
    let mut ui = equipped_book("ilya");
    let hero = ui.id_of("Герой");
    set_number(&mut ui, hero, "health", 41.0);
    let cap = bag_item(&ui, "cap");
    run_book(&mut ui, "preview_item", Some(cap));
    assert_eq!(ui.text(hero, "book_strength").as_deref(), Some("8 (+1)"));
    assert_eq!(ui.text(hero, "book_health").as_deref(), Some("136 (+2)"));
    assert_eq!(ui.number(hero, "strength"), Some(8.0));
    assert!(ui.text(hero, "gear_head").is_none());
    assert_eq!(ui.number(hero, "health"), Some(41.0));
    let axe = bag_item(&ui, "worn_axe");
    run_book(&mut ui, "preview_item", Some(axe));
    assert_eq!(ui.text(hero, "book_damage").as_deref(), Some("14 (−3)"));
    assert_eq!(ui.text(hero, "equip_weapon").as_deref(), Some("axe"));
    let gloves = bag_item(&ui, "gloves");
    run_book(&mut ui, "preview_item", Some(gloves));
    assert_eq!(
        ui.text(hero, "book_swing").as_deref(),
        Some("2,33 (+0,07) уд/с")
    );
    let great_axe = bag_item(&ui, "great_axe");
    run_book(&mut ui, "preview_item", Some(great_axe));
    assert_eq!(
        ui.text(hero, "book_swing").as_deref(),
        Some("2,33 (−0,54) уд/с")
    );
    run_book(&mut ui, "clear_preview", None);
    assert_eq!(ui.text(hero, "book_damage").as_deref(), Some("14"));
    assert_eq!(ui.text(hero, "book_swing").as_deref(), Some("2,33 уд/с"));
    wear(&mut ui, "cap");
    assert_eq!(ui.number(hero, "strength"), Some(9.0));
    assert_eq!(ui.number(hero, "max_health"), Some(138.0));
    assert_eq!(ui.number(hero, "health"), Some(41.0));
    assert_eq!(ui.count("bag_item"), 11);
    wear(&mut ui, "worn_axe");
    assert_eq!(ui.number(hero, "damage"), Some(12.0));
    assert_eq!(
        ui.text(hero, "gear_name_weapon").as_deref(),
        Some("Щербатый топор")
    );
    assert_eq!(
        ui.text(bag_item(&ui, "axe"), "item_icon").as_deref(),
        Some("item_axe")
    );
}

#[test]
fn dragging_an_equipped_item_moves_only_that_item_to_the_backpack() {
    let mut ui = equipped_book("ilya");
    let hero = ui.id_of("Герой");
    let slots = [
        ("weapon", "axe", [-776.0, 108.0]),
        ("head", "cap", [-776.0, -15.0]),
        ("body", "armor", [-618.0, -15.0]),
        ("gloves", "gloves", [-461.0, -15.0]),
        ("belt", "belt", [-303.0, -15.0]),
        ("boots", "boots", [-143.0, -15.0]),
        ("offhand", "shield", [-618.0, 108.0]),
        ("amulet", "amulet", [-461.0, 108.0]),
        ("ring1", "ring_strength", [-303.0, 108.0]),
        ("ring2", "ring_skill", [-143.0, 108.0]),
    ];
    for (_, item, _) in &slots[1..] {
        wear(&mut ui, item);
    }
    let original_count = ui.count("bag_item");
    let last_order = ui.number(bag_item(&ui, "worn_axe"), "bag_order").unwrap();
    let backpack = middle_of(&ui, |element| element.place_name() == Some("backpack"));

    for (index, (slot, item, offset)) in slots.into_iter().enumerate() {
        assert_eq!(
            ui.text(hero, &format!("equip_{slot}")).as_deref(),
            Some(item)
        );
        let icon = ui.text(hero, &format!("gear_{slot}")).unwrap();
        let name = ui.text(hero, &format!("gear_name_{slot}")).unwrap();
        let at = middle_of(&ui, |element| element.placement().offset == offset);
        ui.press(at[0], at[1]);
        ui.move_to(backpack[0], backpack[1]);
        assert_eq!(
            ui.text(hero, &format!("gear_{slot}")).as_deref(),
            Some(icon.as_str())
        );
        ui.release();

        assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
        assert!(ui.text(hero, &format!("gear_{slot}")).is_none(), "{slot}");
        let moved = bag_item(&ui, item);
        assert_eq!(ui.text(moved, "item_icon").as_deref(), Some(icon.as_str()));
        assert_eq!(ui.text(moved, "item_name").as_deref(), Some(name.as_str()));
        assert_eq!(
            ui.number(moved, "bag_order"),
            Some(last_order + (index + 1) as f64)
        );

        ui.click(at[0], at[1]);
        assert_eq!(ui.count("bag_item"), original_count + index + 1);
    }

    ui.key("KeyC");
    ui.key("KeyC");
    assert_eq!(active_screen(&ui), "hero_stats");
    assert_eq!(ui.text(hero, "book_damage").as_deref(), Some("0"));
    assert_eq!(ui.text(hero, "book_strength").as_deref(), Some("8"));
    assert_eq!(ui.count("bag_item"), original_count + slots.len());
    bag_item(&ui, "axe");
}

#[test]
fn dragging_from_the_backpack_equips_in_the_chosen_compatible_slot() {
    let mut ui = equipped_book("ilya");
    let hero = ui.id_of("Герой");
    let ring = bag_item(&ui, "ring_skill");
    let from = middle_of_placed(&ui, |placed| {
        placed.object == Some(ring) && matches!(placed.element, Element::Button { .. })
    });
    let wrong = middle_of(&ui, |element| element.place_name() == Some("weapon_slot"));
    let ring2 = middle_of(&ui, |element| element.place_name() == Some("ring2_slot"));
    for destination in [wrong, from] {
        ui.press(from[0], from[1]);
        ui.move_to(wrong[0], wrong[1]);
        ui.move_to(destination[0], destination[1]);
        ui.release();
        assert_eq!(bag_item(&ui, "ring_skill"), ring);
        assert_eq!(ui.text(hero, "equip_weapon").as_deref(), Some("axe"));
        assert!(ui.text(hero, "gear_ring1").is_none());
        assert!(ui.text(hero, "gear_ring2").is_none());
    }

    ui.press(from[0], from[1]);
    ui.move_to(ring2[0], ring2[1]);
    ui.release();

    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
    assert!(ui.text(hero, "gear_ring1").is_none());
    assert_eq!(ui.text(hero, "equip_ring2").as_deref(), Some("ring_skill"));
    assert_eq!(ui.count("bag_item"), 11);
}

#[test]
fn rings_and_two_handed_weapons_return_exactly_the_displaced_items() {
    let mut ui = equipped_book("ilya");
    let hero = ui.id_of("Герой");
    wear(&mut ui, "ring_strength");
    wear(&mut ui, "ring_skill");
    let third = bag_item(&ui, "ring_wisdom");
    run_book(&mut ui, "preview_item", Some(third));
    assert_eq!(ui.text(hero, "book_strength").as_deref(), Some("9 (−1)"));
    assert_eq!(ui.text(hero, "book_wisdom").as_deref(), Some("2 (+2)"));
    wear(&mut ui, "ring_wisdom");
    assert_eq!(ui.text(hero, "equip_ring1").as_deref(), Some("ring_wisdom"));
    assert_eq!(ui.text(hero, "equip_ring2").as_deref(), Some("ring_skill"));
    bag_item(&ui, "ring_strength");
    assert_eq!(ui.count("bag_item"), 10);

    wear(&mut ui, "shield");
    assert_eq!(ui.number(hero, "block_chance"), Some(0.37));
    let two_handed = bag_item(&ui, "great_axe");
    run_book(&mut ui, "preview_item", Some(two_handed));
    assert_eq!(ui.text(hero, "book_block").as_deref(), Some("37 % (−37 %)"));
    wear(&mut ui, "great_axe");
    assert!(ui.text(hero, "gear_offhand").is_none());
    bag_item(&ui, "axe");
    bag_item(&ui, "shield");
    assert_eq!(ui.count("bag_item"), 10);

    wear(&mut ui, "shield");
    assert!(ui.text(hero, "gear_weapon").is_none());
    assert_eq!(
        ui.text(hero, "gear_offhand").as_deref(),
        Some("item_offhand")
    );
    bag_item(&ui, "great_axe");
    assert_eq!(ui.count("bag_item"), 10);
    wear(&mut ui, "axe");
    assert_eq!(ui.number(hero, "damage"), Some(14.0));
    assert_eq!(ui.count("bag_item"), 9);
    let total = ui.game.world.ids().count();
    for _ in 0..20 {
        wear(&mut ui, "great_axe");
        wear(&mut ui, "axe");
        wear(&mut ui, "shield");
    }
    assert_eq!(ui.game.world.ids().count(), total);
}

#[test]
fn strong_water_can_be_redistributed_only_during_this_opening_without_healing() {
    let mut ui = equipped_book("vasilisa");
    let hero = ui.id_of("Герой");
    set_number(&mut ui, hero, "health", 41.0);
    set_number(&mut ui, hero, "spirit", 1.0);
    run_book(&mut ui, "add_wisdom", None);
    run_book(&mut ui, "add_wisdom", None);
    assert_eq!(ui.number(hero, "wisdom"), Some(10.0));
    assert_eq!(ui.number(hero, "max_spirit"), Some(5.0));
    assert_eq!(ui.number(hero, "spirit"), Some(1.0));
    assert!(!ui.flag(hero, "can_add_water"));
    assert!(ui.flag(hero, "can_undo_wisdom"));
    run_book(&mut ui, "add_strength", None);
    assert_eq!(ui.number(hero, "strength"), Some(3.0));
    run_book(&mut ui, "undo_wisdom", None);
    run_book(&mut ui, "add_strength", None);
    assert_eq!(ui.number(hero, "strength"), Some(4.0));
    assert_eq!(ui.number(hero, "wisdom"), Some(9.0));
    assert_eq!(ui.number(hero, "health"), Some(41.0));
    assert_eq!(ui.number(hero, "spirit"), Some(1.0));
    wear(&mut ui, "amulet");
    assert_eq!(ui.number(hero, "wisdom"), Some(11.0));
    assert_eq!(ui.number(hero, "spirit"), Some(1.0));
    ui.key("KeyC");
    ui.key("KeyC");
    assert!(!ui.flag(hero, "can_undo_wisdom"));
    assert!(!ui.flag(hero, "can_undo_strength"));
    run_book(&mut ui, "undo_strength", None);
    assert_eq!(ui.number(hero, "strength"), Some(4.0));
    assert_eq!(ui.number(hero, "strong_water"), Some(0.0));
    assert_eq!(ui.text(hero, "gear_amulet").as_deref(), Some("item_amulet"));
    assert_eq!(ui.number(hero, "health"), Some(41.0));
    assert_eq!(ui.number(hero, "spirit"), Some(1.0));
}

#[test]
fn a_large_backpack_keeps_both_items_displaced_by_a_two_handed_weapon() {
    let mut scene: Value = serde_json::from_str(&read("scene.json")).unwrap();
    let objects = scene["objects"].as_array_mut().unwrap();
    let player = objects.iter_mut().find(|o| o["hero"] == true).unwrap();
    player["equip_offhand"] = "shield".into();
    for index in 0..301 {
        objects.push(json!({
            "bag_item": true,
            "item_id": if index == 300 { "great_axe" } else { "cap" },
            "bag_order": index + 1,
        }));
    }
    let (mut ui, _) = load_scene(&scene.to_string());
    click_place(&mut ui, "new_game");
    click_place(&mut ui, "go");
    ui.key("KeyC");
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
    assert_eq!(ui.game.max_objects, 256);
    assert!(ui.game.world.alive_count() > ui.game.max_objects);
    assert_eq!(ui.count("bag_item"), 301);
    let hero = ui.id_of("Герой");
    wear(&mut ui, "great_axe");
    assert_eq!(ui.count("bag_item"), 302);
    assert_eq!(ui.text(hero, "equip_weapon").as_deref(), Some("great_axe"));
    assert!(ui.text(hero, "gear_offhand").is_none());
    bag_item(&ui, "axe");
    bag_item(&ui, "shield");
    wear(&mut ui, "axe");
    wear(&mut ui, "shield");
    assert_eq!(ui.count("bag_item"), 301);
    bag_item(&ui, "great_axe");
    assert!(ui.game.messages().iter().all(|m| !m.contains("потолок")));
    ui.key("KeyC");
    ui.ticks(2, 1.0 / 60.0);
    assert!(ui.game.step_count() > 0);
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
}

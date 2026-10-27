//! Фаза 54 — выбор на экране: `choose` у места наведения, выбранное как удержанное наведение, память выбора
//! страницы, запертая кнопка `needs_choice`, подстановка `{choice:…}` в тексте, картинке панели и начальных
//! значениях `new_game`, проверки перед запуском. Ход видео выбранного во времени — в `render::video_play`'s own
//! `mod tests`; проигрыватели и видеокарта в браузере — только QA на стенде. Игра собрана из текстов, её ведёт
//! `ui_support::Ui`.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::game::Game;
use engine::core::interface;
use engine::core::screens::{
    self, ButtonCommand, ChoiceRef, Element, InitialValue, PanelFill, ScreensConfig, TextPart,
};
use engine::core::value::Value;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageDecl, ImageVerdict, NeededMedia, ReverseVerdict, load_rest_with_tables, read_entry,
    read_texts_with_imported,
};
use engine::data::session::PlaySession;
use support::{Ui, VIEWPORT};

const PROPS: &str = r#"{"properties":{"hero_id":"text","hero_title":"text","strength":"number"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const SCENE: &str =
    r#"{"objects":[{"name":"hero","hero_id":"nobody","hero_title":"nobody","strength":1}]}"#;

const IMAGES: &str = r#""images":{
    "ilya_card":{"path":"ilya.png"},
    "alyosha_card":{"path":"alyosha.png"},
    "film":{"path":"film.mp4"},
    "plain":{"path":"plain.png"}}"#;

/// Строки таблицы выбора: Илья и Василиса с картинкой, Алёша с видео. Сила Ильи записана как `8.0`.
const HEROES: &str = r#"{
    "ilya":{"title":"Илья","strength":8.0,"portrait":"ilya_card"},
    "alyosha":{"title":"Алёша","strength":6.5,"portrait":"film"},
    "vasilisa":{"title":"Василиса","strength":7,"portrait":""}}"#;

fn game_json() -> String {
    game_json_with(IMAGES)
}

fn game_json_with(images: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"ui.ttf"}},{images},
         "tables":{{"heroes":"heroes.json","other":"other.json"}}}}}}"##
    )
}

fn screens_json(menu: &str) -> String {
    format!(
        r#"{{"screens":[
    {{"name":"menu","world_runs":false,"elements":[{menu}]}},
    {{"name":"game","world_runs":true,"keys":{{"Escape":["quit"]}},"elements":[]}}
]}}"#
    )
}

fn png() -> ImageVerdict {
    ImageVerdict::Ok {
        width: 8,
        height: 8,
        pixels: vec![255u8; 8 * 8 * 4],
    }
}

/// Видео, которое страница прочитала дважды: второй проигрыватель того же файла в порядке.
fn film() -> ImageVerdict {
    ImageVerdict::Video {
        width: 64,
        height: 128,
        seconds: 3.0,
        reverse: Some(ReverseVerdict::Ok {
            width: 64,
            height: 128,
            seconds: 3.0,
        }),
    }
}

fn verdicts() -> Vec<(String, ImageVerdict)> {
    vec![
        ("ilya_card".to_string(), png()),
        ("alyosha_card".to_string(), png()),
        ("film".to_string(), film()),
        ("plain".to_string(), png()),
    ]
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn try_load_tables(menu: &str, tables: &[(&str, &str)]) -> Result<Loaded, LoadFailure> {
    try_load_scene(menu, tables, SCENE)
}

fn try_load_scene(menu: &str, tables: &[(&str, &str)], scene: &str) -> Result<Loaded, LoadFailure> {
    let game = game_json();
    let (config, _) = read_entry(&game).expect("game.json должен разбираться");
    let tables: Vec<(String, Option<String>)> = tables
        .iter()
        .map(|(name, text)| (name.to_string(), Some(text.to_string())))
        .collect();
    load_rest_with_tables(
        &game,
        config,
        Some(PROPS),
        Some(scene),
        Some(NO_RULES),
        Some(&screens_json(menu)),
        &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
        &[],
        &[],
        &verdicts(),
        None,
        false,
        &tables,
    )
}

fn try_load(menu: &str) -> Result<Loaded, LoadFailure> {
    try_load_tables(menu, &[("heroes", HEROES), ("other", "{}")])
}

fn load(menu: &str) -> Loaded {
    try_load(menu).unwrap_or_else(|e| panic!("игра не загрузилась: {:#?}", e.errors))
}

fn errors_of(result: Result<Loaded, LoadFailure>) -> Vec<GameError> {
    result.expect_err("загрузка должна провалиться").errors
}

fn assert_error(errors: &[GameError], path: &str, message: &str) {
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains(path) && e.message.contains(message)),
        "нет ошибки {path}: {message}: {errors:?}"
    );
}

fn place(name: &str, x: u32, extra: &str) -> String {
    format!(
        r##"{{"kind":"panel","anchor":"top_left","name":"{name}","offset":[{x},0],"size":[100,100],"color":"#00000000","choose":"heroes"{extra}}}"##
    )
}

fn label(text: &str, y: u32, extra: &str) -> String {
    format!(
        r##"{{"kind":"label","anchor":"top_left","offset":[0,{y}],"size":[300,20],"text":"{text}","font":"ui"{extra}}}"##
    )
}

fn button(text: &str, x: u32, extra: &str, command: &str) -> String {
    format!(
        r##"{{"kind":"button","anchor":"top_left","offset":[{x},500],"size":[100,40],"text":"{text}","font":"ui","color":"#808080","color_hover":"#a0a0a0","text_color":"#ffffff","on_click":{command}{extra}}}"##
    )
}

/// Три места героев, их строки по наведению, надпись с выбранным героем, портрет, запертая «Новая игра» и
/// «Играть» без запора — обе начинают партию с героем.
fn hero_menu() -> String {
    let start = r#"["new_game","game",{"hero.hero_id":"{choice:heroes}","hero.hero_title":"{choice:heroes.title}"}]"#;
    [
        place("ilya", 0, ""),
        place("alyosha", 200, ""),
        place("vasilisa", 400, ""),
        label("Налево", 150, r#","show_on_hover":"ilya""#),
        label("Прямо", 150, r#","show_on_hover":"alyosha""#),
        label("Направо", 150, r#","show_on_hover":"vasilisa""#),
        label("{choice:heroes.title}: сила {choice:heroes.strength}", 200, ""),
        r##"{"kind":"panel","anchor":"top_left","offset":[0,300],"size":[50,50],"image":"{choice:heroes.portrait}"}"##
            .to_string(),
        button(
            "Новая игра",
            0,
            r##","needs_choice":"heroes","text_color_disabled":"#ff0000""##,
            start,
        ),
        button("Играть", 200, "", start),
        button(
            "Без цвета",
            400,
            r#","needs_choice":"heroes""#,
            r#"["quit"]"#,
        ),
    ]
    .join(",")
}

fn ui_of(menu: &str) -> Ui {
    let (game, config, _, _) = load(menu);
    Ui::new(game, config)
}

const ILYA: [f32; 2] = [50.0, 50.0];
const ALYOSHA: [f32; 2] = [250.0, 50.0];
const NEW_GAME: [f32; 2] = [50.0, 520.0];
const PLAY: [f32; 2] = [250.0, 520.0];
const NOWHERE: [f32; 2] = [700.0, 300.0];

fn click(ui: &mut Ui, at: [f32; 2]) {
    ui.click(at[0], at[1]);
}

fn active(ui: &Ui) -> &str {
    &ui.config.screens[ui.state.active()].name
}

/// Имена мест, которые горят, когда курсор в точке `pointer`.
fn lit_places(ui: &Ui, pointer: Option<[f32; 2]>) -> Vec<String> {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(VIEWPORT);
    let parts = interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport);
    let hover = screens::hover_place(&parts, pointer.map(|point| frame.point(point)));
    let lit = ui.state.lit(screen, hover);
    screen
        .elements
        .iter()
        .enumerate()
        .filter(|(index, _)| lit.contains(*index))
        .filter_map(|(_, element)| element.place_name().map(str::to_string))
        .collect()
}

/// Подписи надписей `show_on_hover`, которые видны, когда курсор в точке `pointer`.
fn glowing_labels(ui: &Ui, pointer: Option<[f32; 2]>) -> Vec<String> {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(VIEWPORT);
    let parts = interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport);
    let hover = screens::hover_place(&parts, pointer.map(|point| frame.point(point)));
    let lit = ui.state.lit(screen, hover);
    screen
        .elements
        .iter()
        .filter(|element| element.show_on_hover().is_some() && element.visible_over(&lit))
        .map(|element| text_of(ui, element))
        .collect()
}

fn text_of(ui: &Ui, element: &Element) -> String {
    let (Element::Label { text, .. } | Element::Button { text, .. }) = element else {
        panic!("у элемента нет подписи");
    };
    screens::format_text_with(
        text,
        &ui.game.world,
        &ui.game.properties,
        None,
        Some(ui.state.choices(&ui.config)),
    )
}

fn menu_label(ui: &Ui, prefix_or_exact: &str) -> String {
    let screen = &ui.config.screens[ui.screen_id("menu")];
    let element = screen
        .elements
        .iter()
        .find(|element| {
            matches!(element, Element::Label { text, .. }
                if matches!(text.first(), Some(TextPart::Literal(literal)) if literal == prefix_or_exact)
                    || matches!(text.first(), Some(TextPart::Choice(_)) if prefix_or_exact.is_empty()))
        })
        .expect("надпись");
    text_of(ui, element)
}

fn menu_button<'a>(ui: &'a Ui, caption: &str) -> &'a Element {
    ui.config.screens[ui.screen_id("menu")]
        .elements
        .iter()
        .find(|element| {
            matches!(element, Element::Button { text, .. }
                if matches!(text.as_slice(), [TextPart::Literal(literal)] if literal == caption))
        })
        .expect("кнопка")
}

fn locked_buttons(ui: &Ui) -> Vec<bool> {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(VIEWPORT);
    interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport)
        .iter()
        .filter_map(|part| match part {
            interface::Part::Element(placed)
                if matches!(placed.element, Element::Button { .. }) =>
            {
                Some(placed.locked)
            }
            _ => None,
        })
        .collect()
}

fn hero_text(ui: &Ui, property: &str) -> String {
    ui.text(ui.id_of("hero"), property).expect("свойство")
}

// -------------------------------------------------------------------------------------------
// Щелчок выбирает
// -------------------------------------------------------------------------------------------

#[test]
fn a_click_chooses_a_place_and_a_click_on_another_moves_the_choice() {
    let mut ui = ui_of(&hero_menu());
    assert_eq!(ui.state.chosen("heroes"), None);
    click(&mut ui, ILYA);
    assert_eq!(ui.state.chosen("heroes"), Some("ilya"));
    click(&mut ui, ILYA);
    assert_eq!(
        ui.state.chosen("heroes"),
        Some("ilya"),
        "щелчок по выбранному ничего не меняет"
    );
    click(&mut ui, ALYOSHA);
    assert_eq!(ui.state.chosen("heroes"), Some("alyosha"));
    click(&mut ui, NOWHERE);
    assert_eq!(
        ui.state.chosen("heroes"),
        Some("alyosha"),
        "щелчок мимо мест выбор не снимает"
    );
}

#[test]
fn a_chosen_place_keeps_the_hover_answer_after_the_cursor_leaves() {
    let mut ui = ui_of(&hero_menu());
    assert_eq!(glowing_labels(&ui, Some(ILYA)), ["Налево"]);
    assert_eq!(glowing_labels(&ui, Some(NOWHERE)), Vec::<String>::new());

    click(&mut ui, ILYA);
    assert_eq!(glowing_labels(&ui, Some(NOWHERE)), ["Налево"]);
    assert_eq!(
        lit_places(&ui, None),
        ["ilya"],
        "курсор за окном — выбранное горит"
    );
    assert_eq!(
        glowing_labels(&ui, Some(ALYOSHA)),
        ["Налево", "Прямо"],
        "курсор над другим местом зажигает и его, выбранное стоит"
    );
    assert_eq!(lit_places(&ui, Some(ALYOSHA)), ["ilya", "alyosha"]);
}

#[test]
fn moving_the_choice_lets_the_old_place_go_and_lights_the_new_one() {
    let mut ui = ui_of(&hero_menu());
    click(&mut ui, ILYA);
    click(&mut ui, ALYOSHA);
    assert_eq!(lit_places(&ui, Some(NOWHERE)), ["alyosha"]);
    assert_eq!(glowing_labels(&ui, Some(NOWHERE)), ["Прямо"]);
}

#[test]
fn a_button_with_choose_chooses_and_still_runs_its_command() {
    let menu = format!(
        r##"{},{}"##,
        r##"{"kind":"button","anchor":"top_left","name":"ilya","size":[100,100],"text":"Илья","font":"ui","color":"#808080","choose":"heroes","on_click":["show_screen","game"]}"##,
        label("Горит", 150, r#","show_on_hover":"ilya""#)
    );
    let mut ui = ui_of(&menu);
    click(&mut ui, ILYA);
    assert_eq!(ui.state.chosen("heroes"), Some("ilya"));
    assert_eq!(active(&ui), "game");
}

// -------------------------------------------------------------------------------------------
// Выбор живёт на странице
// -------------------------------------------------------------------------------------------

#[test]
fn the_choice_survives_screen_changes_new_game_and_quit() {
    let mut ui = ui_of(&hero_menu());
    click(&mut ui, ALYOSHA);
    click(&mut ui, PLAY);
    assert_eq!(active(&ui), "game");
    assert_eq!(ui.state.chosen("heroes"), Some("alyosha"));

    ui.key("Escape");
    assert_eq!(active(&ui), "menu", "quit возвращает на стартовый экран");
    assert_eq!(ui.state.chosen("heroes"), Some("alyosha"));
    assert_eq!(
        lit_places(&ui, None),
        ["alyosha"],
        "экран открылся заново — выбранное горит"
    );

    ui.show("game");
    ui.show("menu");
    assert_eq!(ui.state.chosen("heroes"), Some("alyosha"));
}

#[test]
fn a_new_screen_state_starts_with_no_choice() {
    let mut ui = ui_of(&hero_menu());
    click(&mut ui, ILYA);
    let fresh = engine::core::screens::ScreenState::new(ui.config.start_screen);
    assert_eq!(fresh.chosen("heroes"), None);
}

// -------------------------------------------------------------------------------------------
// Запертая кнопка
// -------------------------------------------------------------------------------------------

#[test]
fn a_locked_button_is_not_pressed_not_hovered_and_wakes_after_a_choice() {
    let mut ui = ui_of(&hero_menu());
    assert_eq!(locked_buttons(&ui), [true, false, true]);

    click(&mut ui, NEW_GAME);
    assert_eq!(active(&ui), "menu", "запертая кнопка не нажимается");
    ui.move_to(NEW_GAME[0], NEW_GAME[1]);
    assert_eq!(ui.mouse.hover, None, "и не показывает вид под курсором");
    assert_eq!(ui.mouse.captured, None);

    click(&mut ui, ILYA);
    assert_eq!(locked_buttons(&ui), [false, false, false]);
    ui.move_to(NEW_GAME[0], NEW_GAME[1]);
    assert!(ui.mouse.hover.is_some(), "с выбором кнопка обычная");
    click(&mut ui, NEW_GAME);
    assert_eq!(active(&ui), "game");
}

#[test]
fn a_locked_button_is_not_a_hover_place_and_swallows_the_press() {
    let menu = format!(
        r##"{},{},{}"##,
        place("ilya", 400, ""),
        r##"{"kind":"button","anchor":"top_left","name":"gate","size":[100,100],"text":"","font":"ui","color":"#808080","needs_choice":"heroes","on_click":["show_screen","game"]}"##,
        label("Ворота", 150, r#","show_on_hover":"gate""#)
    );
    let mut ui = ui_of(&menu);
    assert_eq!(lit_places(&ui, Some(ILYA)), Vec::<String>::new());
    assert_eq!(glowing_labels(&ui, Some(ILYA)), Vec::<String>::new());
    click(&mut ui, ILYA);
    assert_eq!(active(&ui), "menu");
    assert_eq!(
        ui.state.chosen("heroes"),
        None,
        "запертая кнопка забрала нажатие"
    );
    click(&mut ui, [450.0, 50.0]);
    assert_eq!(ui.state.chosen("heroes"), Some("ilya"));
    assert_eq!(
        lit_places(&ui, Some(ILYA)),
        ["ilya", "gate"],
        "с выбором кнопка стала местом наведения"
    );
}

#[test]
fn a_locked_caption_uses_the_disabled_color_or_the_text_color_at_45_percent() {
    let ui = ui_of(&hero_menu());
    let colors = |caption: &str| {
        let Element::Button {
            text_color,
            text_color_disabled,
            ..
        } = menu_button(&ui, caption)
        else {
            unreachable!("кнопка");
        };
        (*text_color, *text_color_disabled)
    };
    let (own, disabled) = colors("Новая игра");
    assert_eq!(
        screens::button_text_color(own, disabled, true),
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(screens::button_text_color(own, disabled, false), own);
    let (own, none) = colors("Без цвета");
    assert_eq!(none, None);
    let dimmed = screens::button_text_color(own, none, true);
    assert_eq!(&dimmed[..3], &own[..3]);
    assert!((dimmed[3] - 0.45).abs() < 1e-6);
    assert_eq!(screens::button_text_color(own, none, false), own);
}

// -------------------------------------------------------------------------------------------
// Подстановка выбора
// -------------------------------------------------------------------------------------------

#[test]
fn choice_substitutes_into_a_label_and_a_whole_number_has_no_fraction() {
    let mut ui = ui_of(&hero_menu());
    assert_eq!(menu_label(&ui, ""), ": сила ", "ничего не выбрано — пусто");
    click(&mut ui, ILYA);
    assert_eq!(menu_label(&ui, ""), "Илья: сила 8");
    click(&mut ui, ALYOSHA);
    assert_eq!(menu_label(&ui, ""), "Алёша: сила 6.5");
    click(&mut ui, [450.0, 50.0]);
    assert_eq!(menu_label(&ui, ""), "Василиса: сила 7");
}

#[test]
fn choice_without_a_field_is_the_name_of_the_chosen_place() {
    let menu = format!(
        "{},{}",
        place("ilya", 0, ""),
        label("Герой {choice:heroes}", 150, "")
    );
    let mut ui = ui_of(&menu);
    click(&mut ui, ILYA);
    assert_eq!(menu_label(&ui, "Герой "), "Герой ilya");
}

#[test]
fn choice_works_in_a_button_caption_and_in_a_label_on_a_screen_with_a_world() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"panel","anchor":"top_left","name":"ilya","size":[100,100],"color":"#00000000","choose":"heroes"},
            {"kind":"button","anchor":"top_left","offset":[0,200],"size":[100,40],"text":"За {choice:heroes.title}","font":"ui","color":"#808080","on_click":["new_game","game"]}]},
        {"name":"game","world_runs":true,"elements":[
            {"kind":"label","anchor":"top_left","size":[300,20],"text":"Играет {choice:heroes.title}","font":"ui"}]}
    ]}"##;
    let game = game_json();
    let (config, _) = read_entry(&game).expect("game.json");
    let (game, config, _, _) = load_rest_with_tables(
        &game,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(NO_RULES),
        Some(screens),
        &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
        &[],
        &[],
        &verdicts(),
        None,
        false,
        &[
            ("heroes".to_string(), Some(HEROES.to_string())),
            ("other".to_string(), Some("{}".to_string())),
        ],
    )
    .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    let mut ui = Ui::new(game, config);
    click(&mut ui, ILYA);
    let caption = &ui.config.screens[0].elements[1];
    assert_eq!(text_of(&ui, caption), "За Илья");
    click(&mut ui, [50.0, 220.0]);
    assert_eq!(active(&ui), "game");
    let in_world = &ui.config.screens[1].elements[0];
    assert_eq!(text_of(&ui, in_world), "Играет Илья");
}

#[test]
fn a_panel_image_is_the_picture_the_chosen_row_names() {
    let mut ui = ui_of(&hero_menu());
    let Element::Panel {
        fill: PanelFill::Choice { choice, names, .. },
        ..
    } = &ui.config.screens[0].elements[7]
    else {
        panic!("картинка панели из подстановки");
    };
    let choice = choice.clone();
    let names = names.clone();
    let image_of = |ui: &Ui| ui.state.choices(&ui.config).image(&choice, &names);
    assert_eq!(image_of(&ui), None, "ничего не выбрано — панели не видно");

    click(&mut ui, ILYA);
    assert_eq!(
        image_of(&ui),
        names.iter().position(|name| name == "ilya_card")
    );
    click(&mut ui, ALYOSHA);
    assert_eq!(
        image_of(&ui),
        names.iter().position(|name| name == "film"),
        "смена выбора меняет картинку"
    );
    click(&mut ui, [450.0, 50.0]);
    assert_eq!(image_of(&ui), None, "поле пустое — панели не видно");
}

#[test]
fn a_picture_named_by_no_declared_image_shows_nothing() {
    let table = r#"{"ilya":{"portrait":"ghost"}}"#;
    let menu = format!(
        "{},{}",
        place("ilya", 0, ""),
        r##"{"kind":"panel","anchor":"top_left","offset":[0,300],"size":[50,50],"image":"{choice:heroes.portrait}"}"##
    );
    let (game, config, _, _) =
        try_load_tables(&menu, &[("heroes", table), ("other", "{}")]).expect("игра");
    let mut ui = Ui::new(game, config);
    click(&mut ui, ILYA);
    let Element::Panel {
        fill: PanelFill::Choice { choice, names, .. },
        ..
    } = &ui.config.screens[0].elements[1]
    else {
        panic!("картинка из подстановки");
    };
    assert_eq!(ui.state.choices(&ui.config).image(choice, names), None);
}

#[test]
fn a_video_named_by_a_choice_is_a_looped_panel_video() {
    let (_, config, _, images) = load(&hero_menu());
    let loops = config.loop_videos(|image| images[image].video);
    let film = images.iter().position(|image| image.name == "film");
    assert_eq!(loops, film.into_iter().collect::<Vec<_>>());
}

const VIDEO_IMAGES: &str = r#""images":{
    "ilya_card":{"path":"ilya.png"},
    "film":{"path":"film.mp4","reverse":"film_back.mp4"},
    "panel_film":{"path":"panel.mp4"},
    "hover_film":{"path":"hover.mp4","reverse":"hover_back.mp4"},
    "world_film":{"path":"world.mp4"},
    "spare_film":{"path":"spare.mp4"}}"#;

fn video_menu() -> String {
    [
        place("ilya", 0, ""),
        r##"{"kind":"panel","anchor":"top_left","offset":[0,300],"size":[50,50],"image":"{choice:heroes.portrait}"}"##
            .to_string(),
        r##"{"kind":"panel","anchor":"top_left","size":[50,50],"image":"panel_film"}"##.to_string(),
        r##"{"kind":"panel","anchor":"top_left","size":[50,50],"image":"hover_film","play_on_hover":"ilya"}"##
            .to_string(),
    ]
    .join(",")
}

fn needed_media(menu: &str, scene: Option<&str>) -> NeededMedia {
    let (config, _) = read_entry(&game_json_with(VIDEO_IMAGES)).expect("game.json читается");
    read_texts_with_imported(
        &config,
        Some(PROPS),
        scene,
        Some(&screens_json(menu)),
        None,
        None,
    )
}

fn second_files(needed: &NeededMedia) -> Vec<(&str, &str)> {
    needed
        .reverses
        .iter()
        .map(|(name, path)| (name.as_str(), path.as_str()))
        .collect()
}

#[test]
fn a_video_from_a_choice_is_looped_forward_even_with_a_reverse() {
    let needed = needed_media(&video_menu(), None);
    assert!(
        second_files(&needed).contains(&("film", "film.mp4")),
        "второй проигрыватель видео выбора читает тот же файл, а не reverse: {:?}",
        needed.reverses
    );
}

#[test]
fn only_videos_a_choice_table_could_name_get_a_second_player() {
    let scene = r#"{"objects":[{"name":"w","image":"world_film"}]}"#;
    let needed = needed_media(&video_menu(), Some(scene));
    assert_eq!(
        second_files(&needed),
        [
            ("film", "film.mp4"),
            ("hover_film", "hover_back.mp4"),
            ("panel_film", "panel.mp4"),
            ("spare_film", "spare.mp4"),
        ],
        "видео мира, а у видео по наведению его reverse не двойник"
    );
    let without_scene = needed_media(&video_menu(), None);
    assert!(second_files(&without_scene).contains(&("world_film", "world.mp4")));
}

#[test]
fn a_video_a_choice_table_names_cannot_stand_on_another_panel() {
    let choice_panel = r##"{"kind":"panel","anchor":"top_left","offset":[0,300],"size":[50,50],"image":"{choice:heroes.portrait}"}"##;
    let film_panel = r##"{"kind":"panel","anchor":"top_left","size":[50,50],"image":"film"}"##;
    let after = format!("{},{choice_panel},{film_panel}", place("ilya", 0, ""));
    let errors = errors_of(try_load(&after));
    assert_error(&errors, "elements[2].image", "уже играет другая панель");
    let before = format!("{},{film_panel},{choice_panel}", place("ilya", 0, ""));
    let errors = errors_of(try_load(&before));
    assert_error(
        &errors,
        "elements[2].image",
        "берётся из таблицы выбора \"heroes\", а его уже играет другая панель",
    );
}

#[test]
fn a_video_a_choice_table_names_cannot_stand_on_an_object_of_the_world() {
    let menu = format!(
        "{},{}",
        place("ilya", 0, ""),
        r##"{"kind":"panel","anchor":"top_left","offset":[0,300],"size":[50,50],"image":"{choice:heroes.portrait}"}"##
    );
    let scene = r#"{"objects":[{"name":"hero","hero_id":"nobody","hero_title":"nobody","strength":1,"image":"film"}]}"#;
    let errors = errors_of(try_load_scene(
        &menu,
        &[("heroes", HEROES), ("other", "{}")],
        scene,
    ));
    assert_error(
        &errors,
        "elements[1].image",
        "берётся из таблицы выбора \"heroes\", а его называет ещё и объект мира",
    );
}

#[test]
fn a_picture_named_only_by_a_table_field_is_not_reported_as_unused() {
    let (_, _, warnings, _) = load(&hero_menu());
    let unused: Vec<&str> = warnings
        .iter()
        .filter(|w| w.message.contains("картинка") && w.message.contains("объявлена, но"))
        .map(|w| w.path.as_str())
        .collect();
    assert_eq!(
        unused,
        ["files → images → alyosha_card", "files → images → plain"],
        "{warnings:?}"
    );
}

#[test]
fn new_game_writes_the_chosen_row_and_its_fields_over_the_scene_value() {
    let mut ui = ui_of(&hero_menu());
    click(&mut ui, ALYOSHA);
    click(&mut ui, NEW_GAME);
    assert_eq!(active(&ui), "game");
    assert_eq!(hero_text(&ui, "hero_id"), "alyosha");
    assert_eq!(hero_text(&ui, "hero_title"), "Алёша");
}

#[test]
fn new_game_with_nothing_chosen_leaves_the_scene_value() {
    let mut ui = ui_of(&hero_menu());
    click(&mut ui, PLAY);
    assert_eq!(active(&ui), "game");
    assert_eq!(hero_text(&ui, "hero_id"), "nobody");
    assert_eq!(hero_text(&ui, "hero_title"), "nobody");
}

#[test]
fn a_field_a_row_lacks_is_not_written() {
    let table = r#"{"ilya":{"title":"Илья"},"alyosha":{"strength":6}}"#;
    let menu = format!(
        "{},{},{}",
        place("ilya", 0, ""),
        place("alyosha", 200, ""),
        button(
            "Играть",
            0,
            "",
            r#"["new_game","game",{"hero.hero_id":"{choice:heroes}","hero.hero_title":"{choice:heroes.title}"}]"#
        )
    );
    let (game, config, _, _) =
        try_load_tables(&menu, &[("heroes", table), ("other", "{}")]).expect("игра");
    let mut ui = Ui::new(game, config);
    click(&mut ui, ALYOSHA);
    click(&mut ui, [50.0, 520.0]);
    assert_eq!(hero_text(&ui, "hero_id"), "alyosha");
    assert_eq!(hero_text(&ui, "hero_title"), "nobody");
}

#[test]
fn the_initial_values_of_a_button_keep_fixed_and_chosen_entries_apart() {
    let menu = format!(
        "{},{}",
        place("ilya", 0, ""),
        button(
            "Играть",
            0,
            "",
            r#"["new_game","game",{"hero.hero_id":"{choice:heroes}","hero.strength":5}]"#
        )
    );
    let (_, config, _, _) = load(&menu);
    let values = &config.initial_values[0];
    assert!(values.iter().any(|(_, _, value)| matches!(
        value,
        InitialValue::Choice(ChoiceRef { group, field: None }) if group == "heroes"
    )));
    assert!(
        values.iter().any(
            |(_, _, value)| matches!(value, InitialValue::Fixed(Value::Number(n)) if *n == 5.0)
        )
    );
    let Element::Button { on_click, .. } = &config.screens[0].elements[1] else {
        panic!("кнопка");
    };
    assert!(matches!(on_click, ButtonCommand::NewGame(_, Some(0))));
}

fn live_tick(ui: &mut Ui, session: &mut PlaySession, clicks: &[[f32; 2]]) {
    for &[x, y] in clicks {
        ui.queue.push_mouse_move(x, y);
        ui.queue.push_mouse_down();
        ui.queue.push_mouse_up();
    }
    session.tick_live(
        &mut ui.queue,
        &mut ui.mouse,
        &mut ui.runner,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
        VIEWPORT,
        1.0 / 60.0,
        &[],
    );
}

#[test]
fn a_replay_of_a_partiya_started_with_a_choice_gives_the_chosen_value() {
    let mut ui = ui_of(&hero_menu());
    let mut session = PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    live_tick(&mut ui, &mut session, &[ALYOSHA]);
    live_tick(&mut ui, &mut session, &[NEW_GAME]);
    for _ in 0..3 {
        live_tick(&mut ui, &mut session, &[]);
    }
    let steps = ui.game.session_step_count();
    assert!(steps > 0);
    assert_eq!(hero_text(&ui, "hero_id"), "alyosha");
    session.end(&mut ui.game);
    let text = session.recording_text(&ui.game);
    assert!(
        text.contains(r#""hero.hero_id":"alyosha""#) && !text.contains("{choice"),
        "в запись идёт выбранное значение: {text}"
    );

    let mut again = ui_of(&hero_menu());
    let mut replay =
        PlaySession::begin_replay(&text, &mut again.game, &again.config, &mut again.state)
            .expect("запись читается");
    while again.game.session_step_count() < steps {
        replay.step_once(
            &mut again.queue,
            &mut again.mouse,
            &mut again.game,
            &again.config,
            &mut again.state,
            VIEWPORT,
            &[],
        );
    }
    assert_eq!(hero_text(&again, "hero_id"), "alyosha");
    assert_eq!(hero_text(&again, "hero_title"), "Алёша");

    replay.seek(
        0,
        &mut again.queue,
        &mut again.mouse,
        &mut again.game,
        &again.config,
        &mut again.state,
        VIEWPORT,
        &[],
    );
    replay.seek(
        steps,
        &mut again.queue,
        &mut again.mouse,
        &mut again.game,
        &again.config,
        &mut again.state,
        VIEWPORT,
        &[],
    );
    assert_eq!(hero_text(&again, "hero_id"), "alyosha");
}

#[test]
fn a_replay_of_a_partiya_started_with_nothing_chosen_keeps_the_scene_value() {
    let mut ui = ui_of(&hero_menu());
    let mut session = PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    live_tick(&mut ui, &mut session, &[PLAY]);
    for _ in 0..3 {
        live_tick(&mut ui, &mut session, &[]);
    }
    session.end(&mut ui.game);
    let text = session.recording_text(&ui.game);
    assert!(
        !text.contains("hero.hero_id"),
        "ничего не выбрано, значение не пишется: {text}"
    );
}

// -------------------------------------------------------------------------------------------
// Проверки перед запуском
// -------------------------------------------------------------------------------------------

fn fails_with(menu: &str, path: &str, message: &str) {
    assert_error(&errors_of(try_load(menu)), path, message);
}

#[test]
fn choose_that_is_not_a_string_or_has_no_name_is_an_error() {
    fails_with(
        r##"{"kind":"panel","anchor":"top_left","name":"ilya","size":[10,10],"color":"#ffffff","choose":5}"##,
        "choose",
        "ожидалась строка",
    );
    fails_with(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","choose":"heroes"}"##,
        "choose",
        "у \"choose\" нет \"name\"",
    );
}

#[test]
fn choose_on_a_label_a_bar_a_list_an_object_element_or_a_row_is_an_error() {
    let label = r##"{"kind":"label","anchor":"top_left","name":"ilya","size":[10,10],"text":"a","font":"ui","choose":"heroes"}"##;
    let bar = r##"{"kind":"bar","anchor":"top_left","size":[10,10],"for":{"has":["hero_id"]},
        "value":"strength","max":1,"color":"#ffffff","choose":"heroes"}"##;
    let list = r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["hero_id"]},
        "row_height":10,"row":[],"choose":"heroes"}"##;
    let with_for = r##"{"kind":"panel","anchor":"top_left","name":"ilya","size":[10,10],"color":"#ffffff",
        "for":{"has":["hero_id"]},"choose":"heroes"}"##;
    let in_row = r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["hero_id"]},"row_height":10,
        "row":[{"kind":"panel","name":"ilya","size":[10,10],"color":"#ffffff","choose":"heroes"}]}"##;
    for (element, message) in [
        (label, "у надписи нет \"choose\""),
        (bar, "у шкалы нет \"choose\""),
        (list, "у списка нет \"choose\""),
        (with_for, "у элемента с for нет \"choose\""),
        (in_row, "у элемента строки списка нет \"choose\""),
    ] {
        fails_with(element, "choose", message);
    }
}

#[test]
fn choose_or_needs_choice_naming_a_missing_table_is_an_error() {
    fails_with(
        r##"{"kind":"panel","anchor":"top_left","name":"ilya","size":[10,10],"color":"#ffffff","choose":"ghost"}"##,
        "choose",
        "таблицы данных \"ghost\" нет в files.tables",
    );
    fails_with(
        &button("Идти", 0, r#","needs_choice":"ghost""#, r#"["quit"]"#),
        "needs_choice",
        "таблицы данных \"ghost\" нет в files.tables",
    );
}

#[test]
fn a_choice_table_that_is_not_an_object_of_row_objects_is_an_error() {
    let menu = place("ilya", 0, "");
    for table in [r#"[1,2]"#, r#"{"ilya":5}"#, r#"7"#] {
        let errors = errors_of(try_load_tables(
            &menu,
            &[("heroes", table), ("other", "{}")],
        ));
        assert_error(&errors, "choose", "не объект со строками-объектами");
    }
}

#[test]
fn the_name_of_a_chosen_place_must_be_a_row_key() {
    fails_with(
        &place("stranger", 0, ""),
        "name",
        "имя места \"stranger\" — не ключ строки таблицы \"heroes\"",
    );
}

#[test]
fn needs_choice_on_a_panel_or_for_a_group_nobody_can_choose_is_an_error() {
    fails_with(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","needs_choice":"heroes"}"##,
        "needs_choice",
        "у панели нет \"needs_choice\"",
    );
    fails_with(
        &button("Идти", 0, r#","needs_choice":"other""#, r#"["quit"]"#),
        "needs_choice",
        "ни у одной панели или кнопки ни на одном экране нет choose",
    );
}

#[test]
fn a_choice_substitution_naming_a_missing_table_or_field_is_an_error() {
    fails_with(
        &label("{choice:ghost.title}", 0, ""),
        "text",
        "таблицы данных \"ghost\" нет в files.tables",
    );
    fails_with(
        &label("{choice:heroes.height}", 0, ""),
        "text",
        "в строках таблицы \"heroes\" нет поля \"height\"",
    );
    fails_with(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"{choice:heroes.height}"}"##,
        "image",
        "нет поля \"height\"",
    );
    fails_with(
        &label("{choice:}", 0, ""),
        "text",
        "должна быть вида {choice:таблица}",
    );
}

#[test]
fn a_choice_substitution_in_a_tooltip_is_an_error() {
    let screens = r##"{"tooltip":{"font":"ui","font_size":14,"color":"#222222","text_color":"#ffffff","padding":[4,4],"gap":4},
        "screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"{choice:heroes}"}]},
        {"name":"game","world_runs":true,"elements":[]}]}"##;
    let game = game_json();
    let (config, _) = read_entry(&game).expect("game.json");
    let result = load_rest_with_tables(
        &game,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(NO_RULES),
        Some(screens),
        &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
        &[],
        &[],
        &verdicts(),
        None,
        false,
        &[
            ("heroes".to_string(), Some(HEROES.to_string())),
            ("other".to_string(), Some("{}".to_string())),
        ],
    );
    let errors = result.expect_err("подсказка").errors;
    assert_error(&errors, "tooltip", "в подсказке её нет");
}

#[test]
fn text_color_disabled_needs_needs_choice_and_a_real_color() {
    fails_with(
        &button(
            "Идти",
            0,
            r##","text_color_disabled":"#ff0000""##,
            r#"["quit"]"#,
        ),
        "text_color_disabled",
        "у элемента без needs_choice",
    );
    fails_with(
        &format!(
            "{},{}",
            place("ilya", 0, ""),
            button(
                "Идти",
                0,
                r##","needs_choice":"heroes","text_color_disabled":"red""##,
                r#"["quit"]"#
            )
        ),
        "text_color_disabled",
        "цвет должен быть вида",
    );
}

#[test]
fn a_choice_in_an_initial_value_must_fill_a_whole_string_property() {
    let with = |value: &str| {
        format!(
            "{},{}",
            place("ilya", 0, ""),
            button(
                "Играть",
                0,
                "",
                &format!(r#"["new_game","game",{{"hero.hero_id":"{value}"}}]"#)
            )
        )
    };
    fails_with(
        &with("герой {choice:heroes}"),
        "hero.hero_id",
        "занимает его целиком",
    );
    let number = format!(
        "{},{}",
        place("ilya", 0, ""),
        button(
            "Играть",
            0,
            "",
            r#"["new_game","game",{"hero.strength":"{choice:heroes.strength}"}]"#
        )
    );
    fails_with(&number, "hero.strength", "вид число");
    fails_with(
        &with("{choice:heroes.height}"),
        "hero.hero_id",
        "нет поля \"height\"",
    );
}

#[test]
fn a_chosen_place_nobody_answers_or_reads_is_a_warning() {
    let lonely = place("ilya", 0, "");
    let (_, _, warnings, _) = load(&lonely);
    assert!(
        warnings
            .iter()
            .any(|w| w.path.contains("choose") && w.message.contains("место выбора \"ilya\"")),
        "{warnings:?}"
    );

    for answered in [
        format!(
            "{},{}",
            place("ilya", 0, ""),
            label("Строка", 150, r#","show_on_hover":"ilya""#)
        ),
        format!(
            "{},{}",
            place("ilya", 0, ""),
            label("{choice:heroes.title}", 150, "")
        ),
        format!(
            "{},{}",
            place("ilya", 0, ""),
            button("Идти", 0, r#","needs_choice":"heroes""#, r#"["quit"]"#)
        ),
        format!(
            "{},{}",
            place("ilya", 0, ""),
            button(
                "Играть",
                0,
                "",
                r#"["new_game","game",{"hero.hero_id":"{choice:heroes}"}]"#
            )
        ),
    ] {
        let (_, _, warnings, _) = load(&answered);
        assert!(
            !warnings.iter().any(
                |w| w.message.contains("место выбора") || w.message.contains("место наведения")
            ),
            "{answered}: {warnings:?}"
        );
    }
}

#[test]
fn a_table_read_only_by_screens_is_not_reported_as_unused() {
    let (_, _, warnings, _) = load(&hero_menu());
    let unused: Vec<&str> = warnings
        .iter()
        .filter(|w| w.message.contains("объявлена, но не встречается"))
        .map(|w| w.path.as_str())
        .collect();
    assert_eq!(unused, ["files → tables → other"], "{warnings:?}");
}

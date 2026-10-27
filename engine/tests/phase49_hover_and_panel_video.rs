//! Фаза 49 — наведение и видео на панели: место наведения `name`, показ `show_on_hover`, видео панели с
//! `play_on_hover` и файлом `reverse`, проверки перед запуском, предупреждение о месте без отклика и
//! команда `exit`. Ход видео во времени — в `render::video_play`'s own `mod tests`; проигрыватели и
//! видеокарта в браузере — только QA на стенде. Игра собрана из текстов, её ведёт `ui_support::Ui`.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::game::Game;
use engine::core::interface;
use engine::core::screens::{self, Element, Fill, Lit, PanelFill, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{ImageDecl, ImageVerdict, ReverseVerdict, load_rest, read_entry};
use support::{Ui, VIEWPORT};

const PROPS: &str = r#"{"properties":{"car":"flag"}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const NO_OBJECTS: &str = r#"{"objects":[]}"#;
const CAR_SCENE: &str = r#"{"objects":[{"name":"c1","car":true}]}"#;

const VIDEO_IMAGES: &str = r#""images":{
    "hero":{"path":"hero.mp4","reverse":"hero-reverse.mp4"},
    "head":{"path":"head.png"}}"#;

fn game_json(images: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"ui.ttf"}},{images}}}}}"##
    )
}

fn screens_json(menu: &str) -> String {
    format!(
        r#"{{"screens":[
    {{"name":"menu","world_runs":false,"keys":{{"Escape":["exit"]}},"elements":[{menu}]}},
    {{"name":"game","world_runs":true,"elements":[]}}
]}}"#
    )
}

/// Ролик 64 × 128 (кадр 64 × 64) длиной 7,5 с и его `reverse` того же размера и длины.
fn hero(reverse: Option<ReverseVerdict>) -> ImageVerdict {
    ImageVerdict::Video {
        width: 64,
        height: 128,
        seconds: 7.5,
        reverse,
    }
}

fn same_reverse() -> ReverseVerdict {
    ReverseVerdict::Ok {
        width: 64,
        height: 128,
        seconds: 7.5,
    }
}

fn head() -> ImageVerdict {
    ImageVerdict::Ok {
        width: 8,
        height: 8,
        pixels: vec![0u8; 8 * 8 * 4],
    }
}

fn files(reverse: Option<ReverseVerdict>) -> Vec<(&'static str, ImageVerdict)> {
    vec![("hero", hero(reverse)), ("head", head())]
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

/// Игра из текстов с готовым `screens.json`.
fn try_load_screens(
    images: &str,
    scene: &str,
    screens: &str,
    verdicts: &[(&str, ImageVerdict)],
) -> Result<Loaded, LoadFailure> {
    let game = game_json(images);
    let (config, _) = read_entry(&game).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = verdicts
        .iter()
        .map(|(name, verdict)| (name.to_string(), verdict.clone()))
        .collect();
    load_rest(
        &game,
        config,
        Some(PROPS),
        Some(scene),
        Some(NO_RULES),
        Some(screens),
        &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
        &[],
        &[],
        &image_data,
        None,
        false,
    )
}

/// Игра, у которой на экране `menu` стоят элементы `menu`.
fn try_load(
    images: &str,
    scene: &str,
    menu: &str,
    verdicts: &[(&str, ImageVerdict)],
) -> Result<Loaded, LoadFailure> {
    try_load_screens(images, scene, &screens_json(menu), verdicts)
}

fn load_screens(scene: &str, screens: &str) -> Loaded {
    try_load_screens(VIDEO_IMAGES, scene, screens, &files(Some(same_reverse())))
        .unwrap_or_else(|e| panic!("игра не загрузилась: {:#?}", e.errors))
}

fn load(menu: &str) -> Loaded {
    load_screens(CAR_SCENE, &screens_json(menu))
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

fn ui_of(menu: &str) -> Ui {
    let (game, config, _, _) = load(menu);
    Ui::new(game, config)
}

/// Экран из предложенных мест: герой 100 × 100 в углу — невидимое место, его ролик под ним, горящая строка
/// и надпись над ним, значок, кнопка «Выход» и кнопка «Играть» в стороне.
const HERO_MENU: &str = r##"
    {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[100,100],"image":"hero","play_on_hover":"ilya"},
    {"kind":"panel","anchor":"top_left","offset":[0,200],"size":[100,40],"color":"#ffd27a","show_on_hover":"ilya"},
    {"kind":"label","anchor":"top_left","offset":[0,250],"size":[100,20],"text":"Строка","font":"ui","show_on_hover":"ilya"},
    {"kind":"panel","anchor":"top_left","name":"ilya","offset":[0,0],"size":[100,100],"color":"#00000000"},
    {"kind":"panel","anchor":"top_left","offset":[300,100],"size":[10,10],"image":"head"},
    {"kind":"button","anchor":"top_left","offset":[300,300],"size":[100,40],"text":"Выход","font":"ui","color":"#808080","on_click":["exit"]},
    {"kind":"button","anchor":"top_left","offset":[500,300],"size":[100,40],"text":"Играть","font":"ui","color":"#808080","on_click":["new_game","game"]}
"##;

/// Какое место наведения под точкой окна: имя или ничего.
fn place_at(ui: &Ui, point: Option<[f32; 2]>) -> Option<String> {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(VIEWPORT);
    let parts = interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport);
    screens::hover_place(&parts, point.map(|point| frame.point(point))).map(|index| {
        screen.elements[index]
            .place_name()
            .expect("имя")
            .to_string()
    })
}

fn menu(ui: &Ui) -> &[Element] {
    &ui.config.screens[ui.screen_id("menu")].elements
}

// -------------------------------------------------------------------------------------------
// Место наведения
// -------------------------------------------------------------------------------------------

#[test]
fn a_named_panel_or_button_is_a_hover_place_and_the_name_is_optional() {
    let (_, config, warnings, _) = load(HERO_MENU);
    assert_eq!(warnings, Vec::new());
    let elements = &config.screens[0].elements;
    let names: Vec<Option<&str>> = elements.iter().map(Element::place_name).collect();
    assert_eq!(names, [None, None, None, Some("ilya"), None, None, None]);

    let named_button = r##"{"kind":"button","anchor":"top_left","name":"go","offset":[0,0],"size":[10,10],
        "text":"","font":"ui","color":"#808080","on_click":["new_game","game"]},
        {"kind":"panel","anchor":"top_left","offset":[0,20],"size":[10,10],"image":"head","show_on_hover":"go"}"##;
    let (_, config, _, _) = load(named_button);
    assert_eq!(config.screens[0].elements[0].place_name(), Some("go"));
}

#[test]
fn a_name_on_a_label_a_bar_a_list_an_object_element_or_a_row_is_an_error() {
    let label = r##"{"kind":"label","anchor":"top_left","name":"x","size":[10,10],"text":"a","font":"ui"}"##;
    let bar = r##"{"kind":"bar","anchor":"top_left","name":"x","size":[10,10],"for":{"has":["car"]},
        "value":"car","max":1,"color":"#ffffff"}"##;
    let list = r##"{"kind":"list","anchor":"top_left","name":"x","size":[10,10],"for":{"has":["car"]},
        "row_height":10,"row":[]}"##;
    let with_for = r##"{"kind":"panel","anchor":"top_left","name":"x","size":[10,10],"color":"#ffffff",
        "for":{"has":["car"]}}"##;
    let in_row = r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"row_height":10,
        "row":[{"kind":"panel","name":"x","size":[10,10],"color":"#ffffff"}]}"##;
    for (element, message) in [
        (label, "у надписи нет \"name\""),
        (bar, "у шкалы нет \"name\""),
        (list, "у списка нет \"name\""),
        (with_for, "у элемента с for нет \"name\""),
        (in_row, "у элемента строки списка нет \"name\""),
    ] {
        let errors = errors_of(try_load(
            VIDEO_IMAGES,
            CAR_SCENE,
            element,
            &files(Some(same_reverse())),
        ));
        assert_error(&errors, "name", message);
    }
}

#[test]
fn an_empty_repeated_or_non_string_name_is_an_error() {
    let panel = |extra: &str| {
        format!(
            r##"{{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff",{extra}}}"##
        )
    };
    let fail = |menu: String| {
        errors_of(try_load(
            VIDEO_IMAGES,
            NO_OBJECTS,
            &menu,
            &files(Some(same_reverse())),
        ))
    };
    assert_error(&fail(panel(r#""name":"""#)), "name", "не может быть пустым");
    assert_error(&fail(panel(r#""name":7"#)), "name", "");
    let twice = format!("{},{}", panel(r#""name":"a""#), panel(r#""name":"a""#));
    assert_error(&fail(twice), "[1] → name", "повторяется на экране \"menu\"");
}

#[test]
fn the_same_name_on_two_screens_is_not_a_repeat() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#ffffff"},
            {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","show_on_hover":"a"}]},
        {"name":"game","world_runs":true,"elements":[
            {"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#ffffff"},
            {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","show_on_hover":"a"}]}
    ]}"##;
    load_screens(NO_OBJECTS, screens);
}

// -------------------------------------------------------------------------------------------
// Курсор над местом
// -------------------------------------------------------------------------------------------

#[test]
fn the_cursor_is_over_a_place_only_while_it_is_the_topmost_panel_or_button_under_it() {
    let ui = ui_of(HERO_MENU);
    assert_eq!(place_at(&ui, Some([50.0, 50.0])), Some("ilya".to_string()));
    assert_eq!(place_at(&ui, Some([150.0, 50.0])), None, "рядом с местом");
    assert_eq!(
        place_at(&ui, Some([320.0, 320.0])),
        None,
        "над кнопкой без имени"
    );
}

#[test]
fn a_panel_above_the_place_takes_the_cursor_and_a_button_takes_it_whatever_the_order() {
    let covered = r##"
        {"kind":"panel","anchor":"top_left","name":"a","offset":[0,0],"size":[100,100],"color":"#00000000"},
        {"kind":"panel","anchor":"top_left","offset":[50,0],"size":[50,100],"color":"#ffffff"},
        {"kind":"button","anchor":"top_left","offset":[0,50],"size":[40,50],"text":"","font":"ui","color":"#808080","on_click":["new_game","game"]},
        {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","show_on_hover":"a"}
    "##;
    let ui = ui_of(covered);
    assert_eq!(place_at(&ui, Some([20.0, 20.0])), Some("a".to_string()));
    assert_eq!(
        place_at(&ui, Some([70.0, 20.0])),
        None,
        "верхняя панель без имени забирает курсор"
    );
    assert_eq!(
        place_at(&ui, Some([20.0, 70.0])),
        None,
        "нажатие получила бы кнопка"
    );

    let named_button_below = r##"
        {"kind":"button","anchor":"top_left","name":"b","offset":[0,0],"size":[100,100],"text":"","font":"ui","color":"#808080","on_click":["new_game","game"]},
        {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[100,100],"color":"#ffffff"},
        {"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","show_on_hover":"b"}
    "##;
    let ui = ui_of(named_button_below);
    assert_eq!(
        place_at(&ui, Some([20.0, 20.0])),
        Some("b".to_string()),
        "панель поверх кнопки нажатие у кнопки не отнимает"
    );
}

#[test]
fn a_panel_with_show_on_hover_is_transparent_to_the_cursor_shown_or_not() {
    // Свечение строки лежит поверх места: скрытое, оно не закрывает место, а загоревшись, не отнимает курсор у
    // героя — иначе строка мигала бы: курсор ушёл с места, свечение погасло, курсор снова над местом.
    let glow_over_place = r##"
        {"kind":"panel","anchor":"top_left","name":"a","offset":[0,0],"size":[100,100],"color":"#00000000"},
        {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[100,100],"color":"#ffffff","show_on_hover":"a"}
    "##;
    let ui = ui_of(glow_over_place);
    assert_eq!(place_at(&ui, Some([20.0, 20.0])), Some("a".to_string()));
}

#[test]
fn a_cursor_outside_the_window_is_over_no_place() {
    let ui = ui_of(HERO_MENU);
    assert_eq!(place_at(&ui, None), None);
}

#[test]
fn a_fully_transparent_panel_is_a_place_that_catches_the_cursor() {
    let (_, config, _, _) = load(HERO_MENU);
    let Element::Panel {
        fill: PanelFill::Fixed(Fill::Color(color)),
        ..
    } = &config.screens[0].elements[3]
    else {
        panic!("место наведения — панель с цветом");
    };
    assert_eq!(color[3], 0.0);
    let ui = ui_of(HERO_MENU);
    assert_eq!(place_at(&ui, Some([10.0, 10.0])), Some("ilya".to_string()));
}

#[test]
fn the_cursor_is_over_a_place_on_a_screen_without_a_world_and_on_a_pause() {
    let ui = ui_of(HERO_MENU);
    assert!(!ui.game.has_world(), "на меню мира нет");
    assert_eq!(place_at(&ui, Some([50.0, 50.0])), Some("ilya".to_string()));

    let pause = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"button","anchor":"top_left","offset":[300,0],"size":[40,40],"text":"","font":"ui","color":"#808080","on_click":["new_game","game"]}]},
        {"name":"game","world_runs":true,"keys":{"Escape":["show_screen","pause"]},"elements":[]},
        {"name":"pause","world_runs":false,"elements":[
            {"kind":"panel","anchor":"top_left","name":"a","size":[100,100],"color":"#00000000"},
            {"kind":"panel","anchor":"top_left","offset":[0,200],"size":[10,10],"color":"#ffffff","show_on_hover":"a"}]}
    ]}"##;
    let (game, config, _, _) = load_screens(CAR_SCENE, pause);
    let mut ui = Ui::new(game, config);
    ui.click(320.0, 20.0);
    ui.key("Escape");
    assert_eq!(ui.config.screens[ui.state.active()].name, "pause");
    assert!(ui.game.has_world(), "на паузе мир есть и стоит");
    assert_eq!(place_at(&ui, Some([50.0, 50.0])), Some("a".to_string()));
}

// -------------------------------------------------------------------------------------------
// Показ по наведению
// -------------------------------------------------------------------------------------------

#[test]
fn a_panel_and_a_label_with_show_on_hover_are_drawn_only_over_their_place() {
    let ui = ui_of(HERO_MENU);
    let elements = menu(&ui);
    let place = elements.iter().position(|e| e.place_name() == Some("ilya"));
    assert_eq!(elements[1].show_on_hover(), place);
    assert_eq!(elements[2].show_on_hover(), place);
    for glowing in [&elements[1], &elements[2]] {
        assert!(glowing.visible_over(&Lit::over(place)));
        assert!(!glowing.visible_over(&Lit::over(None)));
        assert!(
            !glowing.visible_over(&Lit::over(Some(0))),
            "над другим местом не рисуется"
        );
    }
    for always in [&elements[0], &elements[3], &elements[4]] {
        assert!(always.visible_over(&Lit::over(None)) && always.visible_over(&Lit::over(place)));
    }
}

#[test]
fn show_on_hover_naming_a_missing_place_or_set_on_a_button_a_bar_a_list_or_a_row_is_an_error() {
    let missing = r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","show_on_hover":"ghost"}"##;
    let errors = errors_of(try_load(
        VIDEO_IMAGES,
        NO_OBJECTS,
        missing,
        &files(Some(same_reverse())),
    ));
    assert_error(&errors, "show_on_hover", "нет места наведения \"ghost\"");

    let button = r##"{"kind":"button","anchor":"top_left","size":[10,10],"text":"","font":"ui","color":"#808080",
        "on_click":["quit"],"show_on_hover":"a"}"##;
    let bar = r##"{"kind":"bar","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"value":"car","max":1,
        "color":"#ffffff","show_on_hover":"a"}"##;
    let list = r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"row_height":10,
        "row":[],"show_on_hover":"a"}"##;
    let in_row = r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"row_height":10,
        "row":[{"kind":"label","text":"x","font":"ui","size":[10,10],"show_on_hover":"a"}]}"##;
    for (element, message) in [
        (button, "у кнопки нет \"show_on_hover\""),
        (bar, "у шкалы нет \"show_on_hover\""),
        (list, "у списка нет \"show_on_hover\""),
        (in_row, "у элемента строки списка нет \"show_on_hover\""),
    ] {
        let menu = format!(
            r##"{{"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#00000000"}},{element}"##
        );
        let errors = errors_of(try_load(
            VIDEO_IMAGES,
            CAR_SCENE,
            &menu,
            &files(Some(same_reverse())),
        ));
        assert_error(&errors, "show_on_hover", message);
    }
}

// -------------------------------------------------------------------------------------------
// Видео на панели
// -------------------------------------------------------------------------------------------

fn video_panel(extra: &str) -> String {
    format!(
        r##"{{"kind":"panel","anchor":"top_left","size":[100,100],"image":"hero"{extra}}},
        {{"kind":"panel","anchor":"top_left","name":"ilya","size":[100,100],"color":"#00000000"}}"##
    )
}

#[test]
fn a_video_on_a_panel_with_play_on_hover_and_a_reverse_loads_and_names_its_place() {
    let (_, config, warnings, images) = load(HERO_MENU);
    assert_eq!(warnings, Vec::new());
    let hero = images.iter().position(|decl| decl.name == "hero").unwrap();
    assert_eq!(config.panel_videos(), [hero]);
    assert_eq!(images[hero].reverse.as_deref(), Some("hero-reverse.mp4"));
    let elements = &config.screens[0].elements;
    let place = elements.iter().position(|e| e.place_name() == Some("ilya"));
    assert_eq!(elements[0].play_on_hover(), place);
}

#[test]
fn play_on_hover_naming_a_missing_place_is_an_error() {
    let errors = errors_of(try_load(
        VIDEO_IMAGES,
        NO_OBJECTS,
        &video_panel(r#","play_on_hover":"ghost""#),
        &files(Some(same_reverse())),
    ));
    assert_error(&errors, "play_on_hover", "нет места наведения \"ghost\"");
}

#[test]
fn play_on_hover_on_a_panel_without_a_video_a_button_a_label_or_a_bar_is_an_error() {
    let place =
        r##"{"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#00000000"}"##;
    let picture = r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"head","play_on_hover":"a"}"##;
    let color = r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","play_on_hover":"a"}"##;
    let button = r##"{"kind":"button","anchor":"top_left","size":[10,10],"text":"","font":"ui","color":"#808080",
        "on_click":["quit"],"play_on_hover":"a"}"##;
    let label = r##"{"kind":"label","anchor":"top_left","size":[10,10],"text":"x","font":"ui","play_on_hover":"a"}"##;
    for (element, message) in [
        (picture, "play_on_hover у панели без видео"),
        (color, "play_on_hover у панели без видео"),
        (button, "у кнопки нет \"play_on_hover\""),
        (label, "у надписи нет \"play_on_hover\""),
    ] {
        let errors = errors_of(try_load(
            VIDEO_IMAGES,
            NO_OBJECTS,
            &format!("{place},{element}"),
            &files(Some(same_reverse())),
        ));
        assert_error(&errors, "play_on_hover", message);
    }
}

#[test]
fn a_video_on_a_button_is_still_an_error() {
    let button = r##"{"kind":"button","anchor":"top_left","size":[10,10],"text":"","font":"ui","image":"hero",
        "on_click":["quit"]}"##;
    let errors = errors_of(try_load(
        VIDEO_IMAGES,
        NO_OBJECTS,
        button,
        &files(Some(same_reverse())),
    ));
    assert_error(&errors, "image", "кнопка и шкала её не берут");
}

#[test]
fn a_panel_video_without_a_reverse_is_a_usage_error() {
    let images = r#""images":{"hero":{"path":"hero.mp4"}}"#;
    let errors = errors_of(try_load(
        images,
        NO_OBJECTS,
        &video_panel(r#","play_on_hover":"ilya""#),
        &[("hero", hero(None))],
    ));
    assert_error(&errors, "image", "у него нет reverse");
}

#[test]
fn a_reverse_on_a_picture_or_not_on_an_mp4_is_a_description_error() {
    for (images, path, message) in [
        (
            r#""images":{"head":{"path":"head.png","reverse":"head-reverse.mp4"}}"#,
            "head → reverse",
            "у картинки PNG его не бывает",
        ),
        (
            r#""images":{"hero":{"path":"hero.mp4","reverse":"hero-reverse.png"}}"#,
            "hero → reverse",
            "reverse берёт MP4",
        ),
        (
            r#""images":{"hero":{"path":"hero.mp4","reverse":5}}"#,
            "hero → reverse",
            "",
        ),
    ] {
        let failure = read_entry(&game_json(images)).expect_err("ошибка описания");
        assert_error(&failure.errors, path, message);
    }
}

#[test]
fn a_reverse_of_another_size_or_missing_or_longer_by_more_than_a_frame_is_a_file_error() {
    let fail = |reverse: Option<ReverseVerdict>| {
        errors_of(try_load(
            VIDEO_IMAGES,
            NO_OBJECTS,
            HERO_MENU,
            &files(reverse),
        ))
    };
    let sized = |width, height, seconds| {
        Some(ReverseVerdict::Ok {
            width,
            height,
            seconds,
        })
    };
    assert_error(
        &fail(sized(64, 130, 7.5)),
        "images → hero",
        "reverse размером 64×130",
    );
    assert_error(
        &fail(sized(64, 128, 7.6)),
        "images → hero",
        "не должен отличаться по длине больше чем на кадр",
    );
    assert_error(
        &fail(sized(64, 128, 7.4)),
        "images → hero",
        "не должен отличаться по длине больше чем на кадр",
    );
    assert_error(
        &fail(Some(ReverseVerdict::Missing)),
        "images → hero",
        "файл reverse \"hero-reverse.mp4\" не найден",
    );
    assert_error(&fail(None), "images → hero", "не найден");
    assert_error(
        &fail(Some(ReverseVerdict::Rejected)),
        "images → hero",
        "не берётся играть этот reverse",
    );
}

#[test]
fn a_reverse_within_a_frame_of_the_video_length_is_fine() {
    for seconds in [7.5 - 1.0 / 24.0, 7.5, 7.5 + 1.0 / 24.0] {
        let verdicts = files(Some(ReverseVerdict::Ok {
            width: 64,
            height: 128,
            seconds,
        }));
        let result = try_load(VIDEO_IMAGES, NO_OBJECTS, HERO_MENU, &verdicts);
        assert!(
            result.is_ok(),
            "{seconds}: {:?}",
            result.err().map(|e| e.errors)
        );
    }
}

fn rest_panel(rest_at: &str) -> String {
    video_panel(&format!(r#","play_on_hover":"ilya","rest_at":{rest_at}"#))
}

#[test]
fn rest_at_inside_the_video_loads_and_reaches_the_panel() {
    for (text, expected) in [("2.5", 2.5), ("0", 0.0), ("7.5", 7.5)] {
        let (_, config, _, _) = load(&rest_panel(text));
        let Element::Panel { rest_at, .. } = &config.screens[0].elements[0] else {
            panic!("первый элемент — панель");
        };
        assert_eq!(*rest_at, Some(expected), "rest_at {text}");
    }
    let (_, config, _, _) = load(HERO_MENU);
    let Element::Panel { rest_at, .. } = &config.screens[0].elements[0] else {
        panic!("первый элемент — панель");
    };
    assert_eq!(*rest_at, None);
}

#[test]
fn rest_at_on_a_panel_without_a_video_is_an_error() {
    let place =
        r##"{"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#00000000"}"##;
    let picture =
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"head","rest_at":1}"##;
    let color =
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","rest_at":1}"##;
    for element in [picture, color] {
        let errors = errors_of(try_load(
            VIDEO_IMAGES,
            NO_OBJECTS,
            &format!("{place},{element}"),
            &files(Some(same_reverse())),
        ));
        assert_error(&errors, "rest_at", "rest_at у панели без видео");
    }
}

#[test]
fn rest_at_that_is_not_a_number_negative_or_longer_than_the_video_is_an_error() {
    for (text, message) in [
        (r#""2.5""#, "ожидалось число"),
        ("true", "ожидалось число"),
        ("-0.5", "не может быть отрицательным"),
        ("7.6", "длиннее ролика \"hero\""),
    ] {
        let errors = errors_of(try_load(
            VIDEO_IMAGES,
            NO_OBJECTS,
            &rest_panel(text),
            &files(Some(same_reverse())),
        ));
        assert_error(&errors, "rest_at", message);
    }
}

#[test]
fn one_video_on_two_panels_or_on_a_panel_and_a_world_object_is_a_usage_error() {
    let two = format!(
        "{HERO_MENU},{}",
        r##"{"kind":"panel","anchor":"top_left","offset":[200,0],"size":[100,100],"image":"hero","play_on_hover":"ilya"}"##
    );
    let errors = errors_of(try_load(
        VIDEO_IMAGES,
        NO_OBJECTS,
        &two,
        &files(Some(same_reverse())),
    ));
    assert_error(&errors, "elements[7].image", "уже играет другая панель");

    let scene = r#"{"objects":[{"position":[2,2],"size":[4,2],"image":"hero"}]}"#;
    let errors = errors_of(try_load(
        VIDEO_IMAGES,
        scene,
        HERO_MENU,
        &files(Some(same_reverse())),
    ));
    assert_error(&errors, "elements[0].image", "называет ещё и объект мира");
}

#[test]
fn a_video_with_a_reverse_that_no_panel_plays_is_a_plain_world_video() {
    let scene = r#"{"objects":[{"position":[2,2],"size":[4,2],"image":"hero"}]}"#;
    let (_, config, _, _) = try_load(
        VIDEO_IMAGES,
        scene,
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"head"}"##,
        &files(Some(same_reverse())),
    )
    .unwrap_or_else(|e| panic!("{:?}", e.errors));
    assert_eq!(config.panel_videos(), Vec::<usize>::new());
}

// -------------------------------------------------------------------------------------------
// Предупреждение
// -------------------------------------------------------------------------------------------

#[test]
fn a_place_no_element_answers_to_is_a_warning() {
    let lone = r##"{"kind":"panel","anchor":"top_left","name":"alone","size":[10,10],"color":"#00000000"}"##;
    let (_, _, warnings, _) = load(lone);
    assert!(
        warnings.iter().any(|w| w.message.contains(
            "место наведения \"alone\" на экране \"menu\": на него не откликается ни один элемент"
        )),
        "{warnings:?}"
    );

    let (_, _, warnings, _) = load(HERO_MENU);
    assert_eq!(warnings, Vec::new(), "у места ilya есть отклик");
}

#[test]
fn an_element_on_another_screen_does_not_answer_a_place() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#00000000"}]},
        {"name":"game","world_runs":true,"elements":[]}
    ]}"##;
    let (_, _, warnings, _) = load_screens(NO_OBJECTS, screens);
    assert!(
        warnings.iter().any(|w| w.message.contains("\"a\"")),
        "{warnings:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Команда exit
// -------------------------------------------------------------------------------------------

#[test]
fn the_exit_button_destroys_the_world_and_asks_the_page_to_leave() {
    let mut ui = ui_of(HERO_MENU);
    assert!(!ui.state.exit_requested());
    ui.click(320.0, 320.0);
    assert!(ui.state.exit_requested());
    assert!(!ui.game.has_world());
}

#[test]
fn the_exit_key_of_a_screen_asks_the_page_to_leave_too() {
    let mut ui = ui_of(HERO_MENU);
    ui.key("Escape");
    assert!(ui.state.exit_requested());
}

#[test]
fn exit_in_a_live_game_destroys_the_world() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"button","anchor":"top_left","size":[40,40],"text":"","font":"ui","color":"#808080","on_click":["new_game","game"]}]},
        {"name":"game","world_runs":true,"keys":{"Escape":["exit"]},"elements":[]}
    ]}"##;
    let (game, config, _, _) = load_screens(CAR_SCENE, screens);
    let mut ui = Ui::new(game, config);
    ui.click(20.0, 20.0);
    assert!(ui.game.has_world());
    ui.key("Escape");
    assert!(ui.state.exit_requested());
    assert!(!ui.game.has_world());
}

#[test]
fn exit_takes_no_arguments() {
    let button = r##"{"kind":"button","anchor":"top_left","size":[10,10],"text":"","font":"ui","color":"#808080",
        "on_click":["exit","menu"]}"##;
    let errors = errors_of(try_load(
        VIDEO_IMAGES,
        NO_OBJECTS,
        button,
        &files(Some(same_reverse())),
    ));
    assert_error(
        &errors,
        "on_click",
        "у команды \"exit\" должно быть 0 параметр(ов)",
    );
}

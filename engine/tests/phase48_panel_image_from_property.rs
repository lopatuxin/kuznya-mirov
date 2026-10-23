//! Фаза 48 — картинка панели из свойства объекта: `image: "{свойство}"` у панели строки списка или
//! панели с `for` берёт имя картинки из строкового свойства объекта элемента. Каждый тест собирает свою
//! маленькую игру в коде, как `tests/phase44_world_images.rs`. Имя картинки, которое называет таблица данных,
//! так же, как текст кода, снимает предупреждение «картинка объявлена, но её никто не называет».

use engine::core::game::Game;
use engine::core::interface::{self, Part};
use engine::core::screens::{Element, PanelFill, ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{load_rest, load_rest_with_tables, read_entry};

const VIEWPORT: [f32; 2] = [400.0, 300.0];
const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];
const NO_RULES: &str = r#"{"rules":[]}"#;
const PROPS: &str = r#"{"properties":{"icon":"text","car":"flag","boss":"flag","speed":"number"}}"#;
const GAME_JSON: &str = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{"ui":"ui.ttf"},
"images":{"flatbed":{"path":"icons/flatbed.png"},"van":{"path":"icons/van.png"}}}}"##;

/// Три машины: бортовая, фургон и машина с картинкой, которой нет, — и начальник без значка.
const CARS: &str = r##"{"objects":[
    {"name":"Первая","position":[1,1],"size":[1,1],"car":true,"icon":"flatbed","speed":1},
    {"name":"Вторая","position":[3,1],"size":[1,1],"car":true,"icon":"van","speed":2},
    {"name":"Третья","position":[5,1],"size":[1,1],"car":true,"icon":"ghost","speed":3},
    {"name":"Начальник","position":[7,1],"size":[1,1],"boss":true}
]}"##;

type Loaded = Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure>;

fn load(screens: &str) -> Loaded {
    let (config, _) = read_entry(GAME_JSON).expect("game.json разбирается");
    let fonts = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    load_rest(
        GAME_JSON,
        config,
        Some(PROPS),
        Some(CARS),
        Some(NO_RULES),
        Some(screens),
        &fonts,
        &[],
        &[],
        &[],
        None,
        true,
    )
    .map(|(game, screens, warnings, _images)| (game, screens, warnings))
}

fn screens_with(element: &str) -> String {
    format!(r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[{element}]}}]}}"#)
}

/// Список машин по `speed`: в строке — панель с картинкой из `icon`.
const LIST: &str = r#"{"kind":"list","anchor":"top_left","offset":[0,0],"size":[200,90],
    "for":{"has":["car"]},"sort_by":"speed","row_height":30,"gap":0,
    "row":[{"kind":"panel","offset":[0,0],"size":[40,20],"image":"{icon}"}]}"#;

/// Что нарисуют панели с картинкой из свойства: объект элемента и картинка — или `None`, если её нет.
fn own_pictures(game: &Game, config: &ScreensConfig) -> Vec<(String, Option<usize>)> {
    let state = ScreenState::new(config.start_screen);
    let screen = &config.screens[config.start_screen];
    interface::layout(screen, config.start_screen, &game.world, &state, VIEWPORT)
        .into_iter()
        .filter_map(|part| match part {
            Part::Element(placed) => match placed.element {
                Element::Panel {
                    fill: PanelFill::Own { image, .. },
                    ..
                } => {
                    let id = placed
                        .object
                        .expect("у панели с картинкой из свойства есть объект");
                    let name = game
                        .world
                        .text(id, engine::core::property::NAME)
                        .unwrap_or_default()
                        .to_string();
                    Some((name, image.resolve(&game.world, id, &game.properties)))
                }
                _ => None,
            },
            Part::ScrollBar { .. } => None,
        })
        .collect()
}

#[test]
fn a_panel_in_a_list_row_takes_its_picture_from_the_row_object() {
    let (game, config, _) = load(&screens_with(LIST)).expect("игра должна загрузиться");

    assert_eq!(
        own_pictures(&game, &config),
        vec![
            ("Первая".to_string(), Some(0)),
            ("Вторая".to_string(), Some(1)),
            ("Третья".to_string(), None),
        ]
    );
}

#[test]
fn a_panel_with_for_takes_its_picture_from_its_object() {
    let element = r#"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[40,20],
        "for":{"has":["car"]},"image":"{icon}"}"#;
    let (game, config, _) = load(&screens_with(element)).expect("игра должна загрузиться");

    // Объект элемента с `for` — первый по номеру из отбора.
    assert_eq!(
        own_pictures(&game, &config),
        vec![("Первая".to_string(), Some(0))]
    );
}

#[test]
fn an_object_without_the_property_gets_no_picture() {
    let element = r#"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[40,20],
        "for":{"has":["boss"]},"image":"{icon}"}"#;
    let (game, config, _) = load(&screens_with(element)).expect("игра должна загрузиться");

    assert_eq!(
        own_pictures(&game, &config),
        vec![("Начальник".to_string(), None)]
    );
}

#[test]
fn pictures_named_only_by_property_values_get_no_unused_warning() {
    let (_, _, warnings) = load(&screens_with(LIST)).expect("игра должна загрузиться");

    assert!(
        warnings.iter().all(|w| !w.message.contains("картинка")),
        "{warnings:?}"
    );
}

fn errors_of(element: &str) -> Vec<(String, String, String)> {
    match load(&screens_with(element)) {
        Err(failure) => failure
            .errors
            .into_iter()
            .map(|e| (e.file, e.path, e.message))
            .collect(),
        Ok(_) => Vec::new(),
    }
}

fn assert_one_error_at(errors: &[(String, String, String)], path: &str, needle: &str) {
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (file, actual_path, message) = &errors[0];
    assert_eq!(
        (file.as_str(), actual_path.as_str()),
        ("screens.json", path),
        "{errors:?}"
    );
    assert!(message.contains(needle), "{errors:?}");
}

#[test]
fn a_panel_without_an_object_cannot_take_its_picture_from_a_property() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[40,20],"image":"{icon}"}"#,
    );
    assert_one_error_at(
        &errors,
        "screens[0] → elements → [0] → image",
        "при объекте",
    );
}

#[test]
fn a_substitution_naming_an_undeclared_property_is_an_error() {
    let errors = errors_of(&LIST.replace("{icon}", "{nope}"));
    assert_one_error_at(
        &errors,
        "screens[0] → elements → [0] → row → [0] → image",
        "nope",
    );
}

#[test]
fn a_substitution_naming_a_property_that_is_not_text_is_an_error() {
    let errors = errors_of(&LIST.replace("{icon}", "{speed}"));
    assert_one_error_at(
        &errors,
        "screens[0] → elements → [0] → row → [0] → image",
        "не строковое свойство",
    );
}

#[test]
fn a_substitution_must_fill_the_whole_picture_name() {
    let errors = errors_of(&LIST.replace("{icon}", "car_{icon}"));
    assert_one_error_at(
        &errors,
        "screens[0] → elements → [0] → row → [0] → image",
        "целиком",
    );
}

/// Игра с кодом и таблицей `cars`: код читает таблицу, а в таблице — `table`.
fn warnings_with_table(table: &str) -> Vec<String> {
    let game_json = GAME_JSON.replace(
        r#""screens":"screens.json","#,
        r#""screens":"screens.json","code":"code.lua","tables":{"cars":"tables/cars.json"},"#,
    );
    let (config, _) = read_entry(&game_json).expect("game.json разбирается");
    let fonts = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let screens = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
    let (_, _, warnings, _) = load_rest_with_tables(
        &game_json,
        config,
        Some(PROPS),
        Some(r#"{"objects":[]}"#),
        Some(NO_RULES),
        Some(screens),
        &fonts,
        &[],
        &[],
        &[],
        Some("function cars_of() return tables.cars end"),
        true,
        &[("cars".to_string(), Some(table.to_string()))],
    )
    .expect("игра должна загрузиться");
    warnings
        .into_iter()
        .filter(|w| w.message.contains("картинка"))
        .map(|w| w.path)
        .collect()
}

#[test]
fn a_picture_named_in_a_data_table_is_not_reported_unused() {
    assert_eq!(
        warnings_with_table(r#"[{"icon":"van"}]"#),
        vec!["files → images → flatbed".to_string()]
    );
}

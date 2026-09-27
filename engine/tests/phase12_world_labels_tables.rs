//! Фаза 12 — надписи и полоски в мире, таблицы данных, свойство вида `text` (требования 1–41).
//! Игр в `games/` не заводит — каждый тест собирает свою маленькую игру в коде, как
//! `tests/phase11_camera_click_walk.rs`.

use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::screens::{Align, ScreenState};
use engine::core::world_elements;
use engine::data::error::LoadFailure;
use engine::data::load::{
    load_game_from_texts, load_game_from_texts_with_code, load_game_from_texts_with_tables,
    load_rest_with_tables, read_entry,
};
use engine::data::session::PlaySession;

const VIEWPORT: [f32; 2] = [200.0, 200.0];

const GAME: &str = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{}}}"##;
/// Same as `GAME`, plus a `"ui"` font declared — for the handful of tests whose `screens.json`
/// actually names a font (a world label, or a screen element).
const GAME_WITH_FONT: &str = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_EMPTY: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
/// A four-byte TrueType signature — enough to pass `looks_like_font`'s sniff without being a
/// real, fully parseable font; see `tests/screens_validation.rs`'s own `FONT_BYTES`.
const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];

/// Same as `load_game_from_texts`, but supplies `GAME_WITH_FONT`'s own `"ui"` font bytes — for
/// tests whose `screens.json` names a font, which the plain `load_game_from_texts` never supplies.
fn load_with_font(
    props: &str,
    scene: &str,
    rules: &str,
    screens: &str,
) -> Result<
    (
        engine::core::game::Game,
        engine::core::screens::ScreensConfig,
        Vec<engine::data::error::GameError>,
    ),
    LoadFailure,
> {
    let (config, _entry_warnings) =
        read_entry(GAME_WITH_FONT).expect("game.json должен разбираться");
    let font_bytes = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    load_rest_with_tables(
        GAME_WITH_FONT,
        config,
        Some(props),
        Some(scene),
        Some(rules),
        Some(screens),
        &font_bytes,
        &[],
        &[],
        &[],
        None,
        false,
        &[],
    )
    .map(|(game, screens, warnings, _images)| (game, screens, warnings))
}

fn game_json_with_code() -> String {
    r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","code":"code.lua","fonts":{}}}"##
        .to_string()
}

fn game_json_with_tables(tables: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","code":"code.lua","fonts":{{}},"tables":{tables}}}}}"##
    )
}

fn err_messages(result: &Result<impl std::fmt::Debug, LoadFailure>) -> Vec<String> {
    match result {
        Err(f) => f.errors.iter().map(|e| e.message.clone()).collect(),
        Ok(_) => Vec::new(),
    }
}

/// Each error as `(file, path, message)` — for the tests that pin where an error points, not only
/// what it says.
fn located_errors(
    result: &Result<impl std::fmt::Debug, LoadFailure>,
) -> Vec<(String, String, String)> {
    match result {
        Err(f) => f
            .errors
            .iter()
            .map(|e| (e.file.clone(), e.path.clone(), e.message.clone()))
            .collect(),
        Ok(_) => Vec::new(),
    }
}

fn assert_one_error_at(errors: &[(String, String, String)], file: &str, path: &str, needle: &str) {
    assert_eq!(errors.len(), 1, "{errors:?}");
    let (actual_file, actual_path, message) = &errors[0];
    assert_eq!(
        (actual_file.as_str(), actual_path.as_str()),
        (file, path),
        "{errors:?}"
    );
    assert!(message.contains(needle), "{errors:?}");
}

// -------------------------------------------------------------------------------------------
// Расчёт элементов в мире — сквозь весь конвейер (разбор → разрешение → расчёт)
// -------------------------------------------------------------------------------------------

const PROPS_HP: &str = r#"{"properties":{"hp":"number","max_hp":"number","level":"number"}}"#;

#[test]
fn bar_and_label_parse_and_compute_end_to_end() {
    let scene = r#"{"objects":[{"position":[5,5],"size":[2,2],"hp":30,"max_hp":40,"level":3}]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"bar","for":{"has":["hp"]},"anchor":"top","offset":[0,-0.3],"size":[1.2,0.15],
   "value":"hp","max":"max_hp","color":"#d03030","back_color":"#00000080"},
  {"kind":"label","for":{"has":["level"]},"anchor":"top","offset":[0,-0.6],"size":[2,0.4],
   "text":"Ур. {level}","font":"ui","align":"center","color":"#ffffff"}
]}"##;
    let (game, screens_config, warnings) =
        load_with_font(PROPS_HP, scene, RULES_EMPTY, screens).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert_eq!(screens_config.world_elements.len(), 2);

    let (bars, labels) = world_elements::compute_world_draws(
        &game.world,
        &game.scene,
        &game.properties,
        &screens_config.world_elements,
    );
    assert_eq!(bars.len(), 1);
    assert_eq!(labels.len(), 1);
    // hp/max_hp = 30/40 = 0.75 of the 1.2-cell width.
    assert!((bars[0].fill.size[0] - 0.9).abs() < 1e-4, "{bars:?}");
    assert_eq!(labels[0].text, "Ур. 3");
    // Default font_size — the element's own height (0.4).
    assert_eq!(labels[0].font_size, 0.4);
    assert_eq!(labels[0].align, Align::Center);
}

#[test]
fn world_elements_with_a_color_table_resolve_the_by_property() {
    let props = r#"{"properties":{"danger":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"danger":2}]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"label","for":{"has":["danger"]},"anchor":"center","size":[1,1],"text":"",
   "font":"ui","color":{"table":["#9d9d9d","#1eff00","#ff2020"],"by":"danger"}}
]}"##;
    let (game, screens_config, warnings) =
        load_with_font(props, scene, RULES_EMPTY, screens).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let (_, labels) = world_elements::compute_world_draws(
        &game.world,
        &game.scene,
        &game.properties,
        &screens_config.world_elements,
    );
    assert_eq!(labels[0].color, [1.0, 0.1254902, 0.1254902, 1.0]);
}

/// «Мышь в мире», требование 15: щелчок в мире и `object_at` не видят надписи и полоски — даже
/// там, где полоска, привязанная к одному объекту, нависает над другим объектом или пустым местом,
/// щелчок должен попадать в то, что действительно там нарисовано, а не в объект, которому
/// принадлежит полоска.
#[test]
fn world_elements_are_transparent_to_object_at_and_on_click() {
    let props = r#"{"properties":{"hp":"number"}}"#;
    // Object 0's bar is centered on it but 8 cells wide — far past its own [0,1] rectangle, over
    // object 1's [3,4] one, and over empty scene at cell 2.
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"color":"#ff0000","hp":5,"on_click":[["hp",9]]},
        {"position":[3,0],"size":[1,1],"color":"#0000ff","on_click":[["hp",9]]}
    ]}"##;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[8,1],"value":"hp","max":10.0,
   "color":"#00ff00"}
]}"##;
    let (game, _screens, warnings) =
        load_game_from_texts(GAME, props, scene, RULES_EMPTY, screens).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    // Scene is 20x20, viewport 200x200: scale 10. Cell (3.5, 0.5) — inside the bar, outside object
    // 0's own rectangle, inside object 1's — must hit object 1, not the bar's own object 0.
    let over_object_1 =
        engine::core::scene::object_at(&game.world, &game.scene, [35.0, 5.0], [200.0, 200.0]);
    assert_eq!(
        over_object_1,
        Some(1),
        "точка внутри полоски, над другим объектом"
    );
    let on_click_over_1 =
        engine::core::scene::on_click_target(&game.world, &game.scene, [3.5, 0.5]);
    assert_eq!(on_click_over_1, Some(1));
    // Cell (2, 0.5) — still inside the bar, but over neither object — must hit nothing.
    let over_empty =
        engine::core::scene::object_at(&game.world, &game.scene, [20.0, 5.0], [200.0, 200.0]);
    assert_eq!(over_empty, None, "точка внутри полоски, над пустым местом");
    let on_click_over_empty =
        engine::core::scene::on_click_target(&game.world, &game.scene, [2.0, 0.5]);
    assert_eq!(on_click_over_empty, None);
}

// -------------------------------------------------------------------------------------------
// Проверка перед запуском — world_elements (требование 33–34)
// -------------------------------------------------------------------------------------------

fn load_screens_located_errors(screens: &str) -> Vec<(String, String, String)> {
    let props = r#"{"properties":{"hp":"number","name_prop":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"hp":5}]}"#;
    let result = load_game_from_texts(GAME, props, scene, RULES_EMPTY, screens);
    located_errors(&result)
}

#[test]
fn unknown_root_key_in_screens_json_is_rejected() {
    let screens = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}],"bogus":[]}"#;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(&errors, "screens.json", "bogus", "bogus");
}

#[test]
fn unknown_world_element_kind_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"panel","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(&errors, "screens.json", "world_elements[0] → kind", "panel");
}

#[test]
fn unknown_anchor_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"middle","size":[1,1],
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → anchor",
        "middle",
    );
}

#[test]
fn non_positive_size_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[0,1],
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → size",
        "больше нуля",
    );
}

#[test]
fn font_not_declared_in_files_fonts_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"text":"x","font":"missing","color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → font",
        "missing",
    );
}

#[test]
fn substitution_with_a_dot_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"text":"{hero.hp}","font":"ui","color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(&errors, "screens.json", "world_elements[0] → text", "точки");
}

#[test]
fn substitution_naming_an_unknown_property_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"text":"{ghost}","font":"ui","color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(&errors, "screens.json", "world_elements[0] → text", "ghost");
}

#[test]
fn unclosed_substitution_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"text":"{hp","font":"ui","color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → text",
        "не закрыта",
    );
}

#[test]
fn value_naming_a_non_number_property_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"name_prop","max":10,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → value",
        "number",
    );
}

#[test]
fn a_non_positive_const_max_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":0,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → max",
        "больше нуля",
    );
}

#[test]
fn empty_color_table_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":{"table":[],"by":"hp"}}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → color → table",
        "пуст",
    );
}

#[test]
fn unknown_key_in_a_color_table_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":{"table":["#ffffff"],"by":"hp","extra":1}}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → color → extra",
        "extra",
    );
}

#[test]
fn for_not_an_object_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":["hp"],"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → for",
        "ожидался объект",
    );
}

#[test]
fn unknown_align_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"text":"x","font":"ui","align":"diagonal","color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → align",
        "diagonal",
    );
}

#[test]
fn offset_not_a_pair_of_numbers_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","offset":[0],"size":[1,1],
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → offset",
        "пара",
    );
}

#[test]
fn font_size_not_positive_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"text":"x","font":"ui","font_size":0,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → font_size",
        "больше нуля",
    );
}

#[test]
fn invalid_back_color_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":"#ffffff","back_color":"not-a-color"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → back_color",
        "цвет",
    );
}

#[test]
fn by_naming_a_non_number_property_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":{"table":["#ffffff"],"by":"name_prop"}}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → color → by",
        "number",
    );
}

#[test]
fn by_naming_an_unknown_property_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":{"table":["#ffffff"],"by":"ghost"}}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(
        &errors,
        "screens.json",
        "world_elements[0] → color → by",
        "ghost",
    );
}

#[test]
fn bar_without_size_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center",
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(&errors, "screens.json", "world_elements[0]", "\"size\"");
}

#[test]
fn label_without_text_is_rejected() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"label","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"font":"ui","color":"#ffffff"}]}"##;
    let errors = load_screens_located_errors(screens);
    assert_one_error_at(&errors, "screens.json", "world_elements[0]", "\"text\"");
}

#[test]
fn world_elements_selector_thats_never_satisfiable_gets_a_warning() {
    let props = r#"{"properties":{"hp":"number","other":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"hp":5}]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["other"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let (_g, _s, warnings) = load_game_from_texts(GAME, props, scene, RULES_EMPTY, screens)
        .expect("предупреждение не мешает запуску");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("заведомо не подходит")),
        "{warnings:?}"
    );
}

/// «Формат игры», требование 35: свойство, видное только в подстановке экрана или только в
/// элементе мира, не получает ложного предупреждения «объявлено и не используется».
#[test]
fn a_property_used_only_by_a_screen_or_world_element_is_not_flagged_unused() {
    let props = r#"{"properties":{"score":"number","hp":"number"}}"#;
    let scene = r#"{"objects":[{"name":"head","position":[0,0],"size":[1,1],"score":1,"hp":5}]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"label","anchor":"top_left","offset":[0,0],"size":[50,20],"text":"{head.score}",
         "font":"ui","color":"#ffffff"}
    ]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":"#ffffff"}]}"##;
    let (_g, _s, warnings) =
        load_with_font(props, scene, RULES_EMPTY, screens).expect("должно загрузиться");
    assert!(
        warnings
            .iter()
            .all(|w| !w.message.contains("не используется")),
        "{warnings:?}"
    );
}

// -------------------------------------------------------------------------------------------
// Свойство вида text
// -------------------------------------------------------------------------------------------

#[test]
fn text_property_declares_and_holds_a_string_value_including_empty() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin"},
        {"position":[0,0],"size":[1,1],"enemy":""}]}"#;
    let (game, _s, warnings) = load_game_from_texts(GAME, props, scene, RULES_EMPTY, SCREENS_EMPTY)
        .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let enemy = game.properties.resolve("enemy").unwrap();
    assert_eq!(game.world.text(0, enemy), Some("goblin"));
    assert_eq!(game.world.text(1, enemy), Some(""));
}

#[test]
fn unknown_property_kind_message_lists_all_five() {
    let props = r#"{"properties":{"enemy":"stringly"}}"#;
    let scene = r#"{"objects":[]}"#;
    let result = load_game_from_texts(GAME, props, scene, RULES_EMPTY, SCREENS_EMPTY);
    let errors = err_messages(&result);
    assert!(
        errors
            .iter()
            .any(|m| m.contains("flag") && m.contains("number") && m.contains("text")),
        "{errors:?}"
    );
}

#[test]
fn condition_on_a_text_property_is_a_prestart_error() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin"}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["enemy"]},
        "when":["enemy","==",0],"do":[["give","enemy",{"has":["enemy"]}]]}]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS_EMPTY);
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("text")), "{errors:?}");
}

#[test]
fn condition_on_name_is_a_prestart_error() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"name":"hero"}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["position"]},
        "when":["name","==",0],"do":[["add","layer",1]]}]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS_EMPTY);
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("name")), "{errors:?}");
}

#[test]
fn add_on_a_text_property_is_a_prestart_error() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin"}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["enemy"]},
        "do":[["add","enemy",1]]}]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS_EMPTY);
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("enemy")), "{errors:?}");
}

/// «Правила игры» → `set`: пишет строку в свойство вида `text`, как любое другое значение.
#[test]
fn rule_set_action_writes_a_text_property() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin"}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["enemy"]},
        "do":[["set","enemy","wolf"]]}]}"#;
    let (mut game, _s, warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS_EMPTY).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let enemy = game.properties.resolve("enemy").unwrap();
    assert_eq!(game.world.text(0, enemy), Some("wolf"));
}

/// «Мир на экране» / «Формат игры»: привязка клавиши пишет строку в свойство вида `text` так же,
/// как любое другое значение записи.
#[test]
fn key_binding_writes_a_text_property() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin",
        "keys":{"KeyA":{"press":[["enemy","wolf"]]}}}]}"#;
    let (mut game, _s, warnings) =
        load_game_from_texts(GAME, props, scene, RULES_EMPTY, SCREENS_EMPTY)
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.press_key("KeyA");
    let enemy = game.properties.resolve("enemy").unwrap();
    assert_eq!(game.world.text(0, enemy), Some("wolf"));
}

/// «Экраны и состояние» → «Начальные значения у new_game»: третий элемент команды пишет строку в
/// свойство вида `text`, как любое другое значение `scene.json`.
#[test]
fn new_game_initial_value_sets_a_text_property() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"name":"head","position":[0,0],"size":[1,1],"enemy":"goblin"}]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"button","anchor":"top_left","offset":[0,0],"size":[10,10],"text":"",
         "font":"ui","color":"#ffffff",
         "on_click":["new_game","main",{"head.enemy":"wolf"}]}
    ]}]}"##;
    let (mut game, screens_config, warnings) =
        load_with_font(props, scene, RULES_EMPTY, screens).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let mut state = engine::core::screens::ScreenState::new(screens_config.start_screen);
    let engine::core::screens::Element::Button { on_click, .. } =
        &screens_config.screens[0].elements[0]
    else {
        panic!("ожидалась кнопка");
    };
    engine::core::screens::apply_command(*on_click, &mut game, &screens_config, &mut state);
    let enemy = game.properties.resolve("enemy").unwrap();
    assert_eq!(game.world.text(0, enemy), Some("wolf"));
}

/// «Редактор», требование 43: значение свойства вида `text` на ходу — та же проверка, что
/// значения `scene.json`.
#[test]
fn set_property_accepts_a_string_for_a_text_property() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin"}]}"#;
    let (mut game, _s, _w) = load_game_from_texts(GAME, props, scene, RULES_EMPTY, SCREENS_EMPTY)
        .expect("должно загрузиться");
    let images = Vec::new();
    engine::data::edit::set_property(
        &mut game.world,
        &game.properties,
        &images,
        0,
        "enemy",
        &serde_json::json!("wolf"),
    )
    .expect("строка допустима");
    let enemy = game.properties.resolve("enemy").unwrap();
    assert_eq!(game.world.text(0, enemy), Some("wolf"));
}

#[test]
fn code_reads_and_writes_a_text_property_and_nil_removes_it() {
    let props = r#"{"properties":{"enemy":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"enemy":"goblin"}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["enemy"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    assert(obj.enemy == "goblin")
    obj.enemy = "wolf"
end
"#;
    let (mut game, _s, warnings) = load_game_from_texts_with_code(
        &game_json_with_code(),
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let enemy = game.properties.resolve("enemy").unwrap();
    assert_eq!(game.world.text(0, enemy), Some("wolf"));
}

// -------------------------------------------------------------------------------------------
// Цвет без падения на не-латинских знаках
// -------------------------------------------------------------------------------------------

#[test]
fn non_latin_color_in_scene_is_a_clean_error_not_a_panic() {
    let props = r#"{"properties":{}}"#;
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#aжжжb"}]}"##;
    let result = load_game_from_texts(GAME, props, scene, RULES_EMPTY, SCREENS_EMPTY);
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("цвет")), "{errors:?}");
}

#[test]
fn non_latin_color_on_a_screen_element_is_a_clean_error_not_a_panic() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
        {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[10,10],"color":"#aжжжb"}
    ]}]}"##;
    let result = load_game_from_texts(GAME, props, scene, RULES_EMPTY, screens);
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("цвет")), "{errors:?}");
}

#[test]
fn non_latin_color_on_a_world_element_is_a_clean_error_not_a_panic() {
    let props = r#"{"properties":{"hp":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"hp":1}]}"#;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[{"kind":"bar","for":{"has":["hp"]},"anchor":"center","size":[1,1],
"value":"hp","max":10,"color":"#aжжжb"}]}"##;
    let result = load_game_from_texts(GAME, props, scene, RULES_EMPTY, screens);
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("цвет")), "{errors:?}");
}

#[test]
fn non_latin_color_written_from_code_is_a_clean_runtime_error_not_a_panic() {
    let props = r#"{"properties":{}}"#;
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#112233"}]}"##;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["run","touch"]]}]}"#;
    let code = r##"
function touch(obj)
    obj.color = "#aжжжb"
end
"##;
    let (mut game, _s, warnings) = load_game_from_texts_with_code(
        &game_json_with_code(),
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game.code_error().expect("плохой цвет — ошибка кода");
    assert!(err.message.contains("цвет"), "{err:?}");
}

// -------------------------------------------------------------------------------------------
// Таблицы данных
// -------------------------------------------------------------------------------------------

fn tables(pairs: &[(&str, &str)]) -> Vec<(String, Option<String>)> {
    pairs
        .iter()
        .map(|(name, text)| (name.to_string(), Some(text.to_string())))
        .collect()
}

const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const SCENE_ONE_OBJECT: &str = r#"{"objects":[{"position":[0,0],"size":[1,1]}]}"#;

#[test]
fn empty_table_name_is_rejected() {
    let game_json = game_json_with_tables(r#"{"":"e.json"}"#);
    let result = load_game_from_texts_with_tables(
        &game_json,
        PROPS_EMPTY,
        SCENE_ONE_OBJECT,
        RULES_EMPTY,
        SCREENS_EMPTY,
        None,
        &[],
    );
    let errors = located_errors(&result);
    assert_one_error_at(&errors, "game.json", "files → tables", "непустая");
}

#[test]
fn read_entry_names_declared_tables_by_name_and_path() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let (config, warnings) = read_entry(&game_json).expect("должно разбираться");
    assert_eq!(warnings, Vec::new());
    assert_eq!(
        config.files.tables,
        vec![("enemies".to_string(), "enemies.json".to_string())]
    );
}

#[test]
fn missing_table_file_is_a_prestart_error() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let result = load_game_from_texts_with_tables(
        &game_json,
        PROPS_EMPTY,
        SCENE_ONE_OBJECT,
        RULES_EMPTY,
        SCREENS_EMPTY,
        None,
        &[],
    );
    let errors = err_messages(&result);
    assert!(
        errors
            .iter()
            .any(|m| m.contains("не найден") && m.contains("enemies")),
        "{errors:?}"
    );
}

#[test]
fn a_broken_table_read_by_code_at_load_gives_only_its_own_error() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let code = "local goblin_health = tables.enemies.goblin.health\nfunction touch(obj) end\n";
    // The skipped code check must not turn the rule's `run` into a false «no such function».
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["position"]},"do":[["run","touch"]]}]}"#;
    for (table_texts, path, needle) in [
        (tables(&[("enemies", "{not json")]), "", ""),
        (
            tables(&[("enemies", r#"{"goblin":null}"#)]),
            "goblin",
            "null",
        ),
        (Vec::new(), "", "не найден"),
    ] {
        let result = load_game_from_texts_with_tables(
            &game_json,
            PROPS_EMPTY,
            SCENE_ONE_OBJECT,
            rules,
            SCREENS_EMPTY,
            Some(code),
            &table_texts,
        );
        let errors = located_errors(&result);
        assert_one_error_at(&errors, "enemies.json", path, needle);
    }
}

#[test]
fn table_file_that_does_not_parse_as_json_is_a_prestart_error() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let result = load_game_from_texts_with_tables(
        &game_json,
        PROPS_EMPTY,
        SCENE_ONE_OBJECT,
        RULES_EMPTY,
        SCREENS_EMPTY,
        None,
        &tables(&[("enemies", "{not json")]),
    );
    let errors = err_messages(&result);
    assert!(
        errors
            .iter()
            .any(|m| m.contains("enemies.json") || m.contains("JSON")),
        "{errors:?}"
    );
}

#[test]
fn null_anywhere_in_a_table_is_a_prestart_error_naming_the_place() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let result = load_game_from_texts_with_tables(
        &game_json,
        PROPS_EMPTY,
        SCENE_ONE_OBJECT,
        RULES_EMPTY,
        SCREENS_EMPTY,
        None,
        &tables(&[("enemies", r#"{"goblin":{"health":30,"loot":[1,null,3]}}"#)]),
    );
    let errors = err_messages(&result);
    assert!(errors.iter().any(|m| m.contains("null")), "{errors:?}");
}

#[test]
fn a_game_with_tables_and_no_code_loads_with_a_warning_per_table() {
    let game_json = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"tables":{"enemies":"enemies.json"}}}"##;
    let (_g, _s, warnings) = load_game_from_texts_with_tables(
        game_json,
        PROPS_EMPTY,
        SCENE_ONE_OBJECT,
        RULES_EMPTY,
        SCREENS_EMPTY,
        None,
        &tables(&[("enemies", r#"{"goblin":{"health":30}}"#)]),
    )
    .expect("игра без кода всё равно загружается");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("enemies") && w.message.contains("не встречается")),
        "{warnings:?}"
    );
}

#[test]
fn table_named_in_code_gets_no_unused_warning() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let code = "function noop() local x = tables.enemies end";
    let (_g, _s, warnings) = load_game_from_texts_with_tables(
        &game_json,
        PROPS_EMPTY,
        SCENE_ONE_OBJECT,
        RULES_EMPTY,
        SCREENS_EMPTY,
        Some(code),
        &tables(&[("enemies", r#"{"goblin":{"health":30}}"#)]),
    )
    .expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("enemies")),
        "{warnings:?}"
    );
}

/// «Код игры»: объект JSON — таблица Lua, массив — список с первого номера, числа, строки,
/// логические — «Таблицы данных», требование 27.
#[test]
fn code_reads_objects_arrays_numbers_strings_and_booleans_from_tables() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let props = r#"{"properties":{"health":"number","name_out":"text","first_loot":"number","flag_out":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"health":0,"name_out":"","first_loot":0,"flag_out":false}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["health"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    local goblin = tables.enemies.goblin
    obj.health = goblin.health
    obj.name_out = goblin.name
    obj.first_loot = goblin.loot[1]
    obj.flag_out = goblin.boss
end
"#;
    let (mut game, _s, warnings) = load_game_from_texts_with_tables(
        &game_json,
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
        &tables(&[(
            "enemies",
            r#"{"goblin":{"health":30,"name":"Goblin","loot":[7,8],"boss":false}}"#,
        )]),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let health = game.properties.resolve("health").unwrap();
    let name_out = game.properties.resolve("name_out").unwrap();
    let first_loot = game.properties.resolve("first_loot").unwrap();
    let flag_out = game.properties.resolve("flag_out").unwrap();
    assert_eq!(game.world.number_like(0, health), Some(30.0));
    assert_eq!(game.world.text(0, name_out), Some("Goblin"));
    assert_eq!(game.world.number_like(0, first_loot), Some(7.0));
    assert!(!game.world.flag(0, flag_out));
}

/// «Память кода»: правка таблицы кодом живёт до конца партии, следующая партия получает
/// исходные таблицы заново.
#[test]
fn writing_to_tables_lives_until_new_game_resets_it() {
    let game_json = game_json_with_tables(r#"{"counts":"counts.json"}"#);
    let props = r#"{"properties":{"seen":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"seen":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["seen"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    tables.counts.goblin = tables.counts.goblin + 1
    obj.seen = tables.counts.goblin
end
"#;
    let (mut game, _s, warnings) = load_game_from_texts_with_tables(
        &game_json,
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
        &tables(&[("counts", r#"{"goblin":0}"#)]),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let seen = game.properties.resolve("seen").unwrap();

    game.step(StepInput::empty());
    assert_eq!(game.world.number_like(0, seen), Some(1.0));
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(0, seen),
        Some(2.0),
        "живёт до конца партии"
    );

    game.new_game();
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(0, seen),
        Some(1.0),
        "новая партия получает исходные таблицы заново"
    );
}

/// «Таблицы данных», требование 28: `tables` доступна уже на верхнем уровне файла кода, до первого
/// шага, — не только внутри функций: верхний уровень кладёт значение из таблицы в локальную
/// переменную, а функция из `run` замыкает её и пишет в свойство.
#[test]
fn tables_are_available_at_the_top_level_of_the_code_file_before_the_first_step() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let props = r#"{"properties":{"health":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"health":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["health"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
local goblin_health = tables.enemies.goblin.health
function touch(obj)
    obj.health = goblin_health
end
"#;
    let (mut game, _s, warnings) = load_game_from_texts_with_tables(
        &game_json,
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
        &tables(&[("enemies", r#"{"goblin":{"health":30}}"#)]),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let health = game.properties.resolve("health").unwrap();
    assert_eq!(game.world.number_like(0, health), Some(30.0));
}

/// «Таблицы данных», требование 30: ключи таблиц заполняются в одном и том же порядке при
/// каждой загрузке — обход `pairs` одинаков в двух независимых загрузках тех же файлов.
#[test]
fn pairs_iteration_order_is_the_same_across_two_loads() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let props = r#"{"properties":{"log":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"log":""}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["log"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    local s = ""
    for k, _ in pairs(tables.enemies) do
        s = s .. k .. ","
    end
    obj.log = s
end
"#;
    let table_text = r#"{"z_goblin":1,"a_wolf":2,"m_bat":3}"#;

    let mut logs = Vec::new();
    for _ in 0..2 {
        let (mut game, _s, warnings) = load_game_from_texts_with_tables(
            &game_json,
            props,
            scene,
            rules,
            SCREENS_EMPTY,
            Some(code),
            &tables(&[("enemies", table_text)]),
        )
        .expect("должно загрузиться");
        assert_eq!(warnings, Vec::new(), "{warnings:?}");
        game.step(StepInput::empty());
        assert!(game.code_error().is_none(), "{:?}", game.code_error());
        let log = game.properties.resolve("log").unwrap();
        logs.push(game.world.text(0, log).unwrap().to_string());
    }
    assert_eq!(logs[0], logs[1], "порядок обхода должен совпасть");
}

/// «Таблицы данных», требование 30: обход `pairs` совпадает не только у двух независимых загрузок
/// одних файлов, но и у двух партий подряд в одной загруженной игре (`new_game`), и у повтора
/// записанной партии — не только у первого шага, каждая партия перечитывает код заново.
#[test]
fn pairs_iteration_order_is_the_same_in_two_partiya_in_a_row_and_in_a_replay() {
    let game_json = game_json_with_tables(r#"{"enemies":"enemies.json"}"#);
    let props = r#"{"properties":{"log":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"log":""}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["log"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    local s = ""
    for k, _ in pairs(tables.enemies) do
        s = s .. k .. ","
    end
    obj.log = s
end
"#;
    let table_text = r#"{"z_goblin":1,"a_wolf":2,"m_bat":3}"#;
    let load_it = || {
        load_game_from_texts_with_tables(
            &game_json,
            props,
            scene,
            rules,
            SCREENS_EMPTY,
            Some(code),
            &tables(&[("enemies", table_text)]),
        )
        .expect("должно загрузиться")
    };

    let (mut game, _s, warnings) = load_it();
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let log = game.properties.resolve("log").unwrap();

    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let first = game.world.text(0, log).unwrap().to_string();
    assert!(!first.is_empty(), "код должен был обойти таблицу");

    game.new_game();
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let second = game.world.text(0, log).unwrap().to_string();
    assert_eq!(
        first, second,
        "две партии подряд должны обходить таблицу одинаково"
    );

    // The same first partiya, recorded live and then driven again through a replay.
    let (mut recorded_game, recorded_screens, warnings) = load_it();
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let mut state = ScreenState::new(recorded_screens.start_screen);
    let mut session = PlaySession::begin_live(&mut recorded_game, &recorded_screens, &mut state);
    session.step_once(
        &mut UiQueue::new(),
        &mut MouseState::default(),
        &mut recorded_game,
        &recorded_screens,
        &mut state,
        VIEWPORT,
        &[],
    );
    session.end(&mut recorded_game);
    let text = session.recording_text(&recorded_game);

    let (mut replay_game, replay_screens, warnings) = load_it();
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let mut replay_state = ScreenState::new(replay_screens.start_screen);
    let mut replay =
        PlaySession::begin_replay(&text, &mut replay_game, &replay_screens, &mut replay_state)
            .expect("запись должна разбираться");
    replay.step_once(
        &mut UiQueue::new(),
        &mut MouseState::default(),
        &mut replay_game,
        &replay_screens,
        &mut replay_state,
        VIEWPORT,
        &[],
    );
    assert!(
        replay_game.code_error().is_none(),
        "{:?}",
        replay_game.code_error()
    );
    let replay_log = replay_game.properties.resolve("log").unwrap();
    let third = replay_game.world.text(0, replay_log).unwrap().to_string();
    assert_eq!(
        first, third,
        "повтор записанной партии должен обходить таблицу так же"
    );
}

#[test]
fn scalar_rooted_table_files_become_a_bare_lua_value() {
    let game_json =
        game_json_with_tables(r#"{"version":"version.json","greeting":"greeting.json"}"#);
    let props = r#"{"properties":{"v":"number","g":"text"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"v":0,"g":""}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["v"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    obj.v = tables.version
    obj.g = tables.greeting
end
"#;
    let (mut game, _s, warnings) = load_game_from_texts_with_tables(
        &game_json,
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
        &tables(&[("version", "3"), ("greeting", "\"hi\"")]),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let v = game.properties.resolve("v").unwrap();
    let g = game.properties.resolve("g").unwrap();
    assert_eq!(game.world.number_like(0, v), Some(3.0));
    assert_eq!(game.world.text(0, g), Some("hi"));
}

#[test]
fn two_tables_on_one_file_each_get_their_own_copy() {
    let game_json = game_json_with_tables(r#"{"a":"shared.json","b":"shared.json"}"#);
    let props = r#"{"properties":{"out":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"out":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["out"]},"do":[["run","touch"]]}]}"#;
    let code = r#"
function touch(obj)
    tables.a.n = tables.a.n + 1
    obj.out = tables.b.n
end
"#;
    // The page fetches `shared.json` once but hands its text back under each declared name that
    // points to it — same as two font entries could name the same file.
    let (mut game, _s, warnings) = load_game_from_texts_with_tables(
        &game_json,
        props,
        scene,
        rules,
        SCREENS_EMPTY,
        Some(code),
        &tables(&[("a", r#"{"n":1}"#), ("b", r#"{"n":1}"#)]),
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let out = game.properties.resolve("out").unwrap();
    // `a` and `b` are independent copies of the same file's content — mutating `a` never touches
    // `b`.
    assert_eq!(game.world.number_like(0, out), Some(1.0));
}

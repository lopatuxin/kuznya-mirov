//! Prestart validation for `screens.json` and the screen-related fields of `game.json` — see
//! «Экраны и состояние» и «Интерфейс игры» → «Проверка данных перед запуском». Companion to
//! `prestart_validation.rs`, which covers `scene.json`/`rules.json`/`properties.json`.

use engine::data::error::LoadFailure;
use engine::data::load::{load_rest, read_entry};

const GAME: &str = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{"ui":"ui.ttf"}}}"##;
const PROPS: &str = r##"{"properties":{"score":"number"}}"##;
const SCENE: &str = r##"{"objects":[{"name":"head","position":[0,0],"size":[1,1],"score":0}]}"##;
const RULES_EMPTY: &str = r##"{"rules":[]}"##;
/// A four-byte TrueType signature — enough to pass `looks_like_font`'s sniff without being a
/// real, fully parseable font; see `data::load::looks_like_font`.
const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];

fn load(
    game_json: &str,
    props: &str,
    scene: &str,
    rules: &str,
    screens: &str,
    fonts: &[(&str, &[u8])],
) -> Result<
    (
        engine::core::game::Game,
        engine::core::screens::ScreensConfig,
        Vec<engine::data::error::GameError>,
    ),
    LoadFailure,
> {
    let (config, _entry_warnings) = read_entry(game_json).expect("game.json должен разбираться");
    let font_bytes: Vec<(String, Option<Vec<u8>>)> = fonts
        .iter()
        .map(|(name, bytes)| (name.to_string(), Some(bytes.to_vec())))
        .collect();
    load_rest(
        game_json,
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
    )
}

#[test]
fn missing_screens_file_is_reported() {
    let (config, _warnings) = read_entry(GAME).expect("game.json валиден");
    let result = load_rest(
        GAME,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(RULES_EMPTY),
        None,
        &[],
        &[],
        &[],
        &[],
        None,
    );
    let LoadFailure { errors, .. } = result.expect_err("отсутствующий screens.json — ошибка");
    assert!(
        errors.iter().any(|e| e.file == "screens.json"
            && e.message.contains("не найден")
            && e.message.contains("ожидался")),
        "{errors:?}"
    );
}

#[test]
fn missing_start_screen_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{}}}"##;
    let LoadFailure { errors, .. } = read_entry(game_json).expect_err("нет start_screen — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("start_screen")),
        "{errors:?}"
    );
}

#[test]
fn start_screen_naming_unknown_screen_is_reported() {
    // `GAME`'s start_screen is "menu", but this screens.json only has "other".
    let screens = r##"{"screens":[{"name":"other","world_runs":true,"elements":[]}]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("start_screen называет экран, которого нет в screens.json — ошибка");
    // «Экраны и состояние»: ссылка идёт из `game.json`, значит и файл, и текст сообщения должны
    // называть его, а не `screens.json` и не общую формулировку про кнопку.
    assert!(
        errors.iter().any(|e| e.path == "start_screen"
            && e.file == "game.json"
            && e.message.contains("menu")
            && e.message.contains("start_screen")),
        "{errors:?}"
    );
}

#[test]
fn win_screen_required_when_a_rule_uses_end_game_win() {
    let rules = r##"{"rules":[
        {"kind":"delete","for":{"has":["score"]},"when":["score",">",0],"do":[["end_game","win"]]}
    ]}"##;
    let screens = r##"{"screens":[{"name":"menu","world_runs":true,"elements":[]}]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, rules, screens, &[])
        .expect_err("end_game win без win_screen — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("win_screen")),
        "{errors:?}"
    );
}

#[test]
fn loss_screen_required_when_a_rule_uses_end_game_loss() {
    let rules = r##"{"rules":[
        {"kind":"delete","for":{"has":["score"]},"when":["score",">",0],"do":[["end_game","loss"]]}
    ]}"##;
    let screens = r##"{"screens":[{"name":"menu","world_runs":true,"elements":[]}]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, rules, screens, &[])
        .expect_err("end_game loss без loss_screen — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("loss_screen")),
        "{errors:?}"
    );
}

#[test]
fn win_screen_not_required_without_a_matching_end_game() {
    let screens = r##"{"screens":[{"name":"menu","world_runs":true,"elements":[]}]}"##;
    load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect("без end_game win/loss ни одно из полей не обязательно");
}

#[test]
fn button_target_naming_unknown_screen_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"button","anchor":"center","size":[10,10],"text":"x","font":"ui",
             "color":"#ffffff","on_click":["show_screen","nowhere"]}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("show_screen на несуществующий экран — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("nowhere") && e.message.contains("кнопка")),
        "{errors:?}"
    );
}

#[test]
fn duplicate_screen_names_are_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[]},
        {"name":"menu","world_runs":false,"elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("два экрана с одним именем — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("повторяется")),
        "{errors:?}"
    );
}

#[test]
fn no_live_screen_at_all_is_reported() {
    let screens = r##"{"screens":[{"name":"menu","world_runs":false,"elements":[]}]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("ни один экран не поднял world_runs — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("world_runs")),
        "{errors:?}"
    );
}

#[test]
fn win_screen_with_world_runs_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"start_screen":"menu","win_screen":"game","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{}}}"##;
    let rules = r##"{"rules":[
        {"kind":"delete","for":{"has":["score"]},"when":["score",">",0],"do":[["end_game","win"]]}
    ]}"##;
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[]},
        {"name":"game","world_runs":true,"elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(game_json, PROPS, SCENE, rules, screens, &[])
        .expect_err("у экрана исхода не должен быть поднят world_runs — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("win_screen")),
        "{errors:?}"
    );
}

#[test]
fn new_game_target_without_world_runs_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"button","anchor":"center","size":[10,10],"text":"x","font":"ui",
             "color":"#ffffff","on_click":["new_game","paused"]}
        ]},
        {"name":"paused","world_runs":false,"elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("new_game на экран без world_runs — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("new_game")),
        "{errors:?}"
    );
}

#[test]
fn unknown_button_command_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"button","anchor":"center","size":[10,10],"text":"x","font":"ui",
             "color":"#ffffff","on_click":["teleport"]}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("неизвестная команда кнопки — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("teleport")),
        "{errors:?}"
    );
}

#[test]
fn label_referencing_unknown_object_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"label","anchor":"center","size":[10,10],"text":"{ghost.score}","font":"ui"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("подстановка называет объект, которого нет в scene.json — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("ghost")),
        "{errors:?}"
    );
}

#[test]
fn font_not_in_table_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"label","anchor":"center","size":[10,10],"text":"x","font":"missing"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("шрифт не из files.fonts — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("missing")),
        "{errors:?}"
    );
}

#[test]
fn font_file_missing_is_reported() {
    let screens = r##"{"screens":[{"name":"menu","world_runs":true,"elements":[]}]}"##;
    // The "ui" font is declared in `GAME`'s files.fonts, but no bytes are supplied for it.
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("файл шрифта не найден — ошибка");
    let err = errors
        .iter()
        .find(|e| e.path.contains("fonts") && e.message.contains("не найден"))
        .unwrap_or_else(|| panic!("должна быть ошибка про недостающий шрифт: {errors:?}"));
    assert!(err.message.contains("ожидался файл шрифта"), "{err:?}");
    // «Формат игры» → «Проверка данных перед запуском»: сообщение называет файл И место в нём —
    // это поле объявлено прямо в game.json, значит и место ищется там же.
    assert_eq!(err.file, "game.json", "{err:?}");
    assert!(
        err.line.is_some() && err.column.is_some(),
        "у ошибки про files.fonts, объявленный прямо в game.json, есть текст, а значит и место: {err:?}"
    );
}

#[test]
fn font_file_that_does_not_parse_is_reported() {
    let screens = r##"{"screens":[{"name":"menu","world_runs":true,"elements":[]}]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", b"not a font")],
    )
    .expect_err("файл не разбирается как шрифт — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains("fonts") && e.message.contains("не разбирается")),
        "{errors:?}"
    );
}

#[test]
fn element_missing_anchor_and_size_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","color":"#ffffff"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } =
        load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[]).expect_err("нет anchor — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("anchor")),
        "{errors:?}"
    );

    // `size` on its own, with `anchor` present this time.
    let screens_size = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"center","color":"#ffffff"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } =
        load(GAME, PROPS, SCENE, RULES_EMPTY, screens_size, &[]).expect_err("нет size — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("size")),
        "{errors:?}"
    );
}

#[test]
fn unknown_anchor_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"bottom_rigth","size":[10,10],"color":"#ffffff"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } =
        load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[]).expect_err("опечатка в якоре — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("bottom_rigth")),
        "{errors:?}"
    );
}

#[test]
fn zero_size_element_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"center","size":[0,10],"color":"#ffffff"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } =
        load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[]).expect_err("нулевой размер — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("размер")),
        "{errors:?}"
    );
}

#[test]
fn bad_color_format_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"center","size":[10,10],"color":"red"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("цвет не в формате #rrggbb(aa) — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("цвет")),
        "{errors:?}"
    );
}

#[test]
fn eight_digit_color_with_alpha_is_accepted() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"center","size":[10,10],"color":"#12141acc"}
        ]}
    ]}"##;
    load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect("восьмизначный цвет допустим");
}

#[test]
fn unknown_element_kind_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"slider","anchor":"center","size":[10,10]}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("неизвестный вид элемента — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("slider")),
        "{errors:?}"
    );
}

#[test]
fn malformed_substitution_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"label","anchor":"center","size":[10,10],"text":"{score}","font":"ui"}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("подстановка без имени объекта — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("подстановка")),
        "{errors:?}"
    );
}

#[test]
fn every_error_category_is_collected_in_one_pass() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"bottom_rigth","size":[10,10],"color":"not-a-color"},
            {"kind":"label","anchor":"center","size":[10,10],"text":"{ghost.score}","font":"missing"},
            {"kind":"button","anchor":"center","size":[10,10],"text":"x","font":"ui",
             "color":"#ffffff","on_click":["teleport"]}
        ]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("несколько сломанных элементов — ошибка");
    assert!(errors.len() >= 4, "ожидались все ошибки разом: {errors:?}");
}

#[test]
fn unreachable_screen_is_a_warning_not_an_error() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[]},
        {"name":"orphan","world_runs":false,"elements":[]}
    ]}"##;
    let (_game, _screens, warnings) = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect("недостижимый экран — предупреждение, игра всё равно запускается");
    assert!(
        warnings
            .iter()
            .any(|w| w.file == "screens.json" && w.message.contains("orphan")),
        "{warnings:?}"
    );
}

// «Экраны и состояние» → «Клавиши экрана» → «Проверка данных перед запуском»: the checklist for
// a key's command mirrors `on_click`'s exactly, plus the shape errors specific to `keys` itself.

#[test]
fn screen_key_targeting_unknown_screen_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{"Space":["show_screen","nowhere"]},
         "elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("клавиша экрана ссылается на несуществующий экран — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("nowhere") && e.message.contains("клавиша")),
        "{errors:?}"
    );
}

#[test]
fn screen_key_new_game_target_without_world_runs_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{"Space":["new_game","paused"]},
         "elements":[]},
        {"name":"paused","world_runs":false,"elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect_err("new_game клавиши на экран без world_runs — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("new_game")),
        "{errors:?}"
    );
}

#[test]
fn screen_key_unknown_command_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{"Space":["teleport"]},
         "elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("неизвестная команда клавиши — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("teleport")),
        "{errors:?}"
    );
}

#[test]
fn screen_key_wrong_argument_count_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{"Space":["resume","лишний"]},
         "elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("у команды клавиши не то число настроек — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("параметр")),
        "{errors:?}"
    );
}

#[test]
fn keys_field_not_a_table_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":["Space","show_screen","menu"],
         "elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("keys записана не таблицей — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("объект")),
        "{errors:?}"
    );
}

#[test]
fn screen_key_command_not_a_list_is_reported() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{"Space":"resume"},
         "elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("команда клавиши записана не списком — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("массив")),
        "{errors:?}"
    );
}

#[test]
fn every_screen_key_error_category_is_collected_in_one_pass() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{
             "Space":["show_screen","nowhere"],
             "Escape":["teleport"],
             "KeyA":["show_screen"]
         },
         "elements":[]}
    ]}"##;
    let LoadFailure { errors, .. } = load(GAME, PROPS, SCENE, RULES_EMPTY, screens, &[])
        .expect_err("несколько сломанных клавиш — ошибка");
    assert!(errors.len() >= 3, "ожидались все ошибки разом: {errors:?}");
}

#[test]
fn screen_reachable_only_through_a_key_is_not_flagged_as_unreachable() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,
         "keys":{"Space":["show_screen","paused"]},
         "elements":[]},
        {"name":"paused","world_runs":false,"elements":[]}
    ]}"##;
    let (_game, _screens, warnings) = load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect("клавиша ведёт на экран — игра всё равно запускается без ошибок");
    assert!(
        !warnings.iter().any(|w| w.message.contains("paused")),
        "экран, достижимый только по клавише, не должен считаться недостижимым: {warnings:?}"
    );
}

#[test]
fn valid_screens_with_all_element_kinds_loads() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":true,"elements":[
            {"kind":"panel","anchor":"top_right","offset":[12,12],"size":[180,48],
             "color":"#12141acc"},
            {"kind":"label","anchor":"top_right","offset":[24,24],"size":[156,24],
             "text":"Счёт: {head.score}","font":"ui","font_size":22,"color":"#e8ecf2",
             "align":"right"},
            {"kind":"button","anchor":"center","offset":[0,20],"size":[220,56],
             "text":"Играть","font":"ui","font_size":24,"text_color":"#12141a",
             "color":"#5ad469","color_hover":"#6fe07d","color_pressed":"#3aa54c",
             "on_click":["new_game","menu"]}
        ]}
    ]}"##;
    load(
        GAME,
        PROPS,
        SCENE,
        RULES_EMPTY,
        screens,
        &[("ui", FONT_BYTES)],
    )
    .expect("panel/label/button с корректными полями должны загрузиться");
}

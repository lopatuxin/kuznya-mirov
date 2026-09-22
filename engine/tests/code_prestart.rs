//! «Код игры» → «Проверка перед запуском»: одна ошибка — один тест. Поведение во время партии —
//! в `code_execution.rs`.

use engine::data::error::LoadFailure;
use engine::data::load::{load_game_from_texts_with_code, read_entry};

const GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const SCENE_EMPTY: &str = r#"{"objects":[]}"#;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_MAIN: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

fn errors_for(rules: &str, code: Option<&str>) -> Vec<engine::data::error::GameError> {
    let result =
        load_game_from_texts_with_code(GAME, PROPS_EMPTY, SCENE_EMPTY, rules, SCREENS_MAIN, code);
    match result {
        Ok(_) => panic!("ожидалась ошибка, игра загрузилась"),
        Err(LoadFailure { errors, .. }) => errors,
    }
}

fn messages(errors: &[engine::data::error::GameError]) -> Vec<&str> {
    errors.iter().map(|e| e.message.as_str()).collect()
}

/// «Формат игры»: `files.code` не строка — та же общая проверка, что у любого другого поля.
#[test]
fn files_code_not_a_string_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":5}}"##;
    let result = read_entry(game_json);
    let LoadFailure { errors, .. } = result.expect_err("code не строка — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("строка")),
        "{errors:?}"
    );
}

/// «Код игры»: файл кода объявлен, но страница передала `null` — «файл не найден».
#[test]
fn file_not_found_is_reported() {
    let errors = errors_for(RULES_EMPTY, None);
    assert!(
        errors
            .iter()
            .any(|e| e.file == "code.lua" && e.message.contains("не найден")),
        "{errors:?}"
    );
}

/// «Код игры»: файл не разбирается — сообщение называет строку.
#[test]
fn syntax_error_names_the_line() {
    let errors = errors_for(RULES_EMPTY, Some("if true then\n"));
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].file, "code.lua");
    assert!(errors[0].line.is_some(), "{:?}", errors[0]);
}

/// «Код игры»: `find`/`delete`/`play_sound` на верхнем уровне файла — мира ещё нет.
#[test]
fn find_at_top_level_is_a_world_not_ready_error() {
    let errors = errors_for(RULES_EMPTY, Some("local x = find{}"));
    assert!(
        messages(&errors).iter().any(|m| m.contains("мира ещё нет")),
        "{errors:?}"
    );
}

#[test]
fn delete_at_top_level_is_a_world_not_ready_error() {
    let errors = errors_for(RULES_EMPTY, Some("delete({})"));
    assert!(
        messages(&errors).iter().any(|m| m.contains("мира ещё нет")),
        "{errors:?}"
    );
}

#[test]
fn play_sound_at_top_level_is_a_world_not_ready_error() {
    let errors = errors_for(RULES_EMPTY, Some("play_sound(\"x\")"));
    assert!(
        messages(&errors).iter().any(|m| m.contains("мира ещё нет")),
        "{errors:?}"
    );
}

/// «Код игры» → «Проверка перед запуском»: `run` в игре без `files.code`.
#[test]
fn run_without_files_code_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{}}}"##;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["nothing"]}},
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run","touch"]]}
    ]}"#;
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let result =
        load_game_from_texts_with_code(game_json, props, SCENE_EMPTY, rules, SCREENS_MAIN, None);
    let LoadFailure { errors, .. } = result.expect_err("run без files.code — ошибка");
    assert!(
        messages(&errors).iter().any(|m| m.contains("files.code")),
        "{errors:?}"
    );
}

/// «Код игры»: `run` называет функцию, которой в коде нет — перечисляет объявленные.
#[test]
fn run_naming_an_unknown_function_lists_the_declared_ones() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run","ghost"]]}
    ]}"#;
    let code = "function real_one() end";
    let errors = errors_for_with_props(props, rules, Some(code));
    let msgs = messages(&errors);
    assert!(
        msgs.iter()
            .any(|m| m.contains("ghost") && m.contains("real_one")),
        "{errors:?}"
    );
}

/// Файл не загрузился — у этой беды своя ошибка; правило с `run` ни в чём не виновато, второй
/// ошибки «функции нет» на нём быть не должно.
#[test]
fn a_file_that_failed_to_load_does_not_blame_every_run() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run","touch"]]}
    ]}"#;
    for code in ["function touch() end\nerror('boom')", "function touch(\n"] {
        let errors = errors_for_with_props(props, rules, Some(code));
        assert_eq!(errors.len(), 1, "{errors:?}");
        assert_eq!(errors[0].file, "code.lua", "{errors:?}");
    }
}

/// Функций в коде нет вовсе — сообщение говорит это по-русски, а не «объявлены функций нет».
#[test]
fn run_in_a_file_with_no_functions_says_so() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run","touch"]]}
    ]}"#;
    let errors = errors_for_with_props(props, rules, Some("local x = 1"));
    assert!(
        messages(&errors)
            .iter()
            .any(|m| m.contains("\"touch\"") && m.contains("не объявлено ни одной функции")),
        "{errors:?}"
    );
}

/// «Код игры»: верхний уровень файла может писать в глобальные, и запись `nil` убирает
/// объявленную выше функцию — `run` на неё находит, что функций в коде нет.
#[test]
fn a_function_set_to_nil_at_top_level_is_no_longer_declared() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run","touch"]]}
    ]}"#;
    let errors = errors_for_with_props(props, rules, Some("function touch() end\ntouch = nil"));
    assert!(
        messages(&errors)
            .iter()
            .any(|m| m.contains("\"touch\"") && m.contains("не объявлено ни одной функции")),
        "{errors:?}"
    );
}

fn errors_for_with_props(
    props: &str,
    rules: &str,
    code: Option<&str>,
) -> Vec<engine::data::error::GameError> {
    let result =
        load_game_from_texts_with_code(GAME, props, SCENE_EMPTY, rules, SCREENS_MAIN, code);
    match result {
        Ok(_) => panic!("ожидалась ошибка, игра загрузилась"),
        Err(LoadFailure { errors, .. }) => errors,
    }
}

/// `run` без аргумента, с лишними аргументами и с аргументом не строкой — три отдельных случая,
/// один и тот же текст.
#[test]
fn run_without_an_argument_is_reported() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run"]]}
    ]}"#;
    let errors = errors_for_with_props(props, rules, Some("function touch() end"));
    assert!(
        messages(&errors).iter().any(|m| m.contains("run")),
        "{errors:?}"
    );
}

#[test]
fn run_with_extra_arguments_is_reported() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run","touch","extra"]]}
    ]}"#;
    let errors = errors_for_with_props(props, rules, Some("function touch() end"));
    assert!(
        messages(&errors).iter().any(|m| m.contains("run")),
        "{errors:?}"
    );
}

#[test]
fn run_with_a_non_string_argument_is_reported() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["ball"]},"when":"outside_scene","do":[["run",5]]}
    ]}"#;
    let errors = errors_for_with_props(props, rules, Some("function touch() end"));
    assert!(
        messages(&errors).iter().any(|m| m.contains("run")),
        "{errors:?}"
    );
}

/// «Код игры» → «Проверка перед запуском»: имя свойства, звука или картинки отдельным словом в
/// тексте кода снимает предупреждение «объявлено и не используется».
#[test]
fn a_property_name_mentioned_in_code_text_suppresses_the_unused_warning() {
    let props = r#"{"properties":{"score":"number"}}"#;
    let code = "function touch(obj) return obj.score end";
    let (_game, _screens, warnings) = load_game_from_texts_with_code(
        GAME,
        props,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        Some(code),
    )
    .expect("должно загрузиться");
    assert!(
        !warnings.iter().any(|w| w.message.contains("score")),
        "{warnings:?}"
    );
}

#[test]
fn an_unmentioned_property_still_warns_as_unused() {
    let props = r#"{"properties":{"score":"number"}}"#;
    let code = "function touch() end";
    let (_game, _screens, warnings) = load_game_from_texts_with_code(
        GAME,
        props,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        Some(code),
    )
    .expect("должно загрузиться");
    assert!(
        warnings.iter().any(|w| w.message.contains("score")),
        "{warnings:?}"
    );
}

/// «Картинки» → «Загрузка и проверка»: слово `image` в тексте кода снимает ошибку
/// «opacity задан без image».
#[test]
fn the_word_image_in_code_text_excuses_opacity_without_an_image_source() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"opacity":0.5}]}"#;
    let code = "function touch(obj) obj.image = \"x\" end";
    let result =
        load_game_from_texts_with_code(GAME, props, scene, RULES_EMPTY, SCREENS_MAIN, Some(code));
    match result {
        Ok((_game, _screens, warnings)) => {
            assert!(
                !warnings.iter().any(|w| w.message.contains("opacity")),
                "{warnings:?}"
            );
        }
        Err(failure) => panic!("должно загрузиться: {failure:?}"),
    }
}

/// «Код игры» → «Проверка перед запуском»: проверочный прогон файла — на зерне игры
/// (`config.random_seed`), не на захардкоженном 0, иначе `if math.random(...) == ... then
/// error(...) end` мог пройти проверку и упасть уже при старте партии, где используется
/// настоящее зерно. Число в `error(...)` — ровно первый бросок `math.random(1000000)` на зерне
/// `GAME` (7), посчитанный тем же счётчиком, что использует движок: проверка почти наверняка не
/// заметила бы ошибку на захардкоженном 0 (шанс совпадения — один на миллион), а на настоящем
/// зерне заметит её всегда. Стартовый экран здесь — не живой, чтобы `Game::new` не запускал код
/// заново (уже на настоящем зерне) и не смазывал результат — под проверкой ровно прогон
/// `load_rest`, а не ещё один, отдельный запуск при живом старте.
#[test]
fn the_prestart_check_run_uses_the_games_own_random_seed() {
    let mut rng = engine::core::rng::Rng::new(7);
    let first_roll = 1 + (rng.next_u64() % 1_000_000);
    let code = format!(
        "if math.random(1000000) == {first_roll} then error('первый бросок на зерне игры') end"
    );
    let screens = r#"{"screens":[
        {"name":"main","world_runs":false,"elements":[]},
        {"name":"live","world_runs":true,"elements":[]}
    ]}"#;
    let result = load_game_from_texts_with_code(
        GAME,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        Some(&code),
    );
    let LoadFailure { errors, .. } = result.expect_err(
        "на зерне 0 этот бросок почти наверняка не совпал бы — а на зерне 7 совпадает всегда",
    );
    assert!(
        messages(&errors)
            .iter()
            .any(|m| m.contains("первый бросок")),
        "{errors:?}"
    );
}

#[test]
fn opacity_without_an_image_source_and_without_the_word_image_in_code_still_errors() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"opacity":0.5}]}"#;
    let code = "function touch() end";
    let errors =
        load_game_from_texts_with_code(GAME, props, scene, RULES_EMPTY, SCREENS_MAIN, Some(code))
            .expect_err("opacity без image и без слова image в коде — ошибка");
    assert!(
        messages(&errors.errors)
            .iter()
            .any(|m| m.contains("opacity")),
        "{:?}",
        errors.errors
    );
}

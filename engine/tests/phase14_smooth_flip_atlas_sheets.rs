//! Фаза 14 — сглаживание, отражение и листы атласа (требования 1–13, 20–21). Раскладка атласа по
//! листам, число слоёв для WebGL2 и сама GPU-часть живут в `render::atlas`'s own `mod tests`
//! (native, без браузера) — здесь только сквозная проверка через загрузку игры: разбор `smooth`,
//! проверка `flip_x` перед запуском и то, что отражение не трогает ничего, кроме рисования.
//! Ключ адреса `webgl2` и строка в консоли — не в этом файле: страница их не отдаёт движку никак,
//! которым мог бы воспользоваться тест (см. заметку фазы, «Страница и редактор» — web/).

use engine::core::input::StepInput;
use engine::core::scene::object_at;
use engine::data::error::LoadFailure;
use engine::data::load::{ImageVerdict, load_game_from_texts_with_code, load_rest, read_entry};

fn game_json(files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":10,"height":10,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_MAIN: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

fn ok_pixels(width: u32, height: u32) -> ImageVerdict {
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

#[allow(clippy::too_many_arguments)]
fn load(
    game_json: &str,
    scene: &str,
    rules: &str,
    images: &[(&str, ImageVerdict)],
) -> Result<
    (
        engine::core::game::Game,
        engine::core::screens::ScreensConfig,
        Vec<engine::data::error::GameError>,
    ),
    LoadFailure,
> {
    let (config, _entry_warnings) = read_entry(game_json).expect("game.json должен разбираться");
    let image_data: Vec<(String, ImageVerdict)> = images
        .iter()
        .map(|(name, verdict)| (name.to_string(), verdict.clone()))
        .collect();
    load_rest(
        game_json,
        config,
        Some(PROPS_EMPTY),
        Some(scene),
        Some(rules),
        Some(SCREENS_MAIN),
        &[],
        &[],
        &[],
        &image_data,
        None,
        false,
    )
    .map(|(game, screens, warnings, _images)| (game, screens, warnings))
}

fn messages(errors: &LoadFailure) -> Vec<&str> {
    errors.errors.iter().map(|e| e.message.as_str()).collect()
}

// -------------------------------------------------------------------------------------------
// `smooth`
// -------------------------------------------------------------------------------------------

#[test]
fn smooth_true_and_false_parse_and_default_to_false() {
    let game = game_json(
        r#","images":{"a":{"path":"a.png","smooth":true},"b":{"path":"b.png","smooth":false},
        "c":{"path":"c.png"}}"#,
    );
    let (config, _warnings) = read_entry(&game).expect("smooth true/false/отсутствие — не ошибка");
    let by_name = |name: &str| {
        config
            .files
            .images
            .iter()
            .find(|d| d.name == name)
            .unwrap_or_else(|| panic!("нет картинки {name}"))
    };
    assert!(by_name("a").smooth);
    assert!(!by_name("b").smooth);
    assert!(!by_name("c").smooth, "по умолчанию false");
}

#[test]
fn smooth_not_a_flag_is_reported_with_the_images_own_name() {
    let game = game_json(r#","images":{"head":{"path":"head.png","smooth":"yes"}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("smooth не true/false — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains("head") && e.path.contains("smooth")),
        "{errors:?}"
    );
}

// -------------------------------------------------------------------------------------------
// `flip_x` — вид значения
// -------------------------------------------------------------------------------------------

#[test]
fn flip_x_not_a_flag_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"head","flip_x":"yes"}]}"#;
    let LoadFailure { errors, .. } = load(&game, scene, RULES_EMPTY, &[("head", ok_pixels(8, 8))])
        .expect_err("flip_x не признаком — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("flip_x")),
        "{errors:?}"
    );
}

// -------------------------------------------------------------------------------------------
// `flip_x` — картинка должна как-то достаться, требование 13
// -------------------------------------------------------------------------------------------

#[test]
fn flip_x_without_any_way_to_get_an_image_on_an_object_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"flip_x":true}]}"#;
    let LoadFailure { errors, .. } = load(&game, scene, RULES_EMPTY, &[("head", ok_pixels(8, 8))])
        .expect_err("flip_x без картинки ниоткуда — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("flip_x")),
        "{errors:?}"
    );
}

/// Одна проверка на `opacity` и `flip_x`: объект без картинки с обоими свойствами получает ровно
/// по одной ошибке на каждое, обе — на своём месте в файле.
#[test]
fn opacity_and_flip_x_without_an_image_on_one_object_give_one_error_each() {
    let game = game_json(r#","images":{"head":{"path":"head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"opacity":0.5,"flip_x":true}]}"#;
    let LoadFailure { errors, .. } = load(&game, scene, RULES_EMPTY, &[("head", ok_pixels(8, 8))])
        .expect_err("opacity и flip_x без картинки ниоткуда — ошибки");
    let about = |word: &str| {
        errors
            .iter()
            .filter(|e| e.message.starts_with(word) && e.path.contains("objects[0]"))
            .count()
    };
    assert_eq!(about("opacity"), 1, "{errors:?}");
    assert_eq!(about("flip_x"), 1, "{errors:?}");
}

#[test]
fn flip_x_without_any_way_to_get_an_image_on_a_spawn_template_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"head.png"}}"#);
    let scene = r#"{"objects":[]}"#;
    let rules = r##"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["size"]}}},
        "where":"random_cell",
        "template":{"size":[1,1],"flip_x":true}}]}"##;
    let LoadFailure { errors, .. } = load(&game, scene, rules, &[("head", ok_pixels(8, 8))])
        .expect_err("flip_x на шаблоне без картинки ниоткуда — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("flip_x")),
        "{errors:?}"
    );
}

#[test]
fn flip_x_with_its_own_image_loads() {
    let game = game_json(r#","images":{"head":{"path":"head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"head","flip_x":true}]}"#;
    let result = load(&game, scene, RULES_EMPTY, &[("head", ok_pixels(8, 8))]);
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn flip_x_is_allowed_when_keys_can_give_the_object_an_image() {
    let game = game_json(r#","images":{"tail":{"path":"tail.png"}}"#);
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"flip_x":true,
        "keys":{"Space":{"press":[["image","tail"]]}}}]}"##;
    let result = load(&game, scene, RULES_EMPTY, &[("tail", ok_pixels(8, 8))]);
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn flip_x_is_allowed_when_a_collide_effect_can_set_the_image() {
    let game = game_json(r#","images":{"tail":{"path":"tail.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"flip_x":true}]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["set","image","tail"]]}}
    ]}"#;
    let result = load(&game, scene, rules, &[("tail", ok_pixels(8, 8))]);
    assert!(result.is_ok(), "{:?}", result.err());
}

/// «Крайние случаи»: правило ставит `flip_x` объекту по ходу партии, а не через сцену или
/// шаблон, — проверка перед запуском смотрит только сцену и шаблоны, значит это не ошибка даже
/// когда у объекта вовсе нет способа получить картинку.
#[test]
fn flip_x_given_only_by_a_rule_at_runtime_is_not_checked_even_without_any_image_source() {
    let game = game_json("");
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ff0000"}]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["give","flip_x"]]}}
    ]}"#;
    let result = load(&game, scene, rules, &[]);
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn the_word_image_in_code_text_excuses_flip_x_without_an_image_source() {
    let game = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"flip_x":true}]}"#;
    let code = "function touch(obj) obj.image = \"x\" end";
    let result = load_game_from_texts_with_code(
        game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        Some(code),
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn flip_x_without_an_image_source_and_without_the_word_image_in_code_still_errors() {
    let game = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"flip_x":true}]}"#;
    let code = "function touch() end";
    let failure = load_game_from_texts_with_code(
        game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        Some(code),
    )
    .expect_err("flip_x без image и без слова image в коде — ошибка");
    assert!(
        messages(&failure).iter().any(|m| m.contains("flip_x")),
        "{:?}",
        failure.errors
    );
}

// -------------------------------------------------------------------------------------------
// Отражение не трогает ничего, кроме рисования — требование 11
// -------------------------------------------------------------------------------------------

/// Как `phase13_big_images_ground_tiles.rs`'s
/// `own_size_image_does_not_change_object_at_on_click_or_layer_order`: тот же прямоугольник,
/// тот же порядок — `flip_x` у одного из двух объектов ничего не меняет в выборе щелчка.
#[test]
fn flip_x_does_not_change_object_at_or_layer_order() {
    let game = game_json(r#","images":{"a":{"path":"a.png"},"b":{"path":"b.png"}}"#);
    let scene = r##"{"objects":[
        {"position":[2,2],"size":[1,1],"image":"a","flip_x":true,"on_click":[["layer",1]]},
        {"position":[2,2],"size":[1,1],"image":"b","on_click":[["layer",2]]}
    ]}"##;
    let (loaded, _screens, warnings) = load(
        &game,
        scene,
        RULES_EMPTY,
        &[("a", ok_pixels(8, 8)), ("b", ok_pixels(8, 8))],
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    let picked = object_at(&loaded.world, &loaded.scene, [25.0, 25.0], [100.0, 100.0]);
    assert_eq!(
        picked,
        Some(1),
        "прямоугольник, а не отражение, решает щелчок"
    );
}

/// «Картинки», требование 11: `flip_x` не признак столкновения — объект с ним по-прежнему
/// сталкивается по своему прямоугольнику `position`×`size`, тот же прямоугольник, что и у
/// объекта без отражения.
#[test]
fn flip_x_does_not_change_collision() {
    let props = r#"{"properties":{"hit":"flag"}}"#;
    let game = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"images":{"a":{"path":"a.png"}}}}"##;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"image":"a","flip_x":true,"collides":true},
        {"position":[0,0],"size":[1,1],"color":"#00ff00","collides":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["give","hit"]]}}
    ]}"#;
    let (config, _entry_warnings) = read_entry(game).expect("game.json должен разбираться");
    let (mut loaded, _screens, warnings, _images) = load_rest(
        game,
        config,
        Some(props),
        Some(scene),
        Some(rules),
        Some(SCREENS_MAIN),
        &[],
        &[],
        &[],
        &[("a".to_string(), ok_pixels(8, 8))],
        None,
        false,
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    loaded.step(StepInput::empty());
    let hit = loaded.properties.resolve("hit").expect("hit объявлен");
    assert!(
        loaded.world.flag(0, hit),
        "flip_x не должен мешать обычному столкновению по прямоугольнику"
    );
}

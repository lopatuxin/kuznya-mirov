//! Prestart validation for `files.images`, an object's `image`/`opacity` properties, and a
//! panel/button's `image`/`image_hover`/`image_pressed`/`opacity` fields — «Картинки» → «Загрузка
//! и проверка». Companion to `prestart_validation.rs`, `screens_validation.rs` and
//! `sound_validation.rs`, which cover everything else.

use engine::data::error::LoadFailure;
use engine::data::load::{ImageVerdict, load_rest, read_entry};

/// Builds `game.json` with `files_extra` spliced right after `"fonts":{}` in the `files` block —
/// e.g. `,"images":{"head":{"path":"images/head.png"}}` — so each test only has to name the one
/// table shape it's checking. `properties_extra` does the same for `properties.json` (empty by
/// default, via `PROPS_EMPTY`).
fn game_json(files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":4,"height":4,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"fonts/ui.ttf"}}{files_extra}}}}}"##
    )
}

const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const SCENE_EMPTY: &str = r#"{"objects":[]}"#;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_MAIN: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

/// A four-byte TrueType signature — enough to pass `looks_like_font`'s sniff without being a
/// real, fully parseable font; see `data::load::looks_like_font`. The `"ui"` font every
/// `game_json` here declares is only ever exercised by the button tests below, but declaring it
/// unconditionally keeps this file's `game_json`/`load` as simple as `sound_validation.rs`'s.
const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];

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
    props: &str,
    scene: &str,
    rules: &str,
    screens: &str,
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
    let font_bytes: Vec<(String, Option<Vec<u8>>)> =
        vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
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
        &image_data,
        None,
        false,
    )
    .map(|(game, screens, warnings, _images)| (game, screens, warnings))
}

// ---------------------------------------------------------------------------------------------
// `files.images` — errors caught straight out of `game.json`, so `read_entry` alone already
// fails.
// ---------------------------------------------------------------------------------------------

#[test]
fn images_table_written_as_a_string_is_reported() {
    let game = game_json(r#","images":"images/""#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("files.images строкой — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("files.images")),
        "{errors:?}"
    );
}

#[test]
fn image_description_not_an_object_is_reported() {
    let game = game_json(r#","images":{"head":["images/head.png"]}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("описание картинки массивом — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("объект") && e.path.contains("head")),
        "{errors:?}"
    );
}

#[test]
fn unknown_key_in_image_description_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png","extra":1}}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("неизвестный ключ описания — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("extra")),
        "{errors:?}"
    );
}

#[test]
fn empty_name_in_images_table_is_reported() {
    let game = game_json(r#","images":{"":{"path":"images/head.png"}}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("пустое имя в files.images — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("пустое имя")),
        "{errors:?}"
    );
}

#[test]
fn missing_path_in_image_description_is_reported() {
    let game = game_json(r#","images":{"head":{}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("описание без path — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("path")),
        "{errors:?}"
    );
}

#[test]
fn non_string_path_in_image_description_is_reported() {
    let game = game_json(r#","images":{"head":{"path":5}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("path числом — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("ожидалась строка")),
        "{errors:?}"
    );
}

#[test]
fn non_png_path_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.jpg"}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("не .png путь — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("head.jpg") && e.message.contains(".png")),
        "{errors:?}"
    );
}

#[test]
fn frames_not_an_integer_is_reported() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":2.5,"frame_time":0.1}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("дробные frames — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("frames")),
        "{errors:?}"
    );
}

#[test]
fn frames_below_one_is_reported() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":0,"frame_time":0.1}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("frames 0 — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("frames")),
        "{errors:?}"
    );
}

#[test]
fn frames_beyond_u32_range_is_reported_with_the_written_number() {
    // `n as u32` насыщает до u32::MAX для значения вне диапазона — сообщение должно называть
    // число, написанное в файле (1e30), а не 4294967295.
    let game = game_json(
        r#","images":{"food":{"path":"images/food.png","frames":1e30,"frame_time":0.1}}"#,
    );
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("frames вне диапазона — ошибка");
    let error = errors
        .iter()
        .find(|e| e.path.contains("frames"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert!(
        error.message.contains("1000000000000000000000000000000"),
        "{errors:?}"
    );
}

#[test]
fn frame_time_not_a_number_is_reported() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":4,"frame_time":"x"}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("frame_time строкой — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("frame_time")),
        "{errors:?}"
    );
}

#[test]
fn frame_time_not_positive_is_reported() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":4,"frame_time":0}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("frame_time 0 — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("frame_time")),
        "{errors:?}"
    );
}

#[test]
fn frames_without_frame_time_is_reported() {
    let game = game_json(r#","images":{"food":{"path":"images/food.png","frames":4}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("frames без frame_time — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("frame_time")),
        "{errors:?}"
    );
}

#[test]
fn frame_time_without_frames_is_reported() {
    let game = game_json(r#","images":{"food":{"path":"images/food.png","frame_time":0.1}}"#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("frame_time без frames — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("frames")),
        "{errors:?}"
    );
}

#[test]
fn frames_and_frame_time_together_pass_prestart_check() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":4,"frame_time":0.15}}"#);
    let result = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("food", ok_pixels(128, 32))],
    );
    assert!(
        result.is_ok(),
        "frames и frame_time вместе — не ошибка: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------------------------
// The page's verdict on each declared image — needs `load_rest`.
// ---------------------------------------------------------------------------------------------

#[test]
fn missing_image_file_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[],
    )
    .expect_err("файла нет — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("head.png") && e.message.contains("не найден")),
        "{errors:?}"
    );
}

#[test]
fn rejected_image_file_names_the_browser() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ImageVerdict::Rejected)],
    )
    .expect_err("браузер не разжал — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("head.png") && e.message.contains("браузер")),
        "{errors:?}"
    );
}

#[test]
fn width_not_divisible_by_frames_is_reported_with_remainder() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":3,"frame_time":0.1}}"#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("food", ok_pixels(100, 32))],
    )
    .expect_err("100 не делится на 3 — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("100")
            && e.message.contains("3")
            && e.message.contains("1")),
        "{errors:?}"
    );
}

#[test]
fn width_divisible_by_frames_passes() {
    let game =
        game_json(r#","images":{"food":{"path":"images/food.png","frames":3,"frame_time":0.1}}"#);
    let result = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("food", ok_pixels(99, 32))],
    );
    assert!(
        result.is_ok(),
        "99 делится на 3 нацело — не ошибка: {:?}",
        result.err()
    );
}

#[test]
fn zero_width_and_height_page_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(0, 0))],
    )
    .expect_err("нулевые ширина и высота — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("head.png") && e.message.contains("больше нуля")),
        "{errors:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// An object's `image`/`opacity` properties.
// ---------------------------------------------------------------------------------------------

#[test]
fn image_field_not_a_string_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":5}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    )
    .expect_err("image числом — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("ожидалась строка")),
        "{errors:?}"
    );
}

#[test]
fn undeclared_image_name_lists_the_declared_ones() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"tail"}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    )
    .expect_err("картинки \"tail\" нет — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("tail") && e.message.contains("head")),
        "{errors:?}"
    );
}

#[test]
fn undeclared_image_name_with_opacity_reports_only_the_undeclared_name() {
    // «Картинки»: a broken `image` field must not also trip the "opacity без image" check —
    // `image`'s presence is unknown, not known-absent, so only its own error should fire.
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"nope","opacity":0.5}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    )
    .expect_err("картинки \"nope\" нет — ошибка");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert!(errors[0].message.contains("nope"), "{errors:?}");
}

#[test]
fn color_and_image_together_on_object_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let scene =
        r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ffffff","image":"head"}]}"##;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    )
    .expect_err("color и image вместе — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("color") && e.message.contains("image")),
        "{errors:?}"
    );
}

#[test]
fn opacity_without_image_on_object_is_reported() {
    let game = game_json("");
    let scene =
        r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ffffff","opacity":0.5}]}"##;
    let LoadFailure { errors, .. } =
        load(&game, PROPS_EMPTY, scene, RULES_EMPTY, SCREENS_MAIN, &[])
            .expect_err("opacity без image — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("opacity")),
        "{errors:?}"
    );
}

#[test]
fn opacity_without_image_on_object_reports_scene_file_and_path() {
    // «Формат игры»: каждое сообщение называет файл и путь узла — здесь виноват объект
    // scene.json, а не rules.json, который эта проверка раньше называла всегда.
    let game = game_json("");
    let scene =
        r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ffffff","opacity":0.5}]}"##;
    let LoadFailure { errors, .. } =
        load(&game, PROPS_EMPTY, scene, RULES_EMPTY, SCREENS_MAIN, &[])
            .expect_err("opacity без image — ошибка");
    let error = errors
        .iter()
        .find(|e| e.message.contains("opacity"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(error.file, "scene.json", "{errors:?}");
    assert_eq!(error.path, "objects[0]", "{errors:?}");
    assert!(error.line.is_some(), "{errors:?}");
}

#[test]
fn opacity_without_image_on_a_spawn_template_reports_the_template_node() {
    // Путь шаблона собирается тем же join, что и все остальные, иначе locate не находит узел и
    // строка с колонкой остаются пустыми.
    let game = game_json("");
    let rules = r##"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["size"]}}},
        "where":"random_cell",
        "template":{"size":[1,1],"color":"#ffffff","opacity":0.5}}]}"##;
    let LoadFailure { errors, .. } =
        load(&game, PROPS_EMPTY, SCENE_EMPTY, rules, SCREENS_MAIN, &[])
            .expect_err("opacity без image — ошибка");
    let error = errors
        .iter()
        .find(|e| e.message.contains("opacity"))
        .unwrap_or_else(|| panic!("{errors:?}"));
    assert_eq!(error.file, "rules.json", "{errors:?}");
    assert_eq!(error.path, "rules[0] → template", "{errors:?}");
    assert!(error.line.is_some(), "{errors:?}");
}

#[test]
fn opacity_out_of_range_on_object_is_reported() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"head","opacity":1.5}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    )
    .expect_err("opacity вне отрезка — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("opacity")),
        "{errors:?}"
    );
}

#[test]
fn opacity_without_image_is_allowed_when_keys_can_give_the_object_an_image() {
    // «Картинки»: opacity судится по тому, что объект может получить за игру, а не только по
    // тому, что записано в файле — keys той же клавишей может подставить image уже в игре.
    let game = game_json(r#","images":{"tail":{"path":"images/tail.png"}}"#);
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"opacity":0.5,
        "keys":{"Space":{"press":[["image","tail"]]}}}]}"##;
    let result = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("tail", ok_pixels(32, 32))],
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn opacity_without_image_is_allowed_when_a_collide_effect_can_set_it() {
    let game = game_json(r#","images":{"tail":{"path":"images/tail.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"opacity":0.5}]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["set","image","tail"]]}}
    ]}"#;
    let result = load(
        &game,
        PROPS_EMPTY,
        scene,
        rules,
        SCREENS_MAIN,
        &[("tail", ok_pixels(32, 32))],
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn object_with_only_image_and_valid_opacity_loads() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"head","opacity":0.5}]}"#;
    let result = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

// ---------------------------------------------------------------------------------------------
// `properties.json` may not shadow `image`/`opacity`.
// ---------------------------------------------------------------------------------------------

#[test]
fn author_property_named_image_is_rejected() {
    let game = game_json("");
    let props = r#"{"properties":{"image":"number"}}"#;
    let LoadFailure { errors, .. } =
        load(&game, props, SCENE_EMPTY, RULES_EMPTY, SCREENS_MAIN, &[])
            .expect_err("properties.json называет image — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("image") && e.message.contains("уже объявлено")),
        "{errors:?}"
    );
}

#[test]
fn author_property_named_opacity_is_rejected() {
    let game = game_json("");
    let props = r#"{"properties":{"opacity":"flag"}}"#;
    let LoadFailure { errors, .. } =
        load(&game, props, SCENE_EMPTY, RULES_EMPTY, SCREENS_MAIN, &[])
            .expect_err("properties.json называет opacity — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("opacity") && e.message.contains("уже объявлено")),
        "{errors:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// A panel's `color`/`image`/`opacity`.
// ---------------------------------------------------------------------------------------------

fn screens_with_element(element_extra: &str) -> String {
    format!(
        r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[
            {{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[10,10]{element_extra}}}
        ]}}]}}"#
    )
}

#[test]
fn panel_without_color_or_image_is_reported() {
    let game = game_json("");
    let screens = screens_with_element("");
    let LoadFailure { errors, .. } =
        load(&game, PROPS_EMPTY, SCENE_EMPTY, RULES_EMPTY, &screens, &[])
            .expect_err("панель без заливки — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("color")),
        "{errors:?}"
    );
}

#[test]
fn panel_with_color_and_image_is_reported() {
    let game = game_json(r#","images":{"panel":{"path":"images/panel.png"}}"#);
    let screens = screens_with_element(r##","color":"#ffffff","image":"panel""##);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("panel", ok_pixels(32, 32))],
    )
    .expect_err("панель с color и image — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("color") && e.message.contains("image")),
        "{errors:?}"
    );
}

#[test]
fn image_hover_on_panel_is_reported() {
    let game = game_json(r#","images":{"panel":{"path":"images/panel.png"}}"#);
    let screens = screens_with_element(r#","image":"panel","image_hover":"panel""#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("panel", ok_pixels(32, 32))],
    )
    .expect_err("image_hover у панели — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("image_hover")),
        "{errors:?}"
    );
}

#[test]
fn opacity_without_image_on_panel_is_reported() {
    let game = game_json("");
    let screens = screens_with_element(r##","color":"#ffffff","opacity":0.5"##);
    let LoadFailure { errors, .. } =
        load(&game, PROPS_EMPTY, SCENE_EMPTY, RULES_EMPTY, &screens, &[])
            .expect_err("opacity у панели с color — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("opacity")),
        "{errors:?}"
    );
}

#[test]
fn panel_with_only_image_loads() {
    let game = game_json(r#","images":{"panel":{"path":"images/panel.png"}}"#);
    let screens = screens_with_element(r#","image":"panel","opacity":0.8"#);
    let result = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("panel", ok_pixels(32, 32))],
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

// ---------------------------------------------------------------------------------------------
// A button's `color`/`color_hover`/`color_pressed` vs `image`/`image_hover`/`image_pressed`.
// ---------------------------------------------------------------------------------------------

fn screens_with_button(button_extra: &str) -> String {
    format!(
        r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[
            {{"kind":"button","anchor":"top_left","offset":[0,0],"size":[10,10],
              "text":"Играть","font":"ui","on_click":["quit"]{button_extra}}}
        ]}}]}}"#
    )
}

#[test]
fn button_without_color_or_image_is_reported() {
    let game = game_json("");
    let screens = screens_with_button("");
    let LoadFailure { errors, .. } =
        load(&game, PROPS_EMPTY, SCENE_EMPTY, RULES_EMPTY, &screens, &[])
            .expect_err("кнопка без заливки — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("color")),
        "{errors:?}"
    );
}

#[test]
fn button_with_color_and_image_is_reported() {
    let game = game_json(r#","images":{"button":{"path":"images/button.png"}}"#);
    let screens = screens_with_button(r##","color":"#ffffff","image":"button""##);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("button", ok_pixels(32, 32))],
    )
    .expect_err("кнопка с color и image — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("color") && e.message.contains("image")),
        "{errors:?}"
    );
}

#[test]
fn image_hover_on_a_color_button_is_reported() {
    let game = game_json(r#","images":{"button":{"path":"images/button.png"}}"#);
    let screens = screens_with_button(r##","color":"#ffffff","image_hover":"button""##);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("button", ok_pixels(32, 32))],
    )
    .expect_err("image_hover у кнопки с color — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("image_hover")),
        "{errors:?}"
    );
}

#[test]
fn color_hover_on_an_image_button_is_reported() {
    let game = game_json(r#","images":{"button":{"path":"images/button.png"}}"#);
    let screens = screens_with_button(r##","image":"button","color_hover":"#ffffff""##);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("button", ok_pixels(32, 32))],
    )
    .expect_err("color_hover у кнопки с image — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("color_hover")),
        "{errors:?}"
    );
}

#[test]
fn button_with_image_and_hover_variants_loads() {
    let game = game_json(
        r#","images":{"button":{"path":"images/button.png"},
                       "button_hover":{"path":"images/button_hover.png"},
                       "button_pressed":{"path":"images/button_pressed.png"}}"#,
    );
    let screens = screens_with_button(
        r#","image":"button","image_hover":"button_hover","image_pressed":"button_pressed""#,
    );
    let result = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[
            ("button", ok_pixels(32, 32)),
            ("button_hover", ok_pixels(32, 32)),
            ("button_pressed", ok_pixels(32, 32)),
        ],
    );
    assert!(result.is_ok(), "{:?}", result.err());
}

#[test]
fn unknown_image_hover_and_image_pressed_are_both_reported() {
    // Все ошибки собираются за один заход: `resolve_element` не должен обрывать разбор кнопки
    // на первом же неизвестном имени картинки и терять второе.
    let game = game_json(r#","images":{"button":{"path":"images/button.png"}}"#);
    let screens =
        screens_with_button(r#","image":"button","image_hover":"nope1","image_pressed":"nope2""#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        &screens,
        &[("button", ok_pixels(32, 32))],
    )
    .expect_err("два неизвестных имени картинки — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("nope1")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.message.contains("nope2")),
        "{errors:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Warnings — the game still starts.
// ---------------------------------------------------------------------------------------------

#[test]
fn image_named_only_in_a_key_binding_is_not_reported_as_unused() {
    let game = game_json(r#","images":{"tail":{"path":"images/tail.png"}}"#);
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ffffff",
        "keys":{"ArrowLeft":{"press":[["image","tail"]]}}}]}"##;
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        scene,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("tail", ok_pixels(32, 32))],
    )
    .expect("картинка, названная только в keys, — не ошибка");
    assert!(
        warnings.iter().all(|w| !w.message.contains("tail")),
        "{warnings:?}"
    );
}

#[test]
fn unused_image_is_a_warning() {
    let game = game_json(r#","images":{"head":{"path":"images/head.png"}}"#);
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("head", ok_pixels(32, 32))],
    )
    .expect("объявленная неиспользуемая картинка — не ошибка");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("head") && w.message.contains("не называет")),
        "{warnings:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// A game with no images at all keeps loading exactly as before.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_game_without_any_images_table_loads_with_no_errors_and_no_image_warnings() {
    let game = game_json("");
    let scene = r##"{"objects":[{"position":[0,0],"size":[1,1],"color":"#ffffff"}]}"##;
    let (_game, _screens, warnings) =
        load(&game, PROPS_EMPTY, scene, RULES_EMPTY, SCREENS_MAIN, &[])
            .expect("игра без картинок должна грузиться, как и раньше");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
}

// ---------------------------------------------------------------------------------------------
// `frame_by` — требование 23. `load_rest` резолвит его один раз против `properties.json` и
// отдаёт уже разрешённый список сама, а не заставляет вызывающего (`wasm::Engine::load`) клонировать
// свою собственную копию `config.files.images` заранее и резолвить его ещё раз после.
// ---------------------------------------------------------------------------------------------

/// Воспроизведённый баг: `wasm::Engine::load` держал свою собственную копию `config.files.images`,
/// снятую до `load_rest`, и резолвил её `frame_by` заново уже после, вторым проходом, ошибки
/// которого шли в одноразовый `ErrorSink::new()` и терялись безмолвно — так что при рассинхроне
/// между двумя резолвами страница молча оставалась без `frame_by`. Проверка — без wasm, прямо на
/// `load_rest`: список картинок в её собственном `Ok` уже должен нести разрешённый `frame_by`.
#[test]
fn load_rest_returns_images_with_frame_by_already_resolved() {
    let game =
        game_json(r#","images":{"strip":{"path":"images/strip.png","frames":2,"frame_by":"n"}}"#);
    let props = r#"{"properties":{"n":"number"}}"#;
    let (config, _entry_warnings) = read_entry(&game).expect("game.json должен разбираться");
    let font_bytes: Vec<(String, Option<Vec<u8>>)> =
        vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    let image_data = vec![("strip".to_string(), ok_pixels(64, 32))];
    let (game, _screens, _warnings, images) = load_rest(
        &game,
        config,
        Some(props),
        Some(SCENE_EMPTY),
        Some(RULES_EMPTY),
        Some(SCREENS_MAIN),
        &font_bytes,
        &[],
        &[],
        &image_data,
        None,
        false,
    )
    .expect("должно загрузиться");
    let n = game
        .properties
        .resolve("n")
        .expect("n должно быть в таблице свойств");
    let strip = images
        .iter()
        .find(|decl| decl.name == "strip")
        .expect("strip должна быть в списке картинок, которые отдаёт load_rest");
    assert_eq!(
        strip.frame_by,
        Some(n),
        "load_rest должен отдавать уже разрешённый frame_by"
    );
}

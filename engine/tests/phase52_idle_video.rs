//! Фаза 52 — ролик покоя на панели: `idle` у панели с видео по наведению — видео по кругу, которое панель
//! показывает, пока её видео стоит на первом кадре без курсора. Здесь — разбор и проверки перед запуском; когда
//! панель в покое и как тает кадр покоя — в `render::video_play`'s own `mod idle_tests`; проигрыватели в браузере
//! — только QA на стенде.

use engine::core::game::Game;
use engine::core::screens::{Element, Fill, PanelFill, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageDecl, ImageVerdict, ReverseVerdict, load_rest, read_entry, read_texts,
};

const PROPS: &str = r#"{"properties":{}}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const NO_OBJECTS: &str = r#"{"objects":[]}"#;

const IMAGES: &str = r#""images":{
    "hero":{"path":"hero.mp4","reverse":"hero-reverse.mp4"},
    "hero_idle":{"path":"hero_idle.mp4"},
    "head":{"path":"head.png"}}"#;

fn game_json() -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"ui.ttf"}},{IMAGES}}}}}"##
    )
}

fn screens_json(menu: &str) -> String {
    format!(
        r##"{{"screens":[{{"name":"menu","world_runs":false,"elements":[
    {{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],"name":"spot","color":"#00000000"}},
    {menu}]}},
    {{"name":"game","world_runs":true,"elements":[]}}]}}"##
    )
}

/// Видео `width` × 128 длиной `seconds`; второй файл — того же размера и длины.
fn video(width: u32, seconds: f64) -> ImageVerdict {
    ImageVerdict::Video {
        width,
        height: 128,
        seconds,
        reverse: Some(ReverseVerdict::Ok {
            width,
            height: 128,
            seconds,
        }),
    }
}

fn head() -> ImageVerdict {
    ImageVerdict::Ok {
        width: 8,
        height: 8,
        pixels: vec![0u8; 8 * 8 * 4],
    }
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn try_load(menu: &str, idle_width: u32) -> Result<Loaded, LoadFailure> {
    let game = game_json();
    let (config, _) = read_entry(&game).expect("game.json должен разбираться");
    let image_data = vec![
        ("hero".to_string(), video(64, 7.5)),
        ("hero_idle".to_string(), video(idle_width, 10.0)),
        ("head".to_string(), head()),
    ];
    load_rest(
        &game,
        config,
        Some(PROPS),
        Some(NO_OBJECTS),
        Some(NO_RULES),
        Some(&screens_json(menu)),
        &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
        &[],
        &[],
        &image_data,
        None,
        false,
    )
}

const HERO_WITH_IDLE: &str = r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],
    "image":"hero","play_on_hover":"spot","idle":"hero_idle"}"#;

fn errors_of(menu: &str, idle_width: u32) -> Vec<GameError> {
    try_load(menu, idle_width)
        .expect_err("загрузка должна провалиться")
        .errors
}

fn assert_error(errors: &[GameError], path: &str, message: &str) {
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains(path) && e.message.contains(message)),
        "нет ошибки {path}: {message}: {errors:?}"
    );
}

#[test]
fn a_hover_panel_names_its_idle_video_and_the_idle_video_loops() {
    let (_, config, warnings, images) = try_load(HERO_WITH_IDLE, 64)
        .unwrap_or_else(|e| panic!("игра не загрузилась: {:#?}", e.errors));
    assert!(
        !warnings.iter().any(|w| w.message.contains("hero_idle")),
        "ролик покоя назван панелью: {warnings:?}"
    );
    let idle = config.screens[0]
        .elements
        .iter()
        .find_map(|element| match element {
            Element::Panel {
                fill: PanelFill::Fixed(Fill::Image { .. }),
                idle: Some(idle),
                ..
            } => Some(*idle),
            _ => None,
        })
        .expect("панель с роликом покоя");
    assert_eq!(images[idle].name, "hero_idle");
    let loops = config.loop_videos(|id| images[id].video);
    assert_eq!(loops, [idle], "ролик покоя идёт по кругу");
}

#[test]
fn the_page_reads_the_idle_video_twice_like_a_loop() {
    let game = game_json();
    let (config, _) = read_entry(&game).expect("game.json разобран");
    let texts = read_texts(
        &config,
        Some(PROPS),
        Some(&screens_json(HERO_WITH_IDLE)),
        None,
    );
    assert!(
        texts
            .reverses
            .iter()
            .any(|(name, path)| name == "hero_idle" && path == "hero_idle.mp4"),
        "второй проигрыватель того же файла: {:?}",
        texts.reverses
    );
}

#[test]
fn idle_on_a_panel_without_a_hover_video_is_an_error() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[10,10],"image":"head","idle":"hero_idle"}"#,
        64,
    );
    assert_error(&errors, "idle", "без видео по наведению");
}

#[test]
fn idle_naming_an_undeclared_image_is_an_error() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],
            "image":"hero","play_on_hover":"spot","idle":"nobody"}"#,
        64,
    );
    assert_error(&errors, "idle", "картинки \"nobody\" нет");
}

#[test]
fn idle_naming_a_png_is_an_error() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],
            "image":"hero","play_on_hover":"spot","idle":"head"}"#,
        64,
    );
    assert_error(&errors, "idle", "idle берёт видео");
}

#[test]
fn idle_that_is_not_a_string_is_an_error() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],
            "image":"hero","play_on_hover":"spot","idle":5}"#,
        64,
    );
    assert_error(&errors, "idle", "ожидалась строка");
}

#[test]
fn idle_naming_the_panel_s_own_video_is_an_error() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],
            "image":"hero","play_on_hover":"spot","idle":"hero"}"#,
        64,
    );
    assert_error(&errors, "idle", "видео самой панели");
}

#[test]
fn a_hover_panel_with_a_png_names_one_error_not_a_second_about_idle() {
    let errors = errors_of(
        r#"{"kind":"panel","anchor":"center","offset":[0,0],"size":[100,100],
            "image":"head","play_on_hover":"spot","idle":"hero_idle"}"#,
        64,
    );
    assert_error(&errors, "play_on_hover", "играть нечего");
    assert!(
        !errors.iter().any(|e| e.path.contains("idle")),
        "{errors:?}"
    );
}

#[test]
fn an_idle_video_of_another_size_is_an_error_naming_both_sizes() {
    let errors = errors_of(HERO_WITH_IDLE, 80);
    assert_error(&errors, "idle", "80×128");
    assert_error(&errors, "idle", "64×128");
}

#[test]
fn an_idle_video_also_shown_on_another_panel_is_an_error() {
    let menu = format!(
        r#"{HERO_WITH_IDLE},
        {{"kind":"panel","anchor":"center","offset":[0,0],"size":[10,10],"image":"hero_idle"}}"#
    );
    let errors = errors_of(&menu, 64);
    assert_error(&errors, "", "одно видео — на одной панели");
}

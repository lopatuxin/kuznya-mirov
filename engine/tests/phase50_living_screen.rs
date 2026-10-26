//! Фаза 50 — живой экран: видео панели по кругу, частицы на панелях экрана с `wind` и `cell_pixels` экрана,
//! прозрачность панели-источника для мыши, новые эффекты пух и бабочки в мире и на экране и проверки перед
//! запуском. Формулы полёта пуха и бабочки, ход видео по кругу и рисунки — в `render::particles`,
//! `render::video_play` и `render::particle_shapes`'s own `mod tests`; браузер и видеокарта — только QA на
//! стенде. Игра собрана из текстов, её ведёт `ui_support::Ui`.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::interface;
use engine::core::particles::{Effect, ParticleShape, Settings};
use engine::core::property;
use engine::core::screens::{self, Element, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageDecl, ImageVerdict, ReverseVerdict, load_rest, read_entry, read_texts,
};
use engine::data::session;
use engine::render::atlas::{
    AtlasImage, AtlasRect, BUTTERFLY_RECT, FLUFF_RECT, compose_world_paints, opaque_masks, pack,
    particle_emitters, screen_emitters, screen_particle_paint, screen_particle_paints,
};
use engine::render::particles::Particles;
use engine::render::video_play::{LoopCourse, PlayerReading};
use engine::render::wind::Motion;
use serde_json::json;
use support::{Ui, VIEWPORT};

const PROPS: &str = r##"{"properties":{"mark":"flag","car":"flag"}}"##;
const NO_RULES: &str = r##"{"rules":[]}"##;
const CAR_SCENE: &str = r##"{"objects":[{"name":"c1","car":true}]}"##;

/// Видео `river` без `reverse`, видео `hero` с ним, берёза `birch` — левая половина прозрачна, правая
/// непрозрачна — и сплошная `head`.
const IMAGES: &str = r##""images":{
    "river":{"path":"river.mp4"},
    "hero":{"path":"hero.mp4","reverse":"hero-reverse.mp4"},
    "birch":{"path":"birch.png"},
    "head":{"path":"head.png"}}"##;

fn game_json(extra_scene: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"{extra_scene}}},
"random_seed":1,"start_screen":"menu","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{"ui":"ui.ttf"}},{IMAGES}}}}}"##
    )
}

/// Видео по кругу: страница прочитала тот же файл ещё раз, и второй проигрыватель в порядке.
fn river() -> ImageVerdict {
    hero()
}

fn hero() -> ImageVerdict {
    ImageVerdict::Video {
        width: 64,
        height: 128,
        seconds: 7.5,
        reverse: Some(ReverseVerdict::Ok {
            width: 64,
            height: 128,
            seconds: 7.5,
        }),
    }
}

/// Берёза 8 × 4 точки: левая половина прозрачна, правая непрозрачна.
fn birch() -> ImageVerdict {
    let mut pixels = vec![0u8; 8 * 4 * 4];
    for y in 0..4 {
        for x in 4..8 {
            pixels[(y * 8 + x) * 4 + 3] = 255;
        }
    }
    ImageVerdict::Ok {
        width: 8,
        height: 4,
        pixels,
    }
}

fn head() -> ImageVerdict {
    ImageVerdict::Ok {
        width: 8,
        height: 8,
        pixels: vec![255u8; 8 * 8 * 4],
    }
}

fn verdicts() -> Vec<(String, ImageVerdict)> {
    [
        ("river", river()),
        ("hero", hero()),
        ("birch", birch()),
        ("head", head()),
    ]
    .into_iter()
    .map(|(name, verdict)| (name.to_string(), verdict))
    .collect()
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

fn try_load_full(game: &str, scene: &str, screens: &str) -> Result<Loaded, LoadFailure> {
    try_load_answered(game, scene, screens, &verdicts())
}

fn try_load_answered(
    game: &str,
    scene: &str,
    screens: &str,
    verdicts: &[(String, ImageVerdict)],
) -> Result<Loaded, LoadFailure> {
    let (config, _) = read_entry(game).expect("game.json должен разбираться");
    load_rest(
        game,
        config,
        Some(PROPS),
        Some(scene),
        Some(NO_RULES),
        Some(screens),
        &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
        &[],
        &[],
        verdicts,
        None,
        false,
    )
}

/// `screens.json` с корневыми ключами `root` (с запятой в конце) и экраном `menu` с ключами `keys` (с запятой
/// в конце) и списком `elements`.
fn screens_json_at(root: &str, keys: &str, elements: &str) -> String {
    format!(
        r##"{{{root}"screens":[
    {{"name":"menu","world_runs":false,{keys}"elements":[{elements}]}},
    {{"name":"game","world_runs":true,"elements":[]}}
]}}"##
    )
}

/// `screens.json` с экраном `menu`, у которого ключи `keys` (с запятой в конце) и список `elements`.
fn screens_json(keys: &str, elements: &str) -> String {
    screens_json_at("", keys, elements)
}

fn try_load(keys: &str, elements: &str) -> Result<Loaded, LoadFailure> {
    try_load_full(&game_json(""), CAR_SCENE, &screens_json(keys, elements))
}

fn load(keys: &str, elements: &str) -> Loaded {
    try_load(keys, elements).unwrap_or_else(|e| panic!("игра не загрузилась: {:#?}", e.errors))
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

/// Предупреждения о частицах, ветре и клетке экрана; остальные (картинки без дела и подобное) к делу не
/// относятся.
fn particle_warnings(warnings: &[GameError]) -> Vec<&GameError> {
    warnings
        .iter()
        .filter(|w| {
            [
                "частиц",
                "wind",
                "ветер",
                "cell_pixels",
                "fluff",
                "butterflies",
            ]
            .iter()
            .any(|word| w.message.contains(word) || w.path.contains(word))
        })
        .collect()
}

const CELLS: &str = r##""cell_pixels":40,"##;

/// Берёза на весь угол окна 400 × 300 точек: листья, пух и бабочки.
const BIRCH_PANEL: &str = r##"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[400,300],
    "image":"birch","leaf_fall":1,"fluff":0.5,"butterflies":0.5}"##;

// -------------------------------------------------------------------------------------------
// Видео по кругу: разбор и проверки
// -------------------------------------------------------------------------------------------

#[test]
fn a_video_on_a_panel_without_play_on_hover_goes_round_and_asks_for_no_reverse() {
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let (_, config, warnings, images) = load("", panel);
    assert!(particle_warnings(&warnings).is_empty(), "{warnings:?}");
    let river = images.iter().position(|d| d.name == "river").unwrap();
    assert_eq!(config.loop_videos(|image| images[image].video), [river]);
    assert_eq!(config.panel_videos(), Vec::<usize>::new());
    assert_eq!(images[river].reverse, None);
    let Element::Panel {
        play_on_hover,
        rest_at,
        ..
    } = &config.screens[0].elements[0]
    else {
        panic!("панель");
    };
    assert_eq!((*play_on_hover, *rest_at), (None, None));
}

#[test]
fn a_video_with_a_reverse_can_go_round_on_a_panel_as_well() {
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"hero"}"##;
    let (_, config, _, images) = load("", panel);
    assert_eq!(config.loop_videos(|image| images[image].video).len(), 1);
}

#[test]
fn a_video_with_play_on_hover_still_needs_a_reverse() {
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river","play_on_hover":"a"},
        {"kind":"panel","anchor":"top_left","name":"a","size":[100,100],"color":"#00000000"}"##;
    let errors = errors_of(try_load("", panel));
    assert_error(&errors, "image", "у него нет reverse");
}

#[test]
fn rest_at_on_a_video_that_goes_round_is_an_error_and_on_a_hover_video_it_is_not() {
    let looped =
        r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river","rest_at":1}"##;
    let errors = errors_of(try_load("", looped));
    assert_error(&errors, "rest_at", "rest_at у панели с видео по кругу");

    let hover = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"hero","play_on_hover":"a","rest_at":1},
        {"kind":"panel","anchor":"top_left","name":"a","size":[100,100],"color":"#00000000"}"##;
    load("", hover);
}

#[test]
fn an_error_names_the_element_by_its_number_in_the_file_even_when_earlier_elements_are_broken() {
    let panels = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","fluff":1.5},
        {"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"},
        {"kind":"panel","anchor":"top_left","size":[100,100],"image":"river","rest_at":1}"##;
    let errors = errors_of(try_load(CELLS, panels));
    let rest = errors
        .iter()
        .find(|e| e.message.contains("rest_at у панели с видео по кругу"))
        .unwrap_or_else(|| panic!("нет ошибки rest_at: {errors:?}"));
    assert!(rest.path.ends_with("[2] → rest_at"), "{}", rest.path);
    assert!(rest.line.is_some() && rest.column.is_some(), "{rest:?}");
}

#[test]
fn the_unknown_field_of_a_panel_lists_the_particle_properties_it_accepts() {
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","wind":[1,0]}"##;
    let errors = errors_of(try_load(CELLS, panel));
    assert_error(&errors, "wind", "\"rest_at\", \"idle\", \"smoke\"");
    assert_error(&errors, "wind", "\"butterfly_color\"");
}

#[test]
fn one_video_on_two_panels_or_on_a_panel_and_a_world_object_is_still_an_error_for_a_loop() {
    let two = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"},
        {"kind":"panel","anchor":"top_left","offset":[200,0],"size":[100,100],"image":"river"}"##;
    let errors = errors_of(try_load("", two));
    assert_error(&errors, "elements[1].image", "уже играет другая панель");

    let one = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let scene = r##"{"objects":[{"position":[2,2],"size":[4,2],"image":"river"}]}"##;
    let errors = errors_of(try_load_full(&game_json(""), scene, &screens_json("", one)));
    assert_error(&errors, "elements[0].image", "называет ещё и объект мира");
}

#[test]
fn a_video_on_a_button_a_bar_or_a_picture_panel_is_not_a_loop() {
    let button = r##"{"kind":"button","anchor":"top_left","size":[10,10],"text":"","font":"ui","image":"river",
        "on_click":["quit"]}"##;
    let errors = errors_of(try_load("", button));
    assert_error(&errors, "image", "кнопка и шкала её не берут");

    let picture = r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"head"}"##;
    let (_, config, _, images) = load("", picture);
    assert_eq!(
        config.loop_videos(|image| images[image].video),
        Vec::<usize>::new()
    );
}

#[test]
fn the_page_is_asked_for_a_loop_video_itself_as_its_second_file_and_a_hover_video_for_its_reverse()
{
    let screens = screens_json(
        "",
        r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"},
        {"kind":"panel","anchor":"top_left","size":[100,100],"image":"hero","play_on_hover":"a"},
        {"kind":"panel","anchor":"top_left","name":"a","size":[100,100],"color":"#00000000"}"##,
    );
    let (config, _) = read_entry(&game_json("")).expect("game.json должен разбираться");
    let needed = read_texts(&config, Some(PROPS), Some(&screens), None);
    assert_eq!(
        needed.reverses,
        [
            ("hero".to_string(), "hero-reverse.mp4".to_string()),
            ("river".to_string(), "river.mp4".to_string())
        ]
    );

    let both = screens_json(
        "",
        r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"hero"}"##,
    );
    let needed = read_texts(&config, Some(PROPS), Some(&both), None);
    assert_eq!(
        needed.reverses,
        [("hero".to_string(), "hero.mp4".to_string())],
        "по кругу играет сам ролик, а не его обратный файл"
    );

    let none = screens_json(
        "",
        r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}"##,
    );
    let needed = read_texts(&config, Some(PROPS), Some(&none), None);
    assert_eq!(
        needed.reverses,
        [("hero".to_string(), "hero-reverse.mp4".to_string())],
        "видео без панели никто не крутит"
    );
}

#[test]
fn a_loop_video_whose_second_player_the_browser_would_not_take_is_an_error_that_never_says_reverse()
{
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let answers = |twin: Option<ReverseVerdict>| -> Vec<(String, ImageVerdict)> {
        let mut answers = verdicts();
        answers[0].1 = ImageVerdict::Video {
            width: 64,
            height: 128,
            seconds: 7.5,
            reverse: twin,
        };
        answers
    };
    for twin in [
        None,
        Some(ReverseVerdict::Missing),
        Some(ReverseVerdict::Rejected),
        Some(ReverseVerdict::Ok {
            width: 64,
            height: 64,
            seconds: 7.5,
        }),
        Some(ReverseVerdict::Ok {
            width: 64,
            height: 128,
            seconds: 4.0,
        }),
    ] {
        let errors = errors_of(try_load_answered(
            &game_json(""),
            CAR_SCENE,
            &screens_json("", panel),
            &answers(twin),
        ));
        assert_error(&errors, "river", "вторым проигрывателем");
        assert!(
            errors.iter().all(|e| !e.message.contains("reverse")),
            "{twin:?}: {errors:?}"
        );
    }
    let loaded = try_load_answered(
        &game_json(""),
        CAR_SCENE,
        &screens_json("", panel),
        &answers(Some(ReverseVerdict::Ok {
            width: 64,
            height: 128,
            seconds: 7.5,
        })),
    )
    .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    assert!(
        loaded.2.iter().all(|w| !w.message.contains("reverse")),
        "{:?}",
        loaded.2
    );
}

/// Длина ролика в проверках видео по кругу, секунд.
const LOOP_LENGTH: f64 = 7.5;
const FRAME: f64 = 1.0 / 60.0;

#[derive(Clone, Copy)]
struct Player {
    time: f64,
    playing: bool,
    seek: Option<(u32, f64)>,
}

/// Страница с двумя проигрывателями одного файла и ход видео по кругу, как их ведёт слой отрисовки: ответы
/// проигрывателей уходят в ход, его приказы исполняются.
struct Browser {
    course: LoopCourse,
    players: [Player; 2],
    now: f64,
    /// Время ролика на показанном кадре в каждом кадре отрисовки.
    shown: Vec<Option<f64>>,
}

impl Browser {
    fn new() -> Browser {
        let player = Player {
            time: 0.0,
            playing: false,
            seek: None,
        };
        Browser {
            course: LoopCourse::new(LOOP_LENGTH),
            players: [player; 2],
            now: 10.0,
            shown: Vec::new(),
        }
    }

    /// Кадр отрисовки: `opened` — счёт открытий экрана, пока панель на экране, `None` — панели нет.
    fn frame(&mut self, opened: Option<u64>) {
        let readings = self.players.map(|player| PlayerReading {
            time: player.time,
            seeking: player.seek.is_some(),
            paused: !player.playing,
        });
        // Как в Chromium: проигрыватель, что переходит на новое место, кадра не отдаёт.
        let ready = self.players.map(|player| player.seek.is_none());
        let step = match opened {
            Some(opened) => self.course.step(self.now, opened, ready, readings),
            None => self.course.leave().unwrap_or_default(),
        };
        for (player, order) in self.players.iter_mut().zip(step.orders) {
            if order.pause {
                player.playing = false;
            }
            if let Some(to) = order.seek {
                player.seek = Some((3, to));
            }
            if order.play {
                player.playing = true;
            }
        }
        let ready = self.players.map(|player| player.seek.is_none());
        self.shown.push(
            step.show
                .filter(|&slot| ready[slot])
                .map(|slot| self.players[slot].time),
        );
        for player in &mut self.players {
            match player.seek {
                Some((1, to)) => {
                    player.seek = None;
                    player.time = to;
                }
                Some((left, to)) => player.seek = Some((left - 1, to)),
                None if player.playing => player.time += FRAME,
                None => {}
            }
        }
        self.now += FRAME;
    }

    fn run(&mut self, seconds: f64, opened: Option<u64>) {
        for _ in 0..(seconds / FRAME).round() as u32 {
            self.frame(opened);
        }
    }

    fn picture(&self) -> f64 {
        self.shown
            .iter()
            .rev()
            .flatten()
            .next()
            .copied()
            .expect("кадр показан")
    }
}

#[test]
fn a_screen_opened_again_by_the_engine_starts_its_loop_video_from_the_first_frame() {
    let looping = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let (game, config, _, _) = load("", looping);
    let mut ui = Ui::new(game, config);
    let mut browser = Browser::new();
    browser.run(2.0, Some(ui.state.opened()));
    assert!(
        (browser.picture() - 2.0).abs() < 0.2,
        "через две секунды ролик на второй: {}",
        browser.picture()
    );

    // Между двумя кадрами экран открыли заново: панель стояла и до, и после, но счёт открытий сдвинулся.
    let before = ui.state.opened();
    ui.show("game");
    ui.show("menu");
    assert_eq!(
        ui.state.opened(),
        before + 2,
        "каждая смена экрана — новое открытие"
    );
    browser.shown.clear();
    browser.run(1.0, Some(ui.state.opened()));
    let first = browser.shown.iter().flatten().next().copied();
    assert_eq!(first, Some(0.0), "экран открыли заново — первый кадр");
    assert!(
        browser.picture() < 1.0,
        "и ролик пошёл сначала, а не с двух секунд: {}",
        browser.picture()
    );
}

#[test]
fn a_screen_that_was_left_and_opened_again_starts_its_loop_video_from_the_first_frame() {
    let looping = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let (game, config, _, _) = load("", looping);
    let mut ui = Ui::new(game, config);
    let mut browser = Browser::new();
    browser.run(3.0, Some(ui.state.opened()));
    ui.show("game");
    browser.run(1.0, None);
    assert!(
        browser.players.iter().all(|player| !player.playing),
        "пока панели нет, видео стоит"
    );
    ui.show("menu");
    browser.shown.clear();
    browser.run(0.5, Some(ui.state.opened()));
    assert_eq!(
        browser.shown.iter().flatten().next().copied(),
        Some(0.0),
        "и снова с первого кадра"
    );
    assert!(browser.picture() < 0.6, "{}", browser.picture());
}

#[test]
fn a_loop_video_runs_on_through_its_seam_without_a_missing_picture() {
    let looping = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let (game, config, _, _) = load("", looping);
    let ui = Ui::new(game, config);
    let mut browser = Browser::new();
    browser.run(2.0 * LOOP_LENGTH + 1.0, Some(ui.state.opened()));
    assert!(
        browser.shown.iter().all(Option::is_some),
        "на стыке картинка не пропадает"
    );
    let wraps = browser
        .shown
        .iter()
        .flatten()
        .collect::<Vec<_>>()
        .windows(2)
        .filter(|pair| pair[1] < pair[0])
        .count();
    assert_eq!(wraps, 2, "два стыка за два круга");
}

// -------------------------------------------------------------------------------------------
// Частицы на панелях: разбор
// -------------------------------------------------------------------------------------------

#[test]
fn a_panel_carries_the_thirteen_particle_properties_with_the_values_and_defaults_of_an_object() {
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000",
        "smoke":0.5,"smoke_height":6,"smoke_color":"#bfbfbf","sparks":1,"sparks_reach":2.5,
        "sparks_direction":450,"sparks_spread":45,"leaf_fall":0.3,"leaf_color":"#ff8800",
        "fluff":0.2,"fluff_color":"#ffffee","butterflies":0.7,"butterfly_color":"#00ff00"}"##;
    let (_, config, warnings, _) = load(CELLS, panel);
    assert!(particle_warnings(&warnings).is_empty(), "{warnings:?}");
    let settings = config.screens[0].elements[0].particles().copied().unwrap();
    assert_eq!(settings.smoke, 0.5);
    assert_eq!(settings.smoke_height, 6.0);
    assert_eq!(settings.sparks, 1.0);
    assert_eq!(settings.sparks_reach, 2.5);
    assert_eq!(settings.sparks_direction, 450.0);
    assert_eq!(settings.sparks_spread, 45.0);
    assert_eq!(settings.leaf_fall, 0.3);
    assert_eq!(settings.fluff, 0.2);
    assert_eq!(settings.butterflies, 0.7);
    assert_eq!(settings.smoke_color, [191.0 / 255.0; 3]);
    assert_eq!(settings.butterfly_color, Some([0.0, 1.0, 0.0]));
    assert!(settings.leaf_color.is_some());

    let bare = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","fluff":0.2}"##;
    let (_, config, _, _) = load(CELLS, bare);
    let defaults = config.screens[0].elements[0].particles().copied().unwrap();
    assert_eq!(
        defaults,
        Settings {
            fluff: 0.2,
            ..Settings::default()
        },
        "умолчания те же, что у объекта"
    );
    assert!(config.screens[0].elements[0].particles().is_some());
    let plain = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}"##;
    let (_, config, _, _) = load("", plain);
    assert!(config.screens[0].elements[0].particles().is_none());
}

#[test]
fn the_edges_of_every_particle_range_are_allowed_on_a_panel() {
    for fields in [
        r##""fluff":0"##,
        r##""fluff":1,"butterflies":1"##,
        r##""butterflies":0,"sparks_spread":0"##,
        r##""sparks":1,"sparks_spread":180"##,
        r##""smoke_height":0.01,"sparks_reach":100,"sparks_direction":-90"##,
    ] {
        let panel = format!(
            r##"{{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000",{fields}}}"##
        );
        try_load(CELLS, &panel).unwrap_or_else(|e| panic!("{fields}: {:?}", e.errors));
    }
}

#[test]
fn particle_values_outside_their_ranges_are_errors_that_name_the_screen_and_the_element() {
    let cases = [
        (r##""fluff":1.5"##, "fluff", "fluff: нужно от 0 до 1"),
        (r##""fluff":-0.1"##, "fluff", "fluff: нужно от 0 до 1"),
        (
            r##""butterflies":2"##,
            "butterflies",
            "butterflies: нужно от 0 до 1",
        ),
        (r##""smoke":7"##, "smoke", "smoke: нужно от 0 до 1"),
        (
            r##""leaf_fall":-1"##,
            "leaf_fall",
            "leaf_fall: нужно от 0 до 1",
        ),
        (r##""smoke_height":0"##, "smoke_height", "больше нуля"),
        (r##""sparks_reach":-3"##, "sparks_reach", "больше нуля"),
        (r##""sparks_spread":181"##, "sparks_spread", "от 0 до 180"),
        (r##""fluff":"много""##, "fluff", "ожидалось число"),
        (
            r##""fluff_color":"white""##,
            "fluff_color",
            "цвет должен быть вида",
        ),
        (
            r##""butterfly_color":5"##,
            "butterfly_color",
            "ожидалась строка",
        ),
        (
            r##""leaf_color":"#12""##,
            "leaf_color",
            "цвет должен быть вида",
        ),
    ];
    for (field, path, message) in cases {
        let panel = format!(
            r##"{{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}},
            {{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000",{field}}}"##
        );
        let errors = errors_of(try_load(CELLS, &panel));
        assert_error(&errors, &format!("[1] → {path}"), message);
    }
}

#[test]
fn particles_on_a_screen_without_cell_pixels_are_an_error_that_names_the_panel() {
    let panel = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"},
        {"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","butterflies":0.3}"##;
    let errors = errors_of(try_load("", panel));
    assert_error(&errors, "[1]", "нет cell_pixels");
    assert_error(&errors, "[1]", "у экрана \"menu\"");

    let plain = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}"##;
    load("", plain);
}

#[test]
fn the_cell_of_one_screen_does_not_serve_another_screen() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"cell_pixels":40,"elements":[
            {"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","fluff":0.3}]},
        {"name":"game","world_runs":true,"elements":[
            {"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","fluff":0.3}]}
    ]}"##;
    let errors = errors_of(try_load_full(&game_json(""), CAR_SCENE, screens));
    assert_error(&errors, "screens[1]", "у экрана \"game\" нет cell_pixels");
    assert!(
        errors.iter().all(|e| !e.path.contains("screens[0]")),
        "{errors:?}"
    );
}

#[test]
fn a_bad_cell_pixels_is_one_error_and_not_a_second_one_about_a_missing_cell_for_each_particle_panel()
 {
    let panels = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","fluff":0.3},
        {"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000","butterflies":0.3}"##;
    for keys in [
        r##""cell_pixels":0,"##,
        r##""cell_pixels":-4,"##,
        r##""cell_pixels":"40","##,
    ] {
        let errors = errors_of(try_load(keys, panels));
        assert_eq!(errors.len(), 1, "{keys}: {errors:?}");
        assert_error(&errors, "screens[0] → cell_pixels", "");
    }
}

#[test]
fn a_new_session_never_shares_a_screen_opening_with_the_one_before_it() {
    let looping = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"image":"river"}"##;
    let (game, config, _, _) = load("", looping);
    let mut ui = Ui::new(game, config);
    let first = ui.state.opened();
    let _session =
        engine::data::session::PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    assert_ne!(
        ui.state.opened(),
        first,
        "запуск партии — экран открыт заново"
    );
    let second = ui.state.opened();
    let _session =
        engine::data::session::PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    assert_ne!(ui.state.opened(), second);
}

#[test]
fn cell_pixels_and_wind_at_the_root_of_screens_json_are_unknown_keys() {
    let plain = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}"##;
    for (root, key) in [
        (r##""cell_pixels":40,"##, "cell_pixels"),
        (r##""wind":[1,0],"##, "wind"),
    ] {
        let errors = errors_of(try_load_full(
            &game_json(""),
            CAR_SCENE,
            &screens_json_at(root, "", plain),
        ));
        assert_error(&errors, key, "неизвестное поле");
    }
}

#[test]
fn cell_pixels_that_is_not_a_positive_number_and_a_wind_that_is_not_a_pair_are_errors() {
    let plain = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}"##;
    for (keys, path, message) in [
        (r##""cell_pixels":0,"##, "cell_pixels", "число больше нуля"),
        (r##""cell_pixels":-4,"##, "cell_pixels", "число больше нуля"),
        (r##""cell_pixels":"40","##, "cell_pixels", "ожидалось число"),
        (r##""wind":1.5,"##, "wind", "ожидался массив"),
        (r##""wind":[1],"##, "wind", "ожидалась пара чисел"),
        (r##""wind":[1,"a"],"##, "wind", "ожидалось число"),
    ] {
        let errors = errors_of(try_load(keys, plain));
        assert_error(&errors, &format!("screens[0] → {path}"), message);
    }
}

#[test]
fn the_wind_and_the_cell_of_a_screen_reach_the_config_and_a_wind_without_particles_does_nothing() {
    let plain = r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#00000000"}"##;
    let (_, config, warnings, _) = load(r##""cell_pixels":32,"wind":[1.5,-0.5],"##, plain);
    let (menu, other) = (&config.screens[0], &config.screens[1]);
    assert!(
        particle_warnings(&warnings).is_empty(),
        "ветер без частиц — без предупреждения: {warnings:?}"
    );
    assert_eq!((menu.cell_pixels, menu.wind), (Some(32.0), [1.5, -0.5]));
    assert_eq!(
        (other.cell_pixels, other.wind),
        (None, [0.0, 0.0]),
        "ветер и клетка — своё у каждого экрана"
    );
    let (_, config, _, _) = load("", plain);
    let menu = &config.screens[0];
    assert_eq!((menu.cell_pixels, menu.wind), (None, [0.0, 0.0]));
}

#[test]
fn particle_properties_on_anything_but_a_free_panel_are_errors() {
    let place =
        r##"{"kind":"panel","anchor":"top_left","name":"a","size":[10,10],"color":"#00000000"}"##;
    let cases = [
        (
            r##"{"kind":"label","anchor":"top_left","size":[10,10],"text":"x","font":"ui","fluff":0.3}"##,
            "[1] → fluff",
            "у надписи нет \"fluff\"",
        ),
        (
            r##"{"kind":"button","anchor":"top_left","size":[10,10],"text":"","font":"ui","color":"#808080",
            "on_click":["quit"],"butterflies":0.3}"##,
            "[1] → butterflies",
            "у кнопки нет \"butterflies\"",
        ),
        (
            r##"{"kind":"bar","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"value":"car","max":1,
            "color":"#ffffff","smoke":0.3}"##,
            "[1] → smoke",
            "у шкалы нет \"smoke\"",
        ),
        (
            r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"row_height":10,
            "row":[],"leaf_fall":0.3}"##,
            "[1] → leaf_fall",
            "у списка нет \"leaf_fall\"",
        ),
        (
            r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","for":{"has":["car"]},
            "fluff":0.3}"##,
            "[1] → fluff",
            "у панели с for нет \"fluff\"",
        ),
        (
            r##"{"kind":"list","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"row_height":10,
            "row":[{"kind":"panel","size":[10,10],"color":"#ffffff","butterflies":0.3}]}"##,
            "butterflies",
            "у панели в строке списка нет \"butterflies\"",
        ),
    ];
    for (element, path, message) in cases {
        let errors = errors_of(try_load(CELLS, &format!("{place},{element}")));
        assert_error(&errors, path, message);
    }
}

// -------------------------------------------------------------------------------------------
// Источники экрана, клетка, ветер, часы
// -------------------------------------------------------------------------------------------

/// Источники активного экрана этого кадра: раскладка окна с масштабом интерфейса, как у `compose_ui`.
fn emitters_of(ui: &Ui, window: [f32; 2]) -> Vec<engine::render::particles::Emitter> {
    let screen_id = ui.state.active();
    let frame = ui.config.ui_frame(window);
    let parts = interface::layout(
        &ui.config.screens[screen_id],
        screen_id,
        &ui.game.world,
        &ui.state,
        frame.viewport,
    );
    let cell = ui.config.screens[screen_id].cell_pixels;
    screen_emitters(&parts, cell.unwrap_or(1.0)).collect()
}

fn birch_ui(root: &str, elements: &str) -> Ui {
    let (game, config, _, _) = load(root, elements);
    Ui::new(game, config)
}

/// Движение, ведомое окном: `opened` — открытие экрана, `steps` — часы окна в шагах.
fn update(motion: &mut Motion, ui: &Ui, steps: f64) {
    let emitters = emitters_of(ui, VIEWPORT);
    motion.update_screen_particles(
        ui.state.opened(),
        steps,
        ui.config.screens[ui.state.active()].wind,
        emitters.into_iter(),
    );
}

fn birch_motion() -> Motion {
    let (_, _, _, images) = load(CELLS, BIRCH_PANEL);
    let mut motion = Motion::default();
    let atlas = atlas_images(&images);
    motion.set_opaque_masks(opaque_masks(&atlas, &images, &[]));
    motion
}

fn atlas_images(images: &[ImageDecl]) -> Vec<AtlasImage> {
    images
        .iter()
        .map(|decl| {
            let verdict = verdicts()
                .into_iter()
                .find(|(name, _)| *name == decl.name)
                .map(|(_, verdict)| verdict)
                .expect("картинка теста");
            match verdict {
                ImageVerdict::Ok {
                    width,
                    height,
                    pixels,
                } => AtlasImage {
                    width,
                    height,
                    pixels,
                },
                ImageVerdict::Video { width, height, .. } => {
                    AtlasImage::video_frame(width, height / 2)
                }
                _ => unreachable!("все картинки теста — ok"),
            }
        })
        .collect()
}

#[test]
fn a_source_is_the_rectangle_of_the_panel_in_cells_of_the_screen_and_only_a_running_panel_is_one() {
    let panels = format!(
        r##"{BIRCH_PANEL},
        {{"kind":"panel","anchor":"bottom_right","offset":[40,80],"size":[200,120],"color":"#00000000","fluff":0.3}},
        {{"kind":"panel","anchor":"top_left","size":[50,50],"color":"#00000000","fluff":0}},
        {{"kind":"panel","anchor":"top_left","size":[50,50],"color":"#00000000"}}"##
    );
    let ui = birch_ui(CELLS, &panels);
    let emitters = emitters_of(&ui, VIEWPORT);
    assert_eq!(
        emitters.iter().map(|e| e.id).collect::<Vec<_>>(),
        [0, 1],
        "источник — панель со включённым эффектом, номер — номер элемента"
    );
    assert_eq!(
        emitters[0].rect,
        Some(([0.0, 0.0], [10.0, 7.5])),
        "400 × 300 точек при клетке 40"
    );
    assert_eq!(
        emitters[1].rect,
        Some((
            [(800.0 - 40.0 - 200.0) / 40.0, (600.0 - 80.0 - 120.0) / 40.0],
            [5.0, 3.0]
        ))
    );
    assert!(
        emitters[0].picture.is_some(),
        "листья берёзы рвутся из её картинки"
    );
    assert!(
        emitters[1].picture.is_none(),
        "у панели с цветом — из любой точки"
    );
    assert_eq!((emitters[0].layer, emitters[0].parallax), (0, 1.0));
}

#[test]
fn the_particles_of_a_screen_are_warmed_up_on_the_first_frame_and_fly_without_a_world() {
    let ui = birch_ui(CELLS, BIRCH_PANEL);
    assert!(!ui.game.has_world(), "на экране меню мира нет");
    let mut motion = birch_motion();
    update(&mut motion, &ui, 0.0);
    let sprites = |motion: &Motion| {
        motion
            .screen_particles()
            .live_sprites(0, 0)
            .collect::<Vec<_>>()
    };
    let shapes: Vec<ParticleShape> = sprites(&motion).iter().map(|s| s.shape).collect();
    assert!(shapes.contains(&ParticleShape::Leaf), "листья уже летят");
    assert!(shapes.contains(&ParticleShape::Fluff), "пух уже висит");
    assert!(
        shapes.contains(&ParticleShape::Butterfly),
        "бабочки уже порхают"
    );

    let before: Vec<[f64; 2]> = sprites(&motion).iter().map(|s| s.center).collect();
    update(&mut motion, &ui, 30.0);
    let after: Vec<[f64; 2]> = sprites(&motion).iter().map(|s| s.center).collect();
    assert_ne!(
        before, after,
        "частицы идут временем окна, мир тут ни при чём"
    );
    assert!(!ui.game.has_world());
}

#[test]
fn the_particles_go_on_while_a_paused_world_stands_still() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"elements":[
            {"kind":"button","anchor":"bottom_right","size":[50,50],"text":"","font":"ui","color":"#808080","on_click":["new_game","game"]}]},
        {"name":"game","world_runs":true,"keys":{"Escape":["show_screen","pause"]},"elements":[]},
        {"name":"pause","world_runs":false,"cell_pixels":40,"elements":[
            {"kind":"panel","anchor":"top_left","size":[400,300],"color":"#00000000","fluff":1}]}
    ]}"##;
    let (game, config, _, _) = try_load_full(&game_json(""), CAR_SCENE, screens)
        .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    let mut ui = Ui::new(game, config);
    ui.click(780.0, 580.0);
    ui.ticks(10, 1.0 / 60.0);
    ui.key("Escape");
    assert_eq!(ui.config.screens[ui.state.active()].name, "pause");
    assert!(ui.game.has_world(), "на паузе мир есть и стоит");
    let steps = ui.game.step_count();

    let mut motion = Motion::default();
    update(&mut motion, &ui, 600.0);
    let first = motion.screen_particles().count();
    assert!(first > 30, "пух на паузе: {first}");
    let at: Vec<[f64; 2]> = motion
        .screen_particles()
        .live_sprites(0, 0)
        .map(|s| s.center)
        .collect();
    update(&mut motion, &ui, 660.0);
    let moved: Vec<[f64; 2]> = motion
        .screen_particles()
        .live_sprites(0, 0)
        .map(|s| s.center)
        .collect();
    assert_ne!(at, moved);
    assert_eq!(ui.game.step_count(), steps, "мир не сделал ни шага");
}

#[test]
fn a_screen_that_changes_drops_the_old_particles_and_the_new_one_is_warm() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"cell_pixels":40,"elements":[
            {"kind":"panel","anchor":"top_left","size":[400,300],"color":"#00000000","fluff":1}]},
        {"name":"other","world_runs":false,"cell_pixels":40,"elements":[
            {"kind":"panel","anchor":"top_left","size":[400,300],"color":"#00000000","butterflies":1}]},
        {"name":"game","world_runs":true,"elements":[]}
    ]}"##;
    let (game, config, _, _) = try_load_full(&game_json(""), CAR_SCENE, screens)
        .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    let mut ui = Ui::new(game, config);
    let mut motion = Motion::default();
    update(&mut motion, &ui, 600.0);
    let shapes = |motion: &Motion| {
        let mut shapes: Vec<ParticleShape> = motion
            .screen_particles()
            .live_sprites(0, 0)
            .map(|s| s.shape)
            .collect();
        shapes.dedup();
        shapes
    };
    assert_eq!(shapes(&motion), [ParticleShape::Fluff]);

    ui.show("other");
    update(&mut motion, &ui, 606.0);
    assert!(
        motion
            .screen_particles()
            .live_sprites(0, 0)
            .all(|s| s.shape == ParticleShape::Butterfly),
        "частиц прежнего экрана нет"
    );
    assert!(
        motion.screen_particles().count() > 0,
        "а у нового источники прогреты"
    );
    assert_eq!(motion.screen_particles().orphan_sprites().count(), 0);

    ui.show("menu");
    update(&mut motion, &ui, 612.0);
    assert_eq!(
        shapes(&motion),
        [ParticleShape::Fluff],
        "экран открыт заново — источники снова прогреты"
    );
}

#[test]
fn a_leaf_falls_down_to_the_bottom_edge_of_its_panel_and_the_wind_of_the_screen_carries_it() {
    let leaves = |wind: &str| {
        let panel = r##"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[400,300],"image":"birch","leaf_fall":1}"##;
        let ui = birch_ui(&format!(r##""cell_pixels":40,{wind}"##), panel);
        let mut motion = birch_motion();
        let mut mean_x = Vec::new();
        for second in 0..120u32 {
            update(&mut motion, &ui, 60.0 * 100.0 + 60.0 * f64::from(second));
            let sprites: Vec<_> = motion.screen_particles().live_sprites(0, 0).collect();
            for sprite in &sprites {
                assert!(
                    sprite.center[1] <= 7.5 + 1e-9,
                    "лист не падает ниже нижнего края панели: {}",
                    sprite.center[1]
                );
            }
            if second > 40 && !sprites.is_empty() {
                mean_x
                    .push(sprites.iter().map(|s| s.center[0]).sum::<f64>() / sprites.len() as f64);
            }
        }
        let lying = motion
            .screen_particles()
            .live_sprites(0, 0)
            .filter(|s| (s.center[1] - 7.5).abs() < 1e-9)
            .count();
        (lying, mean_x.iter().sum::<f64>() / mean_x.len() as f64)
    };
    let (lying, calm) = leaves("");
    assert!(lying > 0, "листья ложатся на нижний край панели");
    let (_, windy) = leaves(r##""wind":[2.5,0],"##);
    assert!(
        windy > calm + 1.0,
        "ветер экрана сносит листья: {calm} → {windy}"
    );
}

#[test]
fn leaves_are_torn_from_the_opaque_points_of_the_panel_picture_only() {
    let panel = r##"{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[400,300],"image":"birch","leaf_fall":1}"##;
    let ui = birch_ui(CELLS, panel);
    let emitters = emitters_of(&ui, VIEWPORT);
    let picture = emitters[0].picture.expect("картинка панели");
    assert_eq!(picture.origin, [0.0, 0.0]);
    assert_eq!(picture.across, [10.0, 0.0]);
    assert_eq!(picture.down, [0.0, 7.5]);

    let mut motion = birch_motion();
    motion.update_screen_particles(0, 0.0, [0.0; 2], std::iter::empty());
    for step in 1..=90 {
        update(&mut motion, &ui, f64::from(step));
    }
    let leaves: Vec<_> = motion.screen_particles().live_sprites(0, 0).collect();
    assert!(leaves.len() > 3, "листья пошли: {}", leaves.len());
    for leaf in &leaves {
        assert!(
            (5.0 - 1.0..=10.0 + 1.5).contains(&leaf.center[0]),
            "лист оторван от правой, непрозрачной половины картинки: {}",
            leaf.center[0]
        );
    }
}

#[test]
fn the_particles_grow_and_shrink_with_the_interface() {
    let screens = screens_json_at(r##""ui_reference":[1920,1080],"##, CELLS, BIRCH_PANEL);
    let (game, config, _, _) = try_load_full(&game_json(""), CAR_SCENE, &screens)
        .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    let ui = Ui::new(game, config);
    let mut motion = birch_motion();
    update(&mut motion, &ui, 0.0);
    let butterfly = motion
        .screen_particles()
        .live_sprites(0, 0)
        .find(|s| s.shape == ParticleShape::Butterfly)
        .expect("бабочка");
    let points = screen_particle_paint(&butterfly, 40.0).size[0];
    assert!(
        (points - butterfly.width as f32 * 40.0).abs() < 1e-4 && (34.0..=40.0).contains(&points),
        "кадр бабочки в клетку, крылья в нём — полклетки, 20 точек screens.json: {points}"
    );
    let on_screen = |window: [f32; 2]| points * ui.config.ui_frame(window).scale;
    let big = on_screen([1920.0, 1080.0]);
    let small = on_screen([1280.0, 720.0]);
    assert!((big / small - 1.5).abs() < 1e-4, "{big} и {small}");
    assert!(
        (small - points * 2.0 / 3.0).abs() < 1e-3,
        "в окне 1280 × 720 — две трети"
    );

    let wide = emitters_of(&ui, [1280.0, 720.0]);
    let tall = emitters_of(&ui, [1920.0, 1080.0]);
    assert_eq!(
        wide[0].rect, tall[0].rect,
        "панель в клетках не зависит от окна"
    );
}

#[test]
fn a_panel_beyond_the_window_still_has_a_source() {
    let panel = r##"{"kind":"panel","anchor":"top_left","offset":[700,500],"size":[400,300],"color":"#00000000","fluff":1}"##;
    let ui = birch_ui(CELLS, panel);
    let emitters = emitters_of(&ui, VIEWPORT);
    assert_eq!(emitters.len(), 1);
    let ([x, y], [w, h]) = emitters[0].rect.unwrap();
    assert!(x * 40.0 + w * 40.0 > 800.0 && y * 40.0 + h * 40.0 > 600.0);
    let mut motion = Motion::default();
    update(&mut motion, &ui, 0.0);
    assert!(motion.screen_particles().count() > 0);
}

// -------------------------------------------------------------------------------------------
// Порядок рисования
// -------------------------------------------------------------------------------------------

#[test]
fn the_particles_of_a_panel_are_drawn_right_over_it_before_the_next_element_of_the_list() {
    let panels = r##"{"kind":"panel","anchor":"top_left","size":[400,300],"color":"#102030","fluff":1,"name":"back"},
        {"kind":"panel","anchor":"top_left","offset":[100,100],"size":[100,100],"color":"#ff0000"},
        {"kind":"panel","anchor":"top_left","size":[400,300],"color":"#00000000","butterflies":1,"show_on_hover":"back"},
        {"kind":"panel","anchor":"top_left","offset":[0,0],"size":[10,10],"color":"#ffffff","leaf_fall":1}"##;
    let ui = birch_ui(CELLS, panels);
    let mut motion = Motion::default();
    update(&mut motion, &ui, 600.0);

    let screen_id = ui.state.active();
    let frame = ui.config.ui_frame(VIEWPORT);
    let parts = interface::layout(
        &ui.config.screens[screen_id],
        screen_id,
        &ui.game.world,
        &ui.state,
        frame.viewport,
    );
    let paints: Vec<(usize, _)> =
        screen_particle_paints(&parts, None, motion.screen_particles(), 40.0).collect();
    assert!(!paints.is_empty());
    assert!(
        paints.windows(2).all(|pair| pair[0].0 <= pair[1].0),
        "по порядку списка"
    );
    assert!(
        paints.iter().all(|(index, _)| *index != 2),
        "панель с show_on_hover, пока курсор не над местом, и её частицы не рисуются"
    );
    assert!(
        paints
            .iter()
            .any(|(index, paint)| *index == 0 && paint.atlas_rect == FLUFF_RECT)
    );
    assert!(
        paints.iter().all(|(index, _)| *index != 1),
        "у красной панели частиц нет"
    );
    let over_place = screen_particle_paints(&parts, Some(0), motion.screen_particles(), 40.0);
    assert!(
        over_place
            .into_iter()
            .any(|(index, paint)| index == 2 && paint.atlas_rect.x >= BUTTERFLY_RECT.x),
        "курсор над местом — панель горит, и её бабочки рисуются сразу после неё"
    );
}

#[test]
fn a_particle_is_a_rectangle_in_screen_points_around_its_centre_with_the_glow_of_a_fluff() {
    let ui = birch_ui(CELLS, BIRCH_PANEL);
    let mut motion = birch_motion();
    update(&mut motion, &ui, 600.0);
    let sprites: Vec<_> = motion.screen_particles().live_sprites(0, 0).collect();
    let fluff = sprites
        .iter()
        .find(|s| s.shape == ParticleShape::Fluff)
        .unwrap();
    let leaf = sprites
        .iter()
        .find(|s| s.shape == ParticleShape::Leaf)
        .unwrap();
    let paint = screen_particle_paint(fluff, 40.0);
    assert!(paint.glow && paint.smooth, "пух светится и сглажен");
    assert!((paint.position[0] + paint.size[0] / 2.0 - fluff.center[0] as f32 * 40.0).abs() < 1e-3);
    assert!((paint.position[1] + paint.size[1] / 2.0 - fluff.center[1] as f32 * 40.0).abs() < 1e-3);
    assert!(!screen_particle_paint(leaf, 40.0).glow);
    assert_eq!(paint.angle, fluff.angle as f32);
}

// -------------------------------------------------------------------------------------------
// Мышь
// -------------------------------------------------------------------------------------------

/// Экран `menu` с миром, чтобы нажатие, которое не взяла панель, дошло до мира.
fn live_ui(elements: &str) -> Ui {
    let screens = format!(
        r##"{{"screens":[{{"name":"menu","world_runs":true,"cell_pixels":40,"elements":[{elements}]}}]}}"##
    );
    let (game, config, _, _) = try_load_full(&game_json(""), CAR_SCENE, &screens)
        .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    Ui::new(game, config)
}

fn place_at(ui: &Ui, point: [f32; 2]) -> Option<String> {
    let screen_id = ui.state.active();
    let screen = &ui.config.screens[screen_id];
    let frame = ui.config.ui_frame(VIEWPORT);
    let parts = interface::layout(screen, screen_id, &ui.game.world, &ui.state, frame.viewport);
    screens::hover_place(&parts, Some(frame.point(point))).map(|index| {
        screen.elements[index]
            .place_name()
            .expect("имя")
            .to_string()
    })
}

const HERO: &str = r##"{"kind":"panel","anchor":"top_left","name":"ilya","offset":[0,0],"size":[100,100],"color":"#00000000"},
    {"kind":"panel","anchor":"top_left","offset":[0,200],"size":[100,40],"color":"#ffd27a","show_on_hover":"ilya"}"##;

#[test]
fn a_source_panel_without_a_name_neither_takes_the_press_nor_covers_the_place_under_it() {
    let cloud = r##",{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[400,300],"color":"#00000000","fluff":1}"##;
    let mut ui = live_ui(&format!("{HERO}{cloud}"));
    assert_eq!(
        place_at(&ui, [50.0, 50.0]),
        Some("ilya".to_string()),
        "место под тучей пуха"
    );
    ui.press(300.0, 250.0);
    assert!(
        ui.mouse.world_captured,
        "нажатие над тучей пуха дошло до мира"
    );
    ui.release();
    assert!(
        !screens::over_interface(
            &ui.config,
            &ui.state,
            &ui.game.world,
            VIEWPORT,
            [300.0, 250.0]
        ),
        "и колесо над ней достаётся камере"
    );
}

#[test]
fn an_ordinary_panel_and_a_named_source_still_take_the_press_and_the_place() {
    let ordinary = r##",{"kind":"panel","anchor":"top_left","offset":[0,0],"size":[400,300],"color":"#102030"}"##;
    let mut ui = live_ui(&format!("{HERO}{ordinary}"));
    assert_eq!(
        place_at(&ui, [50.0, 50.0]),
        None,
        "обычная панель закрывает место"
    );
    ui.press(300.0, 250.0);
    assert!(!ui.mouse.world_captured, "и забирает нажатие у мира");
    ui.release();

    let named = r##",{"kind":"panel","anchor":"top_left","name":"cloud","offset":[0,0],"size":[400,300],"color":"#00000000","fluff":1},
        {"kind":"panel","anchor":"top_left","offset":[0,400],"size":[10,10],"color":"#ffffff","show_on_hover":"cloud"}"##;
    let mut ui = live_ui(&format!("{HERO}{named}"));
    assert_eq!(
        place_at(&ui, [50.0, 50.0]),
        Some("cloud".to_string()),
        "с name — место наведения, как раньше"
    );
    ui.press(300.0, 250.0);
    assert!(!ui.mouse.world_captured);
    ui.release();
    assert!(screens::over_interface(
        &ui.config,
        &ui.state,
        &ui.game.world,
        VIEWPORT,
        [300.0, 250.0]
    ));
}

#[test]
fn a_button_under_a_cloud_of_fluff_still_gets_its_click() {
    let screens = r##"{"screens":[
        {"name":"menu","world_runs":false,"cell_pixels":40,"elements":[
            {"kind":"button","anchor":"top_left","offset":[100,100],"size":[100,40],"text":"","font":"ui","color":"#808080","on_click":["new_game","game"]},
            {"kind":"panel","anchor":"top_left","size":[800,600],"color":"#00000000","fluff":1}]},
        {"name":"game","world_runs":true,"elements":[]}
    ]}"##;
    let (game, config, _, _) = try_load_full(&game_json(""), CAR_SCENE, screens)
        .unwrap_or_else(|e| panic!("{:#?}", e.errors));
    let mut ui = Ui::new(game, config);
    ui.click(120.0, 120.0);
    assert_eq!(ui.config.screens[ui.state.active()].name, "game");
}

// -------------------------------------------------------------------------------------------
// Пух и бабочки у объектов мира
// -------------------------------------------------------------------------------------------

/// Экран `menu` с миром: сцена собрана сразу.
const LIVE_SCREENS: &str = r##"{"screens":[{"name":"menu","world_runs":true,"elements":[]}]}"##;

fn world_load(scene: &str) -> Loaded {
    try_load_full(&game_json(""), scene, LIVE_SCREENS).unwrap_or_else(|e| panic!("{:#?}", e.errors))
}

#[test]
fn the_four_properties_are_engine_properties_of_a_world_object() {
    let scene = r##"{"objects":[{"position":[2,2],"size":[10,4],"fluff":0.3,"fluff_color":"#fff0d0",
        "butterflies":0.6,"butterfly_color":"#ff8800"}]}"##;
    let (game, _, warnings, _) = world_load(scene);
    assert_eq!(
        warnings
            .iter()
            .filter(|w| w.message.contains("частиц"))
            .count(),
        0
    );
    assert_eq!(game.world.number_like(0, property::FLUFF), Some(0.3));
    assert_eq!(game.world.number_like(0, property::BUTTERFLIES), Some(0.6));
    assert!(game.world.color(0, property::FLUFF_COLOR).is_some());
    assert!(game.world.color(0, property::BUTTERFLY_COLOR).is_some());
    for (name, id) in [
        ("fluff", property::FLUFF),
        ("fluff_color", property::FLUFF_COLOR),
        ("butterflies", property::BUTTERFLIES),
        ("butterfly_color", property::BUTTERFLY_COLOR),
    ] {
        assert_eq!(game.properties.resolve(name), Some(id), "{name}");
        assert!(property::PARTICLE_PROPERTIES.contains(&id));
    }
    let settings = Settings::read(&game.world, 0);
    assert!(settings.runs(Effect::Fluff) && settings.runs(Effect::Butterflies));
}

#[test]
fn the_new_properties_are_checked_before_the_start_like_the_others() {
    for (field, path, message) in [
        (r##""fluff":1.2"##, "fluff", "fluff: нужно от 0 до 1"),
        (r##""fluff":-1"##, "fluff", "fluff: нужно от 0 до 1"),
        (
            r##""butterflies":3"##,
            "butterflies",
            "butterflies: нужно от 0 до 1",
        ),
        (
            r##""fluff_color":"white""##,
            "fluff_color",
            "цвет должен быть вида",
        ),
        (
            r##""butterfly_color":1"##,
            "butterfly_color",
            "ожидалась строка",
        ),
    ] {
        let scene = format!(r##"{{"objects":[{{"position":[1,1],"size":[1,1],{field}}}]}}"##);
        let errors = errors_of(try_load_full(&game_json(""), &scene, &screens_json("", "")));
        assert_error(&errors, path, message);
    }
    for object in [
        r##"{"size":[1,1],"fluff":0.5}"##,
        r##"{"position":[1,1],"butterflies":0.5}"##,
    ] {
        let errors = errors_of(try_load_full(
            &game_json(""),
            &format!(r##"{{"objects":[{object}]}}"##),
            &screens_json("", ""),
        ));
        assert_error(
            &errors,
            "",
            "свойства частиц разрешены только объекту с position и size",
        );
    }
}

#[test]
fn the_new_properties_in_a_three_dimensional_scene_are_errors() {
    let flat3d = game_json(r##","view_height":12,"camera":{"pitch":55}"##);
    for field in [
        r##""fluff":0.5"##,
        r##""fluff_color":"#ffffff""##,
        r##""butterflies":0.5"##,
        r##""butterfly_color":"#ffffff""##,
    ] {
        let scene = format!(
            r##"{{"objects":[{{"position":[1,1],"size":[1,1],"color":"#ffffff",{field}}}]}}"##
        );
        let errors = errors_of(try_load_full(&flat3d, &scene, &screens_json("", "")));
        assert_error(&errors, "", "только в плоской сцене");
    }
}

#[test]
fn rules_and_code_set_the_new_properties_and_outside_a_party_the_editor_does_too() {
    let (mut game, _, _, images) =
        world_load(r##"{"objects":[{"position":[1,1],"size":[1,1],"mark":true}]}"##);
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "fluff", &json!(0.4)),
        Ok(())
    );
    assert_eq!(
        session::set_property(None, &mut game, &images, 0, "butterflies", &json!(0.2)),
        Ok(())
    );
    assert_eq!(game.world.number_like(0, property::FLUFF), Some(0.4));
    assert_eq!(game.world.number_like(0, property::BUTTERFLIES), Some(0.2));
    assert!(session::set_property(None, &mut game, &images, 0, "fluff", &json!(2)).is_err());
    assert!(
        session::set_property(
            None,
            &mut game,
            &images,
            0,
            "butterfly_color",
            &json!("red")
        )
        .is_err()
    );
    assert_eq!(
        session::set_property(
            None,
            &mut game,
            &images,
            0,
            "butterfly_color",
            &json!("#ffcc00")
        ),
        Ok(())
    );

    let rule = |value: &str| {
        format!(
            r##"{{"rules":[{{"kind":"check","for":{{"has":["mark"]}},"do":[["set","fluff",{value}]]}}]}}"##
        )
    };
    let scene = r##"{"objects":[{"position":[1,1],"size":[1,1],"mark":true,"fluff":0}]}"##;
    let load_rules = |rules: &str| {
        let game = game_json("");
        let (config, _) = read_entry(&game).unwrap();
        load_rest(
            &game,
            config,
            Some(PROPS),
            Some(scene),
            Some(rules),
            Some(LIVE_SCREENS),
            &[("ui".to_string(), Some(vec![0x00, 0x01, 0x00, 0x00]))],
            &[],
            &[],
            &verdicts(),
            None,
            false,
        )
    };
    let (mut ruled, ..) = load_rules(&rule("0.8")).expect("0,8 годится");
    ruled.step(StepInput::empty());
    assert_eq!(ruled.world.number_like(0, property::FLUFF), Some(0.8));
    let errors = errors_of(load_rules(&rule("1.5")));
    assert_error(&errors, "", "fluff: нужно от 0 до 1");
}

/// Мир и его движение, как их ведёт страница: часы доведены, частицы посчитаны.
struct Meadow {
    game: Game,
    images: Vec<ImageDecl>,
    rects: Vec<AtlasRect>,
    motion: Motion,
}

impl Meadow {
    fn new(scene: &str) -> Meadow {
        let (game, _, _, images) = world_load(scene);
        let atlas = atlas_images(&images);
        let rects = pack(&atlas).expect("умещаются").rects;
        let mut motion = Motion::default();
        motion.set_opaque_masks(opaque_masks(&atlas, &images, &[]));
        Meadow {
            game,
            images,
            rects,
            motion,
        }
    }

    fn settle(&mut self) {
        let wind = self.game.wind();
        self.motion.update_particles(
            self.game.has_world(),
            wind,
            particle_emitters(&self.game.world, &self.images),
        );
    }

    fn paints(&self) -> Vec<engine::render::atlas::RectPaint> {
        compose_world_paints(
            &self.game.world,
            &self.game.scene,
            self.game.world.ids(),
            &self.motion,
            &self.images,
            &self.rects,
            &engine::core::scene::LayerView::default(),
        )
    }
}

#[test]
fn a_meadow_from_scene_json_has_fluff_and_butterflies_from_the_first_frame_and_draws_them() {
    let scene = r##"{"objects":[{"position":[2,2],"size":[16,6],"fluff":0.5,"butterflies":0.8}]}"##;
    let mut meadow = Meadow::new(scene);
    meadow.settle();
    let paints = meadow.paints();
    let fluff: Vec<_> = paints
        .iter()
        .filter(|p| p.atlas_rect == FLUFF_RECT)
        .collect();
    let butterflies: Vec<_> = paints
        .iter()
        .filter(|p| {
            p.atlas_rect.x >= BUTTERFLY_RECT.x
                && p.atlas_rect.x < BUTTERFLY_RECT.x + BUTTERFLY_RECT.w
        })
        .collect();
    assert!(
        fluff.len() > 100,
        "{}: полпушинки на клетку, 96 клеток",
        fluff.len()
    );
    assert!(!butterflies.is_empty(), "бабочки уже летят");
    assert!(fluff.iter().all(|p| p.glow && p.smooth), "пух светится");
    assert!(butterflies.iter().all(|p| !p.glow));
    assert!(
        butterflies
            .iter()
            .all(|p| (0.85..=1.0).contains(&p.size[0])),
        "кадр бабочки около клетки — нарисованный размах около половины клетки"
    );
}

#[test]
fn a_meadow_lit_by_a_rule_or_by_the_editor_starts_from_nothing() {
    let scene = r##"{"objects":[{"position":[2,2],"size":[16,6],"mark":true}]}"##;
    let mut meadow = Meadow::new(scene);
    meadow.settle();
    assert_eq!(meadow.motion.particles().count(), 0);
    meadow.game.world.set_number(0, property::FLUFF, 0.5);
    meadow.game.world.set_number(0, property::BUTTERFLIES, 0.5);
    meadow.settle();
    assert_eq!(
        meadow.motion.particles().count(),
        0,
        "загорелось — начинает с нуля"
    );
    meadow.motion.tick(None, 0.1);
    meadow.settle();
    for _ in 0..200 {
        meadow.motion.tick(None, 0.1);
        meadow.settle();
    }
    assert!(meadow.motion.particles().count() > 20);

    meadow.game.world.set_number(0, property::FLUFF, 0.0);
    meadow.game.world.set_number(0, property::BUTTERFLIES, 0.0);
    meadow.settle();
    let shapes: Vec<ParticleShape> = meadow
        .motion
        .particles()
        .orphan_sprites()
        .map(|(_, sprite)| sprite.shape)
        .collect();
    assert!(!shapes.is_empty(), "вылетевшие доживают");
    for _ in 0..300 {
        meadow.motion.tick(None, 0.1);
        meadow.settle();
    }
    assert_eq!(meadow.motion.particles().count(), 0, "и больше не вылетают");
}

#[test]
fn the_wind_of_the_scene_carries_a_fluff_cloud_downwind_more_than_it_would_without() {
    let drift = |wind: &str| {
        let scene = format!(
            r##"{{"wind":{wind},"objects":[{{"position":[2,2],"size":[16,6],"fluff":1}}]}}"##
        );
        let (world, ..) = world_load(&scene);
        let mut particles = Particles::default();
        let around = engine::render::particles::Emitter {
            id: 0,
            generation: 0,
            rect: Some(([2.0, 2.0], [16.0, 6.0])),
            layer: 0,
            parallax: 1.0,
            settings: Settings::read(&world.world, 0),
            picture: None,
        };
        let flat = world.wind();
        let mut total = 0.0;
        let mut samples = 0.0;
        for step in 0..60 * 40u64 {
            particles.update((step + 1) as f64, true, flat, [around].into_iter());
            if step > 60 * 14 && step % 60 == 0 {
                let live: Vec<f64> = particles.live_sprites(0, 0).map(|s| s.center[0]).collect();
                total += live.iter().sum::<f64>() / live.len() as f64;
                samples += 1.0;
            }
        }
        total / samples
    };
    let calm = drift("[0,0]");
    let windy = drift("[1,0]");
    assert!(
        windy - calm > 2.0,
        "пушинка летит вровень с ветром: {calm} → {windy}"
    );
}

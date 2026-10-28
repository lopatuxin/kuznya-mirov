//! Фаза 47 — рамки, шкалы, подсказки, шрифты и масштаб интерфейса: шкала `bar` на экране и в строке
//! списка, подсказка `tooltip` и её вид, настройки шрифта в `files.fonts`, `slice` у картинки, `image_scale`,
//! `ui_reference` и проверки перед запуском. Игра собрана из текстов, её ведёт `ui_support::Ui` теми же
//! вызовами, что `wasm::Engine`.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::game::Game;
use engine::core::interface::{self, Part, TooltipHover, TooltipShow};
use engine::core::screens::{Element, ScreensConfig, TooltipStyle};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    FontSettings, ImageDecl, ImageVerdict, load_rest_with_tables, read_entry, read_texts,
};
use engine::render::atlas::{self, AtlasRect, ImagePlace};
use support::{Ui, VIEWPORT};

const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];

const PROPS: &str = r#"{"properties":{"car":"flag","fuel":"number","tank":"number",
    "selected":"flag","plate":"text"}}"#;

// Машина с бензином 20 из 40 выбрана; у второй бензина больше бака, у третьей меньше нуля, у четвёртой его
// нет вовсе.
const SCENE: &str = r#"{"objects":[
    {"name":"c1","car":true,"fuel":20,"tank":40,"selected":true,"plate":"А001"},
    {"name":"c2","car":true,"fuel":50,"tank":40,"plate":"А002"},
    {"name":"c3","car":true,"fuel":-5,"tank":40,"plate":"А003"},
    {"name":"c4","car":true,"tank":40,"plate":"А004"}
]}"#;

const FONTS: &str = r#""ui":"ui.ttf""#;

/// Рамка окна 64 × 64 с полями 16, табличка 300 × 300 с полями 88, заливка шкалы 40 × 13 с полями 6.
const IMAGES: &str = r#""frame":{"path":"frame.png","slice":[16,16,16,16],"smooth":true},
    "plaque":{"path":"plaque.png","slice":[88,88,88,88]},
    "gauge":{"path":"gauge.png","slice":[6,6,6,6]},
    "icon":{"path":"icon.png"}"#;

const IMAGE_FILES: &[(&str, u32, u32)] = &[
    ("frame", 64, 64),
    ("plaque", 300, 300),
    ("gauge", 40, 13),
    ("icon", 16, 16),
];

const TOOLTIP: &str = r##""tooltip":{"color":"#202020ee","font":"ui","font_size":16,
    "text_color":"#efe9dc","padding":[12,5],"gap":8,"delay":0.4}"##;

/// Элементы обоих экранов: окно с рамкой и подсказкой, кнопка с табличкой и подсказкой под значком без
/// неё, шкала бензина выбранной машины, список машин со шкалой в строке и кнопка в правом верхнем углу.
const ELEMENTS: &str = r##"[
    {"kind":"panel","anchor":"top_left","offset":[20,20],"size":[353,200],"image":"frame",
     "image_scale":0.5,"tooltip":"Окно"},
    {"kind":"button","anchor":"top_left","offset":[100,500],"size":[60,52],"image":"plaque",
     "image_scale":0.35,"text":"","font":"ui","tooltip":"Заказы","on_click":["show_screen","pause"]},
    {"kind":"panel","anchor":"top_left","offset":[110,510],"size":[40,32],"image":"icon"},
    {"kind":"bar","anchor":"top_left","offset":[400,20],"size":[168,13],"for":{"has":["selected"]},
     "value":"fuel","max":40,"image":"gauge","back_color":"#00000080","tooltip":"Бензин {fuel}"},
    {"kind":"list","anchor":"top_left","offset":[400,60],"size":[300,200],"for":{"has":["car"]},
     "row_height":40,"row":[
        {"kind":"label","offset":[0,0],"size":[100,20],"text":"{plate}","font":"ui"},
        {"kind":"bar","offset":[0,24],"size":[168,13],"value":"fuel","max":"tank",
         "color":"#3d7fd9","tooltip":"Бензин {plate}"}
     ]},
    {"kind":"button","anchor":"top_right","offset":[0,0],"size":[40,40],"color":"#808080",
     "text":"×","font":"ui","tooltip":"Закрыть","on_click":["show_screen","main"]}
]"##;

fn screens_json(head: &str) -> String {
    format!(
        r#"{{{head}"screens":[
    {{"name":"main","world_runs":true,"elements":{ELEMENTS}}},
    {{"name":"pause","world_runs":false,"elements":{ELEMENTS}}}
]}}"#
    )
}

fn game_json(fonts: &str, images: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{{fonts}}},"images":{{{images}}}}}}}"##
    )
}

fn image_verdicts(files: &[(&str, u32, u32)]) -> Vec<(String, ImageVerdict)> {
    files
        .iter()
        .map(|(name, width, height)| {
            (
                (*name).to_string(),
                ImageVerdict::Ok {
                    width: *width,
                    height: *height,
                    pixels: vec![255; (*width * *height * 4) as usize],
                },
            )
        })
        .collect()
}

type Loaded = (Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>);

/// Загрузка игры: шрифт `ui.ttf` под первым из объявленных имён, картинки `image_files`.
fn try_load_with(
    fonts: &str,
    images: &str,
    screens: &str,
    image_files: &[(&str, u32, u32)],
) -> Result<Loaded, LoadFailure> {
    let game_json = game_json(fonts, images);
    let (config, _) = read_entry(&game_json)?;
    let font_bytes: Vec<(String, Option<Vec<u8>>)> =
        read_texts(&config, Some(PROPS), Some(screens), None)
            .fonts
            .into_iter()
            .map(|(name, _)| (name, Some(FONT_BYTES.to_vec())))
            .collect();
    load_rest_with_tables(
        &game_json,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(support::NO_RULES),
        Some(screens),
        &font_bytes,
        &[],
        &[],
        &image_verdicts(image_files),
        None,
        false,
        &[],
    )
}

fn try_load(screens: &str) -> Result<Loaded, LoadFailure> {
    try_load_with(FONTS, IMAGES, screens, IMAGE_FILES)
}

fn load_ui(screens: &str) -> (Ui, Vec<ImageDecl>) {
    match try_load(screens) {
        Ok((game, config, _warnings, images)) => (Ui::new(game, config), images),
        Err(failure) => panic!("игра не загрузилась: {:#?}", failure.errors),
    }
}

fn load() -> (Ui, Vec<ImageDecl>) {
    load_ui(&screens_json(&format!("{TOOLTIP},")))
}

fn parts(ui: &Ui) -> Vec<Part<'_>> {
    let screen = ui.state.active();
    let frame = ui.config.ui_frame(VIEWPORT);
    interface::layout(
        &ui.config.screens[screen],
        screen,
        &ui.game.world,
        &ui.state,
        frame.viewport,
    )
}

fn style(ui: &Ui) -> TooltipStyle {
    ui.config.tooltip.expect("блок tooltip")
}

/// Подсказка кадра в момент `now` с курсором в `pointer` — так её спрашивает `wasm::compose_ui`.
fn tooltip_at(ui: &Ui, hover: &mut TooltipHover, pointer: [f32; 2], now: f64) -> Option<String> {
    let parts = parts(ui);
    hover
        .show(
            &parts,
            ui.state.active(),
            Some(pointer),
            &ui.mouse,
            now,
            &style(ui),
            &ui.game.world,
            &ui.game.properties,
        )
        .map(|TooltipShow { text, .. }| text)
}

fn errors_of(result: Result<Loaded, LoadFailure>) -> Vec<String> {
    match result {
        Ok(_) => Vec::new(),
        Err(failure) => failure
            .errors
            .into_iter()
            .map(|e| format!("{} — {}", e.path, e.message))
            .collect(),
    }
}

fn expect_error(result: Result<Loaded, LoadFailure>, fragment: &str) {
    let errors = errors_of(result);
    assert!(
        errors.iter().any(|message| message.contains(fragment)),
        "ожидалась ошибка с «{fragment}», есть {errors:?}"
    );
}

fn screens_with(head: &str, element: &str) -> String {
    format!(r#"{{{head}"screens":[{{"name":"main","world_runs":true,"elements":[{element}]}}]}}"#)
}

fn expect_screen_error(element: &str, fragment: &str) {
    expect_error(
        try_load(&screens_with(&format!("{TOOLTIP},"), element)),
        fragment,
    );
}

#[test]
fn the_fixture_loads_without_warnings() {
    let (_, _, warnings, _) = try_load(&screens_with(
        &format!("{TOOLTIP},"),
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"frame"},
            {"kind":"panel","anchor":"top_left","size":[10,10],"image":"plaque"},
            {"kind":"panel","anchor":"top_left","size":[10,10],"image":"gauge"},
            {"kind":"panel","anchor":"top_left","size":[10,10],"image":"icon","tooltip":"{c1.plate}"},
            {"kind":"bar","anchor":"top_left","size":[10,10],"for":{"has":["selected"]},"value":"fuel",
             "max":"tank","color":"#ffffff"}"##,
    ))
    .unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
    assert_eq!(warnings, Vec::new());
    let (ui, images) = load();
    assert_eq!(ui.config.screens.len(), 2);
    let slice_of = |name: &str| images.iter().find(|decl| decl.name == name).unwrap().slice;
    assert_eq!(slice_of("frame"), Some([16, 16, 16, 16]));
    assert_eq!(slice_of("icon"), None);
}

// -------------------------------------------------------------------------------------------
// Шкала
// -------------------------------------------------------------------------------------------

/// Шкала строки списка машины `object` и её прямоугольник.
fn row_bar<'a>(parts: &'a [Part<'a>], object: u32) -> (&'a Element, [f32; 4]) {
    parts
        .iter()
        .find_map(|part| match part {
            Part::Element(placed)
                if placed.object == Some(object)
                    && matches!(placed.element, Element::Bar { .. }) =>
            {
                Some((placed.element, placed.rect))
            }
            _ => None,
        })
        .expect("шкала в строке")
}

#[test]
fn a_bar_fills_its_share_from_the_left_clamped_to_zero_and_one() {
    let (ui, _) = load();
    let parts = parts(&ui);
    let share = |name: &str| {
        let id = ui.id_of(name);
        let (element, rect) = row_bar(&parts, id);
        let share = interface::bar_share(element, Some(id), &ui.game.world);
        (share, share.and_then(|s| interface::bar_fill_rect(rect, s)))
    };
    // «Бензин 20 из 40, шкала 168 × 13 — заливка 84 × 13», max — свойство tank.
    let (half, fill) = share("c1");
    assert_eq!(half, Some(0.5));
    assert_eq!(fill.map(|rect| [rect[2], rect[3]]), Some([84.0, 13.0]));
    assert_eq!(share("c2").0, Some(1.0), "больше max — полная");
    assert_eq!(share("c3"), (Some(0.0), None), "меньше нуля — без заливки");
    assert_eq!(share("c4").0, None, "нет value — шкалы нет");
}

#[test]
fn a_bar_on_the_screen_reads_the_object_of_its_selector_with_a_constant_max() {
    let (mut ui, _) = load();
    let parts_now = parts(&ui);
    let (element, _) = parts_now
        .iter()
        .find_map(|part| match part {
            Part::Element(placed) if placed.key.element == 3 => {
                Some((placed.element, placed.object))
            }
            _ => None,
        })
        .expect("шкала выбранной машины видна");
    let c1 = ui.id_of("c1");
    assert_eq!(
        interface::bar_share(element, Some(c1), &ui.game.world),
        Some(0.5)
    );
    // Код или правило сменили бензин — шкала со следующего кадра другая.
    let fuel = ui.game.properties.resolve("fuel").unwrap();
    ui.game.world.set_number(c1, fuel, 30.0);
    let parts_after = parts(&ui);
    let Some(Part::Element(placed)) = parts_after
        .iter()
        .find(|part| matches!(part, Part::Element(placed) if placed.key.element == 3))
    else {
        panic!("шкала на месте");
    };
    assert_eq!(
        interface::bar_share(placed.element, placed.object, &ui.game.world),
        Some(0.75)
    );
}

/// «Картинка заливки растягивается по своим полям на ширину заливки»: у короткой заливки углы сжаты.
#[test]
fn an_image_fill_squeezes_its_corners_when_the_fill_is_short() {
    let (ui, images) = load();
    let Element::Bar { fill, .. } = &ui.config.screens[0].elements[3] else {
        panic!("четвёртый элемент — шкала");
    };
    let atlas_rects: Vec<ImagePlace> = images
        .iter()
        .map(|decl| {
            let (_, w, h) = IMAGE_FILES
                .iter()
                .find(|(name, _, _)| *name == decl.name)
                .expect("файл картинки");
            ImagePlace::Whole(AtlasRect {
                x: 0,
                y: 0,
                w: *w,
                h: *h,
                sheet: 0,
            })
        })
        .collect();
    let mut paints = Vec::new();
    // 5 % от 168 — 8,4 точки: углы по 6 сжаты до 4,2, середины нет.
    let short = interface::bar_fill_rect([0.0, 0.0, 168.0, 13.0], 0.05).unwrap();
    atlas::ui_fill_paints(fill, short, 1.0, 0.0, &images, &atlas_rects, &mut paints);
    let widths: Vec<f32> = paints
        .iter()
        .filter(|paint| paint.paint.position[1] == 0.0)
        .map(|paint| paint.paint.size[0])
        .collect();
    assert_eq!(widths.len(), 2, "только два угла: {widths:?}");
    assert!(widths.iter().all(|w| (w - 4.2).abs() < 1e-4), "{widths:?}");
    // Полная заливка — концы своей ширины.
    paints.clear();
    atlas::ui_fill_paints(
        fill,
        [0.0, 0.0, 168.0, 13.0],
        1.0,
        0.0,
        &images,
        &atlas_rects,
        &mut paints,
    );
    assert!(
        paints
            .iter()
            .any(|paint| paint.paint.position == [0.0, 0.0] && paint.paint.size == [6.0, 6.0])
    );
}

#[test]
fn a_bar_is_transparent_for_clicks() {
    let (mut ui, _) = load();
    // Под шкалой выбранной машины нет ничего — нажатие уходит миру, как у надписи.
    ui.press(410.0, 25.0);
    assert!(ui.mouse.world_captured);
    ui.release();
}

// -------------------------------------------------------------------------------------------
// Подсказка
// -------------------------------------------------------------------------------------------

const ON_BUTTON: [f32; 2] = [105.0, 545.0];
const ON_ICON: [f32; 2] = [120.0, 520.0];
const ON_FRAME: [f32; 2] = [30.0, 30.0];

#[test]
fn a_tooltip_appears_after_its_delay_of_window_time() {
    let (ui, _) = load();
    let mut hover = TooltipHover::default();
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 10.0), None);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 10.3), None);
    assert_eq!(
        tooltip_at(&ui, &mut hover, ON_BUTTON, 10.45).as_deref(),
        Some("Заказы")
    );
}

#[test]
fn a_tooltip_appears_on_a_paused_screen_where_the_world_stands() {
    let (mut ui, _) = load();
    ui.show("pause");
    assert!(!ui.state.is_live(&ui.config));
    let mut hover = TooltipHover::default();
    tooltip_at(&ui, &mut hover, ON_FRAME, 0.0);
    let step = ui.game.step_count();
    assert_eq!(
        tooltip_at(&ui, &mut hover, ON_FRAME, 1.0).as_deref(),
        Some("Окно")
    );
    assert_eq!(ui.game.step_count(), step, "мир не шагнул");
}

#[test]
fn an_icon_without_a_tooltip_over_a_button_does_not_hide_the_buttons_one() {
    let (ui, _) = load();
    let mut hover = TooltipHover::default();
    tooltip_at(&ui, &mut hover, ON_ICON, 0.0);
    assert_eq!(
        tooltip_at(&ui, &mut hover, ON_ICON, 0.5).as_deref(),
        Some("Заказы")
    );
}

#[test]
fn the_topmost_element_with_a_tooltip_gives_it_and_substitutes_its_own_object() {
    let (ui, _) = load();
    let mut hover = TooltipHover::default();
    // Шкала второй строки списка — объект c2.
    let point = [410.0, 60.0 + 40.0 + 24.0 + 5.0];
    tooltip_at(&ui, &mut hover, point, 0.0);
    assert_eq!(
        tooltip_at(&ui, &mut hover, point, 0.5).as_deref(),
        Some("Бензин А002")
    );
    // У c4 нет бензина: его шкалы не рисуют — и подсказки она не даёт.
    let hidden = [410.0, 60.0 + 3.0 * 40.0 + 24.0 + 5.0];
    tooltip_at(&ui, &mut hover, hidden, 1.0);
    assert_eq!(tooltip_at(&ui, &mut hover, hidden, 2.0), None);
}

#[test]
fn leaving_hides_the_tooltip_at_once_and_another_element_waits_again() {
    let (ui, _) = load();
    let mut hover = TooltipHover::default();
    tooltip_at(&ui, &mut hover, ON_BUTTON, 0.0);
    assert!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.5).is_some());
    assert_eq!(tooltip_at(&ui, &mut hover, [5.0, 590.0], 0.6), None);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_FRAME, 0.7), None);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_FRAME, 1.0), None);
    assert_eq!(
        tooltip_at(&ui, &mut hover, ON_FRAME, 1.15).as_deref(),
        Some("Окно")
    );
}

#[test]
fn a_press_hides_the_tooltip_until_the_cursor_leaves_the_element() {
    let (mut ui, _) = load();
    let mut hover = TooltipHover::default();
    ui.move_to(ON_FRAME[0], ON_FRAME[1]);
    tooltip_at(&ui, &mut hover, ON_FRAME, 0.0);
    assert!(tooltip_at(&ui, &mut hover, ON_FRAME, 0.5).is_some());
    ui.press(ON_FRAME[0], ON_FRAME[1]);
    ui.release();
    assert_eq!(tooltip_at(&ui, &mut hover, ON_FRAME, 0.6), None);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_FRAME, 5.0), None);
    tooltip_at(&ui, &mut hover, [5.0, 590.0], 5.1);
    tooltip_at(&ui, &mut hover, ON_FRAME, 5.2);
    assert!(tooltip_at(&ui, &mut hover, ON_FRAME, 5.7).is_some());
}

/// Требование 18: щелчок по кнопке, после которого на её месте стоит другая кнопка (синяя табличка
/// сменилась кремовой, здесь — тот же элемент на другом экране), не возвращает подсказку: курсор с места
/// не уходил.
#[test]
fn a_press_that_swaps_the_button_in_place_keeps_its_tooltip_hidden() {
    let (mut ui, _) = load();
    let mut hover = TooltipHover::default();
    ui.move_to(ON_BUTTON[0], ON_BUTTON[1]);
    tooltip_at(&ui, &mut hover, ON_BUTTON, 0.0);
    assert!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.5).is_some());
    // Нажатие и отпускание — в разных кадрах: кнопку на месте сменяет команда отпускания.
    ui.press(ON_BUTTON[0], ON_BUTTON[1]);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.55), None);
    ui.release();
    assert!(!ui.state.is_live(&ui.config), "кнопка открыла экран паузы");
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.6), None);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 5.0), None);
    tooltip_at(&ui, &mut hover, [5.0, 590.0], 5.1);
    tooltip_at(&ui, &mut hover, ON_BUTTON, 5.2);
    assert!(tooltip_at(&ui, &mut hover, ON_BUTTON, 5.7).is_some());
}

/// Требование 18: другой элемент на том же месте без щелчка — экран сменился не щелчком — снова ждёт
/// задержку, как любой другой элемент; и тогда, когда подсказку прежнего элемента спрятало нажатие.
#[test]
fn another_element_in_the_same_place_without_a_click_waits_again() {
    let (mut ui, _) = load();
    let mut hover = TooltipHover::default();
    ui.move_to(ON_BUTTON[0], ON_BUTTON[1]);
    tooltip_at(&ui, &mut hover, ON_BUTTON, 0.0);
    assert!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.5).is_some());
    ui.show("pause");
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.6), None);
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 0.9), None);
    assert!(tooltip_at(&ui, &mut hover, ON_BUTTON, 1.05).is_some());

    // Нажатие прячет подсказку, кнопка закрытия уводит на основной экран; затем экран сменяется без
    // щелчка — кнопка «Заказы» основного экрана на том же месте снова ждёт задержку.
    let close = [VIEWPORT[0] - 20.0, 20.0];
    ui.move_to(close[0], close[1]);
    tooltip_at(&ui, &mut hover, close, 2.0);
    ui.press(close[0], close[1]);
    ui.release();
    assert!(
        ui.state.is_live(&ui.config),
        "кнопка закрытия вернула основной экран"
    );
    ui.move_to(ON_BUTTON[0], ON_BUTTON[1]);
    tooltip_at(&ui, &mut hover, ON_BUTTON, 2.1);
    ui.press(ON_BUTTON[0], ON_BUTTON[1]);
    assert_eq!(
        tooltip_at(&ui, &mut hover, ON_BUTTON, 2.6),
        None,
        "нажатие спрятало подсказку"
    );
    ui.release();
    tooltip_at(&ui, &mut hover, ON_BUTTON, 2.7);
    ui.show("main");
    assert_eq!(tooltip_at(&ui, &mut hover, ON_BUTTON, 2.8), None);
    assert!(
        tooltip_at(&ui, &mut hover, ON_BUTTON, 3.25).is_some(),
        "другой элемент без щелчка ждёт задержку и показывает подсказку"
    );
}

#[test]
fn no_tooltip_until_the_mouse_has_moved_and_none_for_an_empty_text() {
    let (ui, _) = load();
    let parts = parts(&ui);
    let mut hover = TooltipHover::default();
    let style = style(&ui);
    let none = hover.show(
        &parts,
        0,
        None,
        &ui.mouse,
        10.0,
        &style,
        &ui.game.world,
        &ui.game.properties,
    );
    assert_eq!(none, None);

    // У c4 нет бензина: подстановка пуста — подсказки нет.
    let (empty, _) = load_ui(&screens_with(
        &format!("{TOOLTIP},"),
        r##"{"kind":"panel","anchor":"top_left","size":[100,100],"color":"#ffffff",
            "tooltip":"{c4.fuel}"}"##,
    ));
    let mut hover = TooltipHover::default();
    tooltip_at(&empty, &mut hover, [10.0, 10.0], 0.0);
    assert_eq!(tooltip_at(&empty, &mut hover, [10.0, 10.0], 1.0), None);
}

#[test]
fn a_tooltip_stands_centred_above_below_at_the_top_and_inside_the_window() {
    let (ui, _) = load();
    let style = style(&ui);
    // Ширина текста 50, строка 19,2: плашка 74 × 29,2.
    let above = interface::tooltip_rect([100.0, 500.0, 60.0, 52.0], 50.0, 19.2, &style, VIEWPORT);
    assert_eq!(above[0], 93.0);
    assert!((above[1] - (500.0 - 8.0 - 29.2)).abs() < 1e-4);
    assert_eq!(above[2], 74.0);
    assert!((above[3] - 29.2).abs() < 1e-4);
    let below = interface::tooltip_rect([100.0, 10.0, 60.0, 52.0], 50.0, 19.2, &style, VIEWPORT);
    assert_eq!(
        below[1],
        10.0 + 52.0 + 8.0,
        "сверху не помещается — под элементом"
    );
    let right = interface::tooltip_rect([780.0, 300.0, 20.0, 20.0], 50.0, 19.2, &style, VIEWPORT);
    assert_eq!(right[0], 800.0 - 74.0);
    let left = interface::tooltip_rect([0.0, 300.0, 20.0, 20.0], 50.0, 19.2, &style, VIEWPORT);
    assert_eq!(left[0], 0.0);
}

#[test]
fn the_tooltip_style_has_its_defaults() {
    let (ui, _) = load_ui(&screens_json(
        r##""tooltip":{"color":"#000000","font":"ui"},"##,
    ));
    let style = style(&ui);
    assert_eq!(style.delay, 0.5);
    assert_eq!(style.font_size, 16.0);
    assert_eq!(style.image_scale, 1.0);
    assert_eq!(style.gap, 0.0);
}

// -------------------------------------------------------------------------------------------
// Масштаб интерфейса
// -------------------------------------------------------------------------------------------

const SCALED_WINDOW: [f32; 2] = [1280.0, 800.0];

fn scaled_screens() -> String {
    format!(
        r##"{{"ui_reference":[1920,960],{TOOLTIP},"screens":[
    {{"name":"main","world_runs":true,"elements":[
        {{"kind":"panel","anchor":"bottom","offset":[0,10],"size":[1503,97],"color":"#202020"}},
        {{"kind":"button","anchor":"bottom","offset":[180,25],"size":[52,52],"color":"#808080",
         "text":"","font":"ui","on_click":["show_screen","pause"]}},
        {{"kind":"label","anchor":"top_left","size":[100,20],"text":"x","font":"ui","font_size":18}}
    ]}},
    {{"name":"pause","world_runs":false,"elements":[]}}
]}}"##
    )
}

#[test]
fn the_interface_is_multiplied_by_the_smaller_ratio_to_the_reference_window() {
    let (ui, _) = load_ui(&scaled_screens());
    let frame = ui.config.ui_frame(SCALED_WINDOW);
    assert!((frame.scale - 2.0 / 3.0).abs() < 1e-6);
    let screen = ui.state.active();
    let parts = interface::layout(
        &ui.config.screens[screen],
        screen,
        &ui.game.world,
        &ui.state,
        frame.viewport,
    );
    let rects: Vec<[f32; 4]> = parts
        .iter()
        .filter_map(|part| match part {
            Part::Element(placed) => Some(placed.rect.map(|v| v * frame.scale)),
            Part::ScrollBar { .. } => None,
        })
        .collect();
    let close = |a: f32, b: f32| (a - b).abs() < 1e-3;
    // Полоса 1503 — 1002, посередине окна, низ на 10 · ⅔ выше края.
    assert!(close(rects[0][2], 1002.0), "{rects:?}");
    assert!(close(rects[0][0], (1280.0 - 1002.0) / 2.0));
    assert!(close(rects[0][1] + rects[0][3], 800.0 - 10.0 * 2.0 / 3.0));
    // Кнопка 52 — 34,7.
    assert!(close(rects[1][2], 52.0 * 2.0 / 3.0));
    // Надпись 18 — 12.
    let Element::Label { font_size, .. } = &ui.config.screens[0].elements[2] else {
        panic!("третий — надпись");
    };
    assert!(close(font_size * frame.scale, 12.0));
    // Вдвое большее окно образца — всё вдвое крупнее.
    assert_eq!(
        ui.config.ui_frame([2560.0, 1280.0]).scale,
        2.0 * ui.config.ui_frame([1280.0, 640.0]).scale
    );
}

#[test]
fn a_button_takes_the_click_where_it_is_drawn_at_the_scaled_size() {
    let (mut ui, _) = load_ui(&scaled_screens());
    let frame = ui.config.ui_frame(SCALED_WINDOW);
    let screen = ui.state.active();
    let rect = interface::layout(
        &ui.config.screens[screen],
        screen,
        &ui.game.world,
        &ui.state,
        frame.viewport,
    )
    .iter()
    .find_map(|part| match part {
        Part::Element(placed) if matches!(placed.element, Element::Button { .. }) => {
            Some(placed.rect.map(|v| v * frame.scale))
        }
        _ => None,
    })
    .expect("кнопка");
    let click = |ui: &mut Ui, point: [f32; 2]| {
        ui.queue.push_mouse_move(point[0], point[1]);
        ui.queue.push_mouse_down();
        ui.queue.push_mouse_up();
        engine::core::screens::process_ui_queue(
            &mut ui.queue,
            &mut ui.mouse,
            &mut ui.game,
            &ui.config,
            &mut ui.state,
            SCALED_WINDOW,
        );
    };
    let over = |ui: &Ui, point: [f32; 2]| {
        engine::core::screens::over_interface(
            &ui.config,
            &ui.state,
            &ui.game.world,
            SCALED_WINDOW,
            point,
        )
    };
    assert!(over(&ui, [rect[0] + 1.0, rect[1] + 1.0]));
    assert!(!over(&ui, [5.0, 5.0]));
    // Чуть правее нарисованной кнопки — над полосой: нажатие она забирает, но команды нет.
    click(&mut ui, [rect[0] + rect[2] + 5.0, rect[1] + 2.0]);
    assert_eq!(ui.state.active(), ui.screen_id("main"));
    click(&mut ui, [rect[0] + rect[2] - 2.0, rect[1] + rect[3] - 2.0]);
    assert_eq!(ui.state.active(), ui.screen_id("pause"));
}

#[test]
fn without_ui_reference_the_interface_stays_point_for_point() {
    let (ui, _) = load();
    let frame = ui.config.ui_frame(VIEWPORT);
    assert_eq!(frame.scale, 1.0);
    assert_eq!(frame.viewport, VIEWPORT);
    assert_eq!(frame.point([123.0, 45.0]), [123.0, 45.0]);
}

// -------------------------------------------------------------------------------------------
// Шрифты
// -------------------------------------------------------------------------------------------

const TWO_FONTS: &str = r#""ui":{"path":"fonts/Tektur.ttf","width":87.5,"weight":500,"features":["tnum","zero"]},
    "plain":{"path":"fonts/Tektur.ttf"},
    "title":"fonts/Stalinist.ttf""#;

#[test]
fn a_font_is_a_path_or_an_object_with_its_settings() {
    let game_json = game_json(TWO_FONTS, IMAGES);
    let (config, _) = read_entry(&game_json).unwrap_or_else(|f| panic!("{:#?}", f.errors));
    let setting_of = |name: &str| {
        let index = config
            .files
            .fonts
            .iter()
            .position(|(n, _)| n == name)
            .expect("шрифт объявлен");
        config.files.font_settings[index].clone()
    };
    assert_eq!(
        setting_of("ui"),
        FontSettings {
            width: 87.5,
            weight: 500,
            features: vec![*b"tnum", *b"zero"],
        }
    );
    assert_eq!(
        setting_of("plain"),
        FontSettings::default(),
        "объект без настроек — как путь"
    );
    assert_eq!(setting_of("title"), FontSettings::default());
}

#[test]
fn two_names_of_one_file_read_it_once() {
    let game_json = game_json(TWO_FONTS, IMAGES);
    let (config, _) = read_entry(&game_json).unwrap();
    let needed = read_texts(&config, Some(PROPS), None, None);
    let mut paths: Vec<&str> = needed.fonts.iter().map(|(_, path)| path.as_str()).collect();
    paths.sort_unstable();
    assert_eq!(paths, ["fonts/Stalinist.ttf", "fonts/Tektur.ttf"]);
    // Страница отдала байты только под одним именем — игра всё равно грузится.
    let screens =
        screens_json(&format!("{TOOLTIP},")).replace(r#""font":"ui""#, r#""font":"plain""#);
    try_load_with(TWO_FONTS, IMAGES, &screens, IMAGE_FILES)
        .unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
}

fn font_errors(font: &str) -> Vec<String> {
    match read_entry(&game_json(&format!(r#""ui":{font}"#), IMAGES)) {
        Ok(_) => Vec::new(),
        Err(failure) => failure
            .errors
            .into_iter()
            .map(|e| format!("{} — {}", e.path, e.message))
            .collect(),
    }
}

fn expect_font_error(font: &str, fragment: &str) {
    let errors = font_errors(font);
    assert!(
        errors.iter().any(|message| message.contains(fragment)),
        "ожидалась ошибка с «{fragment}», есть {errors:?}"
    );
}

#[test]
fn font_settings_have_to_make_sense() {
    expect_font_error(
        r#"{"width":100}"#,
        "отсутствует обязательная настройка \"path\"",
    );
    expect_font_error(r#"{"path":"a.ttf","slant":10}"#, "slant");
    expect_font_error(
        r#"{"path":"a.ttf","width":90}"#,
        "один из шагов 50, 62.5, 75",
    );
    expect_font_error(r#"{"path":"a.ttf","weight":0}"#, "от 1 до 1000");
    expect_font_error(r#"{"path":"a.ttf","weight":1001}"#, "от 1 до 1000");
    expect_font_error(r#"{"path":"a.ttf","weight":"bold"}"#, "weight");
    expect_font_error(r#"{"path":"a.ttf","features":"tnum"}"#, "features");
    expect_font_error(
        r#"{"path":"a.ttf","features":["tnu"]}"#,
        "четырёх латинских букв или цифр",
    );
    expect_font_error(
        r#"{"path":"a.ttf","features":["tnüm"]}"#,
        "четырёх латинских букв или цифр",
    );
    expect_font_error(
        r#"{"path":"a.ttf","features":[4]}"#,
        "четырёх латинских букв или цифр",
    );
    assert_eq!(
        font_errors(r#"{"path":"a.ttf","width":200,"weight":1,"features":["ss01"]}"#),
        Vec::<String>::new()
    );
}

// -------------------------------------------------------------------------------------------
// Проверки перед запуском
// -------------------------------------------------------------------------------------------

fn image_errors(images: &str, files: &[(&str, u32, u32)]) -> Vec<String> {
    errors_of(try_load_with(
        FONTS,
        images,
        &screens_with(
            "",
            r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"a"}"##,
        ),
        files,
    ))
}

fn expect_image_error(images: &str, files: &[(&str, u32, u32)], fragment: &str) {
    let errors = image_errors(images, files);
    assert!(
        errors.iter().any(|message| message.contains(fragment)),
        "ожидалась ошибка с «{fragment}», есть {errors:?}"
    );
}

#[test]
fn slice_has_to_be_four_whole_numbers_on_a_single_picture_that_fits() {
    let files = &[("a", 20, 30)];
    for slice in [
        "[1,2,3]",
        "[1,2,3,-4]",
        "[1,2,3,4.5]",
        "\"x\"",
        "[1,2,3,4,5]",
    ] {
        expect_image_error(
            &format!(r#""a":{{"path":"a.png","slice":{slice}}}"#),
            files,
            "четыре целых числа от 0",
        );
    }
    expect_image_error(
        r#""a":{"path":"a.png","frames":2,"slice":[1,1,1,1]}"#,
        files,
        "у картинки с кадрами frames и у видео их не бывает",
    );
    expect_image_error(
        r#""a":{"path":"a.mp4","slice":[1,1,1,1]}"#,
        files,
        "у картинки с кадрами frames и у видео их не бывает",
    );
    expect_image_error(
        r#""a":{"path":"a.png","slice":[10,1,10,1]}"#,
        files,
        "слева и справа (10 + 10) должны быть меньше ширины картинки 20",
    );
    expect_image_error(
        r#""a":{"path":"a.png","slice":[1,15,1,15]}"#,
        files,
        "сверху и снизу (15 + 15) должны быть меньше высоты картинки 30",
    );
    assert_eq!(
        image_errors(r#""a":{"path":"a.png","slice":[9,0,10,29]}"#, files),
        Vec::<String>::new()
    );
}

#[test]
fn image_scale_is_a_positive_number_on_an_element_with_a_picture() {
    expect_screen_error(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"frame","image_scale":0}"##,
        "\"image_scale\" должно быть больше нуля",
    );
    expect_screen_error(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"image":"frame","image_scale":"x"}"##,
        "image_scale",
    );
    expect_screen_error(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","image_scale":1}"##,
        "у элемента без картинки его не бывает",
    );
    expect_screen_error(
        r##"{"kind":"button","anchor":"top_left","size":[10,10],"color":"#ffffff","text":"","font":"ui",
            "image_scale":2,"on_click":["show_screen","main"]}"##,
        "у элемента без картинки его не бывает",
    );
    expect_screen_error(
        r##"{"kind":"bar","anchor":"top_left","size":[10,10],"for":{"has":["car"]},"value":"fuel",
            "max":1,"color":"#ffffff","back_color":"#000000","image_scale":2}"##,
        "у элемента без картинки его не бывает",
    );
}

const BAR: &str = r##""kind":"bar","anchor":"top_left","size":[100,10],"for":{"has":["car"]}"##;

#[test]
fn a_bar_needs_value_size_for_and_one_fill() {
    expect_screen_error(
        &format!(r##"{{{BAR},"max":1,"color":"#ffffff"}}"##),
        "отсутствует обязательная настройка \"value\"",
    );
    expect_screen_error(
        r##"{"kind":"bar","anchor":"top_left","for":{"has":["car"]},"value":"fuel","max":1,"color":"#ffffff"}"##,
        "отсутствует обязательная настройка \"size\"",
    );
    expect_screen_error(
        r##"{"kind":"bar","anchor":"top_left","size":[100,10],"value":"fuel","max":1,"color":"#ffffff"}"##,
        "у шкалы на экране нужен \"for\"",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"plate","max":1,"color":"#ffffff"}}"##),
        "value должно называть свойство вида number",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"fuel","max":"plate","color":"#ffffff"}}"##),
        "max должен быть числом или называть свойство вида number",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"fuel","max":0,"color":"#ffffff"}}"##),
        "max должен быть больше нуля",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"fuel","max":1,"color":"#ffffff","image":"gauge"}}"##),
        "заданы и color, и image",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"fuel","max":1}}"##),
        "нужен один из color или image",
    );
    expect_screen_error(
        &format!(
            r##"{{{BAR},"value":"fuel","max":1,"color":"#ffffff","back_color":"#000000","back_image":"gauge"}}"##
        ),
        "заданы и \"back_color\", и \"back_image\"",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"fuel","max":1,"image":"nothing"}}"##),
        "nothing",
    );
    expect_screen_error(
        &format!(r##"{{{BAR},"value":"fuel","max":1,"color":"#ffffff","back_image":"nothing"}}"##),
        "nothing",
    );
}

#[test]
fn a_bar_in_a_row_takes_the_object_of_the_row_without_for() {
    let list = r##"{"kind":"list","anchor":"top_left","size":[300,300],"for":{"has":["car"]},"row_height":40,
        "row":[{"kind":"bar","offset":[0,0],"size":[100,10],"value":"fuel","max":"tank","color":"#ffffff"}]}"##;
    try_load(&screens_with("", list)).unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
    let with_for = r##"{"kind":"list","anchor":"top_left","size":[300,300],"for":{"has":["car"]},"row_height":40,
        "row":[{"kind":"bar","offset":[0,0],"size":[100,10],"for":{"has":["car"]},"value":"fuel","max":1,
        "color":"#ffffff"}]}"##;
    expect_error(
        try_load(&screens_with("", with_for)),
        "у элемента строки списка нет \"for\"",
    );
}

#[test]
fn a_tooltip_needs_the_style_block_and_declared_properties() {
    expect_error(
        try_load(&screens_with(
            "",
            r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"Окно"}"##,
        )),
        "блока \"tooltip\" с видом подсказок в screens.json нет",
    );
    expect_screen_error(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"{c1.nothing}"}"##,
        "nothing",
    );
    expect_screen_error(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"{nobody.fuel}"}"##,
        "nobody",
    );
    expect_screen_error(
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"{fuel}"}"##,
        "fuel",
    );
}

fn style_errors(style: &str) -> Vec<String> {
    errors_of(try_load(&screens_with(
        &format!(r#""tooltip":{style},"#),
        r##"{"kind":"panel","anchor":"top_left","size":[10,10],"color":"#ffffff","tooltip":"x"}"##,
    )))
}

fn expect_style_error(style: &str, fragment: &str) {
    let errors = style_errors(style);
    assert!(
        errors.iter().any(|message| message.contains(fragment)),
        "ожидалась ошибка с «{fragment}», есть {errors:?}"
    );
}

#[test]
fn the_tooltip_style_has_to_make_sense() {
    expect_style_error(
        r##"{"color":"#000000","font":"serif"}"##,
        "шрифт \"serif\" не объявлен",
    );
    expect_style_error(
        r##"{"color":"#000000"}"##,
        "отсутствует обязательная настройка \"font\"",
    );
    expect_style_error(
        r##"{"color":"#000000","image":"frame","font":"ui"}"##,
        "заданы и color, и image",
    );
    expect_style_error(r##"{"font":"ui"}"##, "нужен один из color или image");
    expect_style_error(
        r##"{"color":"#000000","font":"ui","padding":[4]}"##,
        "padding",
    );
    expect_style_error(
        r##"{"color":"#000000","font":"ui","padding":[4,-1]}"##,
        "два числа от 0",
    );
    expect_style_error(
        r##"{"color":"#000000","font":"ui","gap":-1}"##,
        "\"gap\" не может быть меньше нуля",
    );
    expect_style_error(
        r##"{"color":"#000000","font":"ui","delay":-0.1}"##,
        "\"delay\" не может быть меньше нуля",
    );
    expect_style_error(
        r##"{"color":"#000000","font":"ui","font_size":0}"##,
        "\"font_size\" должно быть больше нуля",
    );
    expect_style_error(r##"{"color":"#000000","font":"ui","wrap":true}"##, "wrap");
    expect_style_error(
        r##"{"color":"#000000","font":"ui","image_scale":2}"##,
        "у элемента без картинки его не бывает",
    );
    assert_eq!(
        style_errors(r##"{"image":"frame","image_scale":0.5,"font":"ui","delay":0,"gap":0}"##),
        Vec::<String>::new()
    );
}

#[test]
fn ui_reference_is_a_pair_of_positive_numbers() {
    for reference in ["[1920]", "[1920,0]", "[-1,960]", "\"1920x960\""] {
        expect_error(
            try_load(&screens_with(
                &format!(r#""ui_reference":{reference},"#),
                "",
            )),
            "ui_reference",
        );
    }
    try_load(&screens_with(r#""ui_reference":[1920,960],"#, ""))
        .unwrap_or_else(|failure| panic!("{:#?}", failure.errors));
}

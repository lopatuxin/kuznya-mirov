//! Фаза 44 — картинки над объектами в мире (требования 1–8). Игр в `games/` не заводит — каждый
//! тест собирает свою маленькую игру в коде, как `tests/phase12_world_labels_tables.rs`; «страница»
//! здесь — очередь мыши и экран, которым тесты водят игру теми же вызовами, что `wasm::Engine`.

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::property;
use engine::core::runner::Runner;
use engine::core::scene::LayerView;
use engine::core::screens::{self, ScreenState, ScreensConfig};
use engine::core::world_elements::{
    OverlayRun, compute_world_draws, compute_world_draws_3d, image_object_at, overlay_runs,
};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{load_rest, read_entry};
use engine::data::session::PlaySession;

/// Окно 200 × 200 и сцена 20 × 20: клетка — десять точек, поля нет.
const VIEWPORT: [f32; 2] = [200.0, 200.0];
const FONT_BYTES: &[u8] = &[0x00, 0x01, 0x00, 0x00];
const NO_RULES: &str = r#"{"rules":[]}"#;
const PROPS: &str = r#"{"properties":{"icon":"text","place":"flag","clicked":"flag"}}"#;

fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{{"ui":"ui.ttf"}},
"images":{{"bakery":{{"path":"icons/bakery.png"}},"food":{{"path":"icons/food.png"}}}}}}}}"##
    )
}

type Loaded = Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_with(scene_extra: &str, props: &str, scene: &str, screens: &str) -> Loaded {
    let text = game_json(scene_extra);
    let (config, _) = read_entry(&text).expect("game.json разбирается");
    let fonts = vec![("ui".to_string(), Some(FONT_BYTES.to_vec()))];
    load_rest(
        &text,
        config,
        Some(props),
        Some(scene),
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

/// Три места со значками, дом без значка и колодец без `on_click`.
const PLACES: &str = r##"{"objects":[
    {"name":"Хлебозавод","position":[4,6],"size":[4,2],"place":true,"icon":"bakery",
     "on_click":[["clicked",true]]},
    {"name":"Склад","position":[14,6],"size":[4,2],"place":true,"icon":"food",
     "on_click":[["clicked",true]]},
    {"name":"Дом","position":[8,14],"size":[2,2],"place":true,"on_click":[["clicked",true]]},
    {"name":"Колодец","position":[10,10],"size":[2,2],"place":true,"icon":"food"}
]}"##;

/// Значок над местом: середина на два клетки выше верха, 2 × 2.
const ICON_SCREENS: &str = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"image","for":{"has":["place"]},"anchor":"top","offset":[0,-1.5],"size":[2,2],
   "image":"{icon}"}
]}"##;

/// Окно мира с очередью мыши и экраном.
struct Page {
    game: Game,
    config: ScreensConfig,
    state: ScreenState,
    runner: Runner,
    mouse: MouseState,
    queue: UiQueue,
}

impl Page {
    fn new(game: Game, config: ScreensConfig) -> Page {
        let state = ScreenState::new(config.start_screen);
        Page {
            game,
            config,
            state,
            runner: Runner::new(),
            mouse: MouseState::default(),
            queue: UiQueue::new(),
        }
    }

    fn load(scene: &str, screens: &str) -> Page {
        let (game, config, _warnings) =
            load_with("", PROPS, scene, screens).expect("игра должна загрузиться");
        Page::new(game, config)
    }

    fn id_of(&self, name: &str) -> u32 {
        self.game
            .world
            .ids()
            .find(|&id| self.game.world.text(id, property::NAME) == Some(name))
            .unwrap_or_else(|| panic!("нет объекта {name}"))
    }

    fn clicked(&self, name: &str) -> bool {
        let prop = self.game.properties.resolve("clicked").expect("свойство");
        self.game.world.flag(self.id_of(name), prop)
    }

    /// Объект картинки под клеткой сцены, как его находит страница в кадре.
    fn image_at(&self, cell: [f32; 2]) -> Option<u32> {
        let (_, _, images) = compute_world_draws(
            &self.game.world,
            &self.game.scene,
            &self.game.properties,
            &self.config.world_elements,
            &LayerView::default(),
        );
        image_object_at(&images, cell)
    }

    fn tick(&mut self) {
        screens::engine_call(
            &mut self.queue,
            &mut self.mouse,
            &mut self.runner,
            &mut self.game,
            &self.config,
            &mut self.state,
            VIEWPORT,
            0.1,
        );
    }

    /// Мышь пришла в клетку, нажата и отпущена, как у страницы: курсор мира и нажатие с тем, что
    /// страница нашла под ним.
    fn click(&mut self, cell: [f32; 2]) {
        self.press(cell);
        self.queue.push_mouse_up();
        self.tick();
    }

    fn press(&mut self, cell: [f32; 2]) {
        self.game.set_cursor_cell([cell[0] as f64, cell[1] as f64]);
        self.queue.push_mouse_move(cell[0] * 10.0, cell[1] * 10.0);
        match self.image_at(cell) {
            Some(object) => self.queue.push_mouse_down_on(object),
            None => self.queue.push_mouse_down(),
        }
        self.tick();
    }
}

// -------------------------------------------------------------------------------------------
// Требования 1–5: какая картинка и где
// -------------------------------------------------------------------------------------------

/// Требования 1–4: значок по `{icon}` над хлебозаводом и складом, у дома без `icon` его нет; все
/// картинки названы значениями свойства — предупреждения «картинка нигде не названа» нет.
#[test]
fn each_place_gets_its_own_icon_by_the_property() {
    let page = Page::load(PLACES, ICON_SCREENS);
    let (_, _, images) = compute_world_draws(
        &page.game.world,
        &page.game.scene,
        &page.game.properties,
        &page.config.world_elements,
        &LayerView::default(),
    );

    let drawn: Vec<(u32, usize, [f32; 2], [f32; 2])> = images
        .iter()
        .map(|draw| (draw.object, draw.image, draw.position, draw.size))
        .collect();
    assert_eq!(
        drawn,
        [
            (page.id_of("Хлебозавод"), 0, [5.0, 3.5], [2.0, 2.0]),
            (page.id_of("Склад"), 1, [15.0, 3.5], [2.0, 2.0]),
            (page.id_of("Колодец"), 1, [10.0, 7.5], [2.0, 2.0]),
        ]
    );
    assert!(
        images.iter().all(|draw| draw.object != page.id_of("Дом")),
        "у дома без icon значка нет"
    );
}

/// Картинки, которые называют только значения свойства, не получают предупреждения «нигде не названа».
#[test]
fn pictures_named_only_by_property_values_get_no_unused_warning() {
    let (_, _, warnings) =
        load_with("", PROPS, PLACES, ICON_SCREENS).expect("игра должна загрузиться");

    assert!(
        warnings.iter().all(|w| !w.message.contains("картинка")),
        "{warnings:?}"
    );
}

/// Требование 3: значение `icon` называет картинку, которой нет, — картинки нет, ошибки тоже.
#[test]
fn an_unknown_picture_name_in_the_property_is_just_no_picture() {
    let scene = r##"{"objects":[
        {"position":[4,6],"size":[4,2],"place":true,"icon":"bakery"},
        {"position":[14,6],"size":[4,2],"place":true,"icon":"ghost"},
        {"position":[8,14],"size":[2,2],"place":true,"icon":""}
    ]}"##;
    let (game, config, _warnings) =
        load_with("", PROPS, scene, ICON_SCREENS).expect("неизвестное значение — не ошибка");

    let (_, _, images) = compute_world_draws(
        &game.world,
        &game.scene,
        &game.properties,
        &config.world_elements,
        &LayerView::default(),
    );

    assert_eq!(images.len(), 1);
    assert_eq!(images[0].image, 0);
}

/// Требование 4: картинка без подстановки — одна на всех объектах отбора.
#[test]
fn a_named_picture_without_substitution_is_drawn_over_every_object_of_the_selection() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"image","for":{"has":["place"]},"anchor":"center","size":[1,1],"image":"food"}
]}"##;
    let page = Page::load(PLACES, screens);
    let (_, _, images) = compute_world_draws(
        &page.game.world,
        &page.game.scene,
        &page.game.properties,
        &page.config.world_elements,
        &LayerView::default(),
    );

    assert_eq!(images.len(), 4);
    assert!(images.iter().all(|draw| draw.image == 1));
}

/// Требование 5: картинки и надписи между собой — в порядке списка `world_elements`.
#[test]
fn images_and_labels_keep_the_order_of_the_element_list() {
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"label","for":{"has":["place"]},"anchor":"bottom","size":[3,0.5],"text":"{icon}",
   "font":"ui","color":"#ffffff"},
  {"kind":"image","for":{"has":["icon"]},"anchor":"top","size":[1,1],"image":"{icon}"},
  {"kind":"label","for":{"has":["place"]},"anchor":"center","size":[3,0.5],"text":"{icon}",
   "font":"ui","color":"#ffffff"}
]}"##;
    let (game, config, _) = load_with("", PROPS, PLACES, screens).expect("игра должна загрузиться");
    let (bars, labels, images) = compute_world_draws(
        &game.world,
        &game.scene,
        &game.properties,
        &config.world_elements,
        &LayerView::default(),
    );

    assert_eq!(
        overlay_runs(&bars, &labels, &images),
        [
            OverlayRun::Labels(4),
            OverlayRun::Images(3),
            OverlayRun::Labels(4)
        ]
    );
}

/// Требование 5: полоска и картинка — в порядке списка, текст поверх полосок: полоска после картинки
/// ложится на неё, полоска после надписи уходит под надпись.
#[test]
fn bars_follow_the_list_among_images_and_stay_under_text() {
    let bar = r##"{"kind":"bar","for":{"has":["place"]},"anchor":"bottom","size":[2,0.2],"value":"hp","max":10,"color":"#22c55e"}"##;
    let image =
        r##"{"kind":"image","for":{"has":["place"]},"anchor":"top","size":[1,1],"image":"food"}"##;
    let label = r##"{"kind":"label","for":{"has":["place"]},"anchor":"center","size":[3,0.5],"text":"{icon}","font":"ui","color":"#ffffff"}"##;
    let props = r#"{"properties":{"icon":"text","place":"flag","clicked":"flag","hp":"number"}}"#;
    let scene = r##"{"objects":[
        {"position":[4,6],"size":[2,2],"place":true,"hp":5,"icon":"food"},
        {"position":[8,6],"size":[2,2],"place":true,"hp":5,"icon":"food"}
    ]}"##;
    let runs = |elements: [&str; 3]| {
        let screens = format!(
            r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[]}}],"world_elements":[{}]}}"#,
            elements.join(",")
        );
        let (game, config, _) =
            load_with("", props, scene, &screens).expect("игра должна загрузиться");
        let (bars, labels, images) = compute_world_draws(
            &game.world,
            &game.scene,
            &game.properties,
            &config.world_elements,
            &LayerView::default(),
        );
        overlay_runs(&bars, &labels, &images)
    };

    assert_eq!(
        runs([image, bar, label]),
        [
            OverlayRun::Images(2),
            OverlayRun::Bars(2),
            OverlayRun::Labels(2)
        ],
        "полоска после картинки — над ней"
    );
    assert_eq!(
        runs([bar, image, label]),
        [
            OverlayRun::Bars(2),
            OverlayRun::Images(2),
            OverlayRun::Labels(2)
        ],
        "полоска до картинки — под ней"
    );
    assert_eq!(
        runs([label, image, bar]),
        [
            OverlayRun::Bars(2),
            OverlayRun::Labels(2),
            OverlayRun::Images(2)
        ],
        "текст поверх полосок: полоска после надписи уходит под неё"
    );
}

/// Требования 1, 5 в трёхмерной сцене: значок над фигурой в точках окна, одного размера у ближней и
/// дальней.
#[test]
fn an_icon_over_a_shape_has_the_same_size_near_and_far() {
    let scene = r##"{"objects":[
        {"position":[9,4],"size":[0.9,0.9],"place":true,"shape":"capsule","height":1.3,
         "color":"#7dc243","icon":"bakery"},
        {"position":[9,14],"size":[0.9,0.9],"place":true,"shape":"capsule","height":1.3,
         "color":"#7dc243","icon":"food"}
    ]}"##;
    let (game, config, _) = load_with(
        r#","view_height":20,"camera":{"pitch":60}"#,
        PROPS,
        scene,
        ICON_SCREENS,
    )
    .expect("игра должна загрузиться");
    let window = [1920.0, 1080.0];
    let camera = game.camera_3d(window).expect("камера");

    let (_, _, images) = compute_world_draws_3d(
        &game.world,
        &game.scene,
        &game.properties,
        &config.world_elements,
        &camera,
    );

    assert_eq!(images.len(), 2);
    let cell = camera.cell_points() as f32;
    assert_eq!(images[0].size, [2.0 * cell, 2.0 * cell]);
    assert_eq!(images[0].size, images[1].size);
    let middle = |position: [f32; 2], size: [f32; 2]| {
        [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0]
    };
    for draw in &images {
        assert_eq!(
            image_object_at(&images, middle(draw.position, draw.size)),
            Some(draw.object)
        );
    }
}

// -------------------------------------------------------------------------------------------
// Требования 6–7: щелчок по картинке
// -------------------------------------------------------------------------------------------

/// Требование 6: нажатие над значком — нажатие по его объекту; без значка то же место сцены пусто.
#[test]
fn a_press_over_an_icon_is_a_press_on_its_object() {
    let mut page = Page::load(PLACES, ICON_SCREENS);
    // Над хлебозаводом, на его значке: сам прямоугольник хлебозавода — клетки 6–8 по вертикали.
    let cell = [6.0, 4.5];
    assert_eq!(page.image_at(cell), Some(page.id_of("Хлебозавод")));

    // Без картинки под нажатием то же место сцены пусто: щелчок идёт под курсор.
    page.game.set_cursor_cell([cell[0] as f64, cell[1] as f64]);
    page.queue.push_mouse_move(cell[0] * 10.0, cell[1] * 10.0);
    page.queue.push_mouse_down();
    page.tick();
    page.queue.push_mouse_up();
    page.tick();
    assert!(!page.clicked("Хлебозавод"));

    page.click(cell);

    assert!(page.clicked("Хлебозавод"));
    assert!(!page.clicked("Склад") && !page.clicked("Дом"));
}

/// Требование 6: из двух значков внахлёст щелчок получает верхний — нарисованный позже.
#[test]
fn the_topmost_of_two_overlapping_icons_gets_the_press() {
    let scene = r##"{"objects":[
        {"name":"Нижний","position":[4,6],"size":[2,2],"place":true,"icon":"bakery",
         "on_click":[["clicked",true]]},
        {"name":"Верхний","position":[5,6],"size":[2,2],"place":true,"icon":"food",
         "on_click":[["clicked",true]]}
    ]}"##;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"image","for":{"has":["place"]},"anchor":"top","offset":[0,-1.5],"size":[3,3],
   "image":"{icon}"}
]}"##;
    let mut page = Page::load(scene, screens);
    // Клетки 5–7 принадлежат обоим значкам; слева и справа от них — по одному.
    page.click([5.5, 4.5]);
    assert!(page.clicked("Верхний") && !page.clicked("Нижний"));
    page.click([3.5, 4.5]);
    assert!(
        page.clicked("Нижний"),
        "там, где верхнего нет, получает нижний"
    );
}

/// «Крайние случаи»: объект без `on_click` — щелчок по значку ничего не делает.
#[test]
fn a_press_over_the_icon_of_an_object_without_on_click_changes_nothing() {
    let mut page = Page::load(PLACES, ICON_SCREENS);
    let well = page.id_of("Колодец");
    assert_eq!(page.image_at([11.0, 8.5]), Some(well));

    page.click([11.0, 8.5]);

    assert!(!page.clicked("Хлебозавод") && !page.clicked("Склад") && !page.clicked("Дом"));
    assert!(!page.clicked("Колодец"));
}

/// Требование 6: кнопка экрана выше картинок в мире — нажатие над ней до значка не доходит, а её
/// команда срабатывает как всегда.
#[test]
fn a_screen_button_over_an_icon_takes_the_press() {
    let screens = r##"{"screens":[
  {"name":"main","world_runs":true,"elements":[
    {"kind":"button","anchor":"top_left","offset":[40,30],"size":[40,30],"text":"Пауза",
     "font":"ui","color":"#ffffff","on_click":["show_screen","stop"]}
  ]},
  {"name":"stop","world_runs":false,"elements":[]}],
"world_elements":[
  {"kind":"image","for":{"has":["place"]},"anchor":"top","offset":[0,-1.5],"size":[2,2],
   "image":"{icon}"}
]}"##;
    let mut page = Page::load(PLACES, screens);
    // Значок хлебозавода — окно 50–70 по горизонтали, 35–55 по вертикали; кнопка — 40–80 и 30–60.
    let cell = [6.0, 4.5];
    assert_eq!(page.image_at(cell), Some(page.id_of("Хлебозавод")));

    page.click(cell);

    assert!(!page.clicked("Хлебозавод"), "нажатие забрала кнопка");
    let stop = page
        .config
        .screens
        .iter()
        .position(|screen| screen.name == "stop")
        .expect("экран stop");
    assert_eq!(page.state.active(), stop, "команда кнопки сработала");
}

/// Требование 6: колесо, правая и средняя кнопки над картинкой доходят до камеры игрока —
/// картинка не часть интерфейса, и колесо над ней приближает вид.
#[test]
fn the_wheel_over_an_icon_reaches_the_player_camera() {
    let scene = r##"{"objects":[
        {"name":"Хлебозавод","position":[9,9],"size":[2,2],"place":true,"icon":"bakery"}
    ]}"##;
    let screens = r##"{"screens":[{"name":"main","world_runs":true,"elements":[]}],
"world_elements":[
  {"kind":"image","for":{"has":["place"]},"anchor":"center","size":[2,2],"image":"{icon}"}
]}"##;
    let (mut game, config, _) = load_with(
        r#","view_height":10,"player_camera":{"zoom":[4,30],"start":[10,10]}"#,
        PROPS,
        scene,
        screens,
    )
    .expect("игра должна загрузиться");
    let state = ScreenState::new(config.start_screen);
    let pointer = [100.0, 100.0];
    let before = game.player_view_height().expect("камера игрока");

    assert!(!screens::over_interface(
        &config,
        &state,
        &game.world,
        VIEWPORT,
        pointer
    ));
    assert!(game.camera_wheel(-100.0, pointer, VIEWPORT));

    assert!(game.player_view_height().expect("вид") < before);
}

/// Требование 7: щелчок по картинке идёт в запись как щелчок по её объекту, и повтор попадает в тот
/// же объект — даже когда курсор мира стоял над другим.
#[test]
fn a_recorded_press_on_an_icon_replays_into_the_same_object() {
    let mut page = Page::load(PLACES, ICON_SCREENS);
    let bakery = page.id_of("Хлебозавод");
    let mut session = PlaySession::begin_live(&mut page.game, &page.config, &mut page.state);
    // Курсор мира — над складом, значок — хлебозавода.
    page.game.set_cursor_cell([16.0, 7.0]);
    session.record_cursor(&page.game);
    page.queue.push_mouse_move(60.0, 45.0);
    page.queue.push_mouse_down_on(bakery);
    for _ in 0..2 {
        session.tick_live(
            &mut page.queue,
            &mut page.mouse,
            &mut page.runner,
            &mut page.game,
            &page.config,
            &mut page.state,
            VIEWPORT,
            0.1,
            &[],
        );
        page.queue.push_mouse_up();
    }
    assert!(page.clicked("Хлебозавод"));
    assert!(!page.clicked("Склад"), "щелчок ушёл к объекту картинки");
    session.end(&mut page.game);
    let text = session.recording_text(&page.game);
    assert!(text.contains(&format!("\"click\":{bakery}")), "{text}");

    let mut replay = Page::load(PLACES, ICON_SCREENS);
    let mut replay_session =
        PlaySession::begin_replay(&text, &mut replay.game, &replay.config, &mut replay.state)
            .expect("запись должна разобраться");
    replay_session.seek(
        u64::MAX,
        &mut replay.queue,
        &mut replay.mouse,
        &mut replay.game,
        &replay.config,
        &mut replay.state,
        VIEWPORT,
        &[],
    );

    assert!(replay.clicked("Хлебозавод"));
    assert!(!replay.clicked("Склад") && !replay.clicked("Дом"));
}

/// Нажатие, нацеленное на объект, которого в повторе уже нет, ничего не делает и не падает.
#[test]
fn a_press_aimed_at_a_missing_object_does_nothing() {
    let mut page = Page::load(PLACES, ICON_SCREENS);

    page.game.press_key_on("MouseLeft", 9999);

    assert!(!page.clicked("Хлебозавод") && !page.clicked("Склад") && !page.clicked("Дом"));
}

// -------------------------------------------------------------------------------------------
// Требование 8: проверка данных перед запуском
// -------------------------------------------------------------------------------------------

fn world_element_errors(element: &str) -> Vec<(String, String, String)> {
    let screens = format!(
        r#"{{"screens":[{{"name":"main","world_runs":true,"elements":[]}}],"world_elements":[{element}]}}"#
    );
    match load_with("", PROPS, PLACES, &screens) {
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
fn an_image_element_without_the_image_field_is_an_error() {
    let errors = world_element_errors(
        r#"{"kind":"image","for":{"has":["place"]},"anchor":"top","size":[1,1]}"#,
    );
    assert_one_error_at(&errors, "world_elements[0]", "image");
}

#[test]
fn an_image_element_without_size_is_an_error() {
    let errors = world_element_errors(
        r#"{"kind":"image","for":{"has":["place"]},"anchor":"top","image":"food"}"#,
    );
    assert_one_error_at(&errors, "world_elements[0]", "size");
}

#[test]
fn a_picture_name_not_declared_in_files_images_is_an_error() {
    let errors = world_element_errors(
        r#"{"kind":"image","for":{"has":["place"]},"anchor":"top","size":[1,1],"image":"ghost"}"#,
    );
    assert_one_error_at(&errors, "world_elements[0] → image", "ghost");
}

#[test]
fn a_substitution_naming_an_undeclared_property_is_an_error() {
    let errors = world_element_errors(
        r#"{"kind":"image","for":{"has":["place"]},"anchor":"top","size":[1,1],"image":"{nope}"}"#,
    );
    assert_one_error_at(&errors, "world_elements[0] → image", "nope");
}

#[test]
fn a_substitution_must_fill_the_whole_picture_name() {
    let errors = world_element_errors(
        r#"{"kind":"image","for":{"has":["place"]},"anchor":"top","size":[1,1],"image":"a_{icon}"}"#,
    );
    assert_one_error_at(&errors, "world_elements[0] → image", "целиком");
}

#[test]
fn a_substitution_naming_a_property_that_is_not_text_is_an_error() {
    for property in ["place", "position", "image"] {
        let element = format!(
            r#"{{"kind":"image","for":{{"has":["place"]}},"anchor":"top","size":[1,1],"image":"{{{property}}}"}}"#
        );
        let errors = world_element_errors(&element);
        assert_one_error_at(
            &errors,
            "world_elements[0] → image",
            "не строковое свойство",
        );
    }
}

#[test]
fn an_image_element_has_no_color() {
    let errors = world_element_errors(
        r##"{"kind":"image","for":{"has":["place"]},"anchor":"top","size":[1,1],"image":"food","color":"#ffffff"}"##,
    );
    assert_one_error_at(&errors, "world_elements[0] → color", "color");
}

/// Предупреждение «картинка нигде не названа» остаётся для картинки, которую не называет ни элемент,
/// ни значение свойства.
#[test]
fn a_picture_nobody_names_still_gets_the_unused_warning() {
    let scene = r##"{"objects":[
        {"position":[4,6],"size":[4,2],"place":true,"icon":"bakery"}
    ]}"##;
    let (_, _, warnings) =
        load_with("", PROPS, scene, ICON_SCREENS).expect("игра должна загрузиться");

    let unused: Vec<_> = warnings
        .iter()
        .filter(|w| w.message.contains("картинка"))
        .collect();
    assert_eq!(unused.len(), 1, "{warnings:?}");
    assert!(unused[0].message.contains("food"), "{warnings:?}");
}

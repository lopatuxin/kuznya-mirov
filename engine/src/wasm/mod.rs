use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::core::game::Game;
use crate::core::input::{KeyQueue, MouseQueue, MouseState};
use crate::core::property::{self, PropertyTable};
use crate::core::rules::Outcome;
use crate::core::runner::Runner;
use crate::core::screens::{self, ScreenState, ScreensConfig};
use crate::core::world::World;
use crate::data::error::GameError;
use crate::data::load::{self, GameConfig};
use crate::render::{DrawRect, Renderer, TextDraw};

fn set(obj: &Object, key: &str, value: &JsValue) {
    let _ = Reflect::set(obj, &JsValue::from_str(key), value);
}

fn js_optional_usize(value: Option<usize>) -> JsValue {
    match value {
        Some(n) => JsValue::from_f64(n as f64),
        None => JsValue::NULL,
    }
}

fn js_error_array(items: &[GameError]) -> Array {
    let arr = Array::new();
    for e in items {
        let item = Object::new();
        set(&item, "file", &JsValue::from_str(&e.file));
        set(&item, "path", &JsValue::from_str(&e.path));
        set(&item, "message", &JsValue::from_str(&e.message));
        set(&item, "line", &js_optional_usize(e.line));
        set(&item, "column", &js_optional_usize(e.column));
        arr.push(&item);
    }
    arr
}

/// `load()`'s success shape: warnings are collected the same way errors are, but never stop the
/// game from starting — see «Формат игры» → «Проверка данных перед запуском».
fn js_load_ok(warnings: &[GameError]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::TRUE);
    set(&obj, "warnings", &js_error_array(warnings));
    obj.into()
}

/// `load()`'s failure shape: the errors that stopped the game, next to whatever warnings were
/// collected before the check gave up.
fn js_load_err(errors: &[GameError], warnings: &[GameError]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::FALSE);
    set(&obj, "errors", &js_error_array(errors));
    set(&obj, "warnings", &js_error_array(warnings));
    obj.into()
}

/// `read_entry()`'s success shape: `files` are the paths to fetch next — three text files as
/// before, plus `screens` (a fourth text file) and `fonts` (binary files, one per `files.fonts`
/// entry in `game.json`, each named so `load()` can match the bytes back to its declaration).
/// `warnings` collected the same way `load()`'s are — see `js_load_ok`.
fn js_entry_ok(config: &GameConfig, warnings: &[GameError]) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::TRUE);
    let files = Object::new();
    set(
        &files,
        "properties",
        &JsValue::from_str(&config.files.properties),
    );
    set(&files, "scene", &JsValue::from_str(&config.files.scene));
    set(&files, "rules", &JsValue::from_str(&config.files.rules));
    set(&files, "screens", &JsValue::from_str(&config.files.screens));
    let fonts = Array::new();
    for (name, path) in &config.files.fonts {
        let entry = Object::new();
        set(&entry, "name", &JsValue::from_str(name));
        set(&entry, "path", &JsValue::from_str(path));
        fonts.push(&entry);
    }
    set(&files, "fonts", &fonts);
    set(&obj, "files", &files);
    set(&obj, "warnings", &js_error_array(warnings));
    obj.into()
}

fn js_running() -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::TRUE);
    obj.into()
}

fn js_ended(outcome: Outcome, step: u64) -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::FALSE);
    let outcome_str = match outcome {
        Outcome::Win => "win",
        Outcome::Loss => "loss",
    };
    set(&obj, "outcome", &JsValue::from_str(outcome_str));
    set(&obj, "step", &JsValue::from_f64(step as f64));
    obj.into()
}

/// Reads `fonts` — the page's `[{name, bytes: Uint8Array|null}, ...]` — into the shape
/// `load::load_rest` wants. A `null`/`undefined` `bytes` means the page could not fetch that
/// font, mirroring how a missing text file becomes `None`.
fn parse_font_bytes(fonts: &JsValue) -> Vec<(String, Option<Vec<u8>>)> {
    let arr = Array::from(fonts);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        let name = Reflect::get(&item, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let bytes = Reflect::get(&item, &JsValue::from_str("bytes"))
            .ok()
            .filter(|v| !v.is_null() && !v.is_undefined())
            .map(|v| Uint8Array::new(&v).to_vec());
        out.push((name, bytes));
    }
    out
}

fn compose_instances(game: &Game) -> Vec<DrawRect> {
    let mut ordered: Vec<(i32, u32)> = game
        .world
        .ids()
        .filter(|&id| {
            game.world.vec2(id, property::POSITION).is_some()
                && game.world.vec2(id, property::SIZE).is_some()
                && game.world.color(id, property::COLOR).is_some()
        })
        .map(|id| (game.world.layer(id, property::LAYER).unwrap_or(0), id))
        .collect();
    ordered.sort_unstable();

    ordered
        .into_iter()
        .map(|(_, id)| {
            let p = game
                .world
                .vec2(id, property::POSITION)
                .expect("filtered above");
            let s = game.world.vec2(id, property::SIZE).expect("filtered above");
            let color = game
                .world
                .color(id, property::COLOR)
                .expect("filtered above");
            DrawRect {
                position: [p[0] as f32, p[1] as f32],
                size: [s[0] as f32, s[1] as f32],
                color,
            }
        })
        .collect()
}

/// Panels and buttons become `DrawRect`s in window pixels; labels and button captions become
/// `TextDraw`s. «Интерфейс игры» → «Отрисовка»: elements draw in list order (panels can sit
/// under buttons on the same screen), text always ends up on top of every rectangle because it
/// is collected separately and drawn as its own pass, never interleaved with the rects.
fn compose_ui(
    screen: &screens::Screen,
    world: &World,
    properties: &PropertyTable,
    mouse: &MouseState,
    viewport: [f32; 2],
) -> (Vec<DrawRect>, Vec<TextDraw>) {
    let mut rects = Vec::with_capacity(screen.elements.len());
    let mut texts = Vec::new();
    for (index, element) in screen.elements.iter().enumerate() {
        match element {
            screens::Element::Panel { placement, color } => {
                rects.push(DrawRect {
                    position: placement.top_left(viewport),
                    size: placement.size,
                    color: *color,
                });
            }
            screens::Element::Label {
                placement,
                text,
                font,
                font_size,
                color,
                align,
            } => {
                let [x, y] = placement.top_left(viewport);
                texts.push(TextDraw {
                    text: screens::format_text(text, world, properties),
                    font: *font,
                    font_size_px: *font_size,
                    color: *color,
                    align: *align,
                    rect_px: [x, y, placement.size[0], placement.size[1]],
                });
            }
            screens::Element::Button {
                placement,
                text,
                font,
                font_size,
                text_color,
                color,
                color_hover,
                color_pressed,
                on_click: _,
            } => {
                let [x, y] = placement.top_left(viewport);
                let fill = *screens::button_fill(color, color_hover, color_pressed, index, mouse);
                rects.push(DrawRect {
                    position: [x, y],
                    size: placement.size,
                    color: fill,
                });
                // Button captions have no `align` field in the data — «Интерфейс игры»: they
                // always sit centered in the button.
                texts.push(TextDraw {
                    text: screens::format_text(text, world, properties),
                    font: *font,
                    font_size_px: *font_size,
                    color: *text_color,
                    align: screens::Align::Center,
                    rect_px: [x, y, placement.size[0], placement.size[1]],
                });
            }
        }
    }
    (rects, texts)
}

/// The engine, one per canvas. See the crate's README / contract notes for the exact JS-side
/// call sequence: `create` once, then `read_entry` followed by `load`, then `key_down`/`key_up`,
/// `mouse_move`/`mouse_down`/`mouse_up` and `resize`/`set_pixel_ratio` as events happen, and
/// `tick` once per `requestAnimationFrame`.
#[wasm_bindgen]
pub struct Engine {
    renderer: Renderer,
    game: Option<Game>,
    screens_config: Option<ScreensConfig>,
    screen_state: Option<ScreenState>,
    runner: Runner,
    mouse: MouseState,
    mouse_queue: MouseQueue,
    key_queue: KeyQueue,
    pending_config: load::PendingConfig,
    last_tick_ms: Option<f64>,
}

#[wasm_bindgen]
impl Engine {
    /// Sets up the GPU on `canvas` (WebGPU, falling back to WebGL2). Rejects with a
    /// Russian-language `Error` if neither is available.
    pub async fn create(canvas: HtmlCanvasElement) -> Result<Engine, JsValue> {
        console_error_panic_hook::set_once();
        let width = canvas.width().max(1);
        let height = canvas.height().max(1);
        let renderer = Renderer::new(canvas, width, height, 1, 1, [0.0, 0.0, 0.0, 1.0])
            .await
            .map_err(|e| JsValue::from_str(&e))?;
        Ok(Engine {
            renderer,
            game: None,
            screens_config: None,
            screen_state: None,
            runner: Runner::new(),
            mouse: MouseState::default(),
            mouse_queue: MouseQueue::new(),
            key_queue: KeyQueue::new(),
            pending_config: load::PendingConfig::default(),
            last_tick_ms: None,
        })
    }

    /// Which backend actually got used: `"webgpu"` or `"webgl2"`.
    pub fn backend(&self) -> String {
        self.renderer.backend().label().to_string()
    }

    /// Step one of loading a game: parses `game.json` only and reports which files (paths
    /// relative to the game folder) to fetch next — three text files plus `screens` (a fourth)
    /// and `fonts` (binary, name-tagged). Returns `{ok:true, files:{...}, warnings:[...]}` on
    /// success or `{ok:false, errors:[...], warnings:[...]}` on any prestart-check failure —
    /// same shape `load()` uses, warnings travel alongside either outcome.
    pub fn read_entry(&mut self, game_json: &str) -> JsValue {
        let result = load::read_entry(game_json);
        self.pending_config.set(&result);
        match result {
            Ok((config, warnings)) => js_entry_ok(&config, &warnings),
            Err(failure) => js_load_err(&failure.errors, &failure.warnings),
        }
    }

    /// Step two: hand over the three text files plus `screens_json` `read_entry` named
    /// (`None`/`null` for one the page could not fetch), and `fonts` — a JS array of
    /// `{name, bytes: Uint8Array|null}`, one entry per `files.fonts`. Runs the full prestart
    /// check and, on success, the game is ready to run starting from the next `tick`. Returns
    /// `{ok:true, warnings:[...]}` on success or `{ok:false, errors:[...], warnings:[...]}` on
    /// failure — warnings never stop the game from starting, but the page still needs to see
    /// them either way.
    pub fn load(
        &mut self,
        properties_json: Option<String>,
        scene_json: Option<String>,
        rules_json: Option<String>,
        screens_json: Option<String>,
        fonts: JsValue,
    ) -> JsValue {
        let Some(config) = self.pending_config.take() else {
            return js_load_err(
                &[GameError::new(
                    "engine",
                    "",
                    "load() вызван раньше успешного read_entry()",
                )],
                &[],
            );
        };
        // `config` is about to move into `load_rest` — the font name order it carries is what
        // fixes each font's `FontId`, so it has to be captured before that happens.
        let font_order: Vec<String> = config.files.fonts.iter().map(|(n, _)| n.clone()).collect();
        let font_bytes = parse_font_bytes(&fonts);
        match load::load_rest(
            config,
            properties_json.as_deref(),
            scene_json.as_deref(),
            rules_json.as_deref(),
            screens_json.as_deref(),
            &font_bytes,
        ) {
            Ok((game, screens_config, warnings)) => {
                self.renderer
                    .set_scene(game.scene.width, game.scene.height, game.scene.background);
                for name in &font_order {
                    let bytes = font_bytes
                        .iter()
                        .find(|(n, _)| n == name)
                        .and_then(|(_, b)| b.clone())
                        .unwrap_or_default();
                    self.renderer.load_font(bytes);
                }
                self.screen_state = Some(ScreenState::new(screens_config.start_screen));
                self.screens_config = Some(screens_config);
                self.game = Some(game);
                self.runner = Runner::new();
                self.mouse = MouseState::default();
                self.key_queue = KeyQueue::new();
                js_load_ok(&warnings)
            }
            Err(failure) => js_load_err(&failure.errors, &failure.warnings),
        }
    }

    /// Dropped outright on a screen with no `world_runs`, except a key named in that screen's
    /// own `keys` table — «Экраны и состояние» → «Клавиша экрана».
    pub fn key_down(&mut self, code: &str) {
        if let (Some(game), Some(config), Some(state)) = (
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_ref(),
        ) {
            screens::key_down(game, config, state, code);
        }
    }

    pub fn key_up(&mut self, code: &str) {
        if let (Some(game), Some(config), Some(state)) = (
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_ref(),
        ) {
            screens::key_up(game, config, state, &mut self.key_queue, code);
        }
    }

    /// `x`/`y` in window (CSS) pixels — the same unit `screens.json`'s `anchor`/`offset`/`size`
    /// are written in. Queued, not applied immediately — «Интерфейс игры» → «Мышь».
    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.mouse_queue.push_move(x, y);
    }

    pub fn mouse_down(&mut self) {
        self.mouse_queue.push_down();
    }

    pub fn mouse_up(&mut self) {
        self.mouse_queue.push_up();
    }

    /// Called once per `requestAnimationFrame` with `performance.now()`. Advances the fixed-step
    /// simulation (skipped entirely on a screen without `world_runs`), drains the queued mouse
    /// events against whichever screen ends up active, and draws one frame: world, interface,
    /// text.
    pub fn tick(&mut self, now_ms: f64) -> JsValue {
        let dt_seconds = match self.last_tick_ms {
            Some(prev) => ((now_ms - prev) / 1000.0).max(0.0),
            None => 0.0,
        };
        self.last_tick_ms = Some(now_ms);

        let (Some(game), Some(config), Some(state)) = (
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            let _ = self.renderer.render_frame(&[], &[], &[]);
            return js_running();
        };

        screens::tick(&mut self.runner, game, config, state, dt_seconds);
        let viewport = self.renderer.window_size_css();
        screens::process_mouse_queue(
            &mut self.mouse_queue,
            &mut self.mouse,
            game,
            config,
            state,
            viewport,
        );
        screens::process_key_queue(&mut self.key_queue, game, config, state);

        let world_instances = compose_instances(game);
        let screen = &config.screens[state.active()];
        let (ui_instances, texts) =
            compose_ui(screen, &game.world, &game.properties, &self.mouse, viewport);
        if let Err(e) = self
            .renderer
            .render_frame(&world_instances, &ui_instances, &texts)
        {
            web_sys::console::error_1(&JsValue::from_str(&format!("отрисовка не удалась: {e}")));
        }

        match game.outcome() {
            Some((outcome, step)) => js_ended(outcome, step),
            None => js_running(),
        }
    }

    /// Reconfigures the GPU surface for a new device-pixel canvas size. Call on canvas/container
    /// resize; does not touch the game.
    pub fn resize(&mut self, width_px: u32, height_px: u32) {
        self.renderer.resize(width_px, height_px);
    }

    /// The page's `window.devicePixelRatio` — the interface's own pixel values (declared in CSS
    /// pixels in `screens.json`) get multiplied by this before reaching the GPU. Defaults to 1.0
    /// until called. «Интерфейс игры» → «Плотность экрана».
    pub fn set_pixel_ratio(&mut self, ratio: f32) {
        self.renderer.set_pixel_ratio(ratio);
    }

    /// Call on `visibilitychange` going to hidden: drops accumulated real time so a multi-minute
    /// gap is not caught up in a burst.
    pub fn tab_hidden(&mut self) {
        self.runner.reset();
    }

    /// Once-only runtime warnings collected so far (max_objects reached, random_cell exhausted).
    pub fn messages(&self) -> Array {
        let arr = Array::new();
        if let Some(game) = &self.game {
            for message in game.messages() {
                arr.push(&JsValue::from_str(message));
            }
        }
        arr
    }
}

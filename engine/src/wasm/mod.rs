use js_sys::{Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::core::game::Game;
use crate::core::input::{MouseState, UiQueue};
use crate::core::property::PropertyTable;
use crate::core::rules::Outcome;
use crate::core::runner::{Runner, UiClock};
use crate::core::screens::{self, ScreenState, ScreensConfig};
use crate::core::world::World;
use crate::data::error::GameError;
use crate::data::load::{self, GameConfig, ImageDecl, ImageVerdict, MusicVerdict, NeededMedia};
use crate::render::atlas::{self, AtlasRect};
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
    // «Код игры»: только когда объявлен — страница читает его во втором заходе вместе с
    // остальными текстами, движок этот путь так же, как остальные, не читает сам.
    if let Some(code) = &config.files.code {
        set(&files, "code", &JsValue::from_str(code));
    }
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

/// A `(name, path)` table with no numbering — `read_texts()`'s `fonts`, matched back by name the
/// same way `read_entry()`'s own font list already is.
fn js_media_table(table: &[(String, String)]) -> Array {
    let arr = Array::new();
    for (name, path) in table {
        let item = Object::new();
        set(&item, "name", &JsValue::from_str(name));
        set(&item, "path", &JsValue::from_str(path));
        arr.push(&item);
    }
    arr
}

/// A `(name, path)` table numbered by each entry's position in `full` — «Звук»: that
/// position is the `SoundId`/`MusicId` the loaded game will resolve `play_sound`/`music` names
/// to, so it's what the page has to echo back in `load()`'s `sounds`/`music` arrays rather than
/// re-deriving it. `subset` may skip entries `full` has (an unreferenced track, for `music`); its
/// own position would not agree with `full`'s, so the index is looked up by name instead of
/// assumed from `enumerate()`.
fn js_indexed_media_table(subset: &[(String, String)], full: &[(String, String)]) -> Array {
    let arr = Array::new();
    for (name, path) in subset {
        let Some(index) = full.iter().position(|(n, _)| n == name) else {
            continue;
        };
        let item = Object::new();
        set(&item, "index", &JsValue::from_f64(index as f64));
        set(&item, "name", &JsValue::from_str(name));
        set(&item, "path", &JsValue::from_str(path));
        arr.push(&item);
    }
    arr
}

/// `read_texts()`'s result: which binary files are worth fetching next — «Звук» →«Загрузка и проверка». Never an error shape: a text file this step
/// couldn't parse just names fewer tracks, and the real errors surface later, from `load()`,
/// so every error from every file is collected in the one place that already does that.
/// `all_music` is `config.files.music` in full — `needed.music` only ever names a subset of it
/// (the tracks some screen actually references), so it alone can't supply the true `MusicId`.
fn js_needed_media_ok(needed: &NeededMedia, all_music: &[(String, String)]) -> JsValue {
    let obj = Object::new();
    set(&obj, "fonts", &js_media_table(&needed.fonts));
    // `needed.sounds`/`needed.images` are every declared sound/image, unfiltered — its own
    // position already is the `SoundId`/`ImageId`, so each is its own "full" table here —
    // «Картинки»: read whether or not anything names it yet, unlike `music` below.
    set(
        &obj,
        "sounds",
        &js_indexed_media_table(&needed.sounds, &needed.sounds),
    );
    set(
        &obj,
        "music",
        &js_indexed_media_table(&needed.music, all_music),
    );
    set(
        &obj,
        "images",
        &js_indexed_media_table(&needed.images, &needed.images),
    );
    obj.into()
}

fn js_running() -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::TRUE);
    obj.into()
}

/// «Код игры» → требование 20: текст ошибки во время партии называет функцию, правило
/// (`rules.json → rules[N]`) и шаг в начале `message`; место в самом файле кода — `line`, а
/// `path` пуст: правило — место в другом файле, не в `file`. Страница показывает `{file, path,
/// message, line, column}` тем же форматом, что ошибку загрузки.
fn code_error_message(rules_path: &str, err: &crate::core::code::CodeError) -> String {
    let mut parts = Vec::with_capacity(3);
    if let Some(function) = &err.function {
        parts.push(format!("функция \"{function}\""));
    }
    if let Some(rule) = &err.rule {
        parts.push(format!("правило {rules_path} → {rule}"));
    }
    if let Some(step) = err.step {
        parts.push(format!("шаг {step}"));
    }
    if parts.is_empty() {
        err.message.clone()
    } else {
        format!("{}: {}", parts.join(", "), err.message)
    }
}

/// «Код игры»: ошибка во время партии — та же форма `{running:false, error:{...}}`, что ошибка
/// загрузки принимает по всей странице, чтобы «страница показала её тем же форматом».
fn js_code_error(code_path: &str, rules_path: &str, err: &crate::core::code::CodeError) -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::FALSE);
    let error = Object::new();
    set(&error, "file", &JsValue::from_str(code_path));
    set(&error, "path", &JsValue::from_str(""));
    set(
        &error,
        "message",
        &JsValue::from_str(&code_error_message(rules_path, err)),
    );
    set(
        &error,
        "line",
        &js_optional_usize(err.line.map(|l| l as usize)),
    );
    set(&error, "column", &JsValue::NULL);
    set(&obj, "error", &error);
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

/// Resolves one `{index, ...}` entry's `index` back to the name it was numbered from in
/// `read_texts()`'s response — shared by `parse_sound_bytes` and `parse_music_verdicts`.
fn resolve_indexed_name(item: &JsValue, table: &[(String, String)]) -> Option<String> {
    let index = Reflect::get(item, &JsValue::from_str("index"))
        .ok()
        .and_then(|v| v.as_f64())?;
    if !index.is_finite() || index < 0.0 {
        return None;
    }
    table.get(index as usize).map(|(name, _)| name.clone())
}

/// Reads `sounds` — the page's `[{index, bytes: Uint8Array|null}, ...]`, numbered the way
/// `read_texts()` numbered them — into `(name, bytes)` pairs, the shape `load::load_rest` wants.
fn parse_sound_bytes(
    sounds: &JsValue,
    table: &[(String, String)],
) -> Vec<(String, Option<Vec<u8>>)> {
    let arr = Array::from(sounds);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        let Some(name) = resolve_indexed_name(&item, table) else {
            continue;
        };
        let bytes = Reflect::get(&item, &JsValue::from_str("bytes"))
            .ok()
            .filter(|v| !v.is_null() && !v.is_undefined())
            .map(|v| Uint8Array::new(&v).to_vec());
        out.push((name, bytes));
    }
    out
}

/// Reads `music` — the page's `[{index, verdict: "ok"|"missing"|"rejected"}, ...]` — into
/// `(name, MusicVerdict)` pairs. An unrecognized verdict string reads as `Missing`. An item whose
/// `index` doesn't resolve to a declared track (missing, negative, or NaN) is dropped instead of
/// guessed at — the name then simply stays absent from the result, and `validate_music_files`
/// already treats an absent name as `Missing`, so the "file not found" message still comes
/// through, not a silent pass.
fn parse_music_verdicts(
    music: &JsValue,
    table: &[(String, String)],
) -> Vec<(String, MusicVerdict)> {
    let arr = Array::from(music);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        let Some(name) = resolve_indexed_name(&item, table) else {
            continue;
        };
        let verdict = match Reflect::get(&item, &JsValue::from_str("verdict"))
            .ok()
            .and_then(|v| v.as_string())
            .as_deref()
        {
            Some("ok") => MusicVerdict::Ok,
            Some("rejected") => MusicVerdict::Rejected,
            _ => MusicVerdict::Missing,
        };
        out.push((name, verdict));
    }
    out
}

/// Reads `images` — the page's `[{index, verdict: "ok"|"missing"|"rejected", width?, height?,
/// pixels?: Uint8Array}, ...]` — into `(name, ImageVerdict)` pairs, the shape `load::load_rest`
/// wants. Mirrors `parse_music_verdicts`; `width`/`height`/`pixels` are only read when the
/// verdict is `"ok"` — «Картинки» → «Загрузка и проверка»: at any other verdict the rest of the
/// entry isn't meaningful and is not read.
fn parse_image_verdicts(
    images: &JsValue,
    table: &[(String, String)],
) -> Vec<(String, ImageVerdict)> {
    let arr = Array::from(images);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        let Some(name) = resolve_indexed_name(&item, table) else {
            continue;
        };
        let verdict = match Reflect::get(&item, &JsValue::from_str("verdict"))
            .ok()
            .and_then(|v| v.as_string())
            .as_deref()
        {
            Some("ok") => {
                let width = Reflect::get(&item, &JsValue::from_str("width"))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as u32;
                let height = Reflect::get(&item, &JsValue::from_str("height"))
                    .ok()
                    .and_then(|v| v.as_f64())
                    .unwrap_or(0.0) as u32;
                let pixels = Reflect::get(&item, &JsValue::from_str("pixels"))
                    .ok()
                    .map(|v| Uint8Array::new(&v).to_vec())
                    .unwrap_or_default();
                ImageVerdict::Ok {
                    width,
                    height,
                    pixels,
                }
            }
            Some("rejected") => ImageVerdict::Rejected,
            _ => ImageVerdict::Missing,
        };
        out.push((name, verdict));
    }
    out
}

fn draw_rect(
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    atlas_rect: AtlasRect,
    rotation_quarters: f32,
) -> DrawRect {
    DrawRect {
        position,
        size,
        color,
        atlas_pos: [atlas_rect.x as f32, atlas_rect.y as f32],
        atlas_size: [atlas_rect.w as f32, atlas_rect.h as f32],
        rotation_quarters,
    }
}

/// «Картинки» → «Кадры»: the world's own frame is picked by shots taken (`game.step_count()`),
/// frozen exactly when the world stops stepping (paused, or on the outcome screen) — «Исполнение
/// игры»: rendering never changes the world, this just reads it. The layer ordering and fill
/// choice are `atlas::compose_world_paints`'s job, natively testable; this just turns each result
/// into the GPU's own `DrawRect`.
fn compose_instances(
    game: &Game,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Vec<DrawRect> {
    let steps = game.step_count() as f64;
    atlas::compose_world_paints(&game.world, game.world.ids(), steps, images, atlas_rects)
        .into_iter()
        .map(|paint| {
            draw_rect(
                paint.position,
                paint.size,
                paint.color,
                paint.atlas_rect,
                paint.rotation_quarters as f32,
            )
        })
        .collect()
}

/// Panels and buttons become `DrawRect`s in window pixels; labels and button captions become
/// `TextDraw`s. «Интерфейс игры» → «Отрисовка»: elements draw in list order (panels can sit
/// under buttons on the same screen), text always ends up on top of every rectangle because it
/// is collected separately and drawn as its own pass, never interleaved with the rects.
/// «Картинки» → «Кадры»: `ui_elapsed_steps` is window time, already converted to steps — never
/// frozen, unlike the world's own frame in `compose_instances`.
#[allow(clippy::too_many_arguments)]
fn compose_ui(
    screen: &screens::Screen,
    world: &World,
    properties: &PropertyTable,
    mouse: &MouseState,
    viewport: [f32; 2],
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
    ui_elapsed_steps: f64,
) -> (Vec<DrawRect>, Vec<TextDraw>) {
    let mut rects = Vec::with_capacity(screen.elements.len());
    let mut texts = Vec::new();
    for (index, element) in screen.elements.iter().enumerate() {
        match element {
            screens::Element::Panel { placement, fill } => {
                let (color, atlas_rect) =
                    atlas::fill_paint(fill, ui_elapsed_steps, images, atlas_rects);
                rects.push(draw_rect(
                    placement.top_left(viewport),
                    placement.size,
                    color,
                    atlas_rect,
                    0.0,
                ));
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
                fill,
                fill_hover,
                fill_pressed,
                on_click: _,
            } => {
                let [x, y] = placement.top_left(viewport);
                let chosen = screens::button_fill(fill, fill_hover, fill_pressed, index, mouse);
                let (color, atlas_rect) =
                    atlas::fill_paint(chosen, ui_elapsed_steps, images, atlas_rects);
                rects.push(draw_rect([x, y], placement.size, color, atlas_rect, 0.0));
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
/// call sequence: `create` once, then loading in three passes — `read_entry`, `read_texts`,
/// `load` — «Звук» → «Загрузка и проверка», then `key_down`/
/// `key_up`, `mouse_move`/`mouse_down`/`mouse_up` and `resize`/`set_pixel_ratio` as events
/// happen, and `tick` once per `requestAnimationFrame`.
#[wasm_bindgen]
pub struct Engine {
    renderer: Renderer,
    game: Option<Game>,
    screens_config: Option<ScreensConfig>,
    screen_state: Option<ScreenState>,
    runner: Runner,
    mouse: MouseState,
    ui_queue: UiQueue,
    pending_config: load::PendingConfig,
    /// «Картинки»: `files.images`, in `ImageId` order — kept here (not just inside `Game`) so
    /// `compose_instances`/`compose_ui` can look up each image's `frames`/`frame_steps` without
    /// the core knowing anything about frames or atlases.
    images: Vec<ImageDecl>,
    /// Where `load()`'s atlas build put each declared image's whole strip — parallel to `images`.
    atlas_rects: Vec<AtlasRect>,
    /// «Код игры» → требование 20: `files.rules` of the loaded game — a code error names the rule
    /// that called the function as `rules.json → rules[N]`, and the core only knows `rules[N]`.
    rules_path: String,
    /// «Картинки» → «Кадры»: window time for the interface's own image frames — unlike the
    /// world's own step counter, this never freezes: it keeps advancing on a paused or menu
    /// screen, which is exactly why the interface's own images keep animating there.
    ui_clock: UiClock,
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
            ui_queue: UiQueue::new(),
            pending_config: load::PendingConfig::default(),
            images: Vec::new(),
            atlas_rects: Vec::new(),
            rules_path: String::new(),
            ui_clock: UiClock::new(),
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
        self.pending_config.set(&result, game_json);
        match result {
            Ok((config, warnings)) => js_entry_ok(&config, &warnings),
            Err(failure) => js_load_err(&failure.errors, &failure.warnings),
        }
    }

    /// Step two of loading — «Звук» → «Загрузка и проверка»: a
    /// best-effort read of `properties_json`/`screens_json` (`scene`/`rules` play no part in the
    /// answer and are accepted only for symmetry with `read_entry`'s own file list) that reports
    /// which binary files are worth fetching next — `fonts` (all of them), `sounds` (all of
    /// them, numbered), `music` (only the tracks some screen actually names, numbered). Never
    /// fails: a text file that doesn't parse here just names fewer tracks, and the real errors
    /// surface later, from `load()`, so every error from every file still collects in one place.
    /// Returns `{fonts:[{name,path}], sounds:[{index,name,path}], music:[{index,name,path}]}`; an
    /// empty answer if called before a successful `read_entry()`.
    pub fn read_texts(
        &self,
        properties_json: Option<String>,
        _scene_json: Option<String>,
        _rules_json: Option<String>,
        screens_json: Option<String>,
        // «Код игры»: движок его здесь не читает — принят только для единообразия с остальными
        // текстами второго захода; `load()` читает его по-настоящему.
        _code_json: Option<String>,
    ) -> JsValue {
        let Some(config) = self.pending_config.peek() else {
            return js_needed_media_ok(&NeededMedia::default(), &[]);
        };
        let needed = load::read_texts(config, properties_json.as_deref(), screens_json.as_deref());
        js_needed_media_ok(&needed, &config.files.music)
    }

    /// Step three: hand over the four text files, `fonts` — `[{name, bytes: Uint8Array|null}]`,
    /// one per `files.fonts` — `sounds` — `[{index, bytes: Uint8Array|null}]`, one per
    /// `read_texts()`'s `sounds` — and `music` — `[{index, verdict: "ok"|"missing"|"rejected"}]`,
    /// one per `read_texts()`'s `music`, the executor's (browser's) answer to "can you decompress
    /// this track". Runs the full prestart check and, on success, the game is ready to run
    /// starting from the next `tick`. Returns `{ok:true, warnings:[...]}` on success or
    /// `{ok:false, errors:[...], warnings:[...]}` on failure — warnings never stop the game from
    /// starting, but the page still needs to see them either way.
    #[allow(clippy::too_many_arguments)]
    pub fn load(
        &mut self,
        properties_json: Option<String>,
        scene_json: Option<String>,
        rules_json: Option<String>,
        screens_json: Option<String>,
        fonts: JsValue,
        sounds: JsValue,
        music: JsValue,
        images: JsValue,
        code_json: Option<String>,
    ) -> JsValue {
        let Some((config, game_json)) = self.pending_config.take() else {
            return js_load_err(
                &[GameError::new(
                    "engine",
                    "",
                    "load() вызван раньше успешного read_entry()",
                )],
                &[],
            );
        };
        // `config` is about to move into `load_rest` — the font/sound/music/image name order it
        // carries is what fixes each one's FontId/SoundId/MusicId/ImageId, so it has to be
        // captured before that happens.
        let font_order: Vec<String> = config.files.fonts.iter().map(|(n, _)| n.clone()).collect();
        let rules_path = config.files.rules.clone();
        let image_table: Vec<(String, String)> = config
            .files
            .images
            .iter()
            .map(|decl| (decl.name.clone(), decl.path.clone()))
            .collect();
        let font_bytes = parse_font_bytes(&fonts);
        let sound_bytes = parse_sound_bytes(&sounds, &config.files.sounds);
        let music_verdicts = parse_music_verdicts(&music, &config.files.music);
        let image_verdicts = parse_image_verdicts(&images, &image_table);
        match load::load_rest(
            &game_json,
            config,
            properties_json.as_deref(),
            scene_json.as_deref(),
            rules_json.as_deref(),
            screens_json.as_deref(),
            &font_bytes,
            &sound_bytes,
            &music_verdicts,
            &image_verdicts,
            code_json.as_deref(),
            false,
        ) {
            Ok((game, screens_config, warnings, image_order)) => {
                // «Картинки» → «Атлас и отрисовка»: `load_rest` just checked every declared
                // image's own verdict/dimensions, but not whether the whole set fits in one
                // 2048×2048 canvas — that's the render layer's job, so it only runs once the
                // data layer's own check has already passed.
                //
                // Looked up the same way `validate_image_files` looks a name up in this same
                // vector — the first match by name — rather than through a `HashMap`: a
                // duplicate `index` from the page (`parse_image_verdicts` builds this vector by
                // index) gives two entries for one name, and a `HashMap::from_iter` would keep
                // the last of them while validation judged the first, so the two could disagree
                // on which verdict a name actually has. `Vec::remove` takes the entry by value,
                // so the pixel buffer moves into `AtlasImage` rather than being copied.
                let mut image_verdicts = image_verdicts;
                let atlas_images: Vec<atlas::AtlasImage> = image_order
                    .iter()
                    .map(|decl| {
                        let pos = image_verdicts
                            .iter()
                            .position(|(name, _)| name == &decl.name)
                            .expect(
                                "load_rest succeeded: every declared image already has a verdict",
                            );
                        let (_, verdict) = image_verdicts.remove(pos);
                        match verdict {
                            ImageVerdict::Ok {
                                width,
                                height,
                                pixels,
                            } => atlas::AtlasImage {
                                width,
                                height,
                                pixels,
                            },
                            ImageVerdict::Missing | ImageVerdict::Rejected => unreachable!(
                                "load_rest succeeded: every declared image verdict is ok"
                            ),
                        }
                    })
                    .collect();
                let atlas_rects = match self.renderer.build_atlas(&atlas_images) {
                    Ok(rects) => rects,
                    // `atlas::pack` already writes its own complete message (not fitting isn't
                    // the only failure it reports — an oversized image or a wrong pixel count
                    // are, too), so it goes out verbatim rather than under a second, guessed-at
                    // header.
                    Err(e) => {
                        self.clear_game();
                        return js_load_err(
                            &[GameError::new("game.json", "files → images", e)],
                            &warnings,
                        );
                    }
                };
                self.atlas_rects = atlas_rects;
                self.images = image_order;
                self.rules_path = rules_path;
                self.ui_clock.reset();
                self.renderer
                    .set_scene(game.scene.width, game.scene.height, game.scene.background);
                // «Редактор», требование 20: повторный `load` не копит шрифты прошлой игры.
                self.renderer.reset_fonts();
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
                self.ui_queue = UiQueue::new();
                js_load_ok(&warnings)
            }
            Err(failure) => {
                self.clear_game();
                js_load_err(&failure.errors, &failure.warnings)
            }
        }
    }

    /// «Редактор», требование 20: a failed `load()` leaves the engine exactly as before any game
    /// was ever loaded — `show_scene`/`draw`/`object_at`/`object_rect` all see no game, and fonts
    /// don't linger for a repeated `load()` to pile onto.
    fn clear_game(&mut self) {
        self.game = None;
        self.screens_config = None;
        self.screen_state = None;
        self.images = Vec::new();
        self.atlas_rects = Vec::new();
        self.rules_path = String::new();
        self.renderer.reset_fonts();
    }

    /// «Редактор», требование 16: rebuilds the world from the loaded scene, ready for `draw` —
    /// no partiya, no code, no initial values. Does nothing without a successfully loaded game.
    pub fn show_scene(&mut self) {
        if let Some(game) = self.game.as_mut() {
            game.show_scene();
        }
    }

    /// «Редактор», требование 17: draws one frame of the world alone — no interface, no text; an
    /// image's own frame is the world's frozen step, same as `tick`'s own `compose_instances`. An
    /// empty frame without a loaded game.
    pub fn draw(&mut self) {
        let world_instances = match self.game.as_ref() {
            Some(game) => compose_instances(game, &self.images, &self.atlas_rects),
            None => Vec::new(),
        };
        if let Err(e) = self.renderer.render_frame(&world_instances, &[], &[]) {
            web_sys::console::error_1(&JsValue::from_str(&format!("отрисовка не удалась: {e}")));
        }
    }

    /// «Редактор», требование 18: the topmost object at `(x, y)` — canvas CSS pixels, canvas
    /// top-left origin — or `undefined` off every object, in the letterboxed scene's own margin,
    /// or without a loaded game. See `core::scene::object_at`.
    pub fn object_at(&self, x: f32, y: f32) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return JsValue::UNDEFINED;
        };
        let viewport = self.renderer.window_size_css();
        match crate::core::scene::object_at(&game.world, &game.scene, [x, y], viewport) {
            Some(id) => JsValue::from_f64(id as f64),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требование 19: object `id`'s canvas rectangle — `{x, y, width, height}` in CSS
    /// pixels — or `undefined` without a loaded game, without that object, or without its own
    /// `position`/`size`. See `core::scene::object_rect`.
    pub fn object_rect(&self, id: u32) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return JsValue::UNDEFINED;
        };
        let viewport = self.renderer.window_size_css();
        match crate::core::scene::object_rect(&game.world, &game.scene, id, viewport) {
            Some(rect) => {
                let obj = Object::new();
                set(&obj, "x", &JsValue::from_f64(rect.x as f64));
                set(&obj, "y", &JsValue::from_f64(rect.y as f64));
                set(&obj, "width", &JsValue::from_f64(rect.width as f64));
                set(&obj, "height", &JsValue::from_f64(rect.height as f64));
                obj.into()
            }
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требование 30: moves object `id` to `(x, y)` — scene cells — in the world
    /// alone. The scene `show_scene` built the world from, and the files on disk, are untouched:
    /// the next `show_scene` rebuilds the world from the scene as it was. Does nothing without a
    /// loaded game, without that object, or without its own `position`. See
    /// `core::scene::move_object`.
    pub fn move_object(&mut self, id: u32, x: f32, y: f32) {
        if let Some(game) = self.game.as_mut() {
            crate::core::scene::move_object(&mut game.world, id, [x as f64, y as f64]);
        }
    }

    /// Queued, not applied immediately — judged against whichever screen is active once `tick`
    /// drains the queue, same as a mouse event: dropped outright on a screen with no
    /// `world_runs`, except a key named in that screen's own `keys` table — «Экраны и
    /// состояние» → «Клавиши экрана».
    pub fn key_down(&mut self, code: &str) {
        self.ui_queue.push_key_down(code);
    }

    pub fn key_up(&mut self, code: &str) {
        self.ui_queue.push_key_up(code);
    }

    /// `x`/`y` in window (CSS) pixels — the same unit `screens.json`'s `anchor`/`offset`/`size`
    /// are written in. Queued for the interface's own hover/click handling («Интерфейс игры» →
    /// «Мышь»); the world cursor («Курсор в мире», требование 25–27) is updated right here
    /// instead, whenever a game is already loaded — regardless of whether the active screen is
    /// live, so a move made during a pause is still known once the world starts stepping again.
    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.ui_queue.push_mouse_move(x, y);
        if let Some(game) = self.game.as_mut() {
            game.update_cursor([x, y], self.renderer.window_size_css());
        }
    }

    pub fn mouse_down(&mut self) {
        self.ui_queue.push_mouse_down();
    }

    pub fn mouse_up(&mut self) {
        self.ui_queue.push_mouse_up();
    }

    /// Called once per `requestAnimationFrame` with `performance.now()`. Runs `screens::engine_call`
    /// (queue, then steps, then the sound window — see its own doc comment for why in that order)
    /// and draws one frame on top: world, interface, text.
    pub fn tick(&mut self, now_ms: f64) -> JsValue {
        let dt_seconds = self.runner.dt_since_last_tick(now_ms);
        // «Картинки» → «Кадры»: counted from an absolute timestamp, not accumulated deltas — a
        // hidden tab calls neither `tick()` nor `dt_since_last_tick()`, so a delta-based sum
        // would stand still across the gap; the browser's own clock kept running through it, so
        // the interface's frame has to jump forward on return, not stay put.
        let ui_clock_steps = self.ui_clock.elapsed_steps(now_ms);

        let (Some(game), Some(config), Some(state)) = (
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            // No game loaded yet (or load failed): nothing will ever drain `ui_queue` on this
            // path, so `key_down`/`key_up`/`mouse_*` piling onto a page stuck here would grow it
            // without bound.
            self.ui_queue = UiQueue::new();
            let _ = self.renderer.render_frame(&[], &[], &[]);
            return js_running();
        };

        let viewport = self.renderer.window_size_css();
        screens::engine_call(
            &mut self.ui_queue,
            &mut self.mouse,
            &mut self.runner,
            game,
            config,
            state,
            viewport,
            dt_seconds,
        );

        // «Код игры»: ошибка кода останавливает игру на месте — страница показывает её вместо
        // игры, тем же форматом, что ошибку загрузки; кадр в таком виде мира не рисуется.
        if let Some(err) = game.code_error() {
            return js_code_error(game.code_path(), &self.rules_path, err);
        }

        let world_instances = compose_instances(game, &self.images, &self.atlas_rects);
        let screen = &config.screens[state.active()];
        let (ui_instances, texts) = compose_ui(
            screen,
            &game.world,
            &game.properties,
            &self.mouse,
            viewport,
            &self.images,
            &self.atlas_rects,
            ui_clock_steps,
        );
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
    /// until called. «Интерфейс игры» → «Раскладка».
    pub fn set_pixel_ratio(&mut self, ratio: f32) {
        self.renderer.set_pixel_ratio(ratio);
    }

    /// Call on `visibilitychange` going to hidden: drops accumulated real time so a multi-minute
    /// gap is not caught up in a burst, and forgets the last tick's timestamp so the next `tick()`
    /// does not compute its `dt_seconds` against it — see `Runner::forget_last_tick`.
    pub fn tab_hidden(&mut self) {
        self.runner.reset();
        self.runner.forget_last_tick();
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

    /// «Звук» → «Окно чисел»: address of the sound window inside
    /// this engine's own wasm memory — read it as `new Int32Array(memory.buffer, ptr, len)`,
    /// after every `tick()` returns. Allocated once, at `load()`; null before that.
    pub fn sound_window_ptr(&self) -> *const i32 {
        self.game
            .as_ref()
            .map(|g| g.sound_window().as_ptr())
            .unwrap_or(std::ptr::null())
    }

    /// Length of that window, in `i32` elements (not bytes) — 0 before `load()`.
    pub fn sound_window_len(&self) -> usize {
        self.game
            .as_ref()
            .map(|g| g.sound_window().cell_count())
            .unwrap_or(0)
    }
}

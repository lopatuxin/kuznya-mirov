use js_sys::{Array, Float64Array, Object, Reflect, Uint8Array};
use wasm_bindgen::prelude::*;
use web_sys::HtmlCanvasElement;

use crate::core::camera::{Camera3d, EditorCamera};
use crate::core::game::Game;
use crate::core::input::{MouseState, UiQueue};
use crate::core::math3::Vec3;
use crate::core::property::{self, PropertyTable};
use crate::core::report::{DeleteCause, RuleFired, StepReport};
use crate::core::rules::Outcome;
use crate::core::runner::{Runner, UiClock};
use crate::core::scene::{CellRange, GroundLayer, ObjectTransform, pointer_hit, terrain_hit};
use crate::core::screens::{self, ScreenState, ScreensConfig};
use crate::core::world::World;
use crate::core::world_elements;
use crate::data::edit;
use crate::data::error::GameError;
use crate::data::load::{self, GameConfig, ImageDecl, ImageVerdict, MusicVerdict, NeededMedia};
use crate::data::session::{self, PlaySession};
use crate::render::atlas::{self, AtlasRect};
use crate::render::materials::Relief;
use crate::render::relief::TerrainMesh;
use crate::render::scene3d::{self, Frame3d};
use crate::render::{
    DrawRect, Globals3d, GroundVertex, Renderer, Scene3dFrame, ShapeInstance, TextDraw,
    WorldTextDraw,
};

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
    // «Рельеф»: путь файла высот — страница читает его во втором заходе вместе с таблицами.
    if let Some(terrain) = &config.files.terrain {
        set(&files, "terrain", &JsValue::from_str(terrain));
    }
    let fonts = Array::new();
    for (name, path) in &config.files.fonts {
        let entry = Object::new();
        set(&entry, "name", &JsValue::from_str(name));
        set(&entry, "path", &JsValue::from_str(path));
        fonts.push(&entry);
    }
    set(&files, "fonts", &fonts);
    // «Таблицы данных», требование 25: пути таблиц, названные с их именами — страница читает их
    // во втором заходе вместе с остальными текстами, тем же общим загрузчиком.
    set(&files, "tables", &js_media_table(&config.files.tables));
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

/// Пути карт или масок с их номерами: страница отвечает на них по номеру.
fn js_numbered_paths(paths: &[String]) -> Array {
    let arr = Array::new();
    for (index, path) in paths.iter().enumerate() {
        let item = Object::new();
        set(&item, "index", &JsValue::from_f64(index as f64));
        set(&item, "path", &JsValue::from_str(path));
        arr.push(&item);
    }
    arr
}

/// Ответ на путь, которым ключом служит сам путь: карты материалов и маски покрытий.
fn path_table(paths: &[String]) -> Vec<(String, String)> {
    paths
        .iter()
        .map(|path| (path.clone(), path.clone()))
        .collect()
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
    set(&obj, "materials", &js_numbered_paths(&needed.materials));
    set(&obj, "masks", &js_numbered_paths(&needed.masks));
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

fn outcome_str(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Win => "win",
        Outcome::Loss => "loss",
    }
}

fn js_ended(outcome: Outcome, step: u64) -> JsValue {
    let obj = Object::new();
    set(&obj, "running", &JsValue::FALSE);
    set(&obj, "outcome", &JsValue::from_str(outcome_str(outcome)));
    set(&obj, "step", &JsValue::from_f64(step as f64));
    obj.into()
}

/// «Редактор», требования 9, 33: `step()` reports the same running/ended/error shape `tick()`
/// does — a code error or the game's own outcome is exactly as visible on a single manual step as
/// on any other, instead of the page having no way to see it.
fn game_tick_result(game: &Game, code_path: &str, rules_path: &str) -> JsValue {
    if let Some(err) = game.code_error() {
        return js_code_error(code_path, rules_path, err);
    }
    match game.outcome() {
        Some((outcome, step)) => js_ended(outcome, step),
        None => js_running(),
    }
}

/// «Редактор»: a JSON value the wasm boundary itself never inspects (a live edit's own value, or
/// `object_properties`'s own answer) round-trips through the browser's `JSON` object — the same
/// text `serde_json` already produces/parses, just handed across the boundary as a real JS value
/// instead of a string the caller would have to `JSON.parse` itself.
fn json_to_js(value: &serde_json::Value) -> JsValue {
    js_sys::JSON::parse(&value.to_string()).unwrap_or(JsValue::NULL)
}

fn js_to_json(value: &JsValue) -> serde_json::Value {
    js_sys::JSON::stringify(value)
        .ok()
        .and_then(|s| s.as_string())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or(serde_json::Value::Null)
}

/// «Редактор», требование 43: the shared `{ok:true}`/`{ok:false, error}` shape every live-edit
/// call answers with — `set_property`/`remove_property`/`delete_object` on success carry nothing
/// else; `add_object` adds its own `id` on top.
fn js_edit_ok() -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::TRUE);
    obj.into()
}

fn js_edit_err(message: &str) -> JsValue {
    let obj = Object::new();
    set(&obj, "ok", &JsValue::FALSE);
    set(&obj, "error", &JsValue::from_str(message));
    obj.into()
}

fn js_edit_result(result: Result<(), String>) -> JsValue {
    match result {
        Ok(()) => js_edit_ok(),
        Err(message) => js_edit_err(&message),
    }
}

/// «Редактор», требование 23, 44: `Game::last_report()`'s JS shape — `rules` in file order,
/// `created`/`deleted` with names, `screen_change`/`outcome` when either happened this step.
fn js_step_report(report: &StepReport) -> JsValue {
    let obj = Object::new();
    set(&obj, "step", &JsValue::from_f64(report.step as f64));

    let rules = Array::new();
    for fired in &report.fired {
        let item = Object::new();
        match fired {
            RuleFired::Move { rule, objects } => {
                set(&item, "rule", &JsValue::from_str(rule));
                set(&item, "kind", &JsValue::from_str("move"));
                set(&item, "objects", &js_id_array(objects));
            }
            RuleFired::Check { rule, objects } => {
                set(&item, "rule", &JsValue::from_str(rule));
                set(&item, "kind", &JsValue::from_str("check"));
                set(&item, "objects", &js_id_array(objects));
            }
            RuleFired::Collide { rule, pairs } => {
                set(&item, "rule", &JsValue::from_str(rule));
                set(&item, "kind", &JsValue::from_str("collide"));
                let arr = Array::new();
                for (a, b) in pairs {
                    let pair = Array::new();
                    pair.push(&JsValue::from_f64(*a as f64));
                    pair.push(&JsValue::from_f64(*b as f64));
                    arr.push(&pair);
                }
                set(&item, "pairs", &arr);
            }
            RuleFired::Delete { rule, objects } => {
                set(&item, "rule", &JsValue::from_str(rule));
                set(&item, "kind", &JsValue::from_str("delete"));
                set(&item, "objects", &js_id_array(objects));
            }
            RuleFired::Spawn { rule, objects } => {
                set(&item, "rule", &JsValue::from_str(rule));
                set(&item, "kind", &JsValue::from_str("spawn"));
                set(&item, "objects", &js_id_array(objects));
            }
            RuleFired::Walk { rule, objects } => {
                set(&item, "rule", &JsValue::from_str(rule));
                set(&item, "kind", &JsValue::from_str("walk"));
                set(&item, "objects", &js_id_array(objects));
            }
        }
        rules.push(&item);
    }
    set(&obj, "rules", &rules);

    let created = Array::new();
    for c in &report.created {
        let item = Object::new();
        set(&item, "id", &JsValue::from_f64(c.id as f64));
        set(&item, "name", &js_optional_string(c.name.as_deref()));
        set(&item, "rule", &JsValue::from_str(&c.rule));
        created.push(&item);
    }
    set(&obj, "created", &created);

    let deleted = Array::new();
    for d in &report.deleted {
        let item = Object::new();
        set(&item, "id", &JsValue::from_f64(d.id as f64));
        set(&item, "name", &js_optional_string(d.name.as_deref()));
        let cause = Object::new();
        match &d.cause {
            DeleteCause::Rule(rule) => {
                set(&cause, "kind", &JsValue::from_str("rule"));
                set(&cause, "rule", &JsValue::from_str(rule));
            }
            DeleteCause::Code { function, rule } => {
                set(&cause, "kind", &JsValue::from_str("code"));
                set(&cause, "function", &JsValue::from_str(function));
                set(&cause, "rule", &JsValue::from_str(rule));
            }
            DeleteCause::LifetimeExpired => {
                set(&cause, "kind", &JsValue::from_str("lifetime"));
            }
        }
        set(&item, "cause", &cause);
        deleted.push(&item);
    }
    set(&obj, "deleted", &deleted);

    match &report.screen_change {
        Some((from, to)) => {
            let change = Object::new();
            set(&change, "from", &JsValue::from_str(from));
            set(&change, "to", &JsValue::from_str(to));
            set(&obj, "screenChange", &change);
        }
        None => set(&obj, "screenChange", &JsValue::NULL),
    }
    match report.outcome {
        Some(outcome) => set(&obj, "outcome", &JsValue::from_str(outcome_str(outcome))),
        None => set(&obj, "outcome", &JsValue::NULL),
    }
    obj.into()
}

fn js_id_array(ids: &[u32]) -> Array {
    let arr = Array::new();
    for &id in ids {
        arr.push(&JsValue::from_f64(id as f64));
    }
    arr
}

fn js_optional_string(s: Option<&str>) -> JsValue {
    match s {
        Some(s) => JsValue::from_str(s),
        None => JsValue::NULL,
    }
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

/// Reads `tables` — the page's `[{name, text: string|null}, ...]` — into the shape
/// `load::load_rest_with_tables` wants. A `null`/`undefined` `text` means the page could not fetch
/// that table's file, mirroring how a missing text file becomes `None` elsewhere — «Таблицы
/// данных», требование 25.
fn parse_table_texts(tables: &JsValue) -> Vec<(String, Option<String>)> {
    let arr = Array::from(tables);
    let mut out = Vec::with_capacity(arr.length() as usize);
    for item in arr.iter() {
        let name = Reflect::get(&item, &JsValue::from_str("name"))
            .ok()
            .and_then(|v| v.as_string())
            .unwrap_or_default();
        let text = Reflect::get(&item, &JsValue::from_str("text"))
            .ok()
            .filter(|v| !v.is_null() && !v.is_undefined())
            .and_then(|v| v.as_string());
        out.push((name, text));
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

fn js_point(point: [f64; 2]) -> JsValue {
    let pair = Array::new();
    pair.push(&JsValue::from_f64(point[0]));
    pair.push(&JsValue::from_f64(point[1]));
    pair.into()
}

fn js_point3(point: Vec3) -> JsValue {
    let triple = Array::new();
    for coordinate in point {
        triple.push(&JsValue::from_f64(coordinate));
    }
    triple.into()
}

fn js_editor_camera(camera: &EditorCamera) -> JsValue {
    let obj = Object::new();
    set(
        &obj,
        "target",
        &js_point3([camera.target[0], camera.target[1], camera.target_z]),
    );
    set(&obj, "yaw", &JsValue::from_f64(camera.yaw));
    set(&obj, "pitch", &JsValue::from_f64(camera.pitch));
    set(&obj, "distance", &JsValue::from_f64(camera.distance));
    obj.into()
}

/// A finite number stored under `key` of a JS object.
fn js_finite(object: &JsValue, key: &str) -> Option<f64> {
    Reflect::get(object, &JsValue::from_str(key))
        .ok()
        .and_then(|v| v.as_f64())
        .filter(|v| v.is_finite())
}

/// A pair of finite numbers stored under `key` of a JS object.
fn js_finite_pair(object: &JsValue, key: &str) -> Option<[f64; 2]> {
    let pair = Reflect::get(object, &JsValue::from_str(key)).ok()?;
    let pair = pair.dyn_into::<Array>().ok()?;
    let value = |index| pair.get(index).as_f64().filter(|v| v.is_finite());
    Some([value(0)?, value(1)?])
}

/// «Рельеф»: `[x, y]` или `[x, y, z]` под `key` — место и, если названа, высота основания.
fn js_place(object: &JsValue, key: &str) -> Option<([f64; 2], Option<f64>)> {
    let place = Reflect::get(object, &JsValue::from_str(key)).ok()?;
    let place = place.dyn_into::<Array>().ok()?;
    let value = |index| place.get(index).as_f64().filter(|v| v.is_finite());
    match place.length() {
        2 => Some(([value(0)?, value(1)?], None)),
        3 => Some(([value(0)?, value(1)?], Some(value(2)?))),
        _ => None,
    }
}

/// `transform_object`'s `{position, size, height?, rotation?}`, `position` — `[x, y]` или `[x, y, z]`;
/// `None` when `position` or `size` is not a pair of numbers, or a named `height`/`rotation` is not a
/// finite number.
fn parse_transform(t: &JsValue) -> Option<ObjectTransform> {
    let optional = |key: &str| -> Option<Option<f64>> {
        match Reflect::get(t, &JsValue::from_str(key)) {
            Ok(v) if v.is_undefined() || v.is_null() => Some(None),
            _ => js_finite(t, key).map(Some),
        }
    };
    let (position, z) = js_place(t, "position")?;
    Some(ObjectTransform {
        position,
        z,
        size: js_finite_pair(t, "size")?,
        height: optional("height")?,
        rotation: optional("rotation")?,
    })
}

/// `editor_camera`'s `{target, yaw, pitch, distance}`, `target` — `[x, y]` или `[x, y, z]`; `None` when any
/// of them is not a finite number. Камера без `z` в `target` стоит на высоте 0, а флаг говорит, что
/// высоту надо взять из рельефа.
fn parse_editor_camera(c: &JsValue) -> Option<(EditorCamera, bool)> {
    let (target, target_z) = js_place(c, "target")?;
    let camera = EditorCamera {
        target,
        target_z: target_z.unwrap_or(0.0),
        yaw: js_finite(c, "yaw")?,
        pitch: js_finite(c, "pitch")?,
        distance: js_finite(c, "distance")?,
    };
    Some((camera, target_z.is_none()))
}

/// `set_terrain`'s `water`: `{level, color}` или `undefined`. Уровень не число и цвет не строка
/// доходят до ядра как `NaN` и пустая строка — ядро отвечает на них ошибкой.
fn parse_water(water: &JsValue) -> Option<(f64, String)> {
    if water.is_undefined() || water.is_null() {
        return None;
    }
    let level = Reflect::get(water, &JsValue::from_str("level"))
        .ok()
        .and_then(|v| v.as_f64())
        .unwrap_or(f64::NAN);
    let color = Reflect::get(water, &JsValue::from_str("color"))
        .ok()
        .and_then(|v| v.as_string())
        .unwrap_or_default();
    Some((level, color))
}

fn draw_rect(
    position: [f32; 2],
    size: [f32; 2],
    color: [f32; 4],
    atlas_rect: AtlasRect,
    rotation_quarters: f32,
    smooth: bool,
    flip_x: bool,
) -> DrawRect {
    DrawRect {
        position,
        size,
        color,
        atlas_pos: [atlas_rect.x as f32, atlas_rect.y as f32],
        atlas_size: [atlas_rect.w as f32, atlas_rect.h as f32],
        rotation_quarters,
        atlas_layer: atlas_rect.sheet as f32,
        smooth: smooth as u8 as f32,
        flip_x: flip_x as u8 as f32,
    }
}

/// «Картинки» → «Кадры»: the world's own frame is picked by shots taken (`game.step_count()`),
/// frozen exactly when the world stops stepping (paused, or on the outcome screen) — «Исполнение
/// игры»: rendering never changes the world, this just reads it. The layer ordering and fill
/// choice are `atlas::compose_world_paints`'s job, natively testable; this just turns each result
/// into the GPU's own `DrawRect`.
/// «Камера»: this call's own scale/offset — the core camera in a battle, the plain
/// wall-to-wall letterbox outside one (требование 41). A free function (not an `Engine` method)
/// so `tick`/`step`/`seek`/`step_back` can call it while they already hold `game` mutably
/// borrowed from `self.game` — a `&self`/`&mut self` method there would conflict with that borrow.
fn frame_for(game: &Game, battle_view: bool, viewport: [f32; 2]) -> View {
    if game.scene.camera.is_some() {
        // «Редактор», «Сцена»: вне партии трёхмерная сцена видна камерой редактора.
        let space = if battle_view {
            game.camera_3d(viewport)
        } else {
            game.editor_camera_3d(viewport)
        };
        if let Some(camera) = space {
            return View::Space(camera);
        }
    }
    let (scale, offset) = if battle_view {
        game.camera_frame(viewport)
    } else {
        let scene_cells = [game.scene.width as f32, game.scene.height as f32];
        crate::core::scene::letterbox(viewport, scene_cells)
    };
    View::Flat { scale, offset }
}

/// Как мир виден в этом кадре: плоско, масштабом и сдвигом сцены, или трёхмерной камерой.
enum View {
    Flat { scale: f32, offset: [f32; 2] },
    Space(Camera3d),
}

/// «Курсор в мире», «Трёхмерная сцена» → «Мышь», требования 21, 23: точка под курсором в окне (в
/// трёхмерной сцене — с высотой) и место камеры, от которого щелчок идёт лучом. В трёхмерной сцене
/// точка не прижата к краю сцены: прижимает её шаг там, где она идёт как место сцены. Над горизонтом
/// камеры редактора — её точка на земле.
fn pointer_at(game: &Game, view: &View, window: [f32; 2]) -> (Vec3, Option<Vec3>) {
    match view {
        View::Flat { scale, offset } => {
            let cell = game.scene.window_to_scene_frame(window, *scale, *offset);
            ([cell[0], cell[1], 0.0], None)
        }
        View::Space(camera) => {
            let point = pointer_hit(
                &game.world,
                &game.scene,
                camera,
                [window[0] as f64, window[1] as f64],
            )
            .unwrap_or([camera.target[0], camera.target[1], camera.target_z]);
            (point, Some(camera.eye))
        }
    }
}

/// «Камера», требование 12: recomputes the world cursor from the last known mouse position after
/// a step may have moved the camera under it — a no-op in replay, where only the recording itself
/// ever sets the cursor (требование 39), and while the mouse has not moved yet: an unknown point
/// stays unknown («Крайние случаи»).
fn recompute_cursor_after_step(
    game: &mut Game,
    battle_view: bool,
    viewport: [f32; 2],
    last_mouse_window_pos: Option<[f32; 2]>,
    is_replay: bool,
    session: Option<&mut PlaySession>,
) {
    if is_replay {
        return;
    }
    let Some(last_mouse_window_pos) = last_mouse_window_pos else {
        return;
    };
    let view = frame_for(game, battle_view, viewport);
    let (point, eye) = pointer_at(game, &view, last_mouse_window_pos);
    if game.cursor_point() == Some(point) && game.cursor_eye() == eye {
        return;
    }
    game.set_cursor_point(point, eye);
    if let Some(session) = session {
        session.record_cursor(game);
    }
}

fn to_draw_rects(paints: Vec<atlas::RectPaint>) -> Vec<DrawRect> {
    paints
        .into_iter()
        .map(|paint| {
            draw_rect(
                paint.position,
                paint.size,
                paint.color,
                paint.atlas_rect,
                paint.rotation_quarters as f32,
                paint.smooth,
                paint.flip_x,
            )
        })
        .collect()
}

fn compose_instances(
    game: &Game,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Vec<DrawRect> {
    let steps = game.step_count() as f64;
    to_draw_rects(atlas::compose_world_paints(
        &game.world,
        &game.scene,
        game.world.ids(),
        steps,
        images,
        atlas_rects,
    ))
}

/// «Мир на экране» → «Земля», требования 18, 20–21: the visible tiles under the world's current
/// camera frame — drawn everywhere the world itself is drawn (`draw`/`tick`, battle or not),
/// always first in the caller's own instance list so every object draws on top of them.
fn compose_ground_instances(
    ground: &[GroundLayer],
    visible: CellRange,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
) -> Vec<DrawRect> {
    to_draw_rects(atlas::compose_ground_paints(
        ground,
        visible,
        images,
        atlas_rects,
    ))
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
                let (color, atlas_rect, smooth) =
                    atlas::fill_paint(fill, ui_elapsed_steps, images, atlas_rects);
                rects.push(draw_rect(
                    placement.top_left(viewport),
                    placement.size,
                    color,
                    atlas_rect,
                    0.0,
                    smooth,
                    false,
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
                let (color, atlas_rect, smooth) =
                    atlas::fill_paint(chosen, ui_elapsed_steps, images, atlas_rects);
                rects.push(draw_rect(
                    [x, y],
                    placement.size,
                    color,
                    atlas_rect,
                    0.0,
                    smooth,
                    false,
                ));
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

/// «Надписи и полоски в мире», требования 13, 17, 37: bar backing/fill rectangles (scene cells —
/// appended by the caller into the same `world_instances` list the world's own objects draw
/// through, so they land on top of them for free) and label texts (a separate pass — требование
/// 14: below the interface, above every world rectangle). Drawn whenever the world itself is
/// drawn, battle or not — unlike `compose_ui`, never gated on a live/replay session.
fn compose_world_elements(
    game: &Game,
    screens_config: &ScreensConfig,
    camera: Option<&Camera3d>,
) -> (Vec<DrawRect>, Vec<WorldTextDraw>) {
    let (bars, labels) = match camera {
        None => world_elements::compute_world_draws(
            &game.world,
            &game.scene,
            &game.properties,
            &screens_config.world_elements,
        ),
        Some(camera) => world_elements::compute_world_draws_3d(
            &game.world,
            &game.scene,
            &game.properties,
            &screens_config.world_elements,
            camera,
        ),
    };
    let mut rects = Vec::with_capacity(bars.len() * 2);
    for bar in &bars {
        if let Some(back) = bar.back {
            rects.push(draw_rect(
                back.position,
                back.size,
                back.color,
                atlas::WHITE_PIXEL,
                0.0,
                false,
                false,
            ));
        }
        rects.push(draw_rect(
            bar.fill.position,
            bar.fill.size,
            bar.fill.color,
            atlas::WHITE_PIXEL,
            0.0,
            false,
            false,
        ));
    }
    let texts = labels
        .into_iter()
        .map(|l| WorldTextDraw {
            text: l.text,
            font: l.font,
            font_size_cells: l.font_size,
            color: l.color,
            align: l.align,
            rect_cells: [l.position[0], l.position[1], l.size[0], l.size[1]],
        })
        .collect();
    (rects, texts)
}

/// «Трёхмерная сцена»: кадр мира в данные для видеокарты — фигуры по видам, вершины плиток и
/// плоских объектов на земле, камера, солнце.
fn gpu_frame_parts(frame: &Frame3d) -> (Globals3d, Vec<GroundVertex>, [Vec<ShapeInstance>; 4]) {
    let globals = Globals3d {
        view_proj: frame.view_proj,
        light_view_proj: frame.light_view_proj,
        sun: frame.sun,
        shade: [frame.shadow, frame.depth_per_cell, 0.02, 0.0],
        eye: [frame.eye[0], frame.eye[1], frame.eye[2], 0.0],
        sun_light: [
            frame.sun_light[0],
            frame.sun_light[1],
            frame.sun_light[2],
            0.0,
        ],
        sky_light: [
            frame.sky_light[0],
            frame.sky_light[1],
            frame.sky_light[2],
            0.0,
        ],
    };
    let ground = frame
        .surface
        .iter()
        .map(|vertex| GroundVertex {
            position: vertex.position,
            normal: vertex.normal,
            uv: vertex.uv,
            color: vertex.color,
            uv_min: vertex.uv_min,
            uv_max: vertex.uv_max,
            sheet: [vertex.layer, vertex.smooth],
        })
        .collect();
    let mut shapes: [Vec<ShapeInstance>; 4] = Default::default();
    for shape in &frame.shapes {
        // `Shape` идёт в порядке `Shape::ALL`, как и сетки в `render::gpu3d`.
        shapes[shape.shape as usize].push(ShapeInstance {
            placement: [shape.center[0], shape.center[1], shape.cos, shape.sin],
            dims: [shape.size[0], shape.size[1], shape.height, shape.cap_height],
            color: shape.color,
            base: shape.base,
        });
    }
    (globals, ground, shapes)
}

/// Рисует мир и интерфейс одного кадра: плоскую сцену прежним путём, трёхмерную — тенями, глубиной
/// и надписями над фигурами. `config` — `None` вне загруженных экранов: тогда нет и надписей в мире.
#[allow(clippy::too_many_arguments)]
fn render_world(
    renderer: &mut Renderer,
    game: &Game,
    config: Option<&ScreensConfig>,
    images: &[ImageDecl],
    atlas_rects: &[AtlasRect],
    terrain: Option<&TerrainMesh>,
    view: &View,
    viewport: [f32; 2],
    ui_instances: &[DrawRect],
    texts: &[TextDraw],
) -> Result<(), String> {
    match view {
        View::Flat { scale, offset } => {
            renderer.set_world_frame(*scale, *offset);
            let visible = game.scene.visible_cell_range(*scale, *offset, viewport);
            let mut world_instances =
                compose_ground_instances(&game.ground, visible, images, atlas_rects);
            world_instances.extend(compose_instances(game, images, atlas_rects));
            // «Надписи и полоски в мире», требование 37: рисуются везде, где нарисован мир — в
            // редакторе вне партии тоже, в отличие от интерфейса, который виден только в партии.
            let world_texts = match config {
                Some(config) => {
                    let (bar_rects, world_texts) = compose_world_elements(game, config, None);
                    world_instances.extend(bar_rects);
                    world_texts
                }
                None => Vec::new(),
            };
            renderer.render_frame(&world_instances, &world_texts, ui_instances, texts)
        }
        View::Space(camera) => {
            // Полоски и надписи над фигурами посчитаны сразу в точках окна.
            renderer.set_world_frame(1.0, [0.0, 0.0]);
            let frame = scene3d::compose_frame3d(
                game,
                camera,
                game.step_count() as f64,
                images,
                atlas_rects,
            );
            let (bar_rects, world_texts) = match config {
                Some(config) => compose_world_elements(game, config, Some(camera)),
                None => (Vec::new(), Vec::new()),
            };
            let (globals, ground, shapes) = gpu_frame_parts(&frame);
            let parts = Scene3dFrame {
                globals,
                terrain,
                ground: &ground,
                shapes: [&shapes[0], &shapes[1], &shapes[2], &shapes[3]],
            };
            renderer.render_frame_3d(&parts, &bar_rects, &world_texts, ui_instances, texts)
        }
    }
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
    /// «Рельеф» → «Вид»: сетка рельефа и воды, построенная при загрузке; `None`, пока в сцене нет файла
    /// высот.
    terrain_mesh: Option<TerrainMesh>,
    /// «Код игры» → требование 20: `files.rules` of the loaded game — a code error names the rule
    /// that called the function as `rules.json → rules[N]`, and the core only knows `rules[N]`.
    rules_path: String,
    /// «Картинки» → «Кадры»: window time for the interface's own image frames — unlike the
    /// world's own step counter, this never freezes: it keeps advancing on a paused or menu
    /// screen, which is exactly why the interface's own images keep animating there.
    ui_clock: UiClock,
    /// The last `ui_clock` reading `tick` took — `draw()`'s own interface frame (paused, or a
    /// replay step shown without a `tick`) reuses it rather than a fresh, unavailable timestamp.
    last_ui_elapsed_steps: f64,
    /// «Редактор»: the open play/replay session, if any — `None` on the plain game page and
    /// outside a partiya in the editor. See `data::session::PlaySession`.
    session: Option<session::PlaySession>,
    /// «Камера», требования 41–42: whether the world is drawn/picked/pointed at by камера
    /// (`true`) or by the plain wall-to-wall letterbox of the whole scene (`false`). Defaults to
    /// `true` — the plain game page never calls `show_scene`/`play`/`stop` at all, and always
    /// wants the camera when `view_height` is set; the editor's own `show_scene()` (its "outside
    /// a battle" static preview) is the one call that ever turns it off, `play()`/`replay()` turn
    /// it back on, and `stop()` turns it off again alongside its own `game.show_scene()`.
    battle_view: bool,
    /// The last window-pixel position `mouse_move` reported — «Камера», требование 12: needed to
    /// recompute the world cursor after a step or a resize moves the camera under a mouse that
    /// never itself moved. `None` until the first `mouse_move`, and again from each `play()`.
    last_mouse_window_pos: Option<[f32; 2]>,
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
            terrain_mesh: None,
            rules_path: String::new(),
            ui_clock: UiClock::new(),
            last_ui_elapsed_steps: 0.0,
            session: None,
            battle_view: true,
            last_mouse_window_pos: None,
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
    /// Returns `{fonts:[{name,path}], sounds:[{index,name,path}], music:[{index,name,path}],
    /// images:[{index,name,path}], materials:[{index,path}], masks:[{index,path}]}` — «Свет и
    /// материалы»: `materials` — каждая карта каждого материала в порядке объявления и, внутри
    /// материала, `color`, `normal`, `roughness`, `height`, `ao`; `masks` — маски `covers` файла рельефа
    /// `terrain_json` по порядку слоёв, пусто, если файла нет или он не читается; an empty answer if
    /// called before a successful `read_entry()`.
    pub fn read_texts(
        &self,
        properties_json: Option<String>,
        _scene_json: Option<String>,
        _rules_json: Option<String>,
        screens_json: Option<String>,
        // «Код игры»: движок его здесь не читает — принят только для единообразия с остальными
        // текстами второго захода; `load()` читает его по-настоящему.
        _code_json: Option<String>,
        // «Свет и материалы»: текст файла рельефа — из него берутся пути масок покрытий.
        terrain_json: Option<String>,
    ) -> JsValue {
        let Some(config) = self.pending_config.peek() else {
            return js_needed_media_ok(&NeededMedia::default(), &[]);
        };
        let needed = load::read_texts(
            config,
            properties_json.as_deref(),
            screens_json.as_deref(),
            terrain_json.as_deref(),
        );
        js_needed_media_ok(&needed, &config.files.music)
    }

    /// Step three: hand over the four text files, `fonts` — `[{name, bytes: Uint8Array|null}]`,
    /// one per `files.fonts` — `sounds` — `[{index, bytes: Uint8Array|null}]`, one per
    /// `read_texts()`'s `sounds` — `music` — `[{index, verdict: "ok"|"missing"|"rejected"}]`, one
    /// per `read_texts()`'s `music`, the executor's (browser's) answer to "can you decompress this
    /// track" — and `tables` — `[{name, text: string|null}]`, one per `read_entry()`'s own
    /// `files.tables` («Таблицы данных», требование 25) — and `terrain`, the text of `files.terrain`
    /// if `read_entry()` named one («Рельеф», требование 47). Runs the full prestart check and, on
    /// success, the game is ready to run starting from the next `tick`. Returns `{ok:true,
    /// warnings:[...]}` on success or `{ok:false, errors:[...], warnings:[...]}` on failure —
    /// warnings never stop the game from starting, but the page still needs to see them either way.
    /// «Свет и материалы»: `material_maps` и `cover_masks` — ответы страницы по картам материалов и
    /// маскам покрытий, `[{index, verdict: "ok"|"missing"|"rejected", width?, height?, pixels?}]`, как
    /// у `images`, по номерам из `read_texts()`'s `materials` и `masks`.
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
        tables: JsValue,
        terrain: Option<String>,
        material_maps: JsValue,
        cover_masks: JsValue,
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
        // `config` is about to move into `load_rest_with_tables` — the font/sound/music/image name
        // order it carries is what fixes each one's FontId/SoundId/MusicId/ImageId, so it has to be
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
        let table_texts = parse_table_texts(&tables);
        // «Свет и материалы»: те же списки, что отдал `read_texts()`, — ответы страницы идут по номерам.
        let material_decls = config.files.materials.clone();
        let map_paths: Vec<String> = material_decls
            .iter()
            .flat_map(|material| material.maps().map(|(_, path)| path.to_string()))
            .collect();
        let mask_paths = terrain
            .as_deref()
            .map(load::cover_mask_paths)
            .unwrap_or_default();
        let map_verdicts = parse_image_verdicts(&material_maps, &path_table(&map_paths));
        let mask_verdicts = parse_image_verdicts(&cover_masks, &path_table(&mask_paths));
        match load::load_rest_with_materials(
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
            &table_texts,
            terrain.as_deref(),
            &map_verdicts,
            &mask_verdicts,
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
                let relief = Relief::new(
                    &material_decls,
                    &map_verdicts,
                    game.world.terrain().covers(),
                    &mask_paths,
                    &mask_verdicts,
                    [game.scene.width, game.scene.height],
                );
                self.renderer.set_relief(relief.as_ref());
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
                self.terrain_mesh = TerrainMesh::build(game.world.terrain(), game.scene.background);
                self.game = Some(game);
                self.runner = Runner::new();
                self.mouse = MouseState::default();
                self.ui_queue = UiQueue::new();
                // «Редактор», требование 27: запись живёт до «Запуска», открытия другой записи или
                // закрытия проекта — а `load()` тут же перезагружает те же файлы (после «Стопа» или
                // правки сцены/свойств вне партии), а не открывает новый проект, так что запись
                // остаётся: `play()`/`replay()` — единственные, кто её заменяет.
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
        self.terrain_mesh = None;
        self.screen_state = None;
        self.images = Vec::new();
        self.atlas_rects = Vec::new();
        self.rules_path = String::new();
        self.renderer.reset_fonts();
        self.renderer.set_relief(None);
        self.session = None;
    }

    /// «Редактор», требование 16: rebuilds the world from the loaded scene, ready for `draw` —
    /// no partiya, no code, no initial values. Does nothing without a successfully loaded game.
    /// «Камера», требование 41: вне партии — вся сцена по её пропорциям, камера не действует.
    pub fn show_scene(&mut self) {
        if let Some(game) = self.game.as_mut() {
            game.show_scene();
        }
        self.battle_view = false;
    }

    /// «Камера»: масштаб и сдвиг этого кадра — камера ядра в партии/повторе, letterbox сцены
    /// целиком вне партии (требование 41). `None` без загруженной игры.
    fn frame(&self) -> Option<View> {
        let game = self.game.as_ref()?;
        let viewport = self.renderer.window_size_css();
        Some(frame_for(game, self.battle_view, viewport))
    }

    /// «Камера», требование 12: точка под курсором пересчитывается от последнего известного
    /// положения мыши в пикселях всякий раз, как мог сдвинуться кадр (после `resize` — после
    /// шагов см. `recompute_cursor_after_step`, вызванную прямо в `tick`/`step`/`seek`/
    /// `step_back`, где `self.game` уже занят мутабельно).
    fn recompute_cursor(&mut self) {
        let is_replay = self.session.as_ref().is_some_and(PlaySession::is_replay);
        let viewport = self.renderer.window_size_css();
        let battle_view = self.battle_view;
        let last_pos = self.last_mouse_window_pos;
        let Some(game) = self.game.as_mut() else {
            return;
        };
        recompute_cursor_after_step(
            game,
            battle_view,
            viewport,
            last_pos,
            is_replay,
            self.session.as_mut(),
        );
    }

    /// «Редактор», требования 17, 40: outside a partiya/повтор, the world alone — no interface, no
    /// text, an image's own frame frozen at the world's own step, same as `tick`'s own
    /// `compose_instances`. During one, also the active screen's interface, same as `tick` draws
    /// it, reusing `tick`'s last `ui_clock` reading (there is no fresh timestamp here — a paused
    /// screen's interface simply stays exactly as `tick` last left it, which is what "paused"
    /// means for its own animated fills). An empty frame without a loaded game.
    pub fn draw(&mut self) {
        if self.game.is_none() {
            let _ = self.renderer.render_frame(&[], &[], &[], &[]);
            return;
        }
        let view = self.frame().expect("checked above");
        let game = self.game.as_ref().expect("checked above");
        let viewport = self.renderer.window_size_css();
        let show_ui = self
            .session
            .as_ref()
            .is_some_and(|session| session.is_live() || session.is_replay());
        let (ui_instances, texts) = match (
            show_ui,
            self.screens_config.as_ref(),
            self.screen_state.as_ref(),
        ) {
            (true, Some(config), Some(state)) => {
                let viewport = self.renderer.window_size_css();
                let screen = &config.screens[state.active()];
                compose_ui(
                    screen,
                    &game.world,
                    &game.properties,
                    &self.mouse,
                    viewport,
                    &self.images,
                    &self.atlas_rects,
                    self.last_ui_elapsed_steps,
                )
            }
            _ => (Vec::new(), Vec::new()),
        };
        if let Err(e) = render_world(
            &mut self.renderer,
            game,
            self.screens_config.as_ref(),
            &self.images,
            &self.atlas_rects,
            self.terrain_mesh.as_ref(),
            &view,
            viewport,
            &ui_instances,
            &texts,
        ) {
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
        let picked = match self.frame() {
            Some(View::Flat { scale, offset }) => {
                crate::core::scene::object_at_frame(&game.world, &game.scene, [x, y], scale, offset)
            }
            // «Редактор», «Сцена», требование 7: в трёхмерной сцене — лучом камеры, которой она
            // видна сейчас.
            Some(View::Space(camera)) => crate::core::scene::editor_target_ray(
                &game.world,
                &game.scene,
                &camera,
                [x as f64, y as f64],
            ),
            None => None,
        };
        match picked {
            Some(id) => JsValue::from_f64(id as f64),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требование 19: object `id`'s canvas rectangle — `{x, y, width, height}` in CSS
    /// pixels — or `undefined` without a loaded game, without that object, or without its own
    /// `position`/`size`. See `core::scene::object_rect`. «Редактор», «Вызовы движка»: в
    /// трёхмерной сцене — `{corners: [[x, y] × 4]}`, углы повёрнутого прямоугольника на земле, и
    /// `undefined`, если угол за камерой.
    pub fn object_rect(&self, id: u32) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return JsValue::UNDEFINED;
        };
        match self.frame() {
            Some(View::Flat { scale, offset }) => {
                match crate::core::scene::object_rect_frame(&game.world, id, scale, offset) {
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
            Some(View::Space(camera)) => {
                match crate::core::scene::object_screen_corners(&game.world, id, &camera) {
                    Some(corners) => {
                        let list = Array::new();
                        for corner in corners {
                            list.push(&js_point(corner));
                        }
                        let obj = Object::new();
                        set(&obj, "corners", &list);
                        obj.into()
                    }
                    None => JsValue::UNDEFINED,
                }
            }
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требование 30: moves object `id` to `(x, y)` — scene cells — in the world
    /// alone. The scene `show_scene` built the world from, and the files on disk, are untouched:
    /// the next `show_scene` rebuilds the world from the scene as it was. Does nothing without a
    /// loaded game, without that object, or without its own `position`. See
    /// `core::scene::move_object`.
    pub fn move_object(&mut self, id: u32, x: f32, y: f32, z: Option<f64>) {
        if let Some(game) = self.game.as_mut() {
            // «Камера», требование 6: перенос мышью в плоской сцене двигает камеру без отставания,
            // не только на следующем шаге; в трёхмерной камера игры стоит, требование 18.
            // «Рельеф»: с `z` основание встаёт ровно на него, без — объект садится на поверхность.
            game.move_object(id, [x as f64, y as f64], z.filter(|z| z.is_finite()));
        }
    }

    /// «Рельеф», «Вызовы движка»: основание, на которое встал бы объект `id`, сдвинутый на место
    /// `(x, y)` (`position`, левый верхний угол): с `from` — как после сдвига с этого основания
    /// (настил — на нынешнем), без `from` — как объект без `z` в данных. Прямоугольник — нынешние
    /// `size` и `rotation` объекта. `undefined` в плоской сцене, без объекта или его `position` и `size`.
    pub fn rest_height(&self, id: u32, x: f64, y: f64, from: Option<f64>) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return JsValue::UNDEFINED;
        };
        match crate::core::surface::rest_height_at(
            &game.world,
            id,
            [x, y],
            from.filter(|from| from.is_finite()),
        ) {
            Some(z) => JsValue::from_f64(z),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», «Вызовы движка»: ставит объекту `id` значения ручек в собранном мире —
    /// `t = {position: [x, y] или [x, y, z], size: [w, d], height?, rotation?}`, не названное не
    /// меняется; без третьего числа объект садится на поверхность, с ним основание встаёт ровно.
    /// Файлы, разобранная сцена и камера игры не меняются. Ничего не делает без игры, без объекта
    /// или при значении не из чисел.
    pub fn transform_object(&mut self, id: u32, t: JsValue) {
        let (Some(game), Some(transform)) = (self.game.as_mut(), parse_transform(&t)) else {
            return;
        };
        crate::core::scene::transform_object(&mut game.world, id, transform);
    }

    /// «Редактор», «Вызовы движка»: камера редактора `{target, yaw, pitch, distance}` — вне партии сцена
    /// рисуется и щёлкается ею. `target` — `[x, y]`: высота точки вращения берётся из рельефа под ней, или
    /// `[x, y, z]`: точка вращения ровно на этой высоте. Возвращает высоту, которую взяла, `undefined` без
    /// игры или при значении не из чисел. `show_scene`, `play`, `stop` и `set_terrain` её не сбрасывают.
    pub fn editor_camera(&mut self, c: JsValue) -> Option<f64> {
        let (game, (camera, on_terrain)) = (self.game.as_mut()?, parse_editor_camera(&c)?);
        let camera = if on_terrain {
            camera.on_terrain(game.world.terrain())
        } else {
            camera
        };
        game.set_editor_camera(camera);
        Some(camera.target_z)
    }

    /// «Редактор», «Вызовы движка»: ставит рельеф — `heights` строками сверху вниз, вода `{level, color}`
    /// или `undefined` — так, будто он прочитан из файла, собирает мир из сцены заново и перестраивает
    /// сетку рельефа для отрисовки. Сети поиска пути и камера редактора не меняются. `undefined` —
    /// поставлен; иначе текст ошибки: чисел не столько, высота или уровень не число, цвет не цвет,
    /// сцена плоская, идёт партия.
    pub fn set_terrain(&mut self, heights: &[f64], water: JsValue) -> Option<String> {
        let Some(game) = self.game.as_mut() else {
            return Some("игра не загружена".to_string());
        };
        let water = parse_water(&water);
        let water = water
            .as_ref()
            .map(|(level, color)| (*level, color.as_str()));
        if let Err(message) = edit::set_terrain(game, heights, water) {
            return Some(message);
        }
        self.terrain_mesh = TerrainMesh::build(game.world.terrain(), game.scene.background);
        None
    }

    /// «Редактор», «Вызовы движка»: нынешний рельеф `{columns, rows, heights, water}` — `heights`
    /// (`Float64Array`) строками сверху вниз, как в `set_terrain`, `water` — `{level, color}` или
    /// `null`. Без файла рельефа — нули нужного размера. `undefined` в плоской сцене.
    pub fn terrain_heights(&self) -> JsValue {
        let Some(terrain) = self.game.as_ref().and_then(edit::terrain_heights) else {
            return JsValue::UNDEFINED;
        };
        let obj = Object::new();
        set(&obj, "columns", &JsValue::from_f64(terrain.columns as f64));
        set(&obj, "rows", &JsValue::from_f64(terrain.rows as f64));
        set(
            &obj,
            "heights",
            &Float64Array::from(terrain.heights.as_slice()),
        );
        let water = terrain.water.map_or(JsValue::NULL, |(level, color)| {
            let water = Object::new();
            set(&water, "level", &JsValue::from_f64(level));
            set(&water, "color", &JsValue::from_str(&color));
            water.into()
        });
        set(&obj, "water", &water);
        obj.into()
    }

    /// «Редактор», «Вызовы движка»: место под точкой холста `(x, y)` (CSS-пиксели) камерой, которой
    /// сцена видна сейчас, — `[x, y, z]`: где луч первым встретил рельеф в пределах сцены, минуя воду,
    /// настилы и объекты. `undefined` в плоской сцене, если луч мимо рельефа сцены или не идёт вниз.
    pub fn terrain_at(&self, x: f64, y: f64) -> JsValue {
        let (Some(game), Some(View::Space(camera))) = (self.game.as_ref(), self.frame()) else {
            return JsValue::UNDEFINED;
        };
        match terrain_hit(&game.world, &game.scene, &camera, [x, y]) {
            Some(point) => js_point3(point),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», «Вызовы движка»: высота рельефа в месте сцены `(x, y)`, за краем — высота края.
    /// `undefined` в плоской сцене.
    pub fn terrain_height(&self, x: f64, y: f64) -> Option<f64> {
        let game = self.game.as_ref().filter(|game| game.scene.is_3d())?;
        Some(game.world.terrain().height_at(x, y))
    }

    /// «Редактор», «Вызовы движка»: камера редактора, что видит объект `id` целиком при нынешних
    /// повороте и наклоне, а без номера — всю землю под углом камеры игры; `{target, yaw, pitch,
    /// distance}`, `target` — три числа `[x, y, z]`, или `undefined` в плоской сцене, без игры, без объекта или его `position` и `size`.
    pub fn fit_camera(&self, id: Option<u32>) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return JsValue::UNDEFINED;
        };
        match game.fit_camera(id, self.renderer.window_size_css()) {
            Some(camera) => js_editor_camera(&camera),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», «Вызовы движка»: место под точкой холста `(x, y)` камерой, которой сцена видна
    /// сейчас, — `[x, y, z]`: где луч первым встретил рельеф в пределах сцены, воду или верх настила;
    /// мимо сцены — пересечение с плоскостью высоты 0 без прижатия к краю. `undefined` в плоской
    /// сцене или если луч не идёт вниз.
    pub fn ground_at(&self, x: f32, y: f32) -> JsValue {
        let (Some(game), Some(View::Space(camera))) = (self.game.as_ref(), self.frame()) else {
            return JsValue::UNDEFINED;
        };
        match pointer_hit(&game.world, &game.scene, &camera, [x as f64, y as f64]) {
            Some(point) => js_point3(point),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», «Вызовы движка»: точка холста для места сцены `(x, y)` на высоте `z` клеток той
    /// же камерой — `[x, y]`; `undefined` в плоской сцене или за камерой.
    pub fn screen_point(&self, x: f64, y: f64, z: f64) -> JsValue {
        match self.frame() {
            Some(View::Space(camera)) => match camera.project([x, y, z]) {
                Some(point) => js_point(point),
                None => JsValue::UNDEFINED,
            },
            _ => JsValue::UNDEFINED,
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
    /// «Редактор», требование 46: in a replay the host's own mouse never reaches the game at all —
    /// `update_cursor` writes straight to `game.cursor_current`, bypassing `ui_queue` entirely
    /// (which `tick`/`step`/`seek` already clear before a replay ever reads it), so this is the one
    /// place that has to check `is_replay` itself instead of relying on the queue being emptied.
    pub fn mouse_move(&mut self, x: f32, y: f32) {
        self.ui_queue.push_mouse_move(x, y);
        self.last_mouse_window_pos = Some([x, y]);
        if self.session.as_ref().is_some_and(PlaySession::is_replay) {
            return;
        }
        let Some(view) = self.frame() else {
            return;
        };
        if let Some(game) = self.game.as_mut() {
            let (point, eye) = pointer_at(game, &view, [x, y]);
            game.set_cursor_point(point, eye);
            if let Some(session) = self.session.as_mut() {
                session.record_cursor(game);
            }
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
            let _ = self.renderer.render_frame(&[], &[], &[], &[]);
            return js_running();
        };

        let viewport = self.renderer.window_size_css();
        // «Редактор», требование 40: во время партии этот же вызов ведёт запись и считает
        // `session_step` — страница (без сессии) идёт прежним путём, один в один. В повторе,
        // требование 6, 28: тот же накопитель реального времени, что у партии, гонит `step_once`
        // по записи, а не сам `Game::step` — клавиши и мышь хозяина туда не доходят: очередь
        // сбрасывается перед вызовом, а не читается. После «Стопа» (сессия заморожена) шаг
        // невозможен: `tick_replay` сама убеждается в этом и ничего не делает.
        match self.session.as_mut() {
            Some(session) if session.is_live() => session.tick_live(
                &mut self.ui_queue,
                &mut self.mouse,
                &mut self.runner,
                game,
                config,
                state,
                viewport,
                dt_seconds,
                &self.images,
            ),
            Some(session) => {
                self.ui_queue = UiQueue::new();
                session.tick_replay(
                    &mut self.ui_queue,
                    &mut self.mouse,
                    &mut self.runner,
                    game,
                    config,
                    state,
                    viewport,
                    dt_seconds,
                    &self.images,
                );
            }
            None => screens::engine_call(
                &mut self.ui_queue,
                &mut self.mouse,
                &mut self.runner,
                game,
                config,
                state,
                viewport,
                dt_seconds,
            ),
        }
        self.last_ui_elapsed_steps = ui_clock_steps;
        recompute_cursor_after_step(
            game,
            self.battle_view,
            viewport,
            self.last_mouse_window_pos,
            self.session.as_ref().is_some_and(PlaySession::is_replay),
            self.session.as_mut(),
        );

        // «Код игры»: ошибка кода останавливает игру на месте — страница показывает её вместо
        // игры, тем же форматом, что ошибку загрузки; кадр в таком виде мира не рисуется.
        if let Some(err) = game.code_error() {
            return js_code_error(game.code_path(), &self.rules_path, err);
        }

        // «Камера», требования 41–42: на игровой странице ничего кроме `tick` кадр не задаёт —
        // без этого мир рисовался бы заглушкой, которую поставил последний `set_scene`/`resize`.
        let view = frame_for(game, self.battle_view, viewport);
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
        if let Err(e) = render_world(
            &mut self.renderer,
            game,
            Some(config),
            &self.images,
            &self.atlas_rects,
            self.terrain_mesh.as_ref(),
            &view,
            viewport,
            &ui_instances,
            &texts,
        ) {
            web_sys::console::error_1(&JsValue::from_str(&format!("отрисовка не удалась: {e}")));
        }

        game_tick_result(game, game.code_path(), &self.rules_path)
    }

    /// Reconfigures the GPU surface for a new device-pixel canvas size. Call on canvas/container
    /// resize; does not touch the game.
    pub fn resize(&mut self, width_px: u32, height_px: u32) {
        self.renderer.resize(width_px, height_px);
        self.recompute_cursor();
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

    /// «Запуск», требование 39: starts a partiya at the loaded game's own start screen and opens a
    /// new recording — nothing without a successfully loaded game.
    pub fn play(&mut self) {
        let (Some(game), Some(config), Some(state)) = (
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            return;
        };
        self.session = Some(PlaySession::begin_live(game, config, state));
        self.runner = Runner::new();
        self.mouse = MouseState::default();
        self.ui_queue = UiQueue::new();
        self.ui_clock.reset();
        self.battle_view = true;
        self.last_mouse_window_pos = None;
    }

    /// «Пауза», требование 40: releases held keys, the same release a live screen switch already
    /// triggers — the caller is the one that actually stops calling `tick()`. The release goes into
    /// the recording, so a replay finds the same keys let go at the same step. In a replay this
    /// would instead *diverge* it from its own recording — the replayed world holds only what its
    /// own recorded events put there, and `pause()` has none of its own to add — so a replay's
    /// `pause()` only resets the runner, same as `tick_replay` expects `tab_hidden` to.
    pub fn pause(&mut self) {
        let Some(game) = self.game.as_mut() else {
            return;
        };
        let is_replay = self.session.as_ref().is_some_and(PlaySession::is_replay);
        if !is_replay {
            let released = game.release_held_keys();
            if let Some(session) = self.session.as_mut() {
                session.record_key_releases(game, &released);
            }
        }
        // «Редактор», требование 10: время паузы не догоняется, same as `tab_hidden`.
        self.runner.reset();
        self.runner.forget_last_tick();
    }

    /// «Шаг», требования 9, 33, 40, 47: exactly one step, live or replay, paused or not — returns
    /// the same running/ended/error shape `tick()` does, so a code error raised on a manual step is
    /// visible too, not silent. `{running:true}` without an open session (nothing to step). In a
    /// replay the queue is reset first, same as `tick()` — требование 46: the host's own queued
    /// mouse/keyboard must not reach the replayed game.
    pub fn step(&mut self) -> JsValue {
        let (Some(session), Some(game), Some(config), Some(state)) = (
            self.session.as_mut(),
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            return js_running();
        };
        if session.is_replay() {
            self.ui_queue = UiQueue::new();
        }
        game.sound_window_mut().clear_marks();
        let viewport = self.renderer.window_size_css();
        session.step_once(
            &mut self.ui_queue,
            &mut self.mouse,
            game,
            config,
            state,
            viewport,
            &self.images,
        );
        recompute_cursor_after_step(
            game,
            self.battle_view,
            viewport,
            self.last_mouse_window_pos,
            session.is_replay(),
            Some(session),
        );
        game_tick_result(game, game.code_path(), &self.rules_path)
    }

    /// «Редактор», требование 8: why the "Шаг" button would do nothing right now, in Russian, for
    /// its tooltip — `undefined` when a step would actually happen. Outside a session (or without a
    /// loaded game) this is always «Нет партии»: there is no partiya to advance in the first place.
    pub fn step_blocked(&self) -> JsValue {
        let (Some(session), Some(game), Some(config), Some(state)) = (
            self.session.as_ref(),
            self.game.as_ref(),
            self.screens_config.as_ref(),
            self.screen_state.as_ref(),
        ) else {
            return JsValue::from_str("Нет партии");
        };
        match session.step_blocked_reason(game, config, state) {
            Some(reason) => JsValue::from_str(reason),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требование 8, 13: whether a world exists right now — `false` before `new_game`
    /// on a non-live start screen and after `quit`, for the object list's «Мира нет» caption. An
    /// explicit flag on `Game` (its own doc comment names exactly which calls flip it), not
    /// `world.alive_count() > 0`: a live world every rule has emptied out is still a world, not
    /// "Мира нет".
    pub fn has_world(&self) -> bool {
        self.game.as_ref().is_some_and(Game::has_world)
    }

    /// «Стоп», требование 40: ends the session and rebuilds the world from the scene, same as
    /// `show_scene`. Does nothing without an open session.
    pub fn stop(&mut self) {
        // «Редактор», требование 27: the recording itself outlives the partiya — `session` stays
        // (so `recording()` still answers) and only freezes; `play()`/`replay()` are what actually
        // replace it, and `clear_game()`/a successful `load()` are what drop it for good.
        let (Some(session), Some(game)) = (self.session.as_mut(), self.game.as_mut()) else {
            return;
        };
        session.end(game);
        game.show_scene();
        self.runner = Runner::new();
        self.mouse = MouseState::default();
        self.ui_queue = UiQueue::new();
        self.battle_view = false;
    }

    /// Whether the open session is a replay rather than a live partiya — `false` outside a
    /// session too.
    pub fn is_replay(&self) -> bool {
        self.session.as_ref().is_some_and(PlaySession::is_replay)
    }

    /// «Редактор», требование 41: live objects by ascending number — `[{id, generation, name}]`.
    /// Empty without a game or a world.
    pub fn world_objects(&self) -> Array {
        let arr = Array::new();
        let Some(game) = self.game.as_ref() else {
            return arr;
        };
        for id in game.world.ids() {
            let item = Object::new();
            set(&item, "id", &JsValue::from_f64(id as f64));
            set(
                &item,
                "generation",
                &JsValue::from_f64(game.world.generation(id) as f64),
            );
            set(
                &item,
                "name",
                &js_optional_string(game.world.text(id, property::NAME)),
            );
            arr.push(&item);
        }
        arr
    }

    /// «Редактор», требование 42: object `id`'s properties in file form — `undefined` when it
    /// isn't alive or there is no game.
    pub fn object_properties(&self, id: u32) -> JsValue {
        let Some(game) = self.game.as_ref() else {
            return JsValue::UNDEFINED;
        };
        match edit::object_properties_json(&game.world, &game.properties, &self.images, id) {
            Some(map) => json_to_js(&serde_json::Value::Object(map)),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требование 43. `value` is a JS value (already parsed, not a JSON string).
    pub fn set_property(&mut self, id: u32, name: &str, value: JsValue) -> JsValue {
        let json = js_to_json(&value);
        let (Some(session), Some(game)) = (self.session.as_mut(), self.game.as_mut()) else {
            return js_edit_err("нет партии");
        };
        js_edit_result(session.set_property(game, &self.images, id, name, &json))
    }

    /// «Редактор», требование 43.
    pub fn remove_property(&mut self, id: u32, name: &str) -> JsValue {
        let (Some(session), Some(game)) = (self.session.as_mut(), self.game.as_mut()) else {
            return js_edit_err("нет партии");
        };
        js_edit_result(session.remove_property(game, id, name))
    }

    /// «Редактор», требования 18, 43: a new object under the first free number. `props` is a JS
    /// value (already parsed), in the same shape `object_properties` returns. `{ok:true, id}` on
    /// success.
    pub fn add_object(&mut self, props: JsValue) -> JsValue {
        let json = js_to_json(&props);
        let (Some(session), Some(game)) = (self.session.as_mut(), self.game.as_mut()) else {
            return js_edit_err("нет партии");
        };
        match session.add_object(game, &self.images, &json) {
            Ok(id) => {
                let obj = Object::new();
                set(&obj, "ok", &JsValue::TRUE);
                set(&obj, "id", &JsValue::from_f64(id as f64));
                obj.into()
            }
            Err(message) => js_edit_err(&message),
        }
    }

    /// «Редактор», требование 43.
    pub fn delete_object(&mut self, id: u32) -> JsValue {
        let (Some(session), Some(game)) = (self.session.as_mut(), self.game.as_mut()) else {
            return js_edit_err("нет партии");
        };
        js_edit_result(session.delete_object(game, id))
    }

    /// «Редактор», требования 23, 44: the last step's report — `undefined` before the session's
    /// first step, or without one open (a closed session's own last report never leaks past
    /// `stop()`, since nothing reads it there).
    pub fn step_report(&self) -> JsValue {
        if self.session.is_none() {
            return JsValue::UNDEFINED;
        }
        match self.game.as_ref().and_then(Game::last_report) {
            Some(report) => js_step_report(report),
            None => JsValue::UNDEFINED,
        }
    }

    /// «Редактор», требования 26, 45: every message so far this session, each `{step, text}` —
    /// `messages()` (the page's own call) is unaffected. Empty outside a session.
    pub fn session_messages(&self) -> Array {
        let arr = Array::new();
        let (Some(_), Some(game)) = (self.session.as_ref(), self.game.as_ref()) else {
            return arr;
        };
        for (step, text) in game.session_messages() {
            let item = Object::new();
            set(&item, "step", &JsValue::from_f64(*step as f64));
            set(&item, "text", &JsValue::from_str(text));
            arr.push(&item);
        }
        arr
    }

    /// «Редактор», требование 47: the step the session is currently on — live, or where a replay
    /// is paused. 0 outside a session.
    pub fn current_step(&self) -> f64 {
        match (self.session.as_ref(), self.game.as_ref()) {
            (Some(_), Some(game)) => game.session_step_count() as f64,
            _ => 0.0,
        }
    }

    /// «Редактор», требование 47: the recording's own length — steps simulated so far while live,
    /// the fixed total of a loaded replay. 0 outside a session.
    pub fn recording_length(&self) -> f64 {
        match (self.session.as_ref(), self.game.as_ref()) {
            (Some(session), Some(game)) => session.length(game) as f64,
            _ => 0.0,
        }
    }

    /// «Сохранить запись», требование 46: the current recording as replay-file text. `undefined`
    /// outside a session.
    pub fn recording(&self) -> JsValue {
        match (self.session.as_ref(), self.game.as_ref()) {
            (Some(session), Some(game)) => JsValue::from_str(&session.recording_text(game)),
            _ => JsValue::UNDEFINED,
        }
    }

    /// «Открыть запись»/«Повтор», требования 35, 39, 46: parses `text` and, on success, starts a
    /// replay of it, paused on step 0. `{ok:false, error}` — already prefixed "Это не запись
    /// партии: <причина>" — leaves the current session untouched; `{ok:false, error:"нет
    /// загруженной игры"}` without a loaded game.
    pub fn replay(&mut self, text: &str) -> JsValue {
        let (Some(game), Some(config), Some(state)) = (
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            return js_edit_err("нет загруженной игры");
        };
        match PlaySession::begin_replay(text, game, config, state) {
            Ok(session) => {
                self.session = Some(session);
                self.runner = Runner::new();
                self.mouse = MouseState::default();
                self.ui_queue = UiQueue::new();
                self.ui_clock.reset();
                self.battle_view = true;
                js_edit_ok()
            }
            Err(message) => js_edit_err(&message),
        }
    }

    /// «Повтор», требования 6, 9, 29, 33, 47: recomputes the replay from scratch up to `step`, no
    /// drawing or sound. Does nothing without an open replay. Returns the same running/ended/error
    /// shape `tick()`/`step()` do — требование 33: seeking onto a step whose code raised an error
    /// has to be visible the same way stepping onto it live would be, so the page knows to keep
    /// showing that error instead of clearing it. The queue is reset first, same as `tick()`/
    /// `step()` — требование 46.
    pub fn seek(&mut self, step: u32) -> JsValue {
        let (Some(session), Some(game), Some(config), Some(state)) = (
            self.session.as_mut(),
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            return js_running();
        };
        self.ui_queue = UiQueue::new();
        let viewport = self.renderer.window_size_css();
        session.seek(
            step as u64,
            &mut self.ui_queue,
            &mut self.mouse,
            game,
            config,
            state,
            viewport,
            &self.images,
        );
        game_tick_result(game, game.code_path(), &self.rules_path)
    }

    /// `seek(current - 1)` — требование 47; a no-op on step 0 or without an open replay. See
    /// `seek` for its return value and queue reset.
    pub fn step_back(&mut self) -> JsValue {
        let (Some(session), Some(game), Some(config), Some(state)) = (
            self.session.as_mut(),
            self.game.as_mut(),
            self.screens_config.as_ref(),
            self.screen_state.as_mut(),
        ) else {
            return js_running();
        };
        self.ui_queue = UiQueue::new();
        let viewport = self.renderer.window_size_css();
        session.step_back(
            &mut self.ui_queue,
            &mut self.mouse,
            game,
            config,
            state,
            viewport,
            &self.images,
        );
        game_tick_result(game, game.code_path(), &self.rules_path)
    }
}

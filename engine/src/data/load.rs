use serde_json::Value as Json;

use crate::core::game::Game;
use crate::core::keys::{KeyBinding, KeyEdit, KeyTable};
use crate::core::property::{self, PropertyId, PropertyTable};
use crate::core::rules::{
    CollideEffect, CommonAction, CompareOp, Condition, Outcome, Rule, RuleSet, Selector,
    SpawnCondition, SpawnPlace, TemplateValue,
};
use crate::core::scene::SceneConfig;
use crate::core::time::{seconds_to_steps, seconds_to_steps_delta};
use crate::core::value::{GridSpec, PropKind, Value, Vec2};
use crate::core::world::World;

use super::error::ErrorSink;

pub use super::error::{GameError, LoadFailure};

fn join(base: &str, seg: &str) -> String {
    if base.is_empty() {
        seg.to_string()
    } else {
        format!("{base} → {seg}")
    }
}

fn kind_name(v: &Json) -> &'static str {
    match v {
        Json::Null => "пусто",
        Json::Bool(_) => "логическое значение",
        Json::Number(_) => "число",
        Json::String(_) => "строка",
        Json::Array(_) => "массив",
        Json::Object(_) => "объект",
    }
}

fn parse_json_or_error(file: &str, text: &str, errors: &mut ErrorSink) -> Option<Json> {
    match serde_json::from_str::<Json>(text) {
        Ok(v) => Some(v),
        Err(e) => {
            errors.push(
                file,
                "",
                format!(
                    "файл не разбирается как JSON: {e} (строка {}, столбец {})",
                    e.line(),
                    e.column()
                ),
            );
            None
        }
    }
}

fn require_field<'a>(
    obj: &'a serde_json::Map<String, Json>,
    key: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<&'a Json> {
    match obj.get(key) {
        Some(v) => Some(v),
        None => {
            errors.push(
                file,
                path,
                format!("отсутствует обязательная настройка \"{key}\""),
            );
            None
        }
    }
}

/// «Формат игры»: значений «по умолчанию вместо сломанного» не бывает, а `game.json` — файл с
/// фиксированным, не заданным автором игры набором полей (в отличие от объектов `scene.json`,
/// где сами имена полей — свойства из `properties.json` и опечатку в них ловит
/// `resolve_property`). Здесь опечатку вроде `random_sed` ловит сверка с этим списком: любой ключ
/// вне него — та же ошибка, что и неизвестное свойство.
fn reject_unknown_keys(
    obj: &serde_json::Map<String, Json>,
    known: &[&str],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    for key in obj.keys() {
        if !known.contains(&key.as_str()) {
            errors.push(
                file,
                &join(path, key),
                format!("неизвестное поле \"{key}\""),
            );
        }
    }
}

fn expect_object<'a>(
    value: &'a Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<&'a serde_json::Map<String, Json>> {
    match value.as_object() {
        Some(o) => Some(o),
        None => {
            errors.push(
                file,
                path,
                format!("ожидался объект, получено {}", kind_name(value)),
            );
            None
        }
    }
}

fn expect_array<'a>(
    value: &'a Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<&'a Vec<Json>> {
    match value.as_array() {
        Some(a) => Some(a),
        None => {
            errors.push(
                file,
                path,
                format!("ожидался массив, получено {}", kind_name(value)),
            );
            None
        }
    }
}

fn expect_string(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<String> {
    match value.as_str() {
        Some(s) => Some(s.to_string()),
        None => {
            errors.push(
                file,
                path,
                format!("ожидалась строка, получено {}", kind_name(value)),
            );
            None
        }
    }
}

fn expect_number(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<f64> {
    match value.as_f64() {
        Some(n) => Some(n),
        None => {
            errors.push(
                file,
                path,
                format!("ожидалось число, получено {}", kind_name(value)),
            );
            None
        }
    }
}

fn expect_positive_u32(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<u32> {
    let n = expect_number(value, file, path, errors)?;
    if n <= 0.0 || n.fract() != 0.0 {
        errors.push(
            file,
            path,
            format!("ожидалось целое положительное число, получено {n}"),
        );
        return None;
    }
    Some(n as u32)
}

fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    let s = s.strip_prefix('#')?;
    if s.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&s[0..2], 16).ok()?;
    let g = u8::from_str_radix(&s[2..4], 16).ok()?;
    let b = u8::from_str_radix(&s[4..6], 16).ok()?;
    Some([r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0])
}

fn parse_vec2(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<Vec2> {
    let arr = expect_array(value, file, path, errors)?;
    if arr.len() != 2 {
        errors.push(
            file,
            path,
            format!("ожидалась пара чисел, элементов: {}", arr.len()),
        );
        return None;
    }
    let x = expect_number(&arr[0], file, &join(path, "[0]"), errors)?;
    let y = expect_number(&arr[1], file, &join(path, "[1]"), errors)?;
    Some([x, y])
}

/// Parses one scalar value against the kind declared for `prop`. Time values are converted to
/// steps right here, so nothing downstream ever sees seconds again. `prop` itself (not just its
/// kind) matters once: `size` may not be negative, everything else with the same kind can be.
fn parse_scalar_value(
    value: &Json,
    prop: PropertyId,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Value> {
    let kind = properties.kind(prop);
    match kind {
        PropKind::Flag => value.as_bool().map(Value::Flag).or_else(|| {
            errors.push(
                file,
                path,
                format!("ожидался признак (true), получено {}", kind_name(value)),
            );
            None
        }),
        PropKind::Number => expect_number(value, file, path, errors).map(Value::Number),
        PropKind::Time => {
            expect_number(value, file, path, errors).map(|s| Value::Time(seconds_to_steps(s)))
        }
        PropKind::Vec2 => {
            let v = parse_vec2(value, file, path, errors)?;
            if prop == property::SIZE && (v[0] < 0.0 || v[1] < 0.0) {
                errors.push(
                    file,
                    path,
                    format!(
                        "размер не может быть отрицательным, получено [{}, {}]",
                        v[0], v[1]
                    ),
                );
                return None;
            }
            Some(Value::Vec2(v))
        }
        PropKind::Color => {
            let s = expect_string(value, file, path, errors)?;
            match parse_hex_color(&s) {
                Some(c) => Some(Value::Color(c)),
                None => {
                    errors.push(
                        file,
                        path,
                        format!("цвет должен быть вида \"#rrggbb\", получено \"{s}\""),
                    );
                    None
                }
            }
        }
        PropKind::Layer => {
            let n = expect_number(value, file, path, errors)?;
            if n.fract() != 0.0 {
                errors.push(
                    file,
                    path,
                    format!("layer должен быть целым числом, получено {n}"),
                );
                return None;
            }
            Some(Value::Layer(n as i32))
        }
        PropKind::Text => expect_string(value, file, path, errors).map(Value::Text),
        PropKind::Grid | PropKind::Keys => {
            errors.push(
                file,
                path,
                "это свойство не задаётся простым значением".to_string(),
            );
            None
        }
    }
}

fn parse_grid(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<GridSpec> {
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(obj, &["interval"], file, path, errors);
    let interval_json = require_field(obj, "interval", file, path, errors)?;
    let seconds = expect_number(interval_json, file, &join(path, "interval"), errors)?;
    if seconds <= 0.0 {
        errors.push(
            file,
            &join(path, "interval"),
            format!("interval должен быть больше нуля, получено {seconds}"),
        );
        return None;
    }
    Some(GridSpec {
        interval_steps: seconds_to_steps(seconds),
    })
}

fn parse_key_edits(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Vec<KeyEdit> {
    let mut edits = Vec::new();
    let Some(arr) = expect_array(value, file, path, errors) else {
        return edits;
    };
    for (i, entry) in arr.iter().enumerate() {
        let entry_path = join(path, &format!("[{i}]"));
        let Some(pair) = entry.as_array() else {
            errors.push(
                file,
                &entry_path,
                format!(
                    "ожидалась пара [свойство, значение], получено {}",
                    kind_name(entry)
                ),
            );
            continue;
        };
        if pair.len() != 2 {
            errors.push(
                file,
                &entry_path,
                format!(
                    "ожидалась пара [свойство, значение], элементов: {}",
                    pair.len()
                ),
            );
            continue;
        }
        let Some(name) = expect_string(&pair[0], file, &join(&entry_path, "[0]"), errors) else {
            continue;
        };
        let Some(prop) = resolve_property(&name, properties, file, &entry_path, errors) else {
            continue;
        };
        if let Some(v) = parse_scalar_value(
            &pair[1],
            prop,
            properties,
            file,
            &join(&entry_path, "[1]"),
            errors,
        ) {
            edits.push(KeyEdit {
                property: prop,
                value: v,
            });
        }
    }
    edits
}

fn parse_keys(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<KeyTable> {
    let obj = expect_object(value, file, path, errors)?;
    let mut table = KeyTable::new();
    for (code, binding_json) in obj {
        let binding_path = join(path, code);
        let Some(binding_obj) = expect_object(binding_json, file, &binding_path, errors) else {
            continue;
        };
        reject_unknown_keys(
            binding_obj,
            &["press", "release"],
            file,
            &binding_path,
            errors,
        );
        let press = match binding_obj.get("press") {
            Some(v) => parse_key_edits(v, properties, file, &join(&binding_path, "press"), errors),
            None => Vec::new(),
        };
        let release = match binding_obj.get("release") {
            Some(v) => {
                parse_key_edits(v, properties, file, &join(&binding_path, "release"), errors)
            }
            None => Vec::new(),
        };
        table.insert(code.clone(), KeyBinding { press, release });
    }
    Some(table)
}

fn resolve_property(
    name: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<PropertyId> {
    if name == property::IMAGE_NAME {
        errors.push(file, path, "картинки в этой версии не поддержаны");
        return None;
    }
    match properties.resolve(name) {
        Some(id) => Some(id),
        None => {
            errors.push(
                file,
                path,
                format!(
                    "неизвестное свойство \"{name}\": нет ни среди встроенных, ни в properties.json"
                ),
            );
            None
        }
    }
}

pub struct ParsedObject {
    pub path: String,
    pub name: Option<String>,
    pub values: Vec<(PropertyId, Value)>,
    pub grid: Option<GridSpec>,
    pub keys: Option<KeyTable>,
    pub shape: std::collections::HashSet<PropertyId>,
    /// Properties whose own field value failed to parse (wrong JSON kind, a broken `grid`/`keys`
    /// sub-object, …). Such a property's presence is known-incomplete for a reason already
    /// reported elsewhere, so `require` skips only that property on this object rather than
    /// compounding one data error into two messages — see `validate_property_sufficiency`. A key
    /// that didn't resolve to any property at all isn't tracked here: there's no `PropertyId` to
    /// skip a check for, and `require` never checks against one that doesn't exist.
    pub broken_properties: std::collections::HashSet<PropertyId>,
}

fn parse_scene_object(
    value: &Json,
    index: usize,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Option<ParsedObject> {
    let path = format!("objects[{index}]");
    let obj = expect_object(value, "scene.json", &path, errors)?;

    // `name` is a builtin property of kind `Text`, so the loop below already validates it
    // through `parse_scalar_value` and reports a broken value there. `name` isn't unique across
    // objects (forty arkanoid bricks all carry "brick"), so it's kept only as a human-readable
    // hint next to `path`, never as the sole way to address this object in an error message.
    let name = obj
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let mut values = Vec::new();
    let mut grid = None;
    let mut keys = None;
    let mut shape = std::collections::HashSet::new();
    let mut broken_properties = std::collections::HashSet::new();

    for (key, field_value) in obj {
        let field_path = join(&path, key);
        let Some(prop) = resolve_property(key, properties, "scene.json", &field_path, errors)
        else {
            continue;
        };
        match properties.kind(prop) {
            PropKind::Grid => {
                if let Some(spec) = parse_grid(field_value, "scene.json", &field_path, errors) {
                    grid = Some(spec);
                    shape.insert(prop);
                } else {
                    broken_properties.insert(prop);
                }
            }
            PropKind::Keys => {
                if let Some(table) =
                    parse_keys(field_value, properties, "scene.json", &field_path, errors)
                {
                    keys = Some(table);
                    shape.insert(prop);
                } else {
                    broken_properties.insert(prop);
                }
            }
            _ => {
                if let Some(v) = parse_scalar_value(
                    field_value,
                    prop,
                    properties,
                    "scene.json",
                    &field_path,
                    errors,
                ) {
                    // A flag set to `false` is absent for `World::has`, so it must be absent
                    // from the shape too: the prestart check and the running game have to
                    // agree on which objects a `has: [...]` selector matches.
                    let present = !matches!(v, Value::Flag(false));
                    values.push((prop, v));
                    if present {
                        shape.insert(prop);
                    }
                } else {
                    broken_properties.insert(prop);
                }
            }
        }
    }

    Some(ParsedObject {
        path,
        name,
        values,
        grid,
        keys,
        shape,
        broken_properties,
    })
}

fn parse_scene_json(
    text: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Vec<ParsedObject> {
    let Some(root) = parse_json_or_error("scene.json", text, errors) else {
        return Vec::new();
    };
    let Some(obj) = expect_object(&root, "scene.json", "", errors) else {
        return Vec::new();
    };
    reject_unknown_keys(obj, &["objects"], "scene.json", "", errors);
    let Some(objects_json) = obj.get("objects") else {
        // Same convention `require_field` uses: the path names the parent object (here the
        // root, `""`), not the missing key itself — a path pointing at a key that by definition
        // isn't in the text can never resolve to a location.
        errors.push("scene.json", "", "отсутствует список объектов");
        return Vec::new();
    };
    let Some(arr) = expect_array(objects_json, "scene.json", "objects", errors) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, v)| parse_scene_object(v, i, properties, errors))
        .collect()
}

fn parse_properties_json(text: &str, errors: &mut ErrorSink) -> PropertyTable {
    let mut table = PropertyTable::new();
    let Some(root) = parse_json_or_error("properties.json", text, errors) else {
        return table;
    };
    let Some(obj) = expect_object(&root, "properties.json", "", errors) else {
        return table;
    };
    reject_unknown_keys(obj, &["properties"], "properties.json", "", errors);
    let Some(props_json) = obj.get("properties") else {
        // Same convention `require_field` uses: the path names the parent (here the root, `""`),
        // not the missing key itself — a path pointing at a key that by definition isn't in the
        // text can never resolve to a location.
        errors.push("properties.json", "", "отсутствует объект properties");
        return table;
    };
    let Some(props_obj) = expect_object(props_json, "properties.json", "properties", errors) else {
        return table;
    };
    for (name, kind_json) in props_obj {
        let path = join("properties", name);
        let Some(kind_str) = expect_string(kind_json, "properties.json", &path, errors) else {
            continue;
        };
        let kind = match kind_str.as_str() {
            "flag" => PropKind::Flag,
            "number" => PropKind::Number,
            "time" => PropKind::Time,
            other => {
                errors.push(
                    "properties.json",
                    &path,
                    format!("неизвестный вид свойства \"{other}\": ожидался flag, number или time"),
                );
                continue;
            }
        };
        if let Err(existing) = table.declare_author(name, kind) {
            let _ = existing;
            errors.push(
                "properties.json",
                &path,
                format!("свойство \"{name}\" уже объявлено"),
            );
        }
    }
    table
}

struct SceneJsonConfig {
    width: u32,
    height: u32,
    background: [f32; 4],
}

fn parse_scene_config(value: &Json, errors: &mut ErrorSink) -> Option<SceneJsonConfig> {
    let obj = expect_object(value, "game.json", "scene", errors)?;
    reject_unknown_keys(
        obj,
        &["width", "height", "background"],
        "game.json",
        "scene",
        errors,
    );
    let width = require_field(obj, "width", "game.json", "scene", errors)
        .and_then(|v| expect_positive_u32(v, "game.json", "scene → width", errors));
    let height = require_field(obj, "height", "game.json", "scene", errors)
        .and_then(|v| expect_positive_u32(v, "game.json", "scene → height", errors));
    let background = require_field(obj, "background", "game.json", "scene", errors)
        .and_then(|v| expect_string(v, "game.json", "scene → background", errors))
        .and_then(|s| {
            parse_hex_color(&s).or_else(|| {
                errors.push(
                    "game.json",
                    "scene → background",
                    format!("цвет должен быть вида \"#rrggbb\", получено \"{s}\""),
                );
                None
            })
        });
    Some(SceneJsonConfig {
        width: width?,
        height: height?,
        background: background?,
    })
}

#[derive(Clone)]
pub struct FilePaths {
    pub properties: String,
    pub scene: String,
    pub rules: String,
}

#[derive(Clone)]
pub struct GameConfig {
    pub scene: SceneConfig,
    pub random_seed: u64,
    pub max_objects: usize,
    pub files: FilePaths,
}

/// The handshake between `read_entry` and `load_rest` that `wasm::Engine` drives: `None` before
/// any successful `read_entry`, or once `load_rest` has consumed one; `Some` in between. Kept
/// here, next to the two calls whose contract it enforces, rather than in the wasm-bindgen layer,
/// so the handshake itself needs no browser types and can be tested without one.
#[derive(Default)]
pub struct PendingConfig(Option<GameConfig>);

impl PendingConfig {
    /// Records what `result` means for the pending config: success replaces it, failure clears
    /// it — otherwise a failed `read_entry` would leave `load_rest` silently using the config an
    /// earlier successful call had left behind.
    pub fn set(&mut self, result: &Result<(GameConfig, Vec<GameError>), LoadFailure>) {
        self.0 = result.as_ref().ok().map(|(config, _)| config.clone());
    }

    /// Consumes the pending config, if any — mirrors `load_rest`'s one-shot use of it.
    pub fn take(&mut self) -> Option<GameConfig> {
        self.0.take()
    }
}

/// Parses just enough of `game.json` to tell the caller which files to fetch next. This is the
/// first of the two load calls the page makes; see `load_rest`. Same contract as `load_rest`:
/// warnings travel alongside the config on success and alongside the errors on failure, rather
/// than only on one of the two paths.
pub fn read_entry(text: &str) -> Result<(GameConfig, Vec<GameError>), LoadFailure> {
    let mut errors = ErrorSink::new();
    let config = parse_game_json(text, &mut errors);
    errors.fill_locations("game.json", text);
    let (errs, warnings) = errors.into_parts();
    if !errs.is_empty() {
        return Err(LoadFailure {
            errors: errs,
            warnings,
        });
    }
    Ok((config.expect("no errors means game.json parsed"), warnings))
}

fn parse_game_json(text: &str, errors: &mut ErrorSink) -> Option<GameConfig> {
    let root = parse_json_or_error("game.json", text, errors)?;
    let obj = expect_object(&root, "game.json", "", errors)?;
    reject_unknown_keys(
        obj,
        &["name", "scene", "random_seed", "max_objects", "files"],
        "game.json",
        "",
        errors,
    );

    // `name` isn't stored anywhere — «Формат игры» leaves it for humans and an external model —
    // but a value of the wrong kind is still a data error, not a silent no-op.
    if let Some(name) = obj.get("name") {
        expect_string(name, "game.json", "name", errors);
    }

    let scene = obj.get("scene").and_then(|v| parse_scene_config(v, errors));
    if obj.get("scene").is_none() {
        // Same convention `require_field` uses: the path names the parent (here the root, `""`),
        // not the missing key itself — a path pointing at a key that by definition isn't in the
        // text can never resolve to a location.
        errors.push("game.json", "", "отсутствует размер сцены");
    }

    let max_objects = match obj.get("max_objects") {
        Some(v) => expect_positive_u32(v, "game.json", "max_objects", errors),
        None => {
            errors.push("game.json", "", "отсутствует max_objects");
            None
        }
    };

    // «Формат игры» не ограничивает диапазон random_seed — просто «целое число, записанное в
    // game.json». Значение — сырое состояние splitmix64 (см. rng.rs), знак ему не важен, поэтому
    // принимается любое целое, какое умеет хранить JSON: положительное вплоть до u64::MAX через
    // as_u64, отрицательное — через as_i64 с оборачиванием в u64 по дополнению до двух.
    let random_seed = match obj.get("random_seed") {
        None => Some(0u64),
        Some(v) => match v.as_u64().or_else(|| v.as_i64().map(|n| n as u64)) {
            Some(n) => Some(n),
            None => {
                errors.push(
                    "game.json",
                    "random_seed",
                    format!("ожидалось целое число, получено {}", kind_name(v)),
                );
                None
            }
        },
    };

    let files_obj = obj
        .get("files")
        .and_then(|v| expect_object(v, "game.json", "files", errors));
    if obj.get("files").is_none() {
        errors.push("game.json", "", "отсутствует список файлов игры");
    }
    let files = files_obj.map(|f| {
        reject_unknown_keys(
            f,
            &["properties", "scene", "rules", "images"],
            "game.json",
            "files",
            errors,
        );
        let properties = require_field(f, "properties", "game.json", "files", errors)
            .and_then(|v| expect_string(v, "game.json", "files → properties", errors));
        let scene_path = require_field(f, "scene", "game.json", "files", errors)
            .and_then(|v| expect_string(v, "game.json", "files → scene", errors));
        let rules = require_field(f, "rules", "game.json", "files", errors)
            .and_then(|v| expect_string(v, "game.json", "files → rules", errors));
        // `files.images` isn't read anywhere yet — same "documented but unused" status as
        // `name` — but its value still has to be the string a path is.
        if let Some(images) = f.get("images") {
            expect_string(images, "game.json", "files → images", errors);
        }
        (properties, scene_path, rules)
    });

    let scene = scene?;
    let max_objects = max_objects?;
    let random_seed = random_seed?;
    let (properties, scene_path, rules) = files?;
    let (properties, scene_path, rules) = (properties?, scene_path?, rules?);

    Some(GameConfig {
        scene: SceneConfig {
            width: scene.width,
            height: scene.height,
            background: scene.background,
        },
        random_seed,
        max_objects: max_objects as usize,
        files: FilePaths {
            properties,
            scene: scene_path,
            rules,
        },
    })
}

fn resolve_property_list(
    field: Option<&Json>,
    properties: &PropertyTable,
    file: &str,
    field_path: &str,
    errors: &mut ErrorSink,
) -> Vec<PropertyId> {
    let Some(json) = field else { return Vec::new() };
    let Some(arr) = expect_array(json, file, field_path, errors) else {
        return Vec::new();
    };
    let mut ids = Vec::new();
    for v in arr {
        let Some(name) = expect_string(v, file, field_path, errors) else {
            continue;
        };
        if let Some(id) = resolve_property(&name, properties, file, field_path, errors) {
            ids.push(id);
        }
    }
    ids
}

fn resolve_selector(
    obj: &serde_json::Map<String, Json>,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Selector {
    reject_unknown_keys(obj, &["has", "without"], file, path, errors);
    let has = resolve_property_list(obj.get("has"), properties, file, &join(path, "has"), errors);
    let without = resolve_property_list(
        obj.get("without"),
        properties,
        file,
        &join(path, "without"),
        errors,
    );
    Selector { has, without }
}

fn parse_compare_op(s: &str) -> Option<CompareOp> {
    match s {
        "<" => Some(CompareOp::Lt),
        "<=" => Some(CompareOp::Le),
        ">" => Some(CompareOp::Gt),
        ">=" => Some(CompareOp::Ge),
        "==" => Some(CompareOp::Eq),
        "!=" => Some(CompareOp::Ne),
        _ => None,
    }
}

fn parse_condition(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Condition> {
    if let Some(s) = value.as_str() {
        return if s == "outside_scene" {
            Some(Condition::OutsideScene)
        } else {
            errors.push(file, path, format!("неизвестное условие \"{s}\""));
            None
        };
    }
    if let Some(arr) = value.as_array() {
        if arr.len() != 3 {
            errors.push(
                file,
                path,
                "сравнение должно быть [свойство, оператор, число]".to_string(),
            );
            return None;
        }
        let name = expect_string(&arr[0], file, &join(path, "[0]"), errors)?;
        let prop = resolve_property(&name, properties, file, &join(path, "[0]"), errors)?;
        let op_str = expect_string(&arr[1], file, &join(path, "[1]"), errors)?;
        let op = parse_compare_op(&op_str).or_else(|| {
            errors.push(
                file,
                &join(path, "[1]"),
                format!("неизвестный оператор сравнения \"{op_str}\""),
            );
            None
        })?;
        let num = expect_number(&arr[2], file, &join(path, "[2]"), errors)?;
        return Some(Condition::Compare {
            prop,
            op,
            value: num,
        });
    }
    if let Some(obj) = value.as_object() {
        if let Some(fewer) = obj.get("fewer_than") {
            let fewer_obj = expect_object(fewer, file, &join(path, "fewer_than"), errors)?;
            reject_unknown_keys(
                fewer_obj,
                &["count", "of"],
                file,
                &join(path, "fewer_than"),
                errors,
            );
            let count = require_field(fewer_obj, "count", file, &join(path, "fewer_than"), errors)
                .and_then(|v| {
                    expect_positive_u32(v, file, &join(path, "fewer_than → count"), errors)
                })?;
            let of_json = require_field(fewer_obj, "of", file, &join(path, "fewer_than"), errors)?;
            let of_obj = expect_object(of_json, file, &join(path, "fewer_than → of"), errors)?;
            let of = resolve_selector(
                of_obj,
                properties,
                file,
                &join(path, "fewer_than → of"),
                errors,
            );
            return Some(Condition::FewerThan { count, of });
        }
        if let Some(after) = obj.get("after_move_of") {
            let after_obj = expect_object(after, file, &join(path, "after_move_of"), errors)?;
            let of = resolve_selector(
                after_obj,
                properties,
                file,
                &join(path, "after_move_of"),
                errors,
            );
            return Some(Condition::AfterMoveOf { of });
        }
        errors.push(
            file,
            path,
            "условие не той формы: ожидался outside_scene, сравнение, fewer_than или after_move_of"
                .to_string(),
        );
        return None;
    }
    errors.push(file, path, "условие не той формы".to_string());
    None
}

fn parse_spawn_condition(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<SpawnCondition> {
    match parse_condition(value, properties, file, path, errors)? {
        Condition::FewerThan { count, of } => Some(SpawnCondition::FewerThan { count, of }),
        Condition::AfterMoveOf { of } => Some(SpawnCondition::AfterMoveOf { of }),
        _ => {
            errors.push(
                file,
                path,
                "у «создать» условие должно быть fewer_than или after_move_of".to_string(),
            );
            None
        }
    }
}

fn parse_common_actions(
    value: Option<&Json>,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Vec<CommonAction> {
    let Some(json) = value else { return Vec::new() };
    let Some(arr) = expect_array(json, file, path, errors) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            parse_common_action(
                entry,
                properties,
                file,
                &join(path, &format!("[{i}]")),
                errors,
            )
        })
        .collect()
}

fn require_index<'a>(
    arr: &'a [Json],
    index: usize,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<&'a Json> {
    match arr.get(index) {
        Some(v) => Some(v),
        None => {
            errors.push(file, path, format!("отсутствует элемент [{index}]"));
            None
        }
    }
}

fn parse_common_action(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<CommonAction> {
    let arr = expect_array(value, file, path, errors)?;
    let Some(head_json) = arr.first() else {
        errors.push(
            file,
            path,
            "действие не той формы: пустой список".to_string(),
        );
        return None;
    };
    let head = expect_string(head_json, file, &join(path, "[0]"), errors)?;
    match head.as_str() {
        "end_game" => {
            let outcome_str = require_index(arr, 1, file, path, errors).and_then(|v| v.as_str());
            match outcome_str {
                Some("win") => Some(CommonAction::EndGame(Outcome::Win)),
                Some("loss") => Some(CommonAction::EndGame(Outcome::Loss)),
                _ => {
                    errors.push(
                        file,
                        path,
                        "end_game должен быть \"win\" или \"loss\"".to_string(),
                    );
                    None
                }
            }
        }
        "add" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            let prop = resolve_property(&name, properties, file, &join(path, "[1]"), errors)?;
            let kind = properties.kind(prop);
            if kind != PropKind::Number && kind != PropKind::Time {
                errors.push(
                    file,
                    path,
                    format!(
                        "add применим только к числу или времени, у \"{name}\" вид {}",
                        kind.label()
                    ),
                );
                return None;
            }
            let raw = expect_number(
                require_index(arr, 2, file, path, errors)?,
                file,
                &join(path, "[2]"),
                errors,
            )?;
            let delta = if kind == PropKind::Time {
                Value::Time(seconds_to_steps_delta(raw))
            } else {
                Value::Number(raw)
            };
            Some(CommonAction::Add { prop, value: delta })
        }
        other => {
            errors.push(
                file,
                path,
                format!("неизвестное общее действие \"{other}\""),
            );
            None
        }
    }
}

fn parse_collide_effects(
    value: Option<&Json>,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Vec<CollideEffect> {
    let Some(json) = value else { return Vec::new() };
    let Some(arr) = expect_array(json, file, path, errors) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, entry)| {
            parse_collide_effect(
                entry,
                properties,
                file,
                &join(path, &format!("[{i}]")),
                errors,
            )
        })
        .collect()
}

fn parse_collide_effect(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<CollideEffect> {
    let arr = expect_array(value, file, path, errors)?;
    let Some(head_json) = arr.first() else {
        errors.push(
            file,
            path,
            "действие не той формы: пустой список".to_string(),
        );
        return None;
    };
    let head = expect_string(head_json, file, &join(path, "[0]"), errors)?;
    match head.as_str() {
        "bounce" => Some(CollideEffect::Bounce),
        "delete" => Some(CollideEffect::Delete),
        "add" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            let prop = resolve_property(&name, properties, file, &join(path, "[1]"), errors)?;
            let kind = properties.kind(prop);
            if kind != PropKind::Number && kind != PropKind::Time {
                errors.push(
                    file,
                    path,
                    format!(
                        "add применим только к числу или времени, у \"{name}\" вид {}",
                        kind.label()
                    ),
                );
                return None;
            }
            let raw = expect_number(
                require_index(arr, 2, file, path, errors)?,
                file,
                &join(path, "[2]"),
                errors,
            )?;
            let value = if kind == PropKind::Time {
                Value::Time(seconds_to_steps_delta(raw))
            } else {
                Value::Number(raw)
            };
            Some(CollideEffect::Add { prop, value })
        }
        "set" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            let prop = resolve_property(&name, properties, file, &join(path, "[1]"), errors)?;
            let raw_value = require_index(arr, 2, file, path, errors)?;
            let parsed = parse_scalar_value(
                raw_value,
                prop,
                properties,
                file,
                &join(path, "[2]"),
                errors,
            )?;
            Some(CollideEffect::Set {
                prop,
                value: parsed,
            })
        }
        "give" | "take" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            let prop = resolve_property(&name, properties, file, &join(path, "[1]"), errors)?;
            if properties.kind(prop) != PropKind::Flag {
                errors.push(
                    file,
                    path,
                    format!(
                        "{head} применим только к признаку, у \"{name}\" вид {}",
                        properties.kind(prop).label()
                    ),
                );
                return None;
            }
            if head == "give" {
                Some(CollideEffect::Give { prop })
            } else {
                Some(CollideEffect::Take { prop })
            }
        }
        other => {
            errors.push(
                file,
                path,
                format!("неизвестное действие столкновения \"{other}\""),
            );
            None
        }
    }
}

/// Second element: properties whose template value failed to parse — a field skipped here for a
/// reason already reported as its own error, so `possible_shapes` marks the resulting spawn shape
/// broken on exactly that property, the same way `ParsedObject::broken_properties` does for a
/// scene object — see `validate_property_sufficiency`.
fn parse_template(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> (
    Vec<(PropertyId, TemplateValue)>,
    std::collections::HashSet<PropertyId>,
) {
    let mut template = Vec::new();
    let mut broken_properties = std::collections::HashSet::new();
    let Some(obj) = expect_object(value, file, path, errors) else {
        return (template, broken_properties);
    };
    for (name, field_value) in obj {
        let field_path = join(path, name);
        let Some(prop) = resolve_property(name, properties, file, &field_path, errors) else {
            continue;
        };
        if let Some(from_parent_obj) = field_value.as_object()
            && let Some(parent_prop_json) = from_parent_obj.get("from_parent")
        {
            let Some(parent_name) = expect_string(
                parent_prop_json,
                file,
                &join(&field_path, "from_parent"),
                errors,
            ) else {
                broken_properties.insert(prop);
                continue;
            };
            let Some(parent_prop) = resolve_property(
                &parent_name,
                properties,
                file,
                &join(&field_path, "from_parent"),
                errors,
            ) else {
                broken_properties.insert(prop);
                continue;
            };
            template.push((prop, TemplateValue::FromParent(parent_prop)));
            continue;
        }
        if let Some(v) =
            parse_scalar_value(field_value, prop, properties, file, &field_path, errors)
        {
            template.push((prop, TemplateValue::Const(v)));
        } else {
            broken_properties.insert(prop);
        }
    }
    (template, broken_properties)
}

/// Known keys across every rule kind — used to still catch a typo in a rule whose own `kind` is
/// missing or unrecognized, rather than let it hide behind the `kind` error until a later pass.
const ALL_RULE_KEYS: &[&str] = &[
    "kind", "for", "a", "b", "effects", "do", "when", "where", "template",
];

fn parse_rule(
    value: &Json,
    index: usize,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Option<(Rule, std::collections::HashSet<PropertyId>)> {
    let path = format!("rules[{index}]");
    let obj = expect_object(value, "rules.json", &path, errors)?;
    let kind = match obj.get("kind") {
        Some(v) => expect_string(v, "rules.json", &join(&path, "kind"), errors),
        None => {
            // Same convention `require_field` uses: the path names the rule itself, not the
            // missing key — a path pointing at a key that by definition isn't in the text can
            // never resolve to a location.
            errors.push("rules.json", &path, "отсутствует вид правила (kind)");
            None
        }
    };
    match kind.as_deref() {
        Some("move") => {
            reject_unknown_keys(obj, &["kind", "for"], "rules.json", &path, errors);
            let for_json = require_field(obj, "for", "rules.json", &path, errors)?;
            let for_obj = expect_object(for_json, "rules.json", &join(&path, "for"), errors)?;
            let for_ = resolve_selector(
                for_obj,
                properties,
                "rules.json",
                &join(&path, "for"),
                errors,
            );
            Some((Rule::Move { for_ }, std::collections::HashSet::new()))
        }
        Some("collide") => {
            reject_unknown_keys(
                obj,
                &["kind", "a", "b", "effects", "do"],
                "rules.json",
                &path,
                errors,
            );
            let a_json = require_field(obj, "a", "rules.json", &path, errors)?;
            let b_json = require_field(obj, "b", "rules.json", &path, errors)?;
            let a_obj = expect_object(a_json, "rules.json", &join(&path, "a"), errors)?;
            let b_obj = expect_object(b_json, "rules.json", &join(&path, "b"), errors)?;
            let a = resolve_selector(a_obj, properties, "rules.json", &join(&path, "a"), errors);
            let b = resolve_selector(b_obj, properties, "rules.json", &join(&path, "b"), errors);
            let effects_json = obj
                .get("effects")
                .and_then(|v| expect_object(v, "rules.json", &join(&path, "effects"), errors));
            if let Some(e) = effects_json {
                reject_unknown_keys(
                    e,
                    &["a", "b"],
                    "rules.json",
                    &join(&path, "effects"),
                    errors,
                );
            }
            let effects_a = parse_collide_effects(
                effects_json.and_then(|e| e.get("a")),
                properties,
                "rules.json",
                &join(&path, "effects → a"),
                errors,
            );
            let effects_b = parse_collide_effects(
                effects_json.and_then(|e| e.get("b")),
                properties,
                "rules.json",
                &join(&path, "effects → b"),
                errors,
            );
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                "rules.json",
                &join(&path, "do"),
                errors,
            );
            Some((
                Rule::Collide {
                    a,
                    b,
                    effects_a,
                    effects_b,
                    do_,
                },
                std::collections::HashSet::new(),
            ))
        }
        Some("delete") => {
            reject_unknown_keys(
                obj,
                &["kind", "for", "when", "do"],
                "rules.json",
                &path,
                errors,
            );
            let for_json = require_field(obj, "for", "rules.json", &path, errors)?;
            let for_obj = expect_object(for_json, "rules.json", &join(&path, "for"), errors)?;
            let for_ = resolve_selector(
                for_obj,
                properties,
                "rules.json",
                &join(&path, "for"),
                errors,
            );
            let when_json = require_field(obj, "when", "rules.json", &path, errors)?;
            let when = parse_condition(
                when_json,
                properties,
                "rules.json",
                &join(&path, "when"),
                errors,
            )?;
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                "rules.json",
                &join(&path, "do"),
                errors,
            );
            Some((
                Rule::Delete { for_, when, do_ },
                std::collections::HashSet::new(),
            ))
        }
        Some("spawn") => {
            reject_unknown_keys(
                obj,
                &["kind", "when", "where", "template", "do"],
                "rules.json",
                &path,
                errors,
            );
            let when_json = require_field(obj, "when", "rules.json", &path, errors)?;
            let when = parse_spawn_condition(
                when_json,
                properties,
                "rules.json",
                &join(&path, "when"),
                errors,
            )?;
            let where_json = obj.get("where");
            let place = match where_json.and_then(|v| v.as_str()) {
                Some("at_parent") => SpawnPlace::AtParent,
                Some("random_cell") => SpawnPlace::RandomCell,
                _ => {
                    // Same convention `require_field` uses when the key is missing outright: the
                    // path names the rule itself, not a key that by definition isn't in the text.
                    // When `where` is present with the wrong value, its own path still resolves.
                    let where_path = join(&path, "where");
                    let err_path: &str = if where_json.is_some() {
                        &where_path
                    } else {
                        &path
                    };
                    errors.push(
                        "rules.json",
                        err_path,
                        "ожидалось \"at_parent\" или \"random_cell\"".to_string(),
                    );
                    return None;
                }
            };
            if matches!(place, SpawnPlace::AtParent)
                && matches!(when, SpawnCondition::FewerThan { .. })
            {
                errors.push(
                    "rules.json",
                    &join(&path, "where"),
                    "at_parent недоступен с условием fewer_than: у него нет родителя".to_string(),
                );
                return None;
            }
            let template_json = require_field(obj, "template", "rules.json", &path, errors)?;
            let (template, template_broken) = parse_template(
                template_json,
                properties,
                "rules.json",
                &join(&path, "template"),
                errors,
            );
            if matches!(when, SpawnCondition::FewerThan { .. })
                && template
                    .iter()
                    .any(|(_, v)| matches!(v, TemplateValue::FromParent(_)))
            {
                errors.push(
                    "rules.json",
                    &join(&path, "template"),
                    "from_parent недоступен с условием fewer_than: у него нет родителя".to_string(),
                );
            }
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                "rules.json",
                &join(&path, "do"),
                errors,
            );
            Some((
                Rule::Spawn {
                    when,
                    place,
                    template,
                    do_,
                },
                template_broken,
            ))
        }
        Some(other) => {
            reject_unknown_keys(obj, ALL_RULE_KEYS, "rules.json", &path, errors);
            errors.push(
                "rules.json",
                &join(&path, "kind"),
                format!("неизвестный вид правила \"{other}\""),
            );
            None
        }
        None => {
            reject_unknown_keys(obj, ALL_RULE_KEYS, "rules.json", &path, errors);
            None
        }
    }
}

/// Per-rule load-time metadata that must stay aligned with `RuleSet::rules` by position — one type
/// instead of two same-position vectors held together only by a comment, so the two can never drift
/// apart. `file_index` is this rule's position in `rules.json`'s own `rules` array, distinct from
/// its position in `RuleSet::rules`: a rule earlier in the file that failed to parse (unknown kind,
/// missing field, …) is dropped by `parse_rules_json`'s `filter_map` and never occupies a slot in
/// `RuleSet::rules`, so every rule after it shifts down — an error naming a rule by its
/// `RuleSet::rules` position would then point at the wrong rule in the file. `template_broken` is
/// the set of properties whose spawn-template value failed to parse (empty for every non-`spawn`
/// rule) — `possible_shapes` needs it to mark the spawn shape broken on exactly that property; see
/// `parse_template`.
struct RuleMeta {
    file_index: usize,
    template_broken: std::collections::HashSet<PropertyId>,
}

type ParsedRules = (RuleSet, Vec<RuleMeta>);

fn parse_rules_json(text: &str, properties: &PropertyTable, errors: &mut ErrorSink) -> ParsedRules {
    let Some(root) = parse_json_or_error("rules.json", text, errors) else {
        return (RuleSet::default(), Vec::new());
    };
    let Some(obj) = expect_object(&root, "rules.json", "", errors) else {
        return (RuleSet::default(), Vec::new());
    };
    reject_unknown_keys(obj, &["rules"], "rules.json", "", errors);
    let Some(rules_json) = obj.get("rules") else {
        errors.push("rules.json", "", "отсутствует список правил");
        return (RuleSet::default(), Vec::new());
    };
    let Some(arr) = expect_array(rules_json, "rules.json", "rules", errors) else {
        return (RuleSet::default(), Vec::new());
    };
    let (rules, meta): (Vec<Rule>, Vec<RuleMeta>) = arr
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let (rule, template_broken) = parse_rule(v, i, properties, errors)?;
            Some((
                rule,
                RuleMeta {
                    file_index: i,
                    template_broken,
                },
            ))
        })
        .unzip();
    (RuleSet { rules }, meta)
}

/// A candidate object's shape: `certain` properties are always present at spawn time; `maybe`
/// properties could become present at runtime but aren't guaranteed — a `from_parent` flag whose
/// actual presence depends on the parent, a flag some `give` or a `keys` edit could add;
/// `removable` properties are `certain` ones that could stop being present at runtime — a flag
/// some `take`, a `set` to `false`, or a `keys` edit to `false` could remove, so a `without`
/// selector can't rule them out just because they're certain at load time. `broken_properties`
/// names the properties whose own data failed to parse, making just that property's presence
/// known-incomplete for a reason already reported elsewhere — see `validate_property_sufficiency`.
struct Shape {
    certain: std::collections::HashSet<PropertyId>,
    maybe: std::collections::HashSet<PropertyId>,
    removable: std::collections::HashSet<PropertyId>,
    broken_properties: std::collections::HashSet<PropertyId>,
}

/// Whether some assignment of the `maybe`/`removable` properties could make `selector` match this
/// shape. A property in both `has` and `without` can never be satisfied by any single object —
/// present (for `has`) and absent (for `without`) at once — so such a selector matches nothing,
/// regardless of shape; checked up front because `has` alone accepts a `maybe` property
/// optimistically (assuming present), and `without` alone accepts a `maybe` property
/// optimistically too (assuming absent) and a `removable` `certain` property optimistically
/// (assuming taken away) — combining those optimistic checks would wrongly agree. On the `without`
/// side, a `broken_properties` property's actual presence is unknown, not known-absent, so
/// `without` may not optimistically assume it's missing — not even when a `take`/`set false`
/// elsewhere widened `removable` for it too, since that widening never checks whether the
/// property is broken on this particular shape. `has` needs no matching guard: it accepts a
/// `maybe` property optimistically no matter whether that property is also broken, and that's
/// correct — being in `maybe` already reflects a real way the property could become present at
/// runtime (`give`/`set`/a `keys` edit, or `from_parent`), broken initial value or not; a broken
/// property that was never so widened is simply absent from both `certain` and `maybe`, so `has`
/// still fails to match on it.
fn matches_shape(selector: &Selector, shape: &Shape) -> bool {
    if selector.has.iter().any(|p| selector.without.contains(p)) {
        return false;
    }
    selector
        .has
        .iter()
        .all(|p| shape.certain.contains(p) || shape.maybe.contains(p))
        && selector.without.iter().all(|p| {
            !shape.broken_properties.contains(p)
                && (!shape.certain.contains(p) || shape.removable.contains(p))
        })
}

struct Candidate<'a> {
    shape: &'a Shape,
    /// Where to find this candidate in its file — a machine-unique address (`scene.json →
    /// objects[3]`, `шаблон rules[2]`), with the author's `name` field appended as a hint only
    /// when one is present. `name` alone can't address a candidate: it isn't unique (forty
    /// arkanoid bricks all carry "brick").
    locator: &'a str,
}

fn require(
    candidates: &[Candidate],
    selector: &Selector,
    needed: PropertyId,
    properties: &PropertyTable,
    rule_label: &str,
    seen: &mut std::collections::HashSet<(usize, PropertyId)>,
    errors: &mut ErrorSink,
) {
    for (idx, c) in candidates.iter().enumerate() {
        if !matches_shape(selector, c.shape) || c.shape.certain.contains(&needed) {
            continue;
        }
        if c.shape.broken_properties.contains(&needed) {
            // `needed` itself failed to parse on this candidate — already reported as its own
            // error; reporting it missing too would be a second message for the same data error.
            continue;
        }
        // `needed` is missing from every assignment the selector can match under, unless it's a
        // `maybe` flag the same selector's `has` already forces present whenever it matches.
        let forced_by_selector = c.shape.maybe.contains(&needed) && selector.has.contains(&needed);
        if forced_by_selector {
            continue;
        }
        if !seen.insert((idx, needed)) {
            continue;
        }
        errors.push(
            "rules.json",
            rule_label,
            format!(
                "объекту {} не хватает свойства \"{}\" для этого правила",
                c.locator,
                properties.name(needed)
            ),
        );
    }
}

/// A scene object's own `keys` table can write any of its properties at runtime (`apply_input`
/// in `core/step.rs`), regardless of whether that property is present on the object at load
/// time. A `flag` edit to `false` (`World::set_flag(id, prop, false)`) can only ever remove
/// presence, never grant it, so it widens `removable` instead of `maybe` — the mirror of a
/// collide rule's `take`/`give` split, just via a key press instead of a collision. Every other
/// edit — a flag to `true`, or any value of another kind — can make the property present, so it
/// widens `maybe`.
fn keys_edited_properties(
    obj: &ParsedObject,
) -> (
    std::collections::HashSet<PropertyId>,
    std::collections::HashSet<PropertyId>,
) {
    let mut maybe = std::collections::HashSet::new();
    let mut removable = std::collections::HashSet::new();
    if let Some(table) = &obj.keys {
        for binding in table.values() {
            for edit in binding.press.iter().chain(binding.release.iter()) {
                if matches!(edit.value, Value::Flag(false)) {
                    removable.insert(edit.property);
                } else {
                    maybe.insert(edit.property);
                }
            }
        }
    }
    (maybe, removable)
}

/// A `["give", prop]` collide effect (`CollideEffect::Give`, `core/step.rs`'s `set_flag(id, prop,
/// true)`) hands a flag to whichever object plays its side of the rule, with no requirement that
/// the object already carry it — the whole point of `give` is to add a flag that wasn't there, so
/// it widens `maybe`. `["set", prop, value]` (`World::set_value`) writes unconditionally too,
/// regardless of whether the object carried `prop` before, so any `set` other than to `Flag(false)`
/// widens `maybe` exactly like `give`; `["take", prop]` and `["set", prop, false]` are the mirror
/// at removal — they widen `removable` instead, the same way a `keys` edit to `false` does (see
/// `keys_edited_properties`). Both widenings feed back into which shapes `matches_shape` considers
/// for other selectors, so the pass repeats until nothing new is added: a shape widened by one
/// rule's `give`/`take`/`set` can make it match another rule's selector too.
///
/// A rule only ever fires when something matches *both* its sides — `a` and `b` collide as a
/// pair — so widening from `effects_a` (or `effects_b`) is sound only once some shape already
/// matches the *other* side too; otherwise the rule can never trigger and the widening would be
/// describing a state the game can't reach. Checked fresh each pass, since an earlier rule's
/// widening in the same pass can be what makes the other side matchable.
fn widen_shapes_by_collide_effects(shapes: &mut [(Shape, String)], rules: &RuleSet) {
    let mut changed = true;
    while changed {
        changed = false;
        for rule in &rules.rules {
            let Rule::Collide {
                a,
                b,
                effects_a,
                effects_b,
                ..
            } = rule
            else {
                continue;
            };
            let a_matches = shapes.iter().any(|(s, _)| matches_shape(a, s));
            let b_matches = shapes.iter().any(|(s, _)| matches_shape(b, s));
            if !a_matches || !b_matches {
                continue;
            }
            for (selector, effects) in [(a, effects_a), (b, effects_b)] {
                let mut given = Vec::new();
                let mut taken = Vec::new();
                for effect in effects {
                    match effect {
                        CollideEffect::Give { prop } => given.push(*prop),
                        CollideEffect::Take { prop } => taken.push(*prop),
                        CollideEffect::Set {
                            prop,
                            value: Value::Flag(false),
                        } => taken.push(*prop),
                        CollideEffect::Set { prop, .. } => given.push(*prop),
                        _ => {}
                    }
                }
                if given.is_empty() && taken.is_empty() {
                    continue;
                }
                for (shape, _) in shapes.iter_mut() {
                    if !matches_shape(selector, shape) {
                        continue;
                    }
                    for prop in &given {
                        changed |= shape.maybe.insert(*prop);
                    }
                    for prop in &taken {
                        changed |= shape.removable.insert(*prop);
                    }
                }
            }
        }
    }
}

/// "движок берёт все объекты, какие вообще могут существовать": scene objects plus one shape
/// per spawn template, each widened by every property the game could still hand it or take away
/// at runtime through `give`/`take`/`set` or a `keys` edit — see
/// `widen_shapes_by_collide_effects` and `keys_edited_properties`.
fn possible_shapes(
    scene: &[ParsedObject],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    properties: &PropertyTable,
) -> Vec<(Shape, String)> {
    let mut shapes: Vec<(Shape, String)> = scene
        .iter()
        .map(|o| {
            let locator = join("scene.json", &o.path);
            let locator = match &o.name {
                Some(name) => format!("{locator} (имя \"{name}\")"),
                None => locator,
            };
            let (maybe, removable) = keys_edited_properties(o);
            (
                Shape {
                    certain: o.shape.clone(),
                    maybe,
                    removable,
                    broken_properties: o.broken_properties.clone(),
                },
                locator,
            )
        })
        .collect();
    for (rule, meta) in rules.rules.iter().zip(rule_meta) {
        if let Rule::Spawn { template, .. } = rule {
            // `where` always gives a spawned object a position, even though the template
            // itself never spells it out (Формат игры: "в template его записывать не нужно").
            // A constant `false` flag is absent for `World::has` at runtime (see
            // `false_flag_does_not_satisfy_has_selector_at_prestart`), so it's absent from the
            // possible shape too. A `from_parent` flag's value is unknown at load time — the
            // parent may or may not carry it — so it goes into `maybe`, not `certain`.
            let mut certain: std::collections::HashSet<PropertyId> = template
                .iter()
                .filter(|(p, tv)| {
                    if properties.kind(*p) != PropKind::Flag {
                        return true;
                    }
                    !matches!(
                        tv,
                        TemplateValue::Const(Value::Flag(false)) | TemplateValue::FromParent(_)
                    )
                })
                .map(|(p, _)| *p)
                .collect();
            certain.insert(property::POSITION);

            let maybe: std::collections::HashSet<PropertyId> = template
                .iter()
                .filter(|(p, tv)| {
                    properties.kind(*p) == PropKind::Flag
                        && matches!(tv, TemplateValue::FromParent(_))
                })
                .map(|(p, _)| *p)
                .collect();

            let name = format!("шаблон rules[{}]", meta.file_index);
            shapes.push((
                Shape {
                    certain,
                    maybe,
                    removable: std::collections::HashSet::new(),
                    broken_properties: meta.template_broken.clone(),
                },
                name,
            ));
        }
    }
    widen_shapes_by_collide_effects(&mut shapes, rules);
    shapes
}

fn validate_property_sufficiency(
    shapes: &[(Shape, String)],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) {
    let candidates: Vec<Candidate> = shapes
        .iter()
        .map(|(shape, locator)| Candidate { shape, locator })
        .collect();

    for (rule, meta) in rules.rules.iter().zip(rule_meta) {
        let label = format!("rules[{}]", meta.file_index);
        // One candidate+property pair is reported at most once per rule, even when several
        // require() calls ask for it — e.g. two `add`s on the same property in one `effects`,
        // or `bounce` and an explicit `set velocity` on the same selector.
        let mut seen = std::collections::HashSet::new();
        match rule {
            Rule::Move { for_ } => {
                require(
                    &candidates,
                    for_,
                    property::POSITION,
                    properties,
                    &label,
                    &mut seen,
                    errors,
                );
                require(
                    &candidates,
                    for_,
                    property::VELOCITY,
                    properties,
                    &label,
                    &mut seen,
                    errors,
                );
            }
            Rule::Collide {
                a,
                b,
                effects_a,
                effects_b,
                ..
            } => {
                for (selector, effects) in [(a, effects_a), (b, effects_b)] {
                    require(
                        &candidates,
                        selector,
                        property::POSITION,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                    require(
                        &candidates,
                        selector,
                        property::SIZE,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                    if effects.iter().any(|e| matches!(e, CollideEffect::Bounce)) {
                        require(
                            &candidates,
                            selector,
                            property::VELOCITY,
                            properties,
                            &label,
                            &mut seen,
                            errors,
                        );
                    }
                    // `add` needs the property already present to add to
                    // (`World::add_number_like` silently no-ops on an absent one); `set` doesn't —
                    // `World::set_value` writes unconditionally regardless of prior presence, the
                    // same as a `keys` edit (see `keys_edited_properties`) — so only `add` is
                    // required here.
                    for effect in effects {
                        if let CollideEffect::Add { prop, .. } = effect {
                            require(
                                &candidates,
                                selector,
                                *prop,
                                properties,
                                &label,
                                &mut seen,
                                errors,
                            );
                        }
                    }
                }
            }
            Rule::Delete { for_, when, .. } => match when {
                Condition::Compare { prop, .. } => require(
                    &candidates,
                    for_,
                    *prop,
                    properties,
                    &label,
                    &mut seen,
                    errors,
                ),
                Condition::OutsideScene => {
                    require(
                        &candidates,
                        for_,
                        property::POSITION,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                    require(
                        &candidates,
                        for_,
                        property::SIZE,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                }
                // «Формат игры»: условие спрашивает, сместился ли на этом шаге хоть один объект
                // из отбора `of` — само оно положения кандидата на удаление (`for_`) не читает,
                // так что требовать `position` у `for_` не за что.
                Condition::AfterMoveOf { .. } => {}
                Condition::FewerThan { .. } => {}
            },
            Rule::Spawn { when, template, .. } => {
                if let SpawnCondition::AfterMoveOf { of } = when {
                    for (_, tv) in template {
                        if let TemplateValue::FromParent(parent_prop) = tv {
                            require(
                                &candidates,
                                of,
                                *parent_prop,
                                properties,
                                &label,
                                &mut seen,
                                errors,
                            );
                        }
                    }
                }
            }
        }
        // `do`'s add: declared, and present on at least one object anywhere.
        let do_actions: &[CommonAction] = match rule {
            Rule::Collide { do_, .. } | Rule::Delete { do_, .. } | Rule::Spawn { do_, .. } => do_,
            Rule::Move { .. } => &[],
        };
        // Отметка ставится вставкой последним звеном цепочки условий ниже: только когда сообщение
        // действительно выдаётся. Перестановка звеньев отметит свойство до проверки и проглотит
        // ошибку — порядок здесь значащий.
        let mut reported_do_adds = std::collections::HashSet::new();
        for action in do_actions {
            if let CommonAction::Add { prop, .. } = action
                && !shapes
                    .iter()
                    .any(|(s, _)| s.certain.contains(prop) || s.maybe.contains(prop))
                && reported_do_adds.insert(*prop)
            {
                errors.push(
                    "rules.json",
                    &label,
                    format!(
                        "do → add: свойство \"{}\" не встречается ни у одного объекта",
                        properties.name(*prop)
                    ),
                );
            }
        }
    }
}

fn mark_selector_used(used: &mut std::collections::HashSet<PropertyId>, selector: &Selector) {
    used.extend(selector.has.iter().copied());
    used.extend(selector.without.iter().copied());
}

fn mark_condition_used(used: &mut std::collections::HashSet<PropertyId>, condition: &Condition) {
    match condition {
        Condition::Compare { prop, .. } => {
            used.insert(*prop);
        }
        Condition::OutsideScene => {}
        Condition::FewerThan { of, .. } | Condition::AfterMoveOf { of } => {
            mark_selector_used(used, of);
        }
    }
}

fn mark_spawn_condition_used(
    used: &mut std::collections::HashSet<PropertyId>,
    condition: &SpawnCondition,
) {
    match condition {
        SpawnCondition::FewerThan { of, .. } | SpawnCondition::AfterMoveOf { of } => {
            mark_selector_used(used, of);
        }
    }
}

fn mark_common_actions_used(
    used: &mut std::collections::HashSet<PropertyId>,
    actions: &[CommonAction],
) {
    for action in actions {
        if let CommonAction::Add { prop, .. } = action {
            used.insert(*prop);
        }
    }
}

/// Every property referenced anywhere in the loaded data: on a scene object (including a value
/// set by a `keys` edit), or by a rule's selector, condition, action or spawn template. Whether
/// the reference is itself valid is `validate_property_sufficiency`'s job, not this one — a
/// property named only inside a broken rule still counts as "used" here.
fn collect_used_properties(
    scene: &[ParsedObject],
    rules: &RuleSet,
) -> std::collections::HashSet<PropertyId> {
    let mut used = std::collections::HashSet::new();
    for obj in scene {
        used.extend(obj.shape.iter().copied());
        if let Some(table) = &obj.keys {
            for binding in table.values() {
                used.extend(binding.press.iter().map(|e| e.property));
                used.extend(binding.release.iter().map(|e| e.property));
            }
        }
    }
    for rule in &rules.rules {
        match rule {
            Rule::Move { for_ } => mark_selector_used(&mut used, for_),
            Rule::Collide {
                a,
                b,
                effects_a,
                effects_b,
                do_,
            } => {
                mark_selector_used(&mut used, a);
                mark_selector_used(&mut used, b);
                for effect in effects_a.iter().chain(effects_b) {
                    match effect {
                        CollideEffect::Add { prop, .. }
                        | CollideEffect::Set { prop, .. }
                        | CollideEffect::Give { prop }
                        | CollideEffect::Take { prop } => {
                            used.insert(*prop);
                        }
                        CollideEffect::Bounce | CollideEffect::Delete => {}
                    }
                }
                mark_common_actions_used(&mut used, do_);
            }
            Rule::Delete { for_, when, do_ } => {
                mark_selector_used(&mut used, for_);
                mark_condition_used(&mut used, when);
                mark_common_actions_used(&mut used, do_);
            }
            Rule::Spawn {
                when,
                template,
                do_,
                ..
            } => {
                mark_spawn_condition_used(&mut used, when);
                for (prop, tv) in template {
                    used.insert(*prop);
                    if let TemplateValue::FromParent(parent_prop) = tv {
                        used.insert(*parent_prop);
                    }
                }
                mark_common_actions_used(&mut used, do_);
            }
        }
    }
    used
}

/// «Формат игры»: предупреждение (игра идёт), если объявленное в `properties.json` свойство не
/// встречается нигде — ни на одном объекте сцены, ни в одном правиле.
fn validate_unused_properties(
    properties: &PropertyTable,
    scene: &[ParsedObject],
    rules: &RuleSet,
    errors: &mut ErrorSink,
) {
    let used = collect_used_properties(scene, rules);
    for (id, def) in properties.iter() {
        if def.builtin || used.contains(&id) {
            continue;
        }
        errors.push_warning(
            "properties.json",
            &join("properties", &def.name),
            format!(
                "свойство \"{}\" объявлено, но не встречается ни на одном объекте сцены и ни в одном правиле; ожидалось, что объявленное свойство где-то используется",
                def.name
            ),
        );
    }
}

fn find_value(values: &[(PropertyId, Value)], prop: PropertyId) -> Option<&Value> {
    values.iter().find(|(p, _)| *p == prop).map(|(_, v)| v)
}

fn name_suffix(obj: &ParsedObject) -> String {
    match &obj.name {
        Some(name) => format!(" (имя \"{name}\")"),
        None => String::new(),
    }
}

/// «Формат игры»: предупреждение (игра идёт), если объект сцены целиком стоит за пределами
/// сцены — его прямоугольник не пересекается с `[0,0]..[width,height]` вовсе. Объект без
/// `position` или `size` не проверяется: его прямоугольник неизвестен.
fn validate_objects_within_scene(
    scene: &[ParsedObject],
    config: &SceneConfig,
    errors: &mut ErrorSink,
) {
    for obj in scene {
        let (Some(Value::Vec2(pos)), Some(Value::Vec2(size))) = (
            find_value(&obj.values, property::POSITION),
            find_value(&obj.values, property::SIZE),
        ) else {
            continue;
        };
        let outside = pos[0] + size[0] <= 0.0
            || pos[0] >= config.width as f64
            || pos[1] + size[1] <= 0.0
            || pos[1] >= config.height as f64;
        if !outside {
            continue;
        }
        errors.push_warning(
            "scene.json",
            &join(&obj.path, "position"),
            format!(
                "объект{} стоит за пределами сцены: прямоугольник [{}, {}]..[{}, {}] не пересекается со сценой [0, 0]..[{}, {}]; ожидалось, что объект будет хотя бы частично на сцене",
                name_suffix(obj),
                pos[0],
                pos[1],
                pos[0] + size[0],
                pos[1] + size[1],
                config.width,
                config.height
            ),
        );
    }
}

fn describe_selector(selector: &Selector, properties: &PropertyTable) -> String {
    let has: Vec<&str> = selector.has.iter().map(|&p| properties.name(p)).collect();
    let without: Vec<&str> = selector
        .without
        .iter()
        .map(|&p| properties.name(p))
        .collect();
    format!("has=[{}], without=[{}]", has.join(", "), without.join(", "))
}

/// «Формат игры»: предупреждение (игра идёт), если отбор правила заведомо не подходит ни одному
/// объекту, какой вообще может существовать в игре — ни объекту `scene.json`, ни шаблону
/// «создать», ни объекту, который мог бы получить или потерять свойство во время игры (`give`,
/// `take`, `set`, `keys`; см. `widen_shapes_by_collide_effects`, `keys_edited_properties`).
/// Проверяются все отборы, которые документ называет отбором правила — `for` у «подвинуть» и
/// «удалить», `a`/`b` у «столкнуть», и у «создать» и «удалить» оба отбора, какими может быть их
/// условие: `of` внутри `fewer_than` и `of` внутри `after_move_of`. Документ не делает исключения
/// по виду отбора: пустой `fewer_than.of` делает условие «меньше N» всегда истинным и «создать»
/// сыплет объекты до `max_objects`, ровно то же по сути, что и пустой `for`. Пустой отбор (`has`
/// и `without` оба пусты) подходит всем и предупреждения не даёт.
fn validate_selectors_not_empty(
    shapes: &[(Shape, String)],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) {
    let mut check = |selector: &Selector, label: &str, field: &str| {
        if selector.has.is_empty() && selector.without.is_empty() {
            return;
        }
        if shapes
            .iter()
            .any(|(shape, _)| matches_shape(selector, shape))
        {
            return;
        }
        errors.push_warning(
            "rules.json",
            &join(label, field),
            format!(
                "отбор {} заведомо не подходит ни одному объекту, какой может существовать в игре; ожидалось, что отбору будет соответствовать хотя бы один объект",
                describe_selector(selector, properties)
            ),
        );
    };

    for (rule, meta) in rules.rules.iter().zip(rule_meta) {
        let label = format!("rules[{}]", meta.file_index);
        match rule {
            Rule::Move { for_ } => check(for_, &label, "for"),
            Rule::Collide { a, b, .. } => {
                check(a, &label, "a");
                check(b, &label, "b");
            }
            Rule::Delete { for_, when, .. } => {
                check(for_, &label, "for");
                match when {
                    Condition::AfterMoveOf { of } => check(of, &label, "when → after_move_of"),
                    Condition::FewerThan { of, .. } => check(of, &label, "when → fewer_than → of"),
                    Condition::Compare { .. } | Condition::OutsideScene => {}
                }
            }
            Rule::Spawn { when, .. } => match when {
                SpawnCondition::AfterMoveOf { of } => check(of, &label, "when → after_move_of"),
                SpawnCondition::FewerThan { of, .. } => check(of, &label, "when → fewer_than → of"),
            },
        }
    }
}

/// Second load call: needs the `GameConfig` from `read_entry` and the three files it named.
/// `None` for any of them means the page could not fetch it. A warning is only computed once the
/// three files parsed without error: an object or rule that failed to parse simply drops out of
/// `scene_objects`/`rules`, and a warning computed against that shrunken, incomplete set can be
/// outright wrong about the data the author will have once the real errors are fixed — not just
/// unhelpful. And a warning is «the game still runs» information (see «Формат игры»), which
/// doesn't apply here anyway: with errors present the game never starts.
pub fn load_rest(
    config: GameConfig,
    properties_json: Option<&str>,
    scene_json: Option<&str>,
    rules_json: Option<&str>,
) -> Result<(Game, Vec<GameError>), LoadFailure> {
    let mut errors = ErrorSink::new();

    let properties = match properties_json {
        Some(text) => parse_properties_json(text, &mut errors),
        None => {
            errors.push(&config.files.properties, "", "файл не найден");
            PropertyTable::new()
        }
    };

    let scene_objects = match scene_json {
        Some(text) => parse_scene_json(text, &properties, &mut errors),
        None => {
            errors.push(&config.files.scene, "", "файл не найден");
            Vec::new()
        }
    };

    let (rules, rule_meta) = match rules_json {
        Some(text) => parse_rules_json(text, &properties, &mut errors),
        None => {
            errors.push(&config.files.rules, "", "файл не найден");
            (RuleSet::default(), Vec::new())
        }
    };

    let shapes = possible_shapes(&scene_objects, &rules, &rule_meta, &properties);
    validate_property_sufficiency(&shapes, &rules, &rule_meta, &properties, &mut errors);
    // «Формат игры»: предупреждение не мешает игре запуститься — но раз игра уже не запустится
    // из-за ошибок собранных выше, считать эти три предупреждения незачем: правило, не
    // разобравшееся из-за ошибки, просто выпадает из `rules`, и предупреждение по неполному
    // списку («отбор никого не находит» и т. п.) описывает данные, которых не будет, когда автор
    // ошибку исправит, — то есть может быть попросту неверным, а не просто лишним.
    // `validate_property_sufficiency` — не под этим правилом: она смотрит на
    // `Shape::broken_properties` отдельно для каждого свойства каждого объекта
    // (`ParsedObject::broken_properties`), а не гасит себя целиком по объекту или по файлу, так что
    // настоящая нехватка свойства у исправного объекта не теряется из-за того, что у того же или
    // соседнего объекта сломано что-то ещё.
    if errors.has_no_errors() {
        validate_unused_properties(&properties, &scene_objects, &rules, &mut errors);
        validate_objects_within_scene(&scene_objects, &config.scene, &mut errors);
        validate_selectors_not_empty(&shapes, &rules, &rule_meta, &properties, &mut errors);
    }

    // One second pass per file, after every message about it has been pushed: a message from
    // `validate_*` above (e.g. "rules.json" ← property sufficiency) needs the same file text a
    // parse-time message did, so filling locations any earlier would miss it.
    if let Some(text) = properties_json {
        errors.fill_locations("properties.json", text);
    }
    if let Some(text) = scene_json {
        errors.fill_locations("scene.json", text);
    }
    if let Some(text) = rules_json {
        errors.fill_locations("rules.json", text);
    }

    let (errs, warnings) = errors.into_parts();
    if !errs.is_empty() {
        return Err(LoadFailure {
            errors: errs,
            warnings,
        });
    }

    let mut world = World::new(&properties);
    for parsed in &scene_objects {
        let id = world.create();
        for (prop, value) in &parsed.values {
            world.set_value(id, *prop, value);
        }
        if let Some(spec) = &parsed.grid {
            world.set_grid(id, property::GRID, *spec);
            world.set_grid_counter(id, spec.interval_steps);
        }
        if let Some(table) = &parsed.keys {
            world.set_keys(id, property::KEYS, table.clone());
        }
    }

    let game = Game::new(
        properties,
        world,
        rules,
        config.scene,
        config.max_objects,
        config.random_seed,
    );
    Ok((game, warnings))
}

/// Convenience for tests and native tools: loads all four files at once, doing the same
/// validation `read_entry` + `load_rest` would, in one call. Warnings from both stages are
/// merged, `read_entry`'s first, rather than one stage's warnings silently winning.
pub fn load_game_from_texts(
    game_json: &str,
    properties_json: &str,
    scene_json: &str,
    rules_json: &str,
) -> Result<(Game, Vec<GameError>), LoadFailure> {
    let (config, entry_warnings) = read_entry(game_json)?;
    match load_rest(
        config,
        Some(properties_json),
        Some(scene_json),
        Some(rules_json),
    ) {
        Ok((game, warnings)) => {
            let mut all_warnings = entry_warnings;
            all_warnings.extend(warnings);
            Ok((game, all_warnings))
        }
        Err(mut failure) => {
            let mut all_warnings = entry_warnings;
            all_warnings.extend(std::mem::take(&mut failure.warnings));
            failure.warnings = all_warnings;
            Err(failure)
        }
    }
}

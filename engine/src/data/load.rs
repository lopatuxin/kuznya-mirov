use serde_json::Value as Json;

use crate::core::game::Game;
use crate::core::keys::{KeyBinding, KeyEdit, KeyTable};
use crate::core::property::{self, PropertyId, PropertyTable};
use crate::core::rules::{
    CollideEffect, CommonAction, CompareOp, Condition, Outcome, Rule, RuleSet, Selector, SoundId,
    SpawnCondition, SpawnPlace, TemplateValue,
};
use crate::core::scene::{ObjectSpec, SceneConfig};
use crate::core::screens::{
    Align, Anchor, ButtonCommand, Element, Fill, FontId, MusicId, Placement, Screen, ScreenId,
    ScreenKeyTable, ScreensConfig, TextPart,
};
use crate::core::time::{seconds_to_steps, seconds_to_steps_delta};
use crate::core::value::{GridSpec, ImageId, PropKind, Value, Vec2};
use crate::core::world::World;

use super::error::ErrorSink;
use super::wav;

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
    let unknown_keys: Vec<&String> = obj
        .keys()
        .filter(|key| !known.contains(&key.as_str()))
        .collect();
    if unknown_keys.is_empty() {
        return;
    }
    let allowed = known
        .iter()
        .map(|k| format!("\"{k}\""))
        .collect::<Vec<_>>()
        .join(", ");
    for key in unknown_keys {
        errors.push(
            file,
            &join(path, key),
            format!("неизвестное поле \"{key}\"; ожидалось одно из: {allowed}"),
        );
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
    images: &[ImageDecl],
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
        PropKind::Number => {
            let n = expect_number(value, file, path, errors)?;
            if prop == property::OPACITY && !validate_opacity_range(n, file, path, errors) {
                return None;
            }
            Some(Value::Number(n))
        }
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
        PropKind::Image => {
            let name = expect_string(value, file, path, errors)?;
            resolve_image(&name, images, file, path, errors).map(Value::Image)
        }
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
    images: &[ImageDecl],
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
            images,
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
    images: &[ImageDecl],
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
            Some(v) => parse_key_edits(
                v,
                properties,
                images,
                file,
                &join(&binding_path, "press"),
                errors,
            ),
            None => Vec::new(),
        };
        let release = match binding_obj.get("release") {
            Some(v) => parse_key_edits(
                v,
                properties,
                images,
                file,
                &join(&binding_path, "release"),
                errors,
            ),
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

/// «Звук»: "звука \"eet\" нет" отправляет гадать, а с перечнем объявленных имён
/// чинится сразу — эта строка добавляется к сообщению об отсутствующей ссылке по имени.
fn format_declared_names(table: &[(String, String)]) -> String {
    table
        .iter()
        .map(|(name, _)| format!("\"{name}\""))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `play_sound`'s name lookup — «Звук»: same table for both scopes is legal, so a
/// name missing from `sounds` but present in `music` gets the "wrong table" message instead of
/// "unknown name", and an unknown name lists every declared sound to save the outside model a
/// round trip.
fn resolve_sound(
    name: &str,
    sounds: &[(String, String)],
    music: &[(String, String)],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<SoundId> {
    if let Some(id) = sounds.iter().position(|(n, _)| n == name) {
        return Some(id);
    }
    if music.iter().any(|(n, _)| n == name) {
        errors.push(
            file,
            path,
            format!(
                "\"{name}\" объявлен в files.music; правило умеет звать только звуки из files.sounds"
            ),
        );
        return None;
    }
    let declared = format_declared_names(sounds);
    let message = if declared.is_empty() {
        format!("звука \"{name}\" нет; звуки не объявлены")
    } else {
        format!("звука \"{name}\" нет; объявлены {declared}")
    };
    errors.push(file, path, message);
    None
}

/// A screen's `music` name lookup — the mirror of `resolve_sound`.
fn resolve_music(
    name: &str,
    music: &[(String, String)],
    sounds: &[(String, String)],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<MusicId> {
    if let Some(id) = music.iter().position(|(n, _)| n == name) {
        return Some(id);
    }
    if sounds.iter().any(|(n, _)| n == name) {
        errors.push(
            file,
            path,
            format!(
                "\"{name}\" объявлен в files.sounds; экран умеет называть только музыку из files.music"
            ),
        );
        return None;
    }
    let declared = format_declared_names(music);
    let message = if declared.is_empty() {
        format!("трека \"{name}\" нет; треки не объявлены")
    } else {
        format!("трека \"{name}\" нет; объявлены {declared}")
    };
    errors.push(file, path, message);
    None
}

/// An `image` field's name lookup — «Картинки»: resolves against `files.images`, listing every
/// declared name when it doesn't, the same way `resolve_sound`/`resolve_music` do for their own
/// tables.
fn resolve_image(
    name: &str,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ImageId> {
    if let Some(id) = images.iter().position(|decl| decl.name == name) {
        return Some(id);
    }
    let declared = images
        .iter()
        .map(|decl| format!("\"{}\"", decl.name))
        .collect::<Vec<_>>()
        .join(", ");
    let message = if declared.is_empty() {
        format!("картинки \"{name}\" нет; картинки не объявлены")
    } else {
        format!("картинки \"{name}\" нет; объявлены {declared}")
    };
    errors.push(file, path, message);
    None
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

/// «Картинки»: `opacity` границы и текст ошибки написаны один раз здесь — как для числового
/// значения свойства объекта (`parse_scalar_value`), так и для поля `opacity` панели/кнопки
/// (`parse_opacity`).
fn validate_opacity_range(n: f64, file: &str, path: &str, errors: &mut ErrorSink) -> bool {
    if (0.0..=1.0).contains(&n) {
        true
    } else {
        errors.push(
            file,
            path,
            format!("opacity должен быть от 0 до 1 включительно, получено {n}"),
        );
        false
    }
}

/// «Картинки»: `opacity` без `image` — ошибка данных везде, где может появиться `opacity`:
/// объект, шаблон создания, панель и кнопка. Текст написан один раз здесь и его же берёт
/// `validate_opacity_reachable_image`, проверяющая ту же ошибку по расширенной форме объекта.
const OPACITY_WITHOUT_IMAGE: &str =
    "opacity задан без image: множить не на что, у цвета для этого есть #rrggbbaa";
fn validate_opacity_has_image(
    has_image: bool,
    has_opacity: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if has_opacity && !has_image {
        errors.push(file, path, OPACITY_WITHOUT_IMAGE);
    }
}

/// «Картинки»: `color`+`image` together is a data error, checked once here for every place a
/// fixed set of properties gets built at once — a scene object and a spawn template alike.
/// `opacity` without `image` is checked separately, after `possible_shapes` widens the shape by
/// `keys` and collide effects — see `validate_opacity_reachable_image`: unlike `color`, which a
/// scene object's shape never gains at runtime, `image` can arrive through a `keys` edit or a
/// `set`/`give` collide effect, so judging it here, from the object's file shape alone, would
/// reject a game that only ever adds `image` once the object is already moving.
fn validate_image_fill(
    shape: &std::collections::HashSet<PropertyId>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if shape.contains(&property::COLOR) && shape.contains(&property::IMAGE) {
        errors.push(
            file,
            path,
            "заданы и color, и image; должно быть ровно одно из двух",
        );
    }
}

fn parse_scene_object(
    value: &Json,
    index: usize,
    file: &str,
    properties: &PropertyTable,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> Option<ParsedObject> {
    let path = format!("objects[{index}]");
    let obj = expect_object(value, file, &path, errors)?;

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
        let Some(prop) = resolve_property(key, properties, file, &field_path, errors) else {
            continue;
        };
        match properties.kind(prop) {
            PropKind::Grid => {
                if let Some(spec) = parse_grid(field_value, file, &field_path, errors) {
                    grid = Some(spec);
                    shape.insert(prop);
                } else {
                    broken_properties.insert(prop);
                }
            }
            PropKind::Keys => {
                if let Some(table) =
                    parse_keys(field_value, properties, images, file, &field_path, errors)
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
                    images,
                    file,
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

    validate_image_fill(&shape, file, &path, errors);

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
    file: &str,
    properties: &PropertyTable,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> Vec<ParsedObject> {
    let Some(root) = parse_json_or_error(file, text, errors) else {
        return Vec::new();
    };
    let Some(obj) = expect_object(&root, file, "", errors) else {
        return Vec::new();
    };
    reject_unknown_keys(obj, &["objects"], file, "", errors);
    let Some(objects_json) = obj.get("objects") else {
        // Same convention `require_field` uses: the path names the parent object (here the
        // root, `""`), not the missing key itself — a path pointing at a key that by definition
        // isn't in the text can never resolve to a location.
        errors.push(file, "", "отсутствует список объектов");
        return Vec::new();
    };
    let Some(arr) = expect_array(objects_json, file, "objects", errors) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, v)| parse_scene_object(v, i, file, properties, images, errors))
        .collect()
}

fn parse_properties_json(text: &str, file: &str, errors: &mut ErrorSink) -> PropertyTable {
    let mut table = PropertyTable::new();
    let Some(root) = parse_json_or_error(file, text, errors) else {
        return table;
    };
    let Some(obj) = expect_object(&root, file, "", errors) else {
        return table;
    };
    reject_unknown_keys(obj, &["properties"], file, "", errors);
    let Some(props_json) = obj.get("properties") else {
        // Same convention `require_field` uses: the path names the parent (here the root, `""`),
        // not the missing key itself — a path pointing at a key that by definition isn't in the
        // text can never resolve to a location.
        errors.push(file, "", "отсутствует объект properties");
        return table;
    };
    let Some(props_obj) = expect_object(props_json, file, "properties", errors) else {
        return table;
    };
    for (name, kind_json) in props_obj {
        let path = join("properties", name);
        let Some(kind_str) = expect_string(kind_json, file, &path, errors) else {
            continue;
        };
        let kind = match kind_str.as_str() {
            "flag" => PropKind::Flag,
            "number" => PropKind::Number,
            "time" => PropKind::Time,
            other => {
                errors.push(
                    file,
                    &path,
                    format!("неизвестный вид свойства \"{other}\": ожидался flag, number или time"),
                );
                continue;
            }
        };
        if let Err(existing) = table.declare_author(name, kind) {
            let _ = existing;
            errors.push(file, &path, format!("свойство \"{name}\" уже объявлено"));
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

#[derive(Debug, Clone)]
pub struct FilePaths {
    pub properties: String,
    pub scene: String,
    pub rules: String,
    pub screens: String,
    /// `(имя, путь)`, в порядке `files.fonts` из `game.json` — этот порядок и определяет
    /// `FontId`, на который ссылаются элементы `screens.json` (см. `resolve_font`).
    pub fonts: Vec<(String, String)>,
    /// `(имя, путь)`, в порядке `files.sounds` — этот порядок и определяет `SoundId`, на который
    /// ссылается `play_sound` (см. `resolve_sound`). Пусто, если `files.sounds` не объявлена.
    pub sounds: Vec<(String, String)>,
    /// То же для `files.music` и `MusicId`, на который ссылается поле экрана `music`
    /// (см. `resolve_music`). Пусто, если `files.music` не объявлена.
    pub music: Vec<(String, String)>,
    /// `files.images`, в порядке объявления — этот порядок определяет `ImageId`, на который
    /// ссылается свойство `image` объекта, поле `image` шаблона создания и поля `image`/
    /// `image_hover`/`image_pressed` панели и кнопки (см. `resolve_image`). Пусто, если
    /// `files.images` не объявлена — «Картинки»: нет таблицы, нет картинок, это не ошибка.
    pub images: Vec<ImageDecl>,
    /// `files.code` — «Код игры»: необязательный путь к файлу кода на Lua. `None`, когда ключ не
    /// объявлен — в игре нет кода, она грузится и идёт ровно как раньше.
    pub code: Option<String>,
}

/// One `files.images` entry — «Картинки» → «Таблица картинок»: `frames`/`frame_time` default to
/// a single, motionless frame when the game names neither. `frame_time` is already converted to
/// steps here, at load time, the same way every other duration in the game's files is — nothing
/// downstream ever sees seconds again.
#[derive(Debug, Clone)]
pub struct ImageDecl {
    pub name: String,
    pub path: String,
    pub frames: u32,
    pub frame_steps: i64,
}

/// Ответ исполнителя (браузера) по одному треку из `files.music` — «Звук» → «Загрузка и проверка»: движок не разжимает MP3 сам, а получает уже готовый вердикт.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MusicVerdict {
    Ok,
    Missing,
    Rejected,
}

/// Ответ страницы по одной картинке из `files.images` — «Картинки» → «Загрузка и проверка»:
/// движок не разжимает PNG сам, страница уже разжала его и отдаёт размер и точки. `Ok`'s `pixels`
/// is RGBA, four bytes per pixel, straight from `getImageData` — not premultiplied by alpha.
#[derive(Debug, Clone)]
pub enum ImageVerdict {
    Ok {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
    Missing,
    Rejected,
}

/// Список двоичных файлов, которые стоит прочитать в третьем заходе загрузки — «Звук»
/// → «Загрузка: сначала лёгкое, потом тяжёлое»: шрифты, звуки и картинки читаются все, музыка —
/// только та, которую называет хоть один экран.
#[derive(Debug, Clone, Default)]
pub struct NeededMedia {
    pub fonts: Vec<(String, String)>,
    pub sounds: Vec<(String, String)>,
    pub music: Vec<(String, String)>,
    pub images: Vec<(String, String)>,
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub scene: SceneConfig,
    pub random_seed: u64,
    pub max_objects: usize,
    pub files: FilePaths,
    pub start_screen: String,
    pub win_screen: Option<String>,
    pub loss_screen: Option<String>,
}

/// The handshake between `read_entry` and `load_rest` that `wasm::Engine` drives: `None` before
/// any successful `read_entry`, or once `load_rest` has consumed one; `Some` in between. Carries
/// `game.json`'s own text alongside the config `read_entry` parsed from it, since `load_rest`
/// needs the text back too — to locate its own errors about fields declared in `game.json` (see
/// `load_rest`'s doc comment). Kept here, next to the two calls whose contract it enforces, rather
/// than in the wasm-bindgen layer, so the handshake itself needs no browser types and can be
/// tested without one.
#[derive(Default)]
pub struct PendingConfig(Option<(GameConfig, String)>);

impl PendingConfig {
    /// Records what `result` means for the pending config: success replaces it (keeping `text`
    /// alongside it), failure clears it — otherwise a failed `read_entry` would leave `load_rest`
    /// silently using the config an earlier successful call had left behind.
    pub fn set(&mut self, result: &Result<(GameConfig, Vec<GameError>), LoadFailure>, text: &str) {
        self.0 = result
            .as_ref()
            .ok()
            .map(|(config, _)| (config.clone(), text.to_string()));
    }

    /// Consumes the pending config and `game.json` text, if any — mirrors `load_rest`'s one-shot
    /// use of them.
    pub fn take(&mut self) -> Option<(GameConfig, String)> {
        self.0.take()
    }

    /// Looks at the pending config without consuming it — for `read_texts`, the second of the
    /// three load calls, which sits between `read_entry` (fills this) and `load_rest` (empties
    /// it via `take`).
    pub fn peek(&self) -> Option<&GameConfig> {
        self.0.as_ref().map(|(config, _)| config)
    }
}

/// Parses just enough of `game.json` to tell the caller which files to fetch next. This is the
/// first of the three load calls the page makes — see `read_texts` for the second and `load_rest`
/// for the third. Same contract as `load_rest`: warnings travel alongside the config on success
/// and alongside the errors on failure, rather than only on one of the two paths.
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

/// Second load call — «Звук» → «Загрузка и проверка»: a best-effort
/// parse of the four text files `read_entry` named, just enough to say which binary files are
/// worth fetching next. Tolerant of broken text on purpose: a `screens.json` that fails to parse
/// simply names no referenced track here, same as `parse_screens_json` itself falls back to no
/// screens — the real errors surface later, when `load_rest` parses everything again for real and
/// collects them all in one pass. `scene_json`/`rules_json` play no part in the answer (only a
/// screen's own `music` field decides which tracks are read — «Звук» → «Загрузка и проверка») and are accordingly not asked for here.
pub fn read_texts(
    config: &GameConfig,
    properties_json: Option<&str>,
    screens_json: Option<&str>,
) -> NeededMedia {
    let mut scratch = ErrorSink::new();
    let properties = match properties_json {
        Some(text) => parse_properties_json(text, &config.files.properties, &mut scratch),
        None => PropertyTable::new(),
    };
    let parsed_screens = match screens_json {
        Some(text) => parse_screens_json(text, &config.files.screens, &properties, &mut scratch),
        None => Vec::new(),
    };
    let referenced: std::collections::HashSet<&str> = parsed_screens
        .iter()
        .filter_map(|s| s.music.as_deref())
        .collect();
    let music = config
        .files
        .music
        .iter()
        .filter(|(name, _)| referenced.contains(name.as_str()))
        .cloned()
        .collect();
    NeededMedia {
        fonts: config.files.fonts.clone(),
        sounds: config.files.sounds.clone(),
        music,
        // «Картинки» → «Загрузка и проверка»: все объявленные, whether or not anything
        // names them yet — same treatment `fonts`/`sounds` get, unlike `music`'s filter above.
        images: config
            .files
            .images
            .iter()
            .map(|decl| (decl.name.clone(), decl.path.clone()))
            .collect(),
    }
}

fn parse_game_json(text: &str, errors: &mut ErrorSink) -> Option<GameConfig> {
    let root = parse_json_or_error("game.json", text, errors)?;
    let obj = expect_object(&root, "game.json", "", errors)?;
    reject_unknown_keys(
        obj,
        &[
            "name",
            "scene",
            "random_seed",
            "max_objects",
            "files",
            "start_screen",
            "win_screen",
            "loss_screen",
        ],
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
            &[
                "properties",
                "scene",
                "rules",
                "images",
                "screens",
                "fonts",
                "sounds",
                "music",
                "code",
            ],
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
        let screens = require_field(f, "screens", "game.json", "files", errors)
            .and_then(|v| expect_string(v, "game.json", "files → screens", errors));
        let fonts_json = require_field(f, "fonts", "game.json", "files", errors);
        let fonts =
            fonts_json.and_then(|v| parse_font_table(v, "game.json", "files → fonts", errors));
        // «Звук»: обе таблицы необязательны — нет ключа, нет и звуков/музыки, не ошибка.
        let sounds = match f.get("sounds") {
            Some(v) => {
                parse_media_table(v, MediaKind::Sound, "game.json", "files → sounds", errors)
                    .unwrap_or_default()
            }
            None => Vec::new(),
        };
        let music = match f.get("music") {
            Some(v) => parse_media_table(v, MediaKind::Music, "game.json", "files → music", errors)
                .unwrap_or_default(),
            None => Vec::new(),
        };
        // «Картинки»: необязательна — нет ключа, нет и картинок, не ошибка.
        let images = match f.get("images") {
            Some(v) => {
                parse_images_table(v, "game.json", "files → images", errors).unwrap_or_default()
            }
            None => Vec::new(),
        };
        // «Код игры»: необязателен — нет ключа, нет и кода, игра идёт ровно как раньше.
        let code = f
            .get("code")
            .and_then(|v| expect_string(v, "game.json", "files → code", errors));
        (
            properties, scene_path, rules, screens, fonts, sounds, music, images, code,
        )
    });

    let start_screen = require_field(obj, "start_screen", "game.json", "", errors)
        .and_then(|v| expect_string(v, "game.json", "start_screen", errors));
    let win_screen = obj
        .get("win_screen")
        .and_then(|v| expect_string(v, "game.json", "win_screen", errors));
    let loss_screen = obj
        .get("loss_screen")
        .and_then(|v| expect_string(v, "game.json", "loss_screen", errors));

    let scene = scene?;
    let max_objects = max_objects?;
    let random_seed = random_seed?;
    let (properties, scene_path, rules, screens, fonts, sounds, music, images, code) = files?;
    let (properties, scene_path, rules, screens, fonts) =
        (properties?, scene_path?, rules?, screens?, fonts?);
    let start_screen = start_screen?;

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
            screens,
            fonts,
            sounds,
            music,
            images,
            code,
        },
        start_screen,
        win_screen,
        loss_screen,
    })
}

/// `files.fonts`: an object mapping a font's name (used by `screens.json`'s `font` field) to its
/// path inside the game's folder. Order is preserved — it becomes the font's `FontId`.
fn parse_font_table(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<(String, String)>> {
    let obj = expect_object(value, file, path, errors)?;
    let mut fonts = Vec::with_capacity(obj.len());
    for (name, path_json) in obj {
        if let Some(font_path) = expect_string(path_json, file, &join(path, name), errors) {
            fonts.push((name.clone(), font_path));
        }
    }
    Some(fonts)
}

/// The two ways `files.sounds`/`files.music` differ — everything else about the two tables is
/// identical, so `parse_media_table` takes one of these instead of two near-duplicate functions.
#[derive(Clone, Copy)]
enum MediaKind {
    Sound,
    Music,
}

impl MediaKind {
    fn table_name(self) -> &'static str {
        match self {
            MediaKind::Sound => "files.sounds",
            MediaKind::Music => "files.music",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            MediaKind::Sound => ".wav",
            MediaKind::Music => ".mp3",
        }
    }

    fn other_extension(self) -> &'static str {
        match self {
            MediaKind::Sound => ".mp3",
            MediaKind::Music => ".wav",
        }
    }

    fn format_word(self) -> &'static str {
        match self {
            MediaKind::Sound => "WAV",
            MediaKind::Music => "MP3",
        }
    }

    /// «Звук» → «Загрузка и проверка»: у трека имя нужно экрану, у звука
    /// — правилу.
    fn empty_name_hint(self) -> &'static str {
        match self {
            MediaKind::Sound => "у звука должно быть имя, которым его назовёт правило",
            MediaKind::Music => "у трека должно быть имя, которым его назовёт экран",
        }
    }

    /// The generic wrong-extension hint, when the path isn't the other table's extension either.
    fn wrong_extension_hint(self) -> &'static str {
        match self {
            MediaKind::Sound => "если это музыка, ей место в files.music",
            MediaKind::Music => "если это звук, ему место в files.sounds",
        }
    }

    /// The sharper hint for the most common mistake — the path is actually the other table's own
    /// extension.
    fn wrong_extension_is_other_hint(self) -> &'static str {
        match self {
            MediaKind::Sound => "это MP3, ему место в files.music",
            MediaKind::Music => "это WAV, ему место в files.sounds",
        }
    }
}

/// `files.sounds`/`files.music`: an object mapping a name (used by `play_sound`/the screen
/// `music` field) to its path inside the game's folder — «Звук» → «Загрузка и проверка». Order is preserved — it becomes `SoundId`/`MusicId`. A malformed entry is
/// skipped (its own error already pushed), same as `parse_font_table`; `None` only when the whole
/// value isn't a table at all.
fn parse_media_table(
    value: &Json,
    kind: MediaKind,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<(String, String)>> {
    let Some(obj) = value.as_object() else {
        errors.push(
            file,
            path,
            format!(
                "{} — ожидалась таблица \"имя → файл\", как files.fonts",
                kind.table_name()
            ),
        );
        return None;
    };
    let mut out = Vec::with_capacity(obj.len());
    for (name, path_json) in obj {
        if name.is_empty() {
            errors.push(
                file,
                path,
                format!(
                    "{} → пустое имя: {}",
                    kind.table_name(),
                    kind.empty_name_hint()
                ),
            );
            continue;
        }
        let entry_path = join(path, name);
        let Some(file_path) = path_json.as_str() else {
            errors.push(
                file,
                &entry_path,
                format!(
                    "{} → {name}: ожидался путь к файлу строкой",
                    kind.table_name()
                ),
            );
            continue;
        };
        let lower = file_path.to_ascii_lowercase();
        if !lower.ends_with(kind.extension()) {
            let hint = if lower.ends_with(kind.other_extension()) {
                kind.wrong_extension_is_other_hint()
            } else {
                kind.wrong_extension_hint()
            };
            errors.push(
                file,
                &entry_path,
                format!(
                    "{file_path} — в {} берётся {}; {hint}",
                    kind.table_name(),
                    kind.format_word()
                ),
            );
            continue;
        }
        out.push((name.clone(), file_path.to_string()));
    }
    Some(out)
}

/// `files.images`: an object mapping an image's name (used by an object's `image` property, a
/// spawn template's `image` field, and a panel/button's `image` fields) to its description —
/// «Картинки» → «Таблица картинок». Order is preserved — it becomes the image's `ImageId`. A
/// malformed entry is skipped (its own error already pushed), same as `parse_font_table`; `None`
/// only when the whole value isn't a table at all. `frame_time` is converted to steps right here,
/// the same way `parse_grid`/`parse_scalar_value` convert every other duration at load time.
fn parse_images_table(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<ImageDecl>> {
    let Some(obj) = value.as_object() else {
        errors.push(
            file,
            path,
            "files.images — ожидалась таблица \"имя → описание\", как files.fonts",
        );
        return None;
    };
    let mut out = Vec::with_capacity(obj.len());
    for (name, decl_json) in obj {
        if name.is_empty() {
            errors.push(
                file,
                path,
                "files.images → пустое имя: у картинки должно быть имя, которым её назовёт объект или элемент",
            );
            continue;
        }
        let entry_path = join(path, name);
        let Some(decl_obj) = expect_object(decl_json, file, &entry_path, errors) else {
            continue;
        };
        reject_unknown_keys(
            decl_obj,
            &["path", "frames", "frame_time"],
            file,
            &entry_path,
            errors,
        );
        let Some(image_path) = require_field(decl_obj, "path", file, &entry_path, errors)
            .and_then(|v| expect_string(v, file, &join(&entry_path, "path"), errors))
        else {
            continue;
        };
        if !image_path.to_ascii_lowercase().ends_with(".png") {
            errors.push(
                file,
                &join(&entry_path, "path"),
                format!(
                    "{image_path} — files.images берёт только PNG; путь должен оканчиваться на \".png\""
                ),
            );
            continue;
        }
        let (frames, frame_steps) = match (decl_obj.get("frames"), decl_obj.get("frame_time")) {
            (None, None) => (1, 1),
            (Some(_), None) => {
                errors.push(
                    file,
                    &entry_path,
                    "frames задан без frame_time: оба поля нужны вместе, иначе не нужно ни одного",
                );
                continue;
            }
            (None, Some(_)) => {
                errors.push(
                    file,
                    &entry_path,
                    "frame_time задан без frames: оба поля нужны вместе, иначе не нужно ни одного",
                );
                continue;
            }
            (Some(frames_json), Some(frame_time_json)) => {
                let frames = expect_number(frames_json, file, &join(&entry_path, "frames"), errors)
                    .filter(|n| {
                        if *n < 1.0 || n.fract() != 0.0 || *n > f64::from(u32::MAX) {
                            errors.push(
                                file,
                                &join(&entry_path, "frames"),
                                format!(
                                    "frames должен быть целым числом от 1 до {}, получено {n}",
                                    u32::MAX
                                ),
                            );
                            false
                        } else {
                            true
                        }
                    })
                    .map(|n| n as u32);
                let frame_time = expect_number(
                    frame_time_json,
                    file,
                    &join(&entry_path, "frame_time"),
                    errors,
                )
                .filter(|n| {
                    if *n <= 0.0 {
                        errors.push(
                            file,
                            &join(&entry_path, "frame_time"),
                            format!("frame_time должен быть больше нуля, получено {n}"),
                        );
                        false
                    } else {
                        true
                    }
                });
                match (frames, frame_time) {
                    (Some(frames), Some(frame_time)) => (frames, seconds_to_steps(frame_time)),
                    _ => continue,
                }
            }
        };
        out.push(ImageDecl {
            name: name.clone(),
            path: image_path,
            frames,
            frame_steps,
        });
    }
    Some(out)
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
        // «Формат игры»: время в файлах — секунды, а `World::number_like` отдаёт для `Time`
        // шаги (world.rs), так что порог сравнения переводится здесь же, при разборе — это
        // порог, а не длительность, так что `seconds_to_steps`'s минимум в один шаг тут не к
        // месту: сравнение с нулём должно остаться сравнением с нулём.
        let value = if properties.kind(prop) == PropKind::Time {
            seconds_to_steps_delta(num) as f64
        } else {
            num
        };
        return Some(Condition::Compare { prop, op, value });
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
    sounds: &[(String, String)],
    music: &[(String, String)],
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
                sounds,
                music,
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
    sounds: &[(String, String)],
    music: &[(String, String)],
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
        "play_sound" => {
            // «Звук»: все три формы ошибки — без имени, с двумя, число вместо
            // строки — делят один и тот же текст, а не отдельное сообщение на каждую.
            let bad_form =
                "play_sound берёт ровно одну настройку — имя звука из files.sounds строкой";
            if arr.len() != 2 {
                errors.push(file, path, bad_form.to_string());
                return None;
            }
            let Some(name) = arr[1].as_str() else {
                errors.push(file, path, bad_form.to_string());
                return None;
            };
            let id = resolve_sound(name, sounds, music, file, &join(path, "[1]"), errors)?;
            Some(CommonAction::PlaySound(id))
        }
        "run" => parse_run_action_name(arr, file, path, errors).map(CommonAction::Run),
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

/// `["run", "<функция>"]` — shared by `do` (`CommonAction::Run`) and `effects` (`CollideEffect::
/// Run`): ровно один аргумент-строка, имя функции — «Код игры». Существование самой функции и
/// того, что у игры вообще есть `files.code`, здесь не проверяется: это знает только код после
/// того, как он скомпилирован — см. `validate_run_actions` в `load_rest`.
fn parse_run_action_name(
    arr: &[Json],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<String> {
    let bad_form = "run берёт ровно одну настройку — имя функции кода строкой";
    if arr.len() != 2 {
        errors.push(file, path, bad_form.to_string());
        return None;
    }
    let Some(name) = arr[1].as_str() else {
        errors.push(file, path, bad_form.to_string());
        return None;
    };
    Some(name.to_string())
}

fn parse_collide_effects(
    value: Option<&Json>,
    properties: &PropertyTable,
    images: &[ImageDecl],
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
                images,
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
    images: &[ImageDecl],
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
                images,
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
        "play_sound" => {
            // «Звук»: `effects` адресован стороне столкновения, а у звука стороны
            // нет — его место в `do`, regardless of what follows in this list.
            errors.push(file, path, "это общее действие, его место в do".to_string());
            None
        }
        "run" => parse_run_action_name(arr, file, path, errors).map(CollideEffect::Run),
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
    images: &[ImageDecl],
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
        if let Some(v) = parse_scalar_value(
            field_value,
            prop,
            properties,
            images,
            file,
            &field_path,
            errors,
        ) {
            template.push((prop, TemplateValue::Const(v)));
        } else {
            broken_properties.insert(prop);
        }
    }
    let shape: std::collections::HashSet<PropertyId> = template.iter().map(|(p, _)| *p).collect();
    validate_image_fill(&shape, file, path, errors);
    (template, broken_properties)
}

/// Known keys across every rule kind — used to still catch a typo in a rule whose own `kind` is
/// missing or unrecognized, rather than let it hide behind the `kind` error until a later pass.
const ALL_RULE_KEYS: &[&str] = &[
    "kind", "for", "a", "b", "effects", "do", "when", "where", "template",
];

#[allow(clippy::too_many_arguments)]
fn parse_rule(
    value: &Json,
    index: usize,
    file: &str,
    properties: &PropertyTable,
    images: &[ImageDecl],
    sounds: &[(String, String)],
    music: &[(String, String)],
    errors: &mut ErrorSink,
) -> Option<(Rule, std::collections::HashSet<PropertyId>)> {
    let path = format!("rules[{index}]");
    let obj = expect_object(value, file, &path, errors)?;
    let kind = match obj.get("kind") {
        Some(v) => expect_string(v, file, &join(&path, "kind"), errors),
        None => {
            // Same convention `require_field` uses: the path names the rule itself, not the
            // missing key — a path pointing at a key that by definition isn't in the text can
            // never resolve to a location.
            errors.push(file, &path, "отсутствует вид правила (kind)");
            None
        }
    };
    match kind.as_deref() {
        Some("move") => {
            reject_unknown_keys(obj, &["kind", "for"], file, &path, errors);
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            Some((Rule::Move { for_ }, std::collections::HashSet::new()))
        }
        Some("collide") => {
            reject_unknown_keys(
                obj,
                &["kind", "a", "b", "effects", "do"],
                file,
                &path,
                errors,
            );
            let a_json = require_field(obj, "a", file, &path, errors)?;
            let b_json = require_field(obj, "b", file, &path, errors)?;
            let a_obj = expect_object(a_json, file, &join(&path, "a"), errors)?;
            let b_obj = expect_object(b_json, file, &join(&path, "b"), errors)?;
            let a = resolve_selector(a_obj, properties, file, &join(&path, "a"), errors);
            let b = resolve_selector(b_obj, properties, file, &join(&path, "b"), errors);
            let effects_json = obj
                .get("effects")
                .and_then(|v| expect_object(v, file, &join(&path, "effects"), errors));
            if let Some(e) = effects_json {
                reject_unknown_keys(e, &["a", "b"], file, &join(&path, "effects"), errors);
            }
            let effects_a = parse_collide_effects(
                effects_json.and_then(|e| e.get("a")),
                properties,
                images,
                file,
                &join(&path, "effects → a"),
                errors,
            );
            let effects_b = parse_collide_effects(
                effects_json.and_then(|e| e.get("b")),
                properties,
                images,
                file,
                &join(&path, "effects → b"),
                errors,
            );
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                sounds,
                music,
                file,
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
            reject_unknown_keys(obj, &["kind", "for", "when", "do"], file, &path, errors);
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            let when_json = require_field(obj, "when", file, &path, errors)?;
            let when = parse_condition(when_json, properties, file, &join(&path, "when"), errors)?;
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                sounds,
                music,
                file,
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
                file,
                &path,
                errors,
            );
            let when_json = require_field(obj, "when", file, &path, errors)?;
            let when =
                parse_spawn_condition(when_json, properties, file, &join(&path, "when"), errors)?;
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
                        file,
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
                    file,
                    &join(&path, "where"),
                    "at_parent недоступен с условием fewer_than: у него нет родителя".to_string(),
                );
                return None;
            }
            let template_json = require_field(obj, "template", file, &path, errors)?;
            let (template, template_broken) = parse_template(
                template_json,
                properties,
                images,
                file,
                &join(&path, "template"),
                errors,
            );
            if matches!(when, SpawnCondition::FewerThan { .. })
                && template
                    .iter()
                    .any(|(_, v)| matches!(v, TemplateValue::FromParent(_)))
            {
                errors.push(
                    file,
                    &join(&path, "template"),
                    "from_parent недоступен с условием fewer_than: у него нет родителя".to_string(),
                );
            }
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                sounds,
                music,
                file,
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
            reject_unknown_keys(obj, ALL_RULE_KEYS, file, &path, errors);
            errors.push(
                file,
                &join(&path, "kind"),
                format!("неизвестный вид правила \"{other}\""),
            );
            None
        }
        None => {
            reject_unknown_keys(obj, ALL_RULE_KEYS, file, &path, errors);
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

fn parse_rules_json(
    text: &str,
    file: &str,
    properties: &PropertyTable,
    images: &[ImageDecl],
    sounds: &[(String, String)],
    music: &[(String, String)],
    errors: &mut ErrorSink,
) -> ParsedRules {
    let Some(root) = parse_json_or_error(file, text, errors) else {
        return (RuleSet::default(), Vec::new());
    };
    let Some(obj) = expect_object(&root, file, "", errors) else {
        return (RuleSet::default(), Vec::new());
    };
    reject_unknown_keys(obj, &["rules"], file, "", errors);
    let Some(rules_json) = obj.get("rules") else {
        errors.push(file, "", "отсутствует список правил");
        return (RuleSet::default(), Vec::new());
    };
    let Some(arr) = expect_array(rules_json, file, "rules", errors) else {
        return (RuleSet::default(), Vec::new());
    };
    let (rules, meta): (Vec<Rule>, Vec<RuleMeta>) = arr
        .iter()
        .enumerate()
        .filter_map(|(i, v)| {
            let (rule, template_broken) =
                parse_rule(v, i, file, properties, images, sounds, music, errors)?;
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

/// One shape from `possible_shapes`, with its display `locator` and the file/path of the node it
/// came from — a scene object's own node, or a spawn rule's `template` field — so a check run over
/// these shapes can address its own file, rather than the caller's.
struct PossibleShape {
    shape: Shape,
    locator: String,
    file: String,
    path: String,
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

#[allow(clippy::too_many_arguments)]
fn require(
    candidates: &[Candidate],
    file: &str,
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
            file,
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
fn widen_shapes_by_collide_effects(shapes: &mut [PossibleShape], rules: &RuleSet) {
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
            let a_matches = shapes.iter().any(|ps| matches_shape(a, &ps.shape));
            let b_matches = shapes.iter().any(|ps| matches_shape(b, &ps.shape));
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
                for ps in shapes.iter_mut() {
                    if !matches_shape(selector, &ps.shape) {
                        continue;
                    }
                    for prop in &given {
                        changed |= ps.shape.maybe.insert(*prop);
                    }
                    for prop in &taken {
                        changed |= ps.shape.removable.insert(*prop);
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
    scene_file: &str,
    rules_file: &str,
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    properties: &PropertyTable,
) -> Vec<PossibleShape> {
    let mut shapes: Vec<PossibleShape> = scene
        .iter()
        .map(|o| {
            let locator = join(scene_file, &o.path);
            let locator = match &o.name {
                Some(name) => format!("{locator} (имя \"{name}\")"),
                None => locator,
            };
            let (maybe, removable) = keys_edited_properties(o);
            PossibleShape {
                shape: Shape {
                    certain: o.shape.clone(),
                    maybe,
                    removable,
                    broken_properties: o.broken_properties.clone(),
                },
                locator,
                file: scene_file.to_string(),
                path: o.path.clone(),
            }
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
            let path = join(&format!("rules[{}]", meta.file_index), "template");
            shapes.push(PossibleShape {
                shape: Shape {
                    certain,
                    maybe,
                    removable: std::collections::HashSet::new(),
                    broken_properties: meta.template_broken.clone(),
                },
                locator: name,
                file: rules_file.to_string(),
                path,
            });
        }
    }
    widen_shapes_by_collide_effects(&mut shapes, rules);
    shapes
}

/// «Картинки»: `opacity` declared on a scene object or spawn template needs `image` to multiply,
/// but not necessarily from the object's own file shape — a `keys` edit or a collide rule's
/// `set`/`give` can hand it `image` once the game is running (`possible_shapes`'s `maybe`), and a
/// `broken_properties` `image` is already reported by its own error, so its presence is
/// known-incomplete rather than known-absent (see `validate_image_fill`). Run once here, after
/// `possible_shapes` has widened every shape, instead of at parse time like the `color`+`image`
/// check, which needs no such widening.
fn validate_opacity_reachable_image(
    shapes: &[PossibleShape],
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    // «Код игры»: слово `image` в тексте кода — код тоже мог дать объекту картинку, проверка
    // здесь бессильна отличить это от настоящей нехватки источника, так что просто не спорит.
    if code.is_some_and(|c| code_mentions_word(c, "image")) {
        return;
    }
    for ps in shapes {
        if !ps.shape.certain.contains(&property::OPACITY) {
            continue;
        }
        let has_image = ps.shape.certain.contains(&property::IMAGE)
            || ps.shape.maybe.contains(&property::IMAGE)
            || ps.shape.broken_properties.contains(&property::IMAGE);
        if !has_image {
            errors.push(&ps.file, &ps.path, OPACITY_WITHOUT_IMAGE);
        }
    }
}

/// «Код игры» → «Проверка перед запуском»: whether `word` appears in `code`'s text as its own
/// identifier, not merely as a substring of a longer one — such a mention counts as "used" for
/// the «объявлено и не используется» warnings, and a mention of `image` specifically excuses
/// `validate_opacity_reachable_image`.
fn code_mentions_word(code: &str, word: &str) -> bool {
    if word.is_empty() {
        return false;
    }
    let is_ident = |c: char| c.is_alphanumeric() || c == '_';
    let mut search_from = 0;
    while let Some(rel) = code[search_from..].find(word) {
        let start = search_from + rel;
        let end = start + word.len();
        let before_ok = code[..start]
            .chars()
            .next_back()
            .is_none_or(|c| !is_ident(c));
        let after_ok = code[end..].chars().next().is_none_or(|c| !is_ident(c));
        if before_ok && after_ok {
            return true;
        }
        search_from = start + word.chars().next().map_or(1, char::len_utf8);
        if search_from >= code.len() {
            break;
        }
    }
    false
}

fn describe_declared_functions(declared: &[String]) -> String {
    if declared.is_empty() {
        "в коде не объявлено ни одной функции".to_string()
    } else {
        let names = declared
            .iter()
            .map(|n| format!("\"{n}\""))
            .collect::<Vec<_>>()
            .join(", ");
        format!("объявлены {names}")
    }
}

/// «Код игры» → «Проверка перед запуском»: every `run` is checked once here, after the file (if
/// any) has compiled and its declared functions are known — `parse_run_action_name` itself never
/// sees the code, only the action's own shape. `declared` is `None` when the file is missing or
/// failed to load: that failure already has its own error, and naming every `run` as missing its
/// function on top of it would blame rules that are not at fault.
fn validate_run_actions(
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    file: &str,
    has_code: bool,
    declared: Option<&[String]>,
    errors: &mut ErrorSink,
) {
    let check = |name: &str, label: &str, errors: &mut ErrorSink| {
        if !has_code {
            errors.push(file, label, "run в игре без files.code".to_string());
        } else if let Some(declared) = declared
            && !declared.iter().any(|d| d == name)
        {
            errors.push(
                file,
                label,
                format!(
                    "функции \"{name}\" в коде нет; {}",
                    describe_declared_functions(declared)
                ),
            );
        }
    };
    for (rule, meta) in rules.rules.iter().zip(rule_meta) {
        let label = format!("rules[{}]", meta.file_index);
        match rule {
            Rule::Collide {
                effects_a,
                effects_b,
                do_,
                ..
            } => {
                for e in effects_a {
                    if let CollideEffect::Run(name) = e {
                        check(name, &join(&label, "effects → a"), errors);
                    }
                }
                for e in effects_b {
                    if let CollideEffect::Run(name) = e {
                        check(name, &join(&label, "effects → b"), errors);
                    }
                }
                for a in do_ {
                    if let CommonAction::Run(name) = a {
                        check(name, &join(&label, "do"), errors);
                    }
                }
            }
            Rule::Delete { do_, .. } | Rule::Spawn { do_, .. } => {
                for a in do_ {
                    if let CommonAction::Run(name) = a {
                        check(name, &join(&label, "do"), errors);
                    }
                }
            }
            Rule::Move { .. } => {}
        }
    }
}

/// `do_` from any rule kind that has one — `Move` has none.
fn common_actions(rule: &Rule) -> &[CommonAction] {
    match rule {
        Rule::Collide { do_, .. } | Rule::Delete { do_, .. } | Rule::Spawn { do_, .. } => do_,
        Rule::Move { .. } => &[],
    }
}

fn validate_property_sufficiency(
    shapes: &[PossibleShape],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) {
    let candidates: Vec<Candidate> = shapes
        .iter()
        .map(|ps| Candidate {
            shape: &ps.shape,
            locator: &ps.locator,
        })
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
                    file,
                    for_,
                    property::POSITION,
                    properties,
                    &label,
                    &mut seen,
                    errors,
                );
                require(
                    &candidates,
                    file,
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
                        file,
                        selector,
                        property::POSITION,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                    require(
                        &candidates,
                        file,
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
                            file,
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
                                file,
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
                    file,
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
                        file,
                        for_,
                        property::POSITION,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                    require(
                        &candidates,
                        file,
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
                                file,
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
        let do_actions = common_actions(rule);
        // Отметка ставится вставкой последним звеном цепочки условий ниже: только когда сообщение
        // действительно выдаётся. Перестановка звеньев отметит свойство до проверки и проглотит
        // ошибку — порядок здесь значащий.
        let mut reported_do_adds = std::collections::HashSet::new();
        for action in do_actions {
            if let CommonAction::Add { prop, .. } = action
                && !shapes
                    .iter()
                    .any(|ps| ps.shape.certain.contains(prop) || ps.shape.maybe.contains(prop))
                && reported_do_adds.insert(*prop)
            {
                errors.push(
                    file,
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
                        // «Код игры»: имя функции не свойство — считается использованным по
                        // тексту файла кода, не здесь; см. `code_names_used_as_words`.
                        CollideEffect::Bounce | CollideEffect::Delete | CollideEffect::Run(_) => {}
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
    properties_file: &str,
    scene: &[ParsedObject],
    rules: &RuleSet,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let used = collect_used_properties(scene, rules);
    for (id, def) in properties.iter() {
        // «Код игры»: имя свойства отдельным словом в тексте кода — тоже использование.
        if def.builtin
            || used.contains(&id)
            || code.is_some_and(|c| code_mentions_word(c, &def.name))
        {
            continue;
        }
        errors.push_warning(
            properties_file,
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
    scene_file: &str,
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
            scene_file,
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
    shapes: &[PossibleShape],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    rules_file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) {
    let mut check = |selector: &Selector, label: &str, field: &str| {
        if selector.has.is_empty() && selector.without.is_empty() {
            return;
        }
        if shapes.iter().any(|ps| matches_shape(selector, &ps.shape)) {
            return;
        }
        errors.push_warning(
            rules_file,
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

// ---------------------------------------------------------------------------------------------
// screens.json — «Экраны и состояние» / «Интерфейс игры»
// ---------------------------------------------------------------------------------------------

fn parse_ui_color(s: &str) -> Option<[f32; 4]> {
    let hex = s.strip_prefix('#')?;
    let component = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    match hex.len() {
        6 => Some([
            component(0)? as f32 / 255.0,
            component(2)? as f32 / 255.0,
            component(4)? as f32 / 255.0,
            1.0,
        ]),
        8 => Some([
            component(0)? as f32 / 255.0,
            component(2)? as f32 / 255.0,
            component(4)? as f32 / 255.0,
            component(6)? as f32 / 255.0,
        ]),
        _ => None,
    }
}

fn expect_ui_color(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<[f32; 4]> {
    let s = expect_string(value, file, path, errors)?;
    parse_ui_color(&s).or_else(|| {
        errors.push(
            file,
            path,
            format!("цвет должен быть вида \"#rrggbb\" или \"#rrggbbaa\", получено \"{s}\""),
        );
        None
    })
}

/// One `text` field after parsing: `{имя объекта.свойство}` becomes a resolved `TextPart::Value`,
/// everything else stays literal. «Интерфейс игры» → «Текст».
fn parse_rich_text(
    s: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<TextPart>> {
    let mut parts = Vec::new();
    let mut literal = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '}' {
            errors.push(file, path, format!("непарная закрывающая скобка в \"{s}\""));
            return None;
        }
        if ch != '{' {
            literal.push(ch);
            continue;
        }
        let mut inner = String::new();
        let mut closed = false;
        for c in chars.by_ref() {
            if c == '}' {
                closed = true;
                break;
            }
            inner.push(c);
        }
        if !closed {
            errors.push(
                file,
                path,
                format!("подстановка не закрыта скобкой в \"{s}\""),
            );
            return None;
        }
        let Some(dot) = inner.find('.') else {
            errors.push(
                file,
                path,
                format!("подстановка \"{{{inner}}}\" должна быть вида {{имя объекта.свойство}}"),
            );
            return None;
        };
        let (obj_name, prop_name) = (&inner[..dot], &inner[dot + 1..]);
        if obj_name.is_empty() || prop_name.is_empty() {
            errors.push(
                file,
                path,
                format!("подстановка \"{{{inner}}}\" должна быть вида {{имя объекта.свойство}}"),
            );
            return None;
        }
        let prop = resolve_property(prop_name, properties, file, path, errors)?;
        if !literal.is_empty() {
            parts.push(TextPart::Literal(std::mem::take(&mut literal)));
        }
        parts.push(TextPart::Value {
            object_name: obj_name.to_string(),
            prop,
        });
    }
    if !literal.is_empty() {
        parts.push(TextPart::Literal(literal));
    }
    Some(parts)
}

fn parse_placement(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Placement> {
    let anchor_json = require_field(obj, "anchor", file, path, errors)?;
    let anchor_str = expect_string(anchor_json, file, &join(path, "anchor"), errors)?;
    let anchor = Anchor::parse(&anchor_str).or_else(|| {
        errors.push(
            file,
            &join(path, "anchor"),
            format!("неизвестный якорь \"{anchor_str}\""),
        );
        None
    })?;
    let offset = match obj.get("offset") {
        Some(v) => parse_vec2(v, file, &join(path, "offset"), errors)?,
        None => [0.0, 0.0],
    };
    let size_json = require_field(obj, "size", file, path, errors)?;
    let size = parse_vec2(size_json, file, &join(path, "size"), errors)?;
    if size[0] <= 0.0 || size[1] <= 0.0 {
        errors.push(
            file,
            &join(path, "size"),
            format!(
                "размер должен быть больше нуля, получено [{}, {}]",
                size[0], size[1]
            ),
        );
        return None;
    }
    Some(Placement {
        anchor,
        offset: [offset[0] as f32, offset[1] as f32],
        size: [size[0] as f32, size[1] as f32],
    })
}

#[derive(Debug, Clone)]
enum ParsedCommand {
    ShowScreen(String),
    NewGame(String),
    Resume,
    Quit,
    ToggleSound,
}

fn parse_button_command(
    value: &Json,
    file: &str,
    path: &str,
    what: &str,
    errors: &mut ErrorSink,
) -> Option<ParsedCommand> {
    let arr = expect_array(value, file, path, errors)?;
    let head_json = arr.first();
    let Some(head_json) = head_json else {
        errors.push(
            file,
            path,
            "команда не той формы: пустой список".to_string(),
        );
        return None;
    };
    let head = expect_string(head_json, file, &join(path, "[0]"), errors)?;
    let want_args = |n: usize, errors: &mut ErrorSink| -> bool {
        if arr.len() == n {
            true
        } else {
            errors.push(
                file,
                path,
                format!(
                    "у команды \"{head}\" должно быть {} параметр(ов), получено {}",
                    n - 1,
                    arr.len() - 1
                ),
            );
            false
        }
    };
    match head.as_str() {
        "show_screen" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            want_args(2, errors).then_some(ParsedCommand::ShowScreen(name))
        }
        "new_game" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            want_args(2, errors).then_some(ParsedCommand::NewGame(name))
        }
        "resume" => want_args(1, errors).then_some(ParsedCommand::Resume),
        "quit" => want_args(1, errors).then_some(ParsedCommand::Quit),
        "toggle_sound" => {
            if arr.len() != 1 {
                errors.push(
                    file,
                    path,
                    "у toggle_sound настроек нет: команда переключает звук из любого состояния"
                        .to_string(),
                );
                return None;
            }
            Some(ParsedCommand::ToggleSound)
        }
        other => {
            errors.push(
                file,
                path,
                format!("неизвестная команда {what} \"{other}\""),
            );
            None
        }
    }
}

#[derive(Debug, Clone)]
enum ParsedElementData {
    Panel {
        placement: Placement,
        color: Option<[f32; 4]>,
        image_name: Option<String>,
        opacity: Option<f32>,
    },
    Label {
        placement: Placement,
        text: Vec<TextPart>,
        font_name: String,
        font_size: f32,
        color: [f32; 4],
        align: Align,
    },
    Button {
        placement: Placement,
        text: Vec<TextPart>,
        font_name: String,
        font_size: f32,
        text_color: [f32; 4],
        color: Option<[f32; 4]>,
        color_hover: Option<[f32; 4]>,
        color_pressed: Option<[f32; 4]>,
        image_name: Option<String>,
        image_hover_name: Option<String>,
        image_pressed_name: Option<String>,
        opacity: Option<f32>,
        on_click: ParsedCommand,
    },
}

const PANEL_KEYS: &[&str] = &[
    "kind", "anchor", "offset", "size", "color", "image", "opacity",
];
const LABEL_KEYS: &[&str] = &[
    "kind",
    "anchor",
    "offset",
    "size",
    "text",
    "font",
    "font_size",
    "color",
    "align",
];
const BUTTON_KEYS: &[&str] = &[
    "kind",
    "anchor",
    "offset",
    "size",
    "text",
    "font",
    "font_size",
    "text_color",
    "color",
    "color_hover",
    "color_pressed",
    "image",
    "image_hover",
    "image_pressed",
    "opacity",
    "on_click",
];

/// «Картинки» → «Где появляется картинка»: a panel or button must show exactly one of `color`/
/// `image`, never both and never neither — checked once here for the two element kinds that can
/// carry either.
fn validate_fill_choice(
    has_color: bool,
    has_image: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    match (has_color, has_image) {
        (true, true) => errors.push(
            file,
            path,
            "заданы и color, и image; должно быть ровно одно из двух",
        ),
        (false, false) => errors.push(file, path, "нужен один из color или image"),
        _ => {}
    }
}

fn parse_opacity(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<f32>> {
    match obj.get("opacity") {
        None => Some(None),
        Some(v) => {
            let n = expect_number(v, file, &join(path, "opacity"), errors)?;
            if !validate_opacity_range(n, file, &join(path, "opacity"), errors) {
                return None;
            }
            Some(Some(n as f32))
        }
    }
}

fn parse_element(
    value: &Json,
    path: &str,
    file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Option<ParsedElementData> {
    let obj = expect_object(value, file, path, errors)?;
    let kind_json = require_field(obj, "kind", file, path, errors)?;
    let kind = expect_string(kind_json, file, &join(path, "kind"), errors)?;
    match kind.as_str() {
        "panel" => {
            reject_unknown_keys(obj, PANEL_KEYS, file, path, errors);
            let placement = parse_placement(obj, file, path, errors)?;
            let color = match obj.get("color") {
                Some(v) => Some(expect_ui_color(v, file, &join(path, "color"), errors)?),
                None => None,
            };
            let image_name = match obj.get("image") {
                Some(v) => Some(expect_string(v, file, &join(path, "image"), errors)?),
                None => None,
            };
            validate_fill_choice(color.is_some(), image_name.is_some(), file, path, errors);
            let opacity = parse_opacity(obj, file, path, errors)?;
            validate_opacity_has_image(image_name.is_some(), opacity.is_some(), file, path, errors);
            Some(ParsedElementData::Panel {
                placement,
                color,
                image_name,
                opacity,
            })
        }
        "label" => {
            reject_unknown_keys(obj, LABEL_KEYS, file, path, errors);
            let placement = parse_placement(obj, file, path, errors)?;
            let text_json = require_field(obj, "text", file, path, errors)?;
            let text_str = expect_string(text_json, file, &join(path, "text"), errors)?;
            let text = parse_rich_text(&text_str, properties, file, &join(path, "text"), errors)?;
            let font_json = require_field(obj, "font", file, path, errors)?;
            let font_name = expect_string(font_json, file, &join(path, "font"), errors)?;
            let font_size = match obj.get("font_size") {
                Some(v) => expect_number(v, file, &join(path, "font_size"), errors)?,
                None => 16.0,
            };
            let color = match obj.get("color") {
                Some(v) => expect_ui_color(v, file, &join(path, "color"), errors)?,
                None => [1.0, 1.0, 1.0, 1.0],
            };
            let align = match obj.get("align") {
                Some(v) => {
                    let s = expect_string(v, file, &join(path, "align"), errors)?;
                    Align::parse(&s).or_else(|| {
                        errors.push(
                            file,
                            &join(path, "align"),
                            format!("неизвестное выравнивание \"{s}\""),
                        );
                        None
                    })?
                }
                None => Align::Left,
            };
            Some(ParsedElementData::Label {
                placement,
                text,
                font_name,
                font_size: font_size as f32,
                color,
                align,
            })
        }
        "button" => {
            reject_unknown_keys(obj, BUTTON_KEYS, file, path, errors);
            let placement = parse_placement(obj, file, path, errors)?;
            let text_json = require_field(obj, "text", file, path, errors)?;
            let text_str = expect_string(text_json, file, &join(path, "text"), errors)?;
            let text = parse_rich_text(&text_str, properties, file, &join(path, "text"), errors)?;
            let font_json = require_field(obj, "font", file, path, errors)?;
            let font_name = expect_string(font_json, file, &join(path, "font"), errors)?;
            let font_size = match obj.get("font_size") {
                Some(v) => expect_number(v, file, &join(path, "font_size"), errors)?,
                None => 16.0,
            };
            let text_color = match obj.get("text_color") {
                Some(v) => expect_ui_color(v, file, &join(path, "text_color"), errors)?,
                None => [1.0, 1.0, 1.0, 1.0],
            };
            let color = match obj.get("color") {
                Some(v) => Some(expect_ui_color(v, file, &join(path, "color"), errors)?),
                None => None,
            };
            let color_hover = match obj.get("color_hover") {
                Some(v) => Some(expect_ui_color(
                    v,
                    file,
                    &join(path, "color_hover"),
                    errors,
                )?),
                None => None,
            };
            let color_pressed = match obj.get("color_pressed") {
                Some(v) => Some(expect_ui_color(
                    v,
                    file,
                    &join(path, "color_pressed"),
                    errors,
                )?),
                None => None,
            };
            let image_name = match obj.get("image") {
                Some(v) => Some(expect_string(v, file, &join(path, "image"), errors)?),
                None => None,
            };
            let image_hover_name = match obj.get("image_hover") {
                Some(v) => Some(expect_string(v, file, &join(path, "image_hover"), errors)?),
                None => None,
            };
            let image_pressed_name = match obj.get("image_pressed") {
                Some(v) => Some(expect_string(
                    v,
                    file,
                    &join(path, "image_pressed"),
                    errors,
                )?),
                None => None,
            };
            match (color.is_some(), image_name.is_some()) {
                (true, true) | (false, false) => {
                    validate_fill_choice(color.is_some(), image_name.is_some(), file, path, errors)
                }
                (true, false) => {
                    if image_hover_name.is_some() || image_pressed_name.is_some() {
                        errors.push(
                            file,
                            path,
                            "image_hover и image_pressed допустимы только у кнопки с image, а не с color",
                        );
                    }
                }
                (false, true) => {
                    if color_hover.is_some() || color_pressed.is_some() {
                        errors.push(
                            file,
                            path,
                            "color_hover и color_pressed допустимы только у кнопки с color, а не с image",
                        );
                    }
                }
            }
            let opacity = parse_opacity(obj, file, path, errors)?;
            validate_opacity_has_image(image_name.is_some(), opacity.is_some(), file, path, errors);
            let on_click_json = require_field(obj, "on_click", file, path, errors)?;
            let on_click = parse_button_command(
                on_click_json,
                file,
                &join(path, "on_click"),
                "кнопки",
                errors,
            )?;
            Some(ParsedElementData::Button {
                placement,
                text,
                font_name,
                font_size: font_size as f32,
                text_color,
                color,
                color_hover,
                color_pressed,
                image_name,
                image_hover_name,
                image_pressed_name,
                opacity,
                on_click,
            })
        }
        other => {
            errors.push(
                file,
                &join(path, "kind"),
                format!("неизвестный вид элемента \"{other}\": ожидался panel, label или button"),
            );
            None
        }
    }
}

/// Parses the optional `keys` table — same five commands as `on_click`, resolved later by
/// `resolve_on_click` once every screen's name is known. A JSON object's keys are always
/// strings, so «клавиша названа не строкой» from the checklist can't occur past `expect_object`.
fn parse_screen_keys(
    value: &Json,
    path: &str,
    file: &str,
    errors: &mut ErrorSink,
) -> Vec<(String, ParsedCommand)> {
    let Some(obj) = expect_object(value, file, path, errors) else {
        return Vec::new();
    };
    obj.iter()
        .filter_map(|(code, cmd_json)| {
            let cmd = parse_button_command(cmd_json, file, &join(path, code), "клавиши", errors)?;
            Some((code.clone(), cmd))
        })
        .collect()
}

struct ParsedScreen {
    name: String,
    world_runs: bool,
    elements: Vec<ParsedElementData>,
    keys: Vec<(String, ParsedCommand)>,
    /// Raw `music` name, not yet resolved against `files.music` — «Звук» →«Два вида звука». Resolution happens in `resolve_screens`, same as `font` on an element.
    music: Option<String>,
    path: String,
}

fn parse_screen(
    value: &Json,
    index: usize,
    file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Option<ParsedScreen> {
    let path = format!("screens[{index}]");
    let obj = expect_object(value, file, &path, errors)?;
    reject_unknown_keys(
        obj,
        &["name", "world_runs", "elements", "keys", "music"],
        file,
        &path,
        errors,
    );
    let name = require_field(obj, "name", file, &path, errors)
        .and_then(|v| expect_string(v, file, &join(&path, "name"), errors));
    let world_runs = require_field(obj, "world_runs", file, &path, errors).and_then(|v| {
        v.as_bool().or_else(|| {
            errors.push(
                file,
                &join(&path, "world_runs"),
                format!("ожидался признак (true/false), получено {}", kind_name(v)),
            );
            None
        })
    });
    let elements_path = join(&path, "elements");
    let elements_json = require_field(obj, "elements", file, &path, errors)
        .and_then(|v| expect_array(v, file, &elements_path, errors));
    let elements = elements_json
        .map(|arr| {
            arr.iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    parse_element(
                        v,
                        &join(&elements_path, &format!("[{i}]")),
                        file,
                        properties,
                        errors,
                    )
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let keys = match obj.get("keys") {
        Some(v) => parse_screen_keys(v, &join(&path, "keys"), file, errors),
        None => Vec::new(),
    };
    // «Звук»: список — самая частая опечатка (плейлист вместо одного трека), и у
    // неё свой текст, отдельный от обычного «ожидалась строка».
    let music = match obj.get("music") {
        None => None,
        Some(v) if v.is_array() => {
            errors.push(
                file,
                &join(&path, "music"),
                "плейлиста нет, у экрана один трек".to_string(),
            );
            None
        }
        Some(v) => expect_string(v, file, &join(&path, "music"), errors),
    };
    Some(ParsedScreen {
        name: name?,
        world_runs: world_runs?,
        elements,
        keys,
        music,
        path,
    })
}

fn parse_screens_json(
    text: &str,
    file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Vec<ParsedScreen> {
    let Some(root) = parse_json_or_error(file, text, errors) else {
        return Vec::new();
    };
    let Some(obj) = expect_object(&root, file, "", errors) else {
        return Vec::new();
    };
    reject_unknown_keys(obj, &["screens"], file, "", errors);
    let Some(screens_json) = obj.get("screens") else {
        errors.push(file, "", "отсутствует список экранов");
        return Vec::new();
    };
    let Some(arr) = expect_array(screens_json, file, "screens", errors) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, v)| parse_screen(v, i, file, properties, errors))
        .collect()
}

fn resolve_font(
    name: &str,
    fonts: &[(String, String)],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<FontId> {
    match fonts.iter().position(|(n, _)| n == name) {
        Some(id) => Some(id),
        None => {
            errors.push(
                file,
                path,
                format!("шрифт \"{name}\" не объявлен в files.fonts"),
            );
            None
        }
    }
}

fn validate_text_refs(
    text: &[TextPart],
    scene_names: &std::collections::HashSet<&str>,
    file: &str,
    scene_file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let mut ok = true;
    for part in text {
        if let TextPart::Value { object_name, .. } = part
            && !scene_names.contains(object_name.as_str())
        {
            errors.push(
                file,
                path,
                format!(
                    "надпись ссылается на объект \"{object_name}\", которого нет в {scene_file}"
                ),
            );
            ok = false;
        }
    }
    ok
}

/// Resolves a screen name against `name_to_id` for the three places «Экраны и состояние» names
/// separately: a button's `on_click`, a screen's own `keys`, and `game.json`'s
/// `start_screen`/`win_screen`/`loss_screen` — each needs its own file and wording rather than
/// the one generic "кнопка" text every caller used to get regardless of which of the three it was.
fn resolve_button_target(
    name: &str,
    name_to_id: &std::collections::HashMap<String, ScreenId>,
    file: &str,
    path: &str,
    subject: &str,
    errors: &mut ErrorSink,
) -> Option<ScreenId> {
    match name_to_id.get(name) {
        Some(&id) => Some(id),
        None => {
            errors.push(
                file,
                path,
                format!("{subject} ссылается на несуществующий экран \"{name}\""),
            );
            None
        }
    }
}

fn resolve_on_click(
    cmd: &ParsedCommand,
    name_to_id: &std::collections::HashMap<String, ScreenId>,
    parsed_screens: &[ParsedScreen],
    file: &str,
    path: &str,
    subject: &str,
    errors: &mut ErrorSink,
) -> Option<ButtonCommand> {
    match cmd {
        ParsedCommand::ShowScreen(name) => {
            resolve_button_target(name, name_to_id, file, path, subject, errors)
                .map(ButtonCommand::ShowScreen)
        }
        ParsedCommand::NewGame(name) => {
            let id = resolve_button_target(name, name_to_id, file, path, subject, errors)?;
            if !parsed_screens[id].world_runs {
                errors.push(
                    file,
                    path,
                    format!("new_game ведёт на экран \"{name}\" без world_runs"),
                );
                return None;
            }
            Some(ButtonCommand::NewGame(id))
        }
        ParsedCommand::Resume => Some(ButtonCommand::Resume),
        ParsedCommand::Quit => Some(ButtonCommand::Quit),
        ParsedCommand::ToggleSound => Some(ButtonCommand::ToggleSound),
    }
}

/// A panel or button's fill — exactly one of `color`/`image` at the base level (`default: None`,
/// «Картинки»: both or neither was already flagged by `validate_fill_choice` at parse time, so
/// `None` here just drops the element like any other broken field), or a button's own hover/
/// pressed field if it names one, the base fill otherwise (`default: Some(base)` — «Картинки»:
/// «Нет своей картинки на вид — во всех трёх видах показывается image», the same default
/// `color_hover`/`color_pressed` already had for a plain color).
#[allow(clippy::too_many_arguments)]
fn resolve_fill(
    color: Option<[f32; 4]>,
    image_name: Option<&str>,
    opacity: Option<f32>,
    default: Option<Fill>,
    images: &[ImageDecl],
    file: &str,
    image_path: &str,
    errors: &mut ErrorSink,
) -> Option<Fill> {
    match (color, image_name) {
        (Some(c), None) => Some(Fill::Color(c)),
        (None, Some(name)) => {
            let image = resolve_image(name, images, file, image_path, errors)?;
            Some(Fill::Image {
                image,
                opacity: opacity.unwrap_or(1.0),
            })
        }
        (None, None) => default,
        // `parse_element` never sets both — a button's `color`/`image` and their hover/pressed
        // counterparts are always parsed as one or the other for the same field.
        (Some(_), Some(_)) => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn resolve_element(
    data: &ParsedElementData,
    path: &str,
    file: &str,
    scene_file: &str,
    fonts: &[(String, String)],
    images: &[ImageDecl],
    scene_names: &std::collections::HashSet<&str>,
    name_to_id: &std::collections::HashMap<String, ScreenId>,
    parsed_screens: &[ParsedScreen],
    errors: &mut ErrorSink,
) -> Option<Element> {
    match data {
        ParsedElementData::Panel {
            placement,
            color,
            image_name,
            opacity,
        } => {
            let fill = resolve_fill(
                *color,
                image_name.as_deref(),
                *opacity,
                None,
                images,
                file,
                &join(path, "image"),
                errors,
            )?;
            Some(Element::Panel {
                placement: *placement,
                fill,
            })
        }
        ParsedElementData::Label {
            placement,
            text,
            font_name,
            font_size,
            color,
            align,
        } => {
            let font = resolve_font(font_name, fonts, file, &join(path, "font"), errors);
            let text_ok = validate_text_refs(
                text,
                scene_names,
                file,
                scene_file,
                &join(path, "text"),
                errors,
            );
            let font = font?;
            if !text_ok {
                return None;
            }
            Some(Element::Label {
                placement: *placement,
                text: text.clone(),
                font,
                font_size: *font_size,
                color: *color,
                align: *align,
            })
        }
        ParsedElementData::Button {
            placement,
            text,
            font_name,
            font_size,
            text_color,
            color,
            color_hover,
            color_pressed,
            image_name,
            image_hover_name,
            image_pressed_name,
            opacity,
            on_click,
        } => {
            let font = resolve_font(font_name, fonts, file, &join(path, "font"), errors);
            let text_ok = validate_text_refs(
                text,
                scene_names,
                file,
                scene_file,
                &join(path, "text"),
                errors,
            );
            let on_click = resolve_on_click(
                on_click,
                name_to_id,
                parsed_screens,
                file,
                &join(path, "on_click"),
                "кнопка",
                errors,
            );
            let fill = resolve_fill(
                *color,
                image_name.as_deref(),
                *opacity,
                None,
                images,
                file,
                &join(path, "image"),
                errors,
            );
            let (font, on_click, fill) = (font?, on_click?, fill?);
            let fill_hover = resolve_fill(
                *color_hover,
                image_hover_name.as_deref(),
                *opacity,
                Some(fill),
                images,
                file,
                &join(path, "image_hover"),
                errors,
            );
            let fill_pressed = resolve_fill(
                *color_pressed,
                image_pressed_name.as_deref(),
                *opacity,
                Some(fill),
                images,
                file,
                &join(path, "image_pressed"),
                errors,
            );
            let (fill_hover, fill_pressed) = (fill_hover?, fill_pressed?);
            if !text_ok {
                return None;
            }
            Some(Element::Button {
                placement: *placement,
                text: text.clone(),
                font,
                font_size: *font_size,
                text_color: *text_color,
                fill,
                fill_hover,
                fill_pressed,
                on_click,
            })
        }
    }
}

/// Whether any rule's `do` contains `["end_game","win"]` / `["end_game","loss"]` — decides
/// whether `win_screen`/`loss_screen` are required in `game.json`.
fn rule_end_game_usage(rules: &RuleSet) -> (bool, bool) {
    let (mut uses_win, mut uses_loss) = (false, false);
    for rule in &rules.rules {
        let do_ = common_actions(rule);
        for action in do_ {
            if let CommonAction::EndGame(outcome) = action {
                match outcome {
                    Outcome::Win => uses_win = true,
                    Outcome::Loss => uses_loss = true,
                }
            }
        }
    }
    (uses_win, uses_loss)
}

/// Cross-checks and assembles the final `ScreensConfig` once `screens.json`, `scene.json` and
/// `rules.json` are all parsed: resolves every screen-name and font-name reference, and runs the
/// checks that need more than one file at once — «Экраны и состояние» → «Проверка данных перед
/// запуском».
#[allow(clippy::too_many_arguments)]
fn resolve_screens(
    parsed: &[ParsedScreen],
    file: &str,
    scene_file: &str,
    start_screen_name: &str,
    win_screen_name: Option<&str>,
    loss_screen_name: Option<&str>,
    fonts: &[(String, String)],
    images: &[ImageDecl],
    sounds: &[(String, String)],
    music_table: &[(String, String)],
    scene_objects: &[ParsedObject],
    rules: &RuleSet,
    errors: &mut ErrorSink,
) -> Option<ScreensConfig> {
    let mut name_to_id: std::collections::HashMap<String, ScreenId> =
        std::collections::HashMap::new();
    for (i, screen) in parsed.iter().enumerate() {
        if name_to_id.insert(screen.name.clone(), i).is_some() {
            errors.push(
                file,
                &join(&screen.path, "name"),
                format!("имя экрана \"{}\" повторяется", screen.name),
            );
        }
    }

    if !parsed.is_empty() && !parsed.iter().any(|s| s.world_runs) {
        errors.push(
            file,
            "",
            "ни один экран не помечен world_runs — мир не пойдёт никогда".to_string(),
        );
    }

    let scene_names: std::collections::HashSet<&str> = scene_objects
        .iter()
        .filter_map(|o| o.name.as_deref())
        .collect();

    let screens: Vec<Screen> = parsed
        .iter()
        .map(|screen| {
            let elements_path = join(&screen.path, "elements");
            let elements = screen
                .elements
                .iter()
                .enumerate()
                .filter_map(|(i, data)| {
                    resolve_element(
                        data,
                        &join(&elements_path, &format!("[{i}]")),
                        file,
                        scene_file,
                        fonts,
                        images,
                        &scene_names,
                        &name_to_id,
                        parsed,
                        errors,
                    )
                })
                .collect();
            let keys_path = join(&screen.path, "keys");
            let keys: ScreenKeyTable = screen
                .keys
                .iter()
                .filter_map(|(code, cmd)| {
                    let resolved = resolve_on_click(
                        cmd,
                        &name_to_id,
                        parsed,
                        file,
                        &join(&keys_path, code),
                        "клавиша",
                        errors,
                    )?;
                    Some((code.clone(), resolved))
                })
                .collect();
            let music = screen.music.as_deref().and_then(|name| {
                resolve_music(
                    name,
                    music_table,
                    sounds,
                    file,
                    &join(&screen.path, "music"),
                    errors,
                )
            });
            Screen {
                name: screen.name.clone(),
                world_runs: screen.world_runs,
                elements,
                keys,
                music,
            }
        })
        .collect();

    let start_screen = resolve_button_target(
        start_screen_name,
        &name_to_id,
        "game.json",
        "start_screen",
        "поле start_screen",
        errors,
    );
    let win_screen = win_screen_name.and_then(|name| {
        resolve_button_target(
            name,
            &name_to_id,
            "game.json",
            "win_screen",
            "поле win_screen",
            errors,
        )
    });
    let loss_screen = loss_screen_name.and_then(|name| {
        resolve_button_target(
            name,
            &name_to_id,
            "game.json",
            "loss_screen",
            "поле loss_screen",
            errors,
        )
    });

    let (uses_win, uses_loss) = rule_end_game_usage(rules);
    if uses_win && win_screen_name.is_none() {
        errors.push(
            "game.json",
            "",
            "правило содержит [\"end_game\", \"win\"], а win_screen не назван".to_string(),
        );
    }
    if uses_loss && loss_screen_name.is_none() {
        errors.push(
            "game.json",
            "",
            "правило содержит [\"end_game\", \"loss\"], а loss_screen не назван".to_string(),
        );
    }
    if let Some(id) = win_screen
        && screens[id].world_runs
    {
        errors.push(
            "game.json",
            "win_screen",
            "у экрана исхода не должен быть поднят world_runs".to_string(),
        );
    }
    if let Some(id) = loss_screen
        && screens[id].world_runs
    {
        errors.push(
            "game.json",
            "loss_screen",
            "у экрана исхода не должен быть поднят world_runs".to_string(),
        );
    }

    let mut reachable: std::collections::HashSet<ScreenId> = std::collections::HashSet::new();
    reachable.extend(start_screen);
    reachable.extend(win_screen);
    reachable.extend(loss_screen);
    for screen in &screens {
        for element in &screen.elements {
            if let Element::Button { on_click, .. } = element {
                match on_click {
                    ButtonCommand::ShowScreen(target) | ButtonCommand::NewGame(target) => {
                        reachable.insert(*target);
                    }
                    ButtonCommand::Resume | ButtonCommand::Quit | ButtonCommand::ToggleSound => {}
                }
            }
        }
        for cmd in screen.keys.values() {
            match cmd {
                ButtonCommand::ShowScreen(target) | ButtonCommand::NewGame(target) => {
                    reachable.insert(*target);
                }
                ButtonCommand::Resume | ButtonCommand::Quit | ButtonCommand::ToggleSound => {}
            }
        }
    }
    for (i, screen) in screens.iter().enumerate() {
        if !reachable.contains(&i) {
            errors.push_warning(
                file,
                &format!("screens[{i}]"),
                format!(
                    "до экрана \"{}\" не ведёт ни одна кнопка, и он не назван в game.json; ожидалось, что каждый экран достижим",
                    screen.name
                ),
            );
        }
    }

    let start_screen = start_screen?;
    Some(ScreensConfig {
        screens,
        start_screen,
        win_screen,
        loss_screen,
    })
}

/// Checks the file starts with a recognized TrueType/OpenType/collection magic number. The real
/// glyph parser (`cosmic-text`, via `glyphon`) only exists on the `wasm32` target, so prestart
/// validation settles for this rather than pulling in a full font-parsing crate as a native
/// dependency for one check.
fn looks_like_font(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x00, 0x01, 0x00, 0x00])
        || bytes.starts_with(b"OTTO")
        || bytes.starts_with(b"true")
        || bytes.starts_with(b"ttcf")
}

/// «Интерфейс игры»: a font declared in `files.fonts` but missing or unparseable is an error
/// whether or not any element currently references it — the same way a scene object's own
/// broken field is, not treated as merely unused.
fn validate_font_files(
    fonts: &[(String, String)],
    font_bytes: &[(String, Option<Vec<u8>>)],
    errors: &mut ErrorSink,
) {
    for (name, path) in fonts {
        let bytes = font_bytes
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, b)| b.as_ref());
        let field_path = format!("files → fonts → {name}");
        match bytes {
            None => errors.push(
                "game.json",
                &field_path,
                format!(
                    "файл шрифта \"{path}\" не найден; ожидался файл шрифта, названный в game.json → files → fonts"
                ),
            ),
            Some(b) if !looks_like_font(b) => errors.push(
                "game.json",
                &field_path,
                format!("файл \"{path}\" не разбирается как шрифт"),
            ),
            Some(_) => {}
        }
    }
}

/// «Звук» → «Загрузка и проверка»: every declared sound is read and
/// measured whether or not `play_sound` uses it — same treatment `validate_font_files` gives
/// fonts, unlike an unreferenced track (see `validate_unreferenced_tracks`), which isn't even read.
fn validate_sound_files(
    sounds: &[(String, String)],
    sound_bytes: &[(String, Option<Vec<u8>>)],
    errors: &mut ErrorSink,
) {
    for (name, path) in sounds {
        let bytes = sound_bytes
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, b)| b.as_ref());
        let field_path = format!("files → sounds → {name}");
        let Some(bytes) = bytes else {
            errors.push(
                "game.json",
                &field_path,
                format!(
                    "файл \"{path}\" не найден; ожидался WAV-звук, названный в game.json → files → sounds"
                ),
            );
            continue;
        };
        let Some(info) = wav::parse_wav(bytes) else {
            errors.push(
                "game.json",
                &field_path,
                format!("файл \"{path}\" не разбирается как WAV"),
            );
            continue;
        };
        if info.duration_seconds > 5.0 {
            errors.push(
                "game.json",
                &field_path,
                format!("звук \"{path}\" длиннее пяти секунд: этому место в files.music"),
            );
        }
    }
}

/// «Звук» → «Загрузка и проверка»: only a track some screen
/// actually names is read at all — `referenced` is that set, gathered from the raw parsed screens
/// before name resolution so it doesn't depend on the rest of the game being error-free.
fn validate_music_files(
    music: &[(String, String)],
    referenced: &std::collections::HashSet<String>,
    verdicts: &[(String, MusicVerdict)],
    errors: &mut ErrorSink,
) {
    for (name, path) in music {
        if !referenced.contains(name) {
            continue;
        }
        let field_path = format!("files → music → {name}");
        match verdicts.iter().find(|(n, _)| n == name).map(|(_, v)| v) {
            Some(MusicVerdict::Ok) => {}
            Some(MusicVerdict::Rejected) => errors.push(
                "game.json",
                &field_path,
                format!("{path} — исполнитель (браузер) не берётся разжимать этот файл"),
            ),
            Some(MusicVerdict::Missing) | None => errors.push(
                "game.json",
                &field_path,
                format!(
                    "файл \"{path}\" не найден; ожидался MP3-трек, названный в game.json → files → music"
                ),
            ),
        }
    }
}

/// «Картинки» → «Загрузка и проверка»: every declared image is read and checked whether or not
/// anything names it — same treatment `validate_font_files`/`validate_sound_files` give fonts and
/// sounds, unlike an unreferenced track (`validate_unreferenced_tracks`), which isn't even read.
fn validate_image_files(
    images: &[ImageDecl],
    image_data: &[(String, ImageVerdict)],
    errors: &mut ErrorSink,
) {
    for decl in images {
        let field_path = format!("files → images → {}", decl.name);
        match image_data
            .iter()
            .find(|(n, _)| n == &decl.name)
            .map(|(_, v)| v)
        {
            Some(ImageVerdict::Ok {
                width,
                height,
                pixels,
            }) => {
                if *width == 0 || *height == 0 {
                    errors.push(
                        "game.json",
                        &field_path,
                        format!(
                            "{} — ширина и высота картинки должны быть больше нуля, получено {width}×{height}",
                            decl.path
                        ),
                    );
                    continue;
                }
                let expected_len = *width as usize * *height as usize * 4;
                if pixels.len() != expected_len {
                    errors.push(
                        "game.json",
                        &field_path,
                        format!(
                            "{} — страница отдала {} байт точек, ожидалось {expected_len} ({width}×{height}×4)",
                            decl.path,
                            pixels.len()
                        ),
                    );
                    continue;
                }
                if width % decl.frames != 0 {
                    errors.push(
                        "game.json",
                        &field_path,
                        format!(
                            "{} — ширина {width} не делится на frames ({}) нацело, остаток {}",
                            decl.path,
                            decl.frames,
                            width % decl.frames
                        ),
                    );
                }
            }
            Some(ImageVerdict::Rejected) => errors.push(
                "game.json",
                &field_path,
                format!(
                    "{} — исполнитель (браузер) не берётся разжимать этот файл",
                    decl.path
                ),
            ),
            Some(ImageVerdict::Missing) | None => errors.push(
                "game.json",
                &field_path,
                format!(
                    "файл \"{}\" не найден; ожидалась PNG-картинка, названная в game.json → files → images",
                    decl.path
                ),
            ),
        }
    }
}

fn mark_fill_used(used: &mut std::collections::HashSet<ImageId>, fill: &Fill) {
    if let Fill::Image { image, .. } = fill {
        used.insert(*image);
    }
}

fn mark_key_table_used(used: &mut std::collections::HashSet<ImageId>, table: &KeyTable) {
    for binding in table.values() {
        for edit in binding.press.iter().chain(&binding.release) {
            if let Value::Image(id) = &edit.value {
                used.insert(*id);
            }
        }
    }
}

/// «Картинки»: an image the scene/rules/screens never name — collected across an object's own
/// `image` property, its key bindings' `press`/`release` edits, a spawn template's `image` field
/// (a collide effect's `set` counts too, the only other place a `Value::Image` can come from) and
/// every panel/button fill.
fn collect_used_images(
    scene: &[ParsedObject],
    rules: &RuleSet,
    screens: &[Screen],
) -> std::collections::HashSet<ImageId> {
    let mut used = std::collections::HashSet::new();
    for obj in scene {
        for (_, v) in &obj.values {
            if let Value::Image(id) = v {
                used.insert(*id);
            }
        }
        if let Some(table) = &obj.keys {
            mark_key_table_used(&mut used, table);
        }
    }
    for rule in &rules.rules {
        match rule {
            Rule::Spawn { template, .. } => {
                for (_, tv) in template {
                    if let TemplateValue::Const(Value::Image(id)) = tv {
                        used.insert(*id);
                    }
                }
            }
            Rule::Collide {
                effects_a,
                effects_b,
                ..
            } => {
                for effect in effects_a.iter().chain(effects_b) {
                    if let CollideEffect::Set {
                        value: Value::Image(id),
                        ..
                    } = effect
                    {
                        used.insert(*id);
                    }
                }
            }
            Rule::Move { .. } | Rule::Delete { .. } => {}
        }
    }
    for screen in screens {
        for element in &screen.elements {
            match element {
                Element::Panel { fill, .. } => mark_fill_used(&mut used, fill),
                Element::Button {
                    fill,
                    fill_hover,
                    fill_pressed,
                    ..
                } => {
                    mark_fill_used(&mut used, fill);
                    mark_fill_used(&mut used, fill_hover);
                    mark_fill_used(&mut used, fill_pressed);
                }
                Element::Label { .. } => {}
            }
        }
    }
    used
}

/// «Картинки»: предупреждение (игра идёт), если объявленная картинка не встречается ни на одном
/// объекте сцены, ни в одном шаблоне создания, ни на одном элементе экрана.
fn validate_unused_images(
    images: &[ImageDecl],
    scene: &[ParsedObject],
    rules: &RuleSet,
    screens: &[Screen],
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let used = collect_used_images(scene, rules, screens);
    for (i, decl) in images.iter().enumerate() {
        if !used.contains(&i) && !code.is_some_and(|c| code_mentions_word(c, &decl.name)) {
            errors.push_warning(
                "game.json",
                &format!("files → images → {}", decl.name),
                format!(
                    "картинка \"{}\" объявлена, но её не называет ни один объект, ни один шаблон создания и ни один элемент экрана",
                    decl.name
                ),
            );
        }
    }
}

/// «Звук»: объявленный звук, на который не ссылается ни один `play_sound` — читан
/// и проверен, просто не нужен, как объявленное и не используемое свойство.
fn validate_unused_sounds(
    sounds: &[(String, String)],
    rules: &RuleSet,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let mut used: std::collections::HashSet<SoundId> = std::collections::HashSet::new();
    for rule in &rules.rules {
        let do_ = common_actions(rule);
        for action in do_ {
            if let CommonAction::PlaySound(id) = action {
                used.insert(*id);
            }
        }
    }
    for (i, (name, _)) in sounds.iter().enumerate() {
        if !used.contains(&i) && !code.is_some_and(|c| code_mentions_word(c, name)) {
            errors.push_warning(
                "game.json",
                &format!("files → sounds → {name}"),
                format!(
                    "звук \"{name}\" объявлен, но на него не ссылается ни один play_sound; ожидалось, что объявленный звук где-то используется"
                ),
            );
        }
    }
}

/// «Звук» → «Загрузка и проверка»: unlike an unused sound, an
/// unreferenced track isn't read or checked by anything, so its warning names that consequence
/// rather than just "declared but unused".
fn validate_unreferenced_tracks(
    music: &[(String, String)],
    referenced: &std::collections::HashSet<String>,
    errors: &mut ErrorSink,
) {
    for (name, _) in music {
        if !referenced.contains(name) {
            errors.push_warning(
                "game.json",
                &format!("files → music → {name}"),
                format!(
                    "трек \"{name}\" объявлен, но его не называет ни один экран: он не прочитан и потому не проверен; сославшись на него, проверку придётся проходить заново"
                ),
            );
        }
    }
}

fn any_toggle_sound_bound(screens: &[Screen]) -> bool {
    screens.iter().any(|screen| {
        let button_bound = screen.elements.iter().any(|element| {
            matches!(
                element,
                Element::Button {
                    on_click: ButtonCommand::ToggleSound,
                    ..
                }
            )
        });
        button_bound
            || screen
                .keys
                .values()
                .any(|cmd| matches!(cmd, ButtonCommand::ToggleSound))
    })
}

/// «Звук» → «Загрузка и проверка»: игрок должен иметь способ выключить
/// звук, если он в игре вообще есть — хоть один `play_sound`, хоть один экран с `music`.
fn validate_toggle_sound_presence(
    rules: &RuleSet,
    screens: &[Screen],
    file: &str,
    errors: &mut ErrorSink,
) {
    let any_play_sound = rules.rules.iter().any(|rule| {
        common_actions(rule)
            .iter()
            .any(|a| matches!(a, CommonAction::PlaySound(_)))
    });
    let any_music = screens.iter().any(|s| s.music.is_some());
    if (any_play_sound || any_music) && !any_toggle_sound_bound(screens) {
        errors.push_warning(
            file,
            "",
            "в игре есть звук, но toggle_sound не назначен ни одной кнопке и ни одной клавише экрана"
                .to_string(),
        );
    }
}

/// «Экраны и состояние» → «Клавиши экрана»: a key a *live* screen names never reaches the world —
/// warns when that key also drives one of the scene's own key bindings, so the author isn't
/// silently missing input. Only scene objects can carry a `keys` table (a spawn template can't —
/// `parse_template` never parses one), so scanning `scene_objects` covers every object that could
/// ever hold such a binding.
fn validate_screen_key_collisions(
    parsed_screens: &[ParsedScreen],
    screens_file: &str,
    scene_objects: &[ParsedObject],
    scene_file: &str,
    errors: &mut ErrorSink,
) {
    for screen in parsed_screens {
        if !screen.world_runs {
            continue;
        }
        for (code, _) in &screen.keys {
            for obj in scene_objects {
                let Some(table) = &obj.keys else { continue };
                if !table.contains_key(code) {
                    continue;
                }
                errors.push_warning(
                    screens_file,
                    &join(&screen.path, "keys"),
                    format!(
                        "клавиша \"{code}\" экрана \"{}\" совпадает с клавишей, привязанной к объекту {}{}: на этом экране привязка работать не будет",
                        screen.name,
                        join(scene_file, &obj.path),
                        name_suffix(obj),
                    ),
                );
            }
        }
    }
}

/// Third and last load call: needs `game_json`'s own text back (some of this call's own errors —
/// `start_screen`/`win_screen`/`loss_screen`, a missing font/sound/track file — are about fields
/// declared there, and «Формат игры» → «Проверка данных перед запуском» promises every message a
/// location, not just a file), the `GameConfig` `read_entry` parsed from it, the four text files
/// `read_texts` was given, and the binary files `read_texts` named as needed — fonts and sounds by
/// bytes, music by the executor's own verdict on each readable track (see `MusicVerdict`).
/// `None`/an empty slice for any of them means the page could not fetch it. A warning is only
/// computed once the four text files parsed without error: an object or rule that failed to parse
/// simply drops out of `scene_objects`/`rules`, and a warning computed against that shrunken,
/// incomplete set can be outright wrong about the data the author will have once the real errors
/// are fixed — not just unhelpful. And a warning is «the game still runs» information (see
/// «Формат игры»), which doesn't apply here anyway: with errors present the game never starts.
#[allow(clippy::too_many_arguments)]
pub fn load_rest(
    game_json: &str,
    config: GameConfig,
    properties_json: Option<&str>,
    scene_json: Option<&str>,
    rules_json: Option<&str>,
    screens_json: Option<&str>,
    font_bytes: &[(String, Option<Vec<u8>>)],
    sound_bytes: &[(String, Option<Vec<u8>>)],
    music_verdicts: &[(String, MusicVerdict)],
    image_data: &[(String, ImageVerdict)],
    code_json: Option<&str>,
) -> Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure> {
    let mut errors = ErrorSink::new();

    let properties = match properties_json {
        Some(text) => parse_properties_json(text, &config.files.properties, &mut errors),
        None => {
            errors.push(
                &config.files.properties,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → properties",
            );
            PropertyTable::new()
        }
    };

    let scene_objects = match scene_json {
        Some(text) => parse_scene_json(
            text,
            &config.files.scene,
            &properties,
            &config.files.images,
            &mut errors,
        ),
        None => {
            errors.push(
                &config.files.scene,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → scene",
            );
            Vec::new()
        }
    };

    let (rules, rule_meta) = match rules_json {
        Some(text) => parse_rules_json(
            text,
            &config.files.rules,
            &properties,
            &config.files.images,
            &config.files.sounds,
            &config.files.music,
            &mut errors,
        ),
        None => {
            errors.push(
                &config.files.rules,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → rules",
            );
            (RuleSet::default(), Vec::new())
        }
    };

    // «Код игры» → «Проверка перед запуском»: файл кода выполняется один раз здесь, в свежем,
    // одноразовом исполнителе — мира ещё нет, а `rng`/`messages` одноразовые: `print` и
    // `math.random` этого прогона в игру не идут (пункт 3). Успешный прогон отдаёт только имена
    // объявленных функций — сам исполнитель отбрасывается; настоящий, на партию, строит `Game::
    // new`/`new_game` заново («каждая партия создаёт свежий исполнитель»).
    let image_names: Vec<String> = config.files.images.iter().map(|d| d.name.clone()).collect();
    let sound_names: Vec<String> = config.files.sounds.iter().map(|(n, _)| n.clone()).collect();
    let declared_functions: Option<Vec<String>> = match (&config.files.code, code_json) {
        (Some(path), None) => {
            errors.push(
                path,
                "",
                "файл не найден; ожидался файл кода на Lua, названный в game.json → files → code",
            );
            None
        }
        (Some(path), Some(text)) => {
            // «Код игры» → «Проверка перед запуском»: на зерне игры, не на захардкоженном 0 —
            // иначе `if math.random(...) == ... then error(...) end` мог пройти проверку и упасть
            // уже при старте партии, где используется настоящее зерно.
            let mut check_rng = crate::core::rng::Rng::new(config.random_seed);
            let mut check_messages = Vec::new();
            match crate::core::code::Runner::compile(
                text,
                path,
                &properties,
                &image_names,
                &sound_names,
                &mut check_rng,
                &mut check_messages,
            ) {
                Ok(runner) => Some(runner.declared_functions().to_vec()),
                Err(err) => {
                    errors.push_at(path, "", err.message, err.line.map(|l| l as usize));
                    None
                }
            }
        }
        (None, _) => None,
    };
    validate_run_actions(
        &rules,
        &rule_meta,
        &config.files.rules,
        config.files.code.is_some(),
        declared_functions.as_deref(),
        &mut errors,
    );

    let parsed_screens = match screens_json {
        Some(text) => parse_screens_json(text, &config.files.screens, &properties, &mut errors),
        None => {
            errors.push(
                &config.files.screens,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → screens",
            );
            Vec::new()
        }
    };
    validate_font_files(&config.files.fonts, font_bytes, &mut errors);
    validate_sound_files(&config.files.sounds, sound_bytes, &mut errors);
    validate_image_files(&config.files.images, image_data, &mut errors);
    // «Звук» → «Загрузка и проверка»: captured from the raw parse,
    // before `resolve_screens` resolves each screen's `music` name, so an unknown or malformed
    // name still counts as "referenced" here — its own error comes from `resolve_music` regardless.
    let referenced_music: std::collections::HashSet<String> = parsed_screens
        .iter()
        .filter_map(|s| s.music.clone())
        .collect();
    validate_music_files(
        &config.files.music,
        &referenced_music,
        music_verdicts,
        &mut errors,
    );
    let screens_config = resolve_screens(
        &parsed_screens,
        &config.files.screens,
        &config.files.scene,
        &config.start_screen,
        config.win_screen.as_deref(),
        config.loss_screen.as_deref(),
        &config.files.fonts,
        &config.files.images,
        &config.files.sounds,
        &config.files.music,
        &scene_objects,
        &rules,
        &mut errors,
    );

    let shapes = possible_shapes(
        &scene_objects,
        &config.files.scene,
        &config.files.rules,
        &rules,
        &rule_meta,
        &properties,
    );
    validate_property_sufficiency(
        &shapes,
        &rules,
        &rule_meta,
        &config.files.rules,
        &properties,
        &mut errors,
    );
    validate_opacity_reachable_image(&shapes, code_json, &mut errors);
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
        validate_screen_key_collisions(
            &parsed_screens,
            &config.files.screens,
            &scene_objects,
            &config.files.scene,
            &mut errors,
        );
        validate_unused_properties(
            &properties,
            &config.files.properties,
            &scene_objects,
            &rules,
            code_json,
            &mut errors,
        );
        validate_objects_within_scene(
            &scene_objects,
            &config.files.scene,
            &config.scene,
            &mut errors,
        );
        validate_selectors_not_empty(
            &shapes,
            &rules,
            &rule_meta,
            &config.files.rules,
            &properties,
            &mut errors,
        );
        validate_unused_sounds(&config.files.sounds, &rules, code_json, &mut errors);
        validate_unreferenced_tracks(&config.files.music, &referenced_music, &mut errors);
        // `screens_config` is `Some` whenever no error has been pushed — `resolve_screens` only
        // returns `None` by failing to resolve `start_screen`, which always pushes one.
        if let Some(sc) = &screens_config {
            validate_toggle_sound_presence(&rules, &sc.screens, &config.files.screens, &mut errors);
            validate_unused_images(
                &config.files.images,
                &scene_objects,
                &rules,
                &sc.screens,
                code_json,
                &mut errors,
            );
        }
    }

    // One second pass per file, after every message about it has been pushed: a message from
    // `validate_*` above (e.g. `config.files.rules` ← property sufficiency) needs the same file
    // text a parse-time message did, so filling locations any earlier would miss it. `game.json`
    // itself is included here too — `start_screen`/`win_screen`/`loss_screen` and a missing
    // font/sound/track file are only known to be wrong once `resolve_screens`/`validate_*_files`
    // run, well after `read_entry`'s own pass over this same text.
    errors.fill_locations("game.json", game_json);
    if let Some(text) = properties_json {
        errors.fill_locations(&config.files.properties, text);
    }
    if let Some(text) = scene_json {
        errors.fill_locations(&config.files.scene, text);
    }
    if let Some(text) = rules_json {
        errors.fill_locations(&config.files.rules, text);
    }
    if let Some(text) = screens_json {
        errors.fill_locations(&config.files.screens, text);
    }

    let (errs, warnings) = errors.into_parts();
    if !errs.is_empty() {
        return Err(LoadFailure {
            errors: errs,
            warnings,
        });
    }
    let screens_config = screens_config.expect("no errors means screens.json resolved");

    let scene_specs: Vec<ObjectSpec> = scene_objects
        .iter()
        .map(|parsed| ObjectSpec {
            values: parsed.values.clone(),
            grid: parsed.grid,
            keys: parsed.keys.clone(),
        })
        .collect();

    // «Экраны и состояние»: миру родиться только если у стартового экрана поднят world_runs —
    // на экране меню его просто нет, пока игрок не нажмёт «Играть».
    let start_is_live = screens_config.screens[screens_config.start_screen].world_runs;
    let mut world = World::new(&properties);
    if start_is_live {
        for spec in &scene_specs {
            let id = world.create();
            for (prop, value) in &spec.values {
                world.set_value(id, *prop, value);
            }
            if let Some(grid) = &spec.grid {
                world.set_grid(id, property::GRID, *grid);
                world.set_grid_counter(id, grid.interval_steps);
            }
            if let Some(table) = &spec.keys {
                world.set_keys(id, property::KEYS, table.clone());
            }
        }
    }

    let sound_count = config.files.sounds.len();
    let game = Game::new(
        properties,
        world,
        rules,
        config.scene,
        config.max_objects,
        config.random_seed,
        scene_specs,
        sound_count,
        code_json.map(str::to_string),
        config.files.code.clone().unwrap_or_default(),
        image_names,
        sound_names,
        start_is_live,
    );
    Ok((game, screens_config, warnings))
}

/// Convenience for tests and native tools: loads all five files at once, doing the same
/// validation `read_entry` + `load_rest` would, in one call. Warnings from both stages are
/// merged, `read_entry`'s first, rather than one stage's warnings silently winning. Passes no
/// font bytes — callers that need to exercise font-file validation call `load_rest` directly.
pub fn load_game_from_texts(
    game_json: &str,
    properties_json: &str,
    scene_json: &str,
    rules_json: &str,
    screens_json: &str,
) -> Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure> {
    load_game_from_texts_with_code(
        game_json,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        None,
    )
}

/// Same as `load_game_from_texts`, but also runs `files.code`'s prestart check and load — for
/// tests that exercise «Код игры» without going through the wasm layer's three-round handshake.
#[allow(clippy::too_many_arguments)]
pub fn load_game_from_texts_with_code(
    game_json: &str,
    properties_json: &str,
    scene_json: &str,
    rules_json: &str,
    screens_json: &str,
    code_json: Option<&str>,
) -> Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure> {
    let (config, entry_warnings) = read_entry(game_json)?;
    match load_rest(
        game_json,
        config,
        Some(properties_json),
        Some(scene_json),
        Some(rules_json),
        Some(screens_json),
        &[],
        &[],
        &[],
        &[],
        code_json,
    ) {
        Ok((game, screens, warnings)) => {
            let mut all_warnings = entry_warnings;
            all_warnings.extend(warnings);
            Ok((game, screens, all_warnings))
        }
        Err(mut failure) => {
            let mut all_warnings = entry_warnings;
            all_warnings.extend(std::mem::take(&mut failure.warnings));
            failure.warnings = all_warnings;
            Err(failure)
        }
    }
}

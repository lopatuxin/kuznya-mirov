use std::rc::Rc;
use std::sync::Arc;

use serde_json::Value as Json;

use crate::core::camera;
use crate::core::clouds::CloudMisfit;
use crate::core::code::ObjectParser;
use crate::core::footprint::Footprint;
use crate::core::game::{self, Game};
use crate::core::imprints::StampTable;
use crate::core::keys::{EditValue, KeyBinding, KeyEdit, KeyTable};
use crate::core::particles::Settings;
use crate::core::property::{self, PropertyId, PropertyTable};
use crate::core::roads::Side;
use crate::core::rules::{
    CollideEffect, CommonAction, CompareOp, Condition, NumberExpr, Outcome, Rule, RuleSet,
    Selector, SetValue, ShiftSpec, SoundId, SpawnCell, SpawnCondition, SpawnPlace, SpawnVariant,
    TemplateValue, TurnDir, TurnSpec,
};
use crate::core::scene::{
    CameraConfig, GroundLayer, LightConfig, ObjectSpec, PlayerCameraConfig, Projection, SceneConfig,
};
use crate::core::screens::{
    Align, Anchor, ButtonCommand, ChoiceRef, ChoiceTable, Element, Fill, FontId, ImageRef,
    InitialValue, MaxSpec, MusicId, PanelFill, Placement, RunCall, RunId, Screen, ScreenId,
    ScreenKeyTable, ScreensConfig, TextPart, TooltipStyle, WorldColor, WorldElement,
    WorldElementKind, WorldTextPart,
};
use crate::core::terrain::{DENSITIES, Terrain};
use crate::core::time::{seconds_to_steps, seconds_to_steps_delta};
use crate::core::value::{
    FollowAxis, GridSpec, ImageId, PropKind, Rotation, Value, Vec2, parse_color,
};
use crate::core::world::World;

use super::error::ErrorSink;
use super::wav;

mod imported;
mod materials;
mod models;
mod motions;
mod road_kinds;
mod stamps;
mod water;

pub use super::error::{GameError, LoadFailure};
pub use imported::ImportedPlan;
pub use materials::{
    CoverMask, MaterialDecl, check_cover_masks, parse_edit_covers, terrain_image_paths,
};
use materials::{
    declaration_order, parse_covers, parse_materials_table, validate_material_files,
    warn_unused_materials,
};
pub use models::{ModelDecl, ModelFile};
use models::{load_models, named_in_scene, parse_models_table, warn_unused_models};
pub use motions::{END_GAP_LIMIT, rebuild_skeleton};
use motions::{MotionSet, parse_motions, warn_unused_motions};
pub use road_kinds::{JunctionArmDecl, JunctionDecl, RoadKindDecl};
use road_kinds::{
    build_road_kinds, parse_road_junctions, parse_road_kinds, validate_junctions,
    validate_kind_models, warn_unused_road_kinds,
};
pub use stamps::parse_edit_imprints;
use stamps::{parse_imprints, parse_stamp_table, warn_unused_stamps};
use water::{parse_edit_water, parse_water};

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
    reject_unknown_keys_except(obj, known, &[], file, path, errors);
}

/// То же, но ключи `tolerated` не считаются неизвестными и среди ожидаемых не названы: их разбирает
/// и отвергает отдельная проверка со своим текстом.
fn reject_unknown_keys_except(
    obj: &serde_json::Map<String, Json>,
    known: &[&str],
    tolerated: &[&str],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    let unknown_keys: Vec<&String> = obj
        .keys()
        .filter(|key| !known.contains(&key.as_str()) && !tolerated.contains(&key.as_str()))
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

/// Scene, rules and `game.json` colors: `#rrggbb` only, without the opacity pair.
fn parse_hex_color(s: &str) -> Option<[f32; 4]> {
    parse_color(s).filter(|_| s.len() == 7)
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

const HEIGHT_OF_PLACE_IN_FLAT_SCENE: &str =
    "третье число есть только в трёхмерной сцене: у scene в game.json нет camera";

/// «Рельеф», требования 6, 38: `position` и `walk_to` — `[x, y]` или, в трёхмерной сцене,
/// `[x, y, z]`, где `z` — высота основания; любое число, но не больше трёх.
fn parse_place(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Value> {
    let arr = expect_array(value, file, path, errors)?;
    match arr.len() {
        2 => parse_vec2(value, file, path, errors).map(Value::Vec2),
        3 if properties.three_d() => {
            let x = expect_number(&arr[0], file, &join(path, "[0]"), errors)?;
            let y = expect_number(&arr[1], file, &join(path, "[1]"), errors)?;
            let z = expect_number(&arr[2], file, &join(path, "[2]"), errors)?;
            Some(Value::Vec3([x, y, z]))
        }
        3 => {
            errors.push(file, path, HEIGHT_OF_PLACE_IN_FLAT_SCENE);
            None
        }
        n => {
            let expected = if properties.three_d() {
                "пара или тройка чисел"
            } else {
                "пара чисел"
            };
            errors.push(file, path, format!("ожидалась {expected}, элементов: {n}"));
            None
        }
    }
}

/// Parses one scalar value against the kind declared for `prop`. Time values are converted to
/// steps right here, so nothing downstream ever sees seconds again. `prop` itself (not just its
/// kind) matters once: `size` may not be negative, everything else with the same kind can be.
pub(crate) fn parse_scalar_value(
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
        PropKind::Flag => {
            if prop == property::DECK && !properties.three_d() {
                errors.push(file, path, DECK_IN_FLAT_SCENE);
                return None;
            }
            if prop == property::ROAD && !properties.three_d() {
                errors.push(file, path, ROAD_IN_FLAT_SCENE);
                return None;
            }
            if !depth_layer_fits_the_scene(prop, properties, file, path, errors)
                || !engine_alone_writes_ok(prop, file, path, errors)
            {
                return None;
            }
            value.as_bool().map(Value::Flag).or_else(|| {
                errors.push(
                    file,
                    path,
                    format!("ожидался признак (true), получено {}", kind_name(value)),
                );
                None
            })
        }
        PropKind::Number => {
            let n = expect_number(value, file, path, errors)?;
            if prop == property::OPACITY && !validate_opacity_range(n, file, path, errors) {
                return None;
            }
            if prop == property::HEIGHT && !validate_height(n, properties, file, path, errors) {
                return None;
            }
            if prop == property::PARALLAX && !validate_parallax(n, properties, file, path, errors) {
                return None;
            }
            if prop == property::SWAY && !validate_sway(n, properties, file, path, errors) {
                return None;
            }
            if prop == property::TIME_SCALE && !validate_time_scale(n, file, path, errors) {
                return None;
            }
            if RUN_JUMP_NUMBERS.contains(&prop)
                && !validate_run_jump_number(prop, n, properties, file, path, errors)
            {
                return None;
            }
            if DRIVE_NUMBERS.contains(&prop)
                && !validate_drive_number(prop, n, properties, file, path, errors)
            {
                return None;
            }
            if prop == property::CLIMB_SPEED && !validate_climb_speed(n, file, path, errors) {
                return None;
            }
            if prop == property::BEND && !validate_bend(n, properties, file, path, errors) {
                return None;
            }
            if (property::PARTICLE_PROPERTIES.contains(&prop)
                || property::FIRE_PROPERTIES.contains(&prop)
                || prop == property::CLOUDS)
                && !validate_particle_number(prop, n, properties, file, path, errors)
            {
                return None;
            }
            Some(Value::Number(n))
        }
        PropKind::Time => {
            expect_number(value, file, path, errors).map(|s| Value::Time(seconds_to_steps(s)))
        }
        // «Свойства» → `timer`: секунды, неотрицательное число, без нижней границы в один шаг —
        // `seconds_to_steps_delta`, не `seconds_to_steps`, и отрицательное в файле — ошибка (не
        // просто клампится, как во время партии).
        PropKind::Timer => {
            let s = expect_number(value, file, path, errors)?;
            if s < 0.0 {
                errors.push(
                    file,
                    path,
                    format!("timer не может быть отрицательным, получено {s}"),
                );
                return None;
            }
            Some(Value::Timer(seconds_to_steps_delta(s)))
        }
        PropKind::Rotation if prop == property::FACING && !properties.three_d() => {
            errors.push(file, path, FACING_IN_FLAT_SCENE);
            None
        }
        PropKind::Rotation if properties.three_d() => {
            let n = expect_number(value, file, path, errors)?;
            Rotation::from_degrees(n).map(Value::Rotation).or_else(|| {
                errors.push(
                    file,
                    path,
                    format!("rotation должен быть числом, получено {n}"),
                );
                None
            })
        }
        PropKind::Rotation => {
            let n = expect_number(value, file, path, errors)?;
            match Rotation::from_degrees_exact(n) {
                Some(r) => Some(Value::Rotation(r)),
                None => {
                    errors.push(
                        file,
                        path,
                        format!("rotation должен быть 0, 90, 180 или 270, получено {n}"),
                    );
                    None
                }
            }
        }
        PropKind::Shape => {
            if !properties.three_d() {
                errors.push(file, path, SHAPE_IN_FLAT_SCENE);
                return None;
            }
            let s = expect_string(value, file, path, errors)?;
            crate::core::value::Shape::parse(&s)
                .map(Value::Shape).or_else(|| {
                errors.push(
                    file,
                    path,
                    format!(
                        "неизвестная фигура \"{s}\"; ожидалась одна из: box, cylinder, capsule, sphere"
                    ),
                );
                None
            })
        }
        PropKind::FollowMouse => {
            let s = expect_string(value, file, path, errors)?;
            match FollowAxis::parse(&s) {
                Some(a) => Some(Value::FollowMouse(a)),
                None => {
                    errors.push(
                        file,
                        path,
                        format!(
                            "follow_mouse должен быть \"x\", \"y\" или \"xy\", получено \"{s}\""
                        ),
                    );
                    None
                }
            }
        }
        PropKind::Vec2 if prop == property::POSITION || prop == property::WALK_TO => {
            parse_place(value, properties, file, path, errors)
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
            if !depth_layer_fits_the_scene(prop, properties, file, path, errors) {
                return None;
            }
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
        PropKind::Text if prop == property::MODEL => {
            if !properties.three_d() {
                errors.push(file, path, MODEL_IN_FLAT_SCENE);
                return None;
            }
            let name = expect_string(value, file, path, errors)?;
            resolve_model(&name, properties, file, path, errors).map(Value::Text)
        }
        PropKind::Text if prop == property::ANIMATION => {
            if !properties.three_d() {
                errors.push(file, path, ANIMATION_IN_FLAT_SCENE);
                return None;
            }
            let name = expect_string(value, file, path, errors)?;
            resolve_animation(&name, properties, file, path, errors).map(Value::Text)
        }
        PropKind::Text if prop == property::ROAD_KIND => {
            if !properties.three_d() {
                errors.push(file, path, ROAD_KIND_IN_FLAT_SCENE);
                return None;
            }
            let name = expect_string(value, file, path, errors)?;
            resolve_road_kind(&name, properties, file, path, errors).map(Value::Text)
        }
        PropKind::Text => expect_string(value, file, path, errors).map(Value::Text),
        PropKind::Image => {
            let name = expect_string(value, file, path, errors)?;
            resolve_image(&name, images, file, path, errors).map(Value::Image)
        }
        PropKind::ImageList => {
            if !depth_layer_fits_the_scene(prop, properties, file, path, errors) {
                return None;
            }
            parse_image_list(value, properties, images, file, path, errors).map(Value::ImageList)
        }
        PropKind::Grid | PropKind::Keys | PropKind::OnClick => {
            errors.push(
                file,
                path,
                "это свойство не задаётся простым значением".to_string(),
            );
            None
        }
    }
}

/// «Ветер и частицы» → «Проверка перед запуском»: `cloud_images` — список имён объявленных картинок, и
/// каждая годится облакам (`PropertyTable::cloud_misfit`). Пустой список допустим.
fn parse_image_list(
    value: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<ImageId>> {
    let Some(items) = value.as_array() else {
        errors.push(
            file,
            path,
            format!(
                "ожидался список имён картинок, получено {}",
                kind_name(value)
            ),
        );
        return None;
    };
    let mut list = Vec::with_capacity(items.len());
    let mut fits = true;
    for (index, item) in items.iter().enumerate() {
        let item_path = join(path, &format!("[{index}]"));
        let Some(name) = expect_string(item, file, &item_path, errors) else {
            fits = false;
            continue;
        };
        let Some(image) = resolve_image(&name, images, file, &item_path, errors) else {
            fits = false;
            continue;
        };
        if let Some(misfit) = properties.cloud_misfit(image) {
            errors.push(
                file,
                &item_path,
                format!(
                    "картинка \"{name}\" не годится облакам: {}",
                    misfit.reason()
                ),
            );
            fits = false;
            continue;
        }
        list.push(image);
    }
    fits.then_some(list)
}

pub(crate) fn parse_grid(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<GridSpec> {
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
        if let Some(value) = parse_edit_value(
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
                value,
            });
        }
    }
    edits
}

/// «Мышь в мире», требования 17, 35–36: one record's value — `"cursor"` only when `prop` is a
/// `Vec2` (иначе — ошибка требования 36: «cursor в записи к свойству, которое не пара»), an
/// ordinary constant otherwise, parsed exactly like a scene object's own field.
fn parse_edit_value(
    value: &Json,
    prop: PropertyId,
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<EditValue> {
    if value.as_str() == Some("cursor") {
        return if properties.kind(prop) == PropKind::Vec2 {
            Some(EditValue::Cursor)
        } else {
            errors.push(
                file,
                path,
                "\"cursor\" можно только у свойства-пары".to_string(),
            );
            None
        };
    }
    parse_scalar_value(value, prop, properties, images, file, path, errors).map(EditValue::Const)
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
    pub on_click: Option<Vec<KeyEdit>>,
    pub shape: std::collections::HashSet<PropertyId>,
    /// Properties whose own field value failed to parse (wrong JSON kind, a broken `grid`/`keys`
    /// sub-object, …). Such a property's presence is known-incomplete for a reason already
    /// reported elsewhere, so `require` skips only that property on this object rather than
    /// compounding one data error into two messages — see `validate_property_sufficiency`. A key
    /// that didn't resolve to any property at all isn't tracked here: there's no `PropertyId` to
    /// skip a check for, and `require` never checks against one that doesn't exist.
    pub broken_properties: std::collections::HashSet<PropertyId>,
}

/// «Исполнение игры» → «Шаг и кадр»: `time_scale` — число от 0 до `MAX_TIME_SCALE`.
fn validate_time_scale(n: f64, file: &str, path: &str, errors: &mut ErrorSink) -> bool {
    if (0.0..=property::MAX_TIME_SCALE).contains(&n) {
        return true;
    }
    errors.push(
        file,
        path,
        format!(
            "свойство \"time_scale\" должно быть от 0 до {}, получено {n}",
            property::MAX_TIME_SCALE
        ),
    );
    false
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

const MODEL_IN_FLAT_SCENE: &str =
    "свойство \"model\" есть только в трёхмерной сцене: у scene в game.json нет camera";

/// Модель по имени: `model` называет только объявленную в `files.models`.
fn resolve_model(
    name: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<String> {
    let models = properties.models();
    if models.index_of(name).is_some() {
        return Some(name.to_string());
    }
    let declared = models
        .names()
        .map(|known| format!("\"{known}\""))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(
        file,
        path,
        if models.is_empty() {
            format!("модели \"{name}\" нет; модели не объявлены")
        } else {
            format!("модели \"{name}\" нет; объявлены {declared}")
        },
    );
    None
}

const FACING_IN_FLAT_SCENE: &str =
    "свойство \"facing\" есть только в трёхмерной сцене: у scene в game.json нет camera";

const ANIMATION_IN_FLAT_SCENE: &str =
    "свойство \"animation\" есть только в трёхмерной сцене: у scene в game.json нет camera";

/// Движение по имени: `animation` называет движение из файла какой-нибудь модели игры; пустое имя возвращает
/// объект к `idle` и `walk`.
fn resolve_animation(
    name: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<String> {
    let skeletons = || {
        properties
            .models()
            .iter()
            .filter_map(|model| model.skeleton.as_ref())
    };
    if name.is_empty() || skeletons().any(|skeleton| skeleton.clip_named(name).is_some()) {
        return Some(name.to_string());
    }
    let known: Vec<String> = skeletons()
        .flat_map(|skeleton| &skeleton.clips)
        .map(|clip| format!("\"{}\"", clip.name))
        .collect();
    errors.push(
        file,
        path,
        if known.is_empty() {
            format!("движения \"{name}\" нет ни в одной модели игры; движений в моделях нет")
        } else {
            format!(
                "движения \"{name}\" нет ни в одной модели игры; в моделях есть {}",
                known.join(", ")
            )
        },
    );
    None
}

/// `animation` объекта с `model` в данных названо в файле его модели.
fn validate_animation_of_model(
    animation: Option<&Value>,
    model: Option<&Value>,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    let (Some(Value::Text(animation)), Some(Value::Text(model_name))) = (animation, model) else {
        return;
    };
    let models = properties.models();
    let Some(model) = models.index_of(model_name).and_then(|at| models.get(at)) else {
        return;
    };
    if animation.is_empty() {
        return;
    }
    match &model.skeleton {
        Some(skeleton) if skeleton.clip_named(animation).is_some() => {}
        Some(_) => errors.push(
            file,
            path,
            format!("в модели \"{model_name}\" нет движения \"{animation}\" из animation"),
        ),
        None => errors.push(
            file,
            path,
            format!("animation у объекта с моделью \"{model_name}\", а у модели нет скелета"),
        ),
    }
}

/// `animation`, которое клавиши и `on_click` объекта пишут константой, названо в файле модели объекта, если она
/// задана в данных константой.
fn validate_edited_animations(
    keys: Option<&KeyTable>,
    on_click: &[KeyEdit],
    model: Option<&Value>,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    let mut check = |edits: &[KeyEdit], at: String| {
        for edit in edits.iter().filter(|e| e.property == property::ANIMATION) {
            if let EditValue::Const(value) = &edit.value {
                validate_animation_of_model(Some(value), model, properties, file, &at, errors);
            }
        }
    };
    let mut bindings: Vec<_> = keys.into_iter().flatten().collect();
    bindings.sort_by_key(|(code, _)| code.as_str());
    for (code, binding) in bindings {
        let at = join(&join(path, "keys"), code);
        check(&binding.press, join(&at, "press"));
        check(&binding.release, join(&at, "release"));
    }
    check(on_click, join(path, "on_click"));
}

/// Значение `prop` в полях уровней `layers` (верхний последним): записанное константой; `None`, если его нет
/// или оно берётся у родителя и до запуска неизвестно.
fn layered_const<'a>(
    layers: &[&'a [(PropertyId, TemplateValue)]],
    prop: PropertyId,
) -> Option<&'a Value> {
    let (_, value) = layers
        .iter()
        .rev()
        .find_map(|fields| fields.iter().find(|(field, _)| *field == prop))?;
    match value {
        TemplateValue::Const(value) => Some(value),
        TemplateValue::FromParent(_) => None,
    }
}

/// `animation` создаваемого правилом объекта названо в файле модели того же объекта. Поля идут template → вариант
/// → клетка; сочетание проверяется у каждого уровня, что называет `model` или `animation`.
fn validate_spawned_animations(
    template: &[(PropertyId, TemplateValue)],
    variants: &[SpawnVariant],
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    let mut check = |layers: &[&[(PropertyId, TemplateValue)]], at: String| {
        validate_animation_of_model(
            layered_const(layers, property::ANIMATION),
            layered_const(layers, property::MODEL),
            properties,
            file,
            &at,
            errors,
        );
    };
    check(&[template], join(path, "template"));
    let names = |fields: &[(PropertyId, TemplateValue)]| {
        fields
            .iter()
            .any(|(prop, _)| matches!(*prop, property::MODEL | property::ANIMATION))
    };
    for (i, variant) in variants.iter().enumerate() {
        let variant_path = join(&join(path, "pick_one"), &format!("[{i}]"));
        if names(&variant.fields) {
            check(&[template, &variant.fields], variant_path.clone());
        }
        for (j, cell) in variant.cells.iter().enumerate() {
            if names(&cell.fields) {
                let cell_path = join(&join(&variant_path, "cells"), &format!("[{j}]"));
                check(&[template, &variant.fields, &cell.fields], cell_path);
            }
        }
    }
}

const ROAD_KIND_IN_FLAT_SCENE: &str =
    "свойство \"road_kind\" есть только в трёхмерной сцене: у scene в game.json нет camera";

/// Вид дороги по имени: `road_kind` называет только вид из `scene.road_kinds`.
fn resolve_road_kind(
    name: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<String> {
    let kinds = properties.road_kinds();
    if kinds.index_of(name).is_some() {
        return Some(name.to_string());
    }
    let declared = kinds
        .names()
        .map(|known| format!("\"{known}\""))
        .collect::<Vec<_>>()
        .join(", ");
    errors.push(
        file,
        path,
        if kinds.is_empty() {
            format!("вида дороги \"{name}\" нет; виды не объявлены")
        } else {
            format!("вида дороги \"{name}\" нет; объявлены {declared}")
        },
    );
    None
}

const SHAPE_IN_FLAT_SCENE: &str =
    "shape есть только в трёхмерной сцене: у scene в game.json нет camera";

const WIND_IN_3D_SCENE: &str = "wind есть только в плоской сцене: у scene в game.json есть camera";

const DECK_IN_FLAT_SCENE: &str =
    "deck есть только в трёхмерной сцене: у scene в game.json нет camera";

const ROAD_IN_FLAT_SCENE: &str =
    "road есть только в трёхмерной сцене: у scene в game.json нет camera";

/// «Мир на экране» → «Проверка перед запуском»: `parallax` и `repeat_x` есть только в плоской
/// сцене — везде, где их можно записать: объект, шаблон, клавиша, `on_click`, правила. «Ветер и
/// частицы» → «Проверка перед запуском»: `sway` и свойства частиц — так же.
fn depth_layer_fits_the_scene(
    prop: PropertyId,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if (matches!(
        prop,
        property::PARALLAX | property::REPEAT_X | property::SWAY
    ) || property::PARTICLE_PROPERTIES.contains(&prop)
        || property::FIRE_PROPERTIES.contains(&prop)
        || property::CLOUD_PROPERTIES.contains(&prop))
        && properties.three_d()
    {
        errors.push(
            file,
            path,
            format!(
                "{} есть только в плоской сцене: у scene в game.json есть camera",
                properties.name(prop)
            ),
        );
        return false;
    }
    true
}

/// «Мир на экране» → «Слои глубины»: `parallax` — число не меньше нуля, и только в плоской сцене.
fn validate_parallax(
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if !depth_layer_fits_the_scene(property::PARALLAX, properties, file, path, errors) {
        return false;
    }
    if n >= 0.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!("parallax не может быть меньше нуля, получено {n}"),
    );
    false
}

/// «Ветер и частицы» → «Качание»: `sway` — число не меньше нуля, и только в плоской сцене.
fn validate_sway(
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if !depth_layer_fits_the_scene(property::SWAY, properties, file, path, errors) {
        return false;
    }
    if n >= 0.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!("sway не может быть меньше нуля, получено {n}"),
    );
    false
}

/// «Бег и прыжок» → «Проверка перед запуском»: числа героя — `run_speed` не меньше нуля, `jump_height` и
/// `jump_time` больше нуля.
const RUN_JUMP_NUMBERS: [PropertyId; 3] = [
    property::RUN_SPEED,
    property::JUMP_HEIGHT,
    property::JUMP_TIME,
];

fn validate_run_jump_number(
    prop: PropertyId,
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let name = properties.name(prop);
    if prop == property::RUN_SPEED {
        if n >= 0.0 {
            return true;
        }
        errors.push(
            file,
            path,
            format!("свойство \"{name}\" не может быть меньше нуля, получено {n}"),
        );
        return false;
    }
    if n > 0.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!("свойство \"{name}\" должно быть больше нуля, получено {n}"),
    );
    false
}

/// «Езда по дорогам» → «Проверка перед запуском»: изгиб оси `bend` — число меньше 180° по модулю и только в
/// трёхмерной сцене.
fn validate_bend(
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if !properties.three_d() {
        errors.push(file, path, BEND_IN_FLAT_SCENE);
        return false;
    }
    if n.abs() < 180.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!("свойство \"bend\" должно быть меньше 180 градусов по модулю, получено {n}"),
    );
    false
}

const BEND_IN_FLAT_SCENE: &str =
    "свойство \"bend\" есть только в трёхмерной сцене: у scene в game.json нет camera";

/// «Езда по дорогам» → «Проверка перед запуском»: `drive_speed` и `road_speed` не меньше нуля.
const DRIVE_NUMBERS: [PropertyId; 2] = [property::DRIVE_SPEED, property::ROAD_SPEED];

fn validate_drive_number(
    prop: PropertyId,
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if n >= 0.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!(
            "свойство \"{}\" не может быть меньше нуля, получено {n}",
            properties.name(prop)
        ),
    );
    false
}

/// «Лазание» → «Проверка перед запуском»: `climb_speed` больше нуля.
fn validate_climb_speed(n: f64, file: &str, path: &str, errors: &mut ErrorSink) -> bool {
    if n > 0.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!("свойство \"climb_speed\" должно быть больше нуля, получено {n}"),
    );
    false
}

/// «Бег и прыжок», «Лазание» → «Проверка перед запуском»: `on_ground` и `climbing` пишет только движок — ни файл,
/// ни клавиша, ни правило, ни шаблон их не ставят и не снимают.
fn engine_alone_writes_ok(
    prop: PropertyId,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let name = match prop {
        property::ON_GROUND => "on_ground",
        property::CLIMBING => "climbing",
        _ => return true,
    };
    errors.push(
        file,
        path,
        format!(
            "признак \"{name}\" пишет только движок: его нельзя задать ни в файле, ни клавишей, ни правилом"
        ),
    );
    false
}

/// «Ветер и частицы» → «Проверка перед запуском», «Огонь» → «Проверка перед запуском»: числовое свойство
/// частиц, облаков и огня — только в плоской сцене; `smoke`, `sparks`, `leaf_fall`, `fluff`,
/// `butterflies`, `clouds`, `fire`, `fire_glow`, `fire_coals` и `fire_smolder` — от 0 до 1,
/// `smoke_height` и `sparks_reach` — больше нуля, `sparks_spread` — от 0 до 180, `sparks_direction` —
/// любое число.
fn validate_particle_number(
    prop: PropertyId,
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    depth_layer_fits_the_scene(prop, properties, file, path, errors)
        && particle_number_fits(prop, n, properties.name(prop), file, path, errors)
}

/// Значение числового свойства частиц, облаков или огня `name` лежит в своих пределах — у объекта сцены и
/// у панели экрана одни и те же.
fn particle_number_fits(
    prop: PropertyId,
    n: f64,
    name: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let range = match prop {
        property::SMOKE
        | property::SPARKS
        | property::LEAF_FALL
        | property::FLUFF
        | property::BUTTERFLIES
        | property::CLOUDS
        | property::FIRE
        | property::FIRE_GLOW
        | property::FIRE_COALS
        | property::FIRE_SMOLDER => Some((0.0, 1.0)),
        property::SPARKS_SPREAD => Some((0.0, 180.0)),
        _ => None,
    };
    let fits = match (prop, range) {
        (_, Some((low, high))) => (low..=high).contains(&n),
        (property::SMOKE_HEIGHT | property::SPARKS_REACH, None) => n > 0.0,
        _ => true,
    };
    if fits {
        return true;
    }
    let message = match range {
        Some((low, high)) => {
            format!("{name}: нужно от {low} до {high} включительно, получено {n}")
        }
        None => format!("{name}: нужно больше нуля, получено {n}"),
    };
    errors.push(file, path, message);
    false
}

/// «Ветер и частицы» → «Проверка перед запуском»: свойства частиц — только у объекта с `position` и `size`, и
/// не с `repeat_x`. `check_position` — как у `validate_follow_mouse_shape`.
fn validate_particles_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    for message in particles_shape_errors(shape, check_position) {
        errors.push(file, path, message);
    }
}

const PARTICLES_NEED_POSITION_AND_SIZE: &str =
    "свойства частиц разрешены только объекту с position и size";
const PARTICLES_WITH_REPEAT_X: &str = "свойства частиц заданы вместе с repeat_x: частицы вылетают из одного места, повторить его нельзя";

/// Тексты нарушений формы объекта со свойствами частиц — общие для загрузки и правки на ходу.
pub(super) fn particles_shape_errors(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
) -> Vec<&'static str> {
    let mut messages = Vec::new();
    if !property::PARTICLE_PROPERTIES
        .iter()
        .any(|prop| shape.contains(prop))
    {
        return messages;
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        messages.push(PARTICLES_NEED_POSITION_AND_SIZE);
    }
    if shape.contains(&property::REPEAT_X) {
        messages.push(PARTICLES_WITH_REPEAT_X);
    }
    messages
}

/// «Ветер и частицы» → «Проверка перед запуском»: свойства облаков — только у объекта с `position`,
/// `size` и `repeat_x`. `check_position` — как у `validate_follow_mouse_shape`.
fn validate_clouds_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    for message in clouds_shape_errors(shape, check_position) {
        errors.push(file, path, message);
    }
}

const CLOUDS_NEED_POSITION_AND_SIZE: &str =
    "свойства облаков разрешены только объекту с position и size";
const CLOUDS_NEED_REPEAT_X: &str =
    "свойства облаков заданы без repeat_x: облака идут по небу, а небо — объект с repeat_x";

/// Тексты нарушений формы объекта со свойствами облаков — общие для загрузки и правки на ходу.
pub(super) fn clouds_shape_errors(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
) -> Vec<&'static str> {
    let mut messages = Vec::new();
    if !property::CLOUD_PROPERTIES
        .iter()
        .any(|prop| shape.contains(prop))
    {
        return messages;
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        messages.push(CLOUDS_NEED_POSITION_AND_SIZE);
    }
    if !shape.contains(&property::REPEAT_X) {
        messages.push(CLOUDS_NEED_REPEAT_X);
    }
    messages
}

/// «Огонь» → «Проверка перед запуском»: свойства огня — только у объекта с `position` и `size`, и не с
/// `repeat_x`. `check_position` — как у `validate_follow_mouse_shape`.
fn validate_fire_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    for message in fire_shape_errors(shape, check_position) {
        errors.push(file, path, message);
    }
}

const FIRE_NEEDS_POSITION_AND_SIZE: &str =
    "свойства огня разрешены только объекту с position и size";
const FIRE_WITH_REPEAT_X: &str =
    "свойства огня заданы вместе с repeat_x: пламя стоит в одном месте, повторить его нельзя";

/// Тексты нарушений формы объекта со свойствами огня — общие для загрузки и правки на ходу.
pub(super) fn fire_shape_errors(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
) -> Vec<&'static str> {
    let mut messages = Vec::new();
    if !property::FIRE_PROPERTIES
        .iter()
        .any(|prop| shape.contains(prop))
    {
        return messages;
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        messages.push(FIRE_NEEDS_POSITION_AND_SIZE);
    }
    if shape.contains(&property::REPEAT_X) {
        messages.push(FIRE_WITH_REPEAT_X);
    }
    messages
}

/// «Слои глубины», требование 23: объект с `parallax`, отличным от 1, только рисуется — с этими
/// свойствами в одном объекте сцены или шаблона он не уживается. `present` — свойства, которые у
/// объекта заведомо есть.
fn validate_depth_layer_only_draws(
    is_layer: bool,
    present: &std::collections::HashSet<PropertyId>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if !is_layer {
        return;
    }
    for (prop, name) in [
        (property::COLLIDES, "collides"),
        (property::ON_CLICK, "on_click"),
        (property::CAMERA_FOLLOWS, "camera_follows"),
        (property::FOLLOW_MOUSE, "follow_mouse"),
    ] {
        if present.contains(&prop) {
            errors.push(
                file,
                path,
                format!(
                    "{name} задан объекту с parallax, отличным от 1: объект слоя глубины только рисуется"
                ),
            );
        }
    }
}

/// «Рельеф», требование 38: настил — только объект с `position` и `size`. `check_position` — как у
/// `validate_follow_mouse_shape`.
fn validate_deck_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if !shape.contains(&property::DECK) {
        return;
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        errors.push(file, path, "deck разрешён только объекту с position и size");
    }
}

/// «Езда по дорогам», требование 30: дорога — только объект с `position` и `size`. `check_position` —
/// как у `validate_follow_mouse_shape`.
fn validate_road_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if !shape.contains(&property::ROAD) {
        return;
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        errors.push(file, path, "road разрешён только объекту с position и size");
    }
}

/// «Езда по дорогам» → «Проверка перед запуском»: `bend` — только дороге (`road`).
fn validate_bend_shape(
    shape: &std::collections::HashSet<PropertyId>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if shape.contains(&property::BEND) && !shape.contains(&property::ROAD) {
        errors.push(
            file,
            path,
            "свойство \"bend\" задано объекту без road: изгиб есть только у дороги",
        );
    }
}

/// «Езда по дорогам» → «Проверка перед запуском»: радиус дуги оси — хорда, делённая на два синуса половины
/// изгиба, — не меньше ширины дороги.
fn validate_bend_radius(
    values: &[(PropertyId, Value)],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    let (Some(Value::Number(bend)), Some(Value::Vec2([long, short]))) = (
        find_value(values, property::BEND),
        find_value(values, property::SIZE),
    ) else {
        return;
    };
    let (chord, width) = (long.max(*short), long.min(*short));
    let radius = chord / (2.0 * libm::sin(bend.abs().to_radians() / 2.0));
    if *bend != 0.0 && bend.abs() < 180.0 && radius < width {
        errors.push(
            file,
            path,
            format!(
                "радиус дуги {radius} меньше ширины дороги {width}: изгиб {bend} слишком велик для дороги длиной {chord}"
            ),
        );
    }
}

/// «Вид дороги», требование 22: `road_kind` — только дороге (`road`) с `position` и `size`, которая не рисуется
/// ничем другим: ни `color`, ни `image`, ни `shape`, ни `model`. `check_position` — как у
/// `validate_follow_mouse_shape`.
fn validate_road_kind_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if !shape.contains(&property::ROAD_KIND) {
        return;
    }
    if !shape.contains(&property::ROAD) {
        errors.push(
            file,
            path,
            "свойство \"road_kind\" задано объекту без road: вид есть только у дороги",
        );
    }
    for (prop, name) in [
        (property::COLOR, "color"),
        (property::IMAGE, "image"),
        (property::SHAPE, "shape"),
        (property::MODEL, "model"),
    ] {
        if shape.contains(&prop) {
            errors.push(
                file,
                path,
                format!(
                    "свойство \"{name}\" задано объекту со свойством \"road_kind\": дорога вида рисуется кусками своего вида"
                ),
            );
        }
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        errors.push(
            file,
            path,
            "свойство \"road_kind\" разрешено только объекту с position и size",
        );
    }
}

/// «Трёхмерная сцена», требование 30: `height` есть только в трёхмерной сцене — везде, где его можно
/// записать: объект, шаблон, клавиша, `on_click`, `add`/`set` правил.
fn height_fits_the_scene(
    prop: PropertyId,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if prop == property::HEIGHT && !properties.three_d() {
        errors.push(
            file,
            path,
            "свойство \"height\" есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
        return false;
    }
    true
}

/// «Трёхмерная сцена», «Загрузка и проверка»: `height` — число больше нуля и только в трёхмерной
/// сцене; текст ошибки написан один раз для объекта, шаблона, клавиши и правила.
fn validate_height(
    n: f64,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    if !height_fits_the_scene(property::HEIGHT, properties, file, path, errors) {
        return false;
    }
    if n > 0.0 {
        return true;
    }
    errors.push(
        file,
        path,
        format!("свойство \"height\" должно быть больше нуля, получено {n}"),
    );
    false
}

/// «Картинки»: `opacity` без `image` — ошибка данных везде, где может появиться `opacity`:
/// объект, шаблон создания, панель и кнопка. Текст написан один раз здесь и его же берёт
/// `validate_reachable_image`, проверяющая ту же ошибку по расширенной форме объекта.
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
/// `keys` and collide effects — see `validate_reachable_image`: unlike `color`, which a
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

/// «Трёхмерная сцена», требование 30: фигуре нужен цвет и место на земле, картинки, `flip_x` и
/// `opacity` ей не полагаются. `check_position` — как у `validate_follow_mouse_shape`.
fn validate_shape_fill(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if !shape.contains(&property::SHAPE) {
        return;
    }
    if !shape.contains(&property::COLOR) {
        errors.push(
            file,
            path,
            "shape задан без color: цвет — единственная заливка фигуры",
        );
    }
    for (prop, name) in [
        (property::IMAGE, "image"),
        (property::FLIP_X, "flip_x"),
        (property::OPACITY, "opacity"),
    ] {
        if shape.contains(&prop) {
            errors.push(
                file,
                path,
                format!("{name} задан объекту с shape: фигура рисуется цветом"),
            );
        }
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        errors.push(
            file,
            path,
            "shape разрешён только объекту с position и size",
        );
    }
}

/// «Трёхмерная сцена» → «Модели и движения»: модели нужно место на земле, а заливки, картинки, `flip_x` и
/// `opacity` ей не полагаются. `check_position` — как у `validate_follow_mouse_shape`.
fn validate_model_fill(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    if !shape.contains(&property::MODEL) {
        return;
    }
    for (prop, name) in [
        (property::SHAPE, "shape"),
        (property::IMAGE, "image"),
        (property::COLOR, "color"),
        (property::FLIP_X, "flip_x"),
        (property::OPACITY, "opacity"),
    ] {
        if shape.contains(&prop) {
            errors.push(
                file,
                path,
                format!(
                    "свойство \"{name}\" задано объекту со свойством \"model\": модель рисуется своими материалами"
                ),
            );
        }
    }
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if missing_size || missing_position {
        errors.push(
            file,
            path,
            "свойство \"model\" разрешено только объекту с position и size",
        );
    }
}

/// «Свойства», требование 3; «Камера»/«Мышь в мире», требование 36: `follow_mouse`,
/// `camera_follows` и `on_click` разрешены только объекту с `position` и `size`. `check_position`
/// is `false` for a spawn template, whose object always gets `position` from `where` even though
/// the template itself never spells it out.
fn validate_follow_mouse_shape(
    shape: &std::collections::HashSet<PropertyId>,
    check_position: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    let missing_size = !shape.contains(&property::SIZE);
    let missing_position = check_position && !shape.contains(&property::POSITION);
    if !missing_size && !missing_position {
        return;
    }
    for (prop, name) in [
        (property::FOLLOW_MOUSE, "follow_mouse"),
        (property::CAMERA_FOLLOWS, "camera_follows"),
        (property::ON_CLICK, "on_click"),
    ] {
        if shape.contains(&prop) {
            errors.push(
                file,
                path,
                format!("{name} разрешён только объекту с position и size"),
            );
        }
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
    let mut on_click = None;
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
            PropKind::OnClick => {
                let edits =
                    parse_key_edits(field_value, properties, images, file, &field_path, errors);
                on_click = Some(edits);
                shape.insert(prop);
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
    validate_follow_mouse_shape(&shape, true, file, &path, errors);
    validate_shape_fill(&shape, true, file, &path, errors);
    validate_model_fill(&shape, true, file, &path, errors);
    validate_animation_of_model(
        find_value(&values, property::ANIMATION),
        find_value(&values, property::MODEL),
        properties,
        file,
        &path,
        errors,
    );
    validate_edited_animations(
        keys.as_ref(),
        on_click.as_deref().unwrap_or_default(),
        find_value(&values, property::MODEL),
        properties,
        file,
        &path,
        errors,
    );
    validate_deck_shape(&shape, true, file, &path, errors);
    validate_road_shape(&shape, true, file, &path, errors);
    validate_road_kind_shape(&shape, true, file, &path, errors);
    validate_bend_shape(&shape, file, &path, errors);
    validate_bend_radius(&values, file, &path, errors);
    validate_particles_shape(&shape, true, file, &path, errors);
    validate_fire_shape(&shape, true, file, &path, errors);
    validate_clouds_shape(&shape, true, file, &path, errors);
    let is_layer = matches!(
        find_value(&values, property::PARALLAX),
        Some(Value::Number(n)) if *n != 1.0
    );
    validate_depth_layer_only_draws(is_layer, &shape, file, &path, errors);

    Some(ParsedObject {
        path,
        name,
        values,
        grid,
        keys,
        on_click,
        shape,
        broken_properties,
    })
}

/// «Создание объектов кодом», требования 3, 5: запись, что код передал в `create`, разбирается и
/// проверяется как объект `scene.json`; ошибка называет свойство.
fn parse_created_object(
    record: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
) -> Result<ObjectSpec, String> {
    let mut errors = ErrorSink::new();
    let parsed = parse_scene_object(record, 0, "create", properties, images, &mut errors);
    let (errs, _) = errors.into_parts();
    if let Some(first) = errs.first() {
        let place = first.path.strip_prefix("objects[0]").unwrap_or(&first.path);
        let place = place.trim_start_matches(" → ");
        return Err(if place.is_empty() {
            first.message.clone()
        } else {
            format!("{place}: {}", first.message)
        });
    }
    let parsed = parsed.ok_or("запись не разобрана")?;
    Ok(ObjectSpec {
        values: parsed.values,
        grid: parsed.grid,
        keys: parsed.keys,
        on_click: parsed.on_click,
    })
}

/// Разбор записей `create` игры: те же картинки, что видит `scene.json`.
fn object_parser(images: Vec<ImageDecl>) -> ObjectParser {
    ObjectParser::new(move |properties, record| parse_created_object(record, properties, &images))
}

/// «Мир на экране» → «Земля», требования 13–14: `scene.json`'s root — `objects` (required) and
/// `ground` (optional, empty without the key — «без ground всё как сейчас»).
#[derive(Default)]
struct ParsedSceneFile {
    objects: Vec<ParsedObject>,
    ground: Vec<GroundLayer>,
    /// «Ветер и частицы» → «Ветер»: `wind` — скорость ровного ветра в клетках в секунду; без ключа
    /// безветрие.
    wind: Vec2,
}

fn parse_scene_json(
    text: &str,
    file: &str,
    properties: &PropertyTable,
    images: &[ImageDecl],
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> ParsedSceneFile {
    let Some(root) = parse_json_or_error(file, text, errors) else {
        return ParsedSceneFile::default();
    };
    let Some(obj) = expect_object(&root, file, "", errors) else {
        return ParsedSceneFile::default();
    };
    reject_unknown_keys(obj, &["objects", "ground", "wind"], file, "", errors);
    let Some(objects_json) = obj.get("objects") else {
        // Same convention `require_field` uses: the path names the parent object (here the
        // root, `""`), not the missing key itself — a path pointing at a key that by definition
        // isn't in the text can never resolve to a location.
        errors.push(file, "", "отсутствует список объектов");
        return ParsedSceneFile::default();
    };
    let objects = match expect_array(objects_json, file, "objects", errors) {
        Some(arr) => arr
            .iter()
            .enumerate()
            .filter_map(|(i, v)| parse_scene_object(v, i, file, properties, images, errors))
            .collect(),
        None => Vec::new(),
    };
    let ground = match obj.get("ground") {
        Some(v) => parse_ground(v, file, images, scene, errors),
        None => Vec::new(),
    };
    let wind = match obj.get("wind") {
        Some(v) => parse_wind(v, file, scene, errors).unwrap_or_default(),
        None => Vec2::default(),
    };
    ParsedSceneFile {
        objects,
        ground,
        wind,
    }
}

/// «Ветер и частицы» → «Проверка перед запуском»: `wind` — пара чисел, и только в плоской сцене.
fn parse_wind(
    value: &Json,
    file: &str,
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<Vec2> {
    if scene.is_3d() {
        errors.push(file, "wind", WIND_IN_3D_SCENE);
        return None;
    }
    parse_vec2(value, file, "wind", errors)
}

/// «Мир на экране» → «Земля», требования 13, 24: `ground` — a list of layers; anything else is
/// the same error `expect_array` already gives for other misshapen lists.
fn parse_ground(
    value: &Json,
    file: &str,
    images: &[ImageDecl],
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Vec<GroundLayer> {
    let Some(arr) = expect_array(value, file, "ground", errors) else {
        return Vec::new();
    };
    arr.iter()
        .enumerate()
        .filter_map(|(i, v)| parse_ground_layer(v, i, file, images, scene, errors))
        .collect()
}

/// «Мир на экране» → «Земля», требования 14–17, 24: one `ground` layer — `image` (a declared tile
/// set: no `frame_time`/`frame_by`/`size`) and `cells` (one row per scene cell of height, one
/// number per row per scene cell of width, each `-1` or a tile number of `image`'s own set).
fn parse_ground_layer(
    value: &Json,
    index: usize,
    file: &str,
    images: &[ImageDecl],
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<GroundLayer> {
    let path = format!("ground[{index}]");
    let obj = expect_object(value, file, &path, errors)?;
    reject_unknown_keys(obj, &["image", "cells"], file, &path, errors);

    let image_path = join(&path, "image");
    let image_name = require_field(obj, "image", file, &path, errors)
        .and_then(|v| expect_string(v, file, &image_path, errors))?;
    let image = resolve_image(&image_name, images, file, &image_path, errors)?;
    let decl = &images[image];
    if decl.video {
        errors.push(
            file,
            &image_path,
            format!("\"{image_name}\" объявлена видео: земля берёт только набор плиток"),
        );
        return None;
    }
    if decl.frame_by.is_some() || decl.animated || decl.size.is_some() {
        errors.push(
            file,
            &image_path,
            format!(
                "\"{image_name}\" объявлена с frame_time, frame_by или size: земля берёт только набор плиток"
            ),
        );
        return None;
    }

    let cells_json = require_field(obj, "cells", file, &path, errors)?;
    let cells_path = join(&path, "cells");
    let rows_json = expect_array(cells_json, file, &cells_path, errors)?;
    if rows_json.len() != scene.height as usize {
        errors.push(
            file,
            &cells_path,
            format!(
                "cells: {} строк, а высота сцены {} клеток",
                rows_json.len(),
                scene.height
            ),
        );
        return None;
    }
    let mut cells = Vec::with_capacity(rows_json.len());
    let mut ok = true;
    for (r, row_json) in rows_json.iter().enumerate() {
        let row_path = format!("{cells_path}[{r}]");
        let Some(row_arr) = expect_array(row_json, file, &row_path, errors) else {
            ok = false;
            continue;
        };
        if row_arr.len() != scene.width as usize {
            errors.push(
                file,
                &row_path,
                format!(
                    "cells[{r}]: {} чисел, а ширина сцены {} клеток",
                    row_arr.len(),
                    scene.width
                ),
            );
            ok = false;
            continue;
        }
        let mut row = Vec::with_capacity(row_arr.len());
        for (c, cell_json) in row_arr.iter().enumerate() {
            let cell_path = format!("{row_path}[{c}]");
            match expect_number(cell_json, file, &cell_path, errors) {
                Some(n) if n.fract() == 0.0 && n >= -1.0 && n < decl.frames as f64 => {
                    row.push(n as i32);
                }
                Some(n) => {
                    errors.push(
                        file,
                        &cell_path,
                        format!(
                            "номер плитки должен быть целым от -1 до {} (кадров в наборе {}), получено {n}",
                            decl.frames as i64 - 1,
                            decl.frames
                        ),
                    );
                    ok = false;
                }
                None => ok = false,
            }
        }
        cells.push(row);
    }
    if !ok {
        return None;
    }
    Some(GroundLayer { image, cells })
}

/// «Рельеф», требования 1, 38: `heights` — `d × высота + 1` строк по `d × ширина + 1` чисел; `water`
/// необязательна — море, озёра и реки с `color` → «Вода»; `covers` — слои покрытий из `materials`, их маски и карта
/// цвета `tint` проверены по ответу страницы `masks`, если он есть; `stamps` — отпечатки из штампов `stamps`.
fn parse_terrain(
    text: &str,
    file: &str,
    scene: &SceneConfig,
    materials: &[MaterialDecl],
    masks: Option<&[(String, ImageVerdict)]>,
    stamps: &StampTable,
    errors: &mut ErrorSink,
) -> Option<Terrain> {
    let root = parse_json_or_error(file, text, errors)?;
    let obj = expect_object(&root, file, "", errors)?;
    reject_unknown_keys(
        obj,
        &["heights", "water", "covers", "tint", "stamps"],
        file,
        "",
        errors,
    );
    let rows = require_field(obj, "heights", file, "", errors)
        .and_then(|v| parse_heights(v, file, scene, errors));
    let water = match obj.get("water") {
        None => Some(None),
        Some(v) => parse_water(v, file, scene, errors).map(Some),
    };
    let covers = match obj.get("covers") {
        None => Some(Vec::new()),
        Some(v) => parse_covers(v, file, materials, masks, errors),
    };
    let tint = match obj.get("tint") {
        None => Some(false),
        Some(v) => materials::parse_tint(v, file, masks, errors).map(|()| true),
    };
    let imprints = match obj.get("stamps") {
        None => Some(Vec::new()),
        Some(v) => parse_imprints(v, file, stamps, errors),
    };
    let terrain = Terrain::from_rows([scene.width, scene.height], &rows?, water?)
        .map_err(|message| errors.push(file, "heights", message))
        .ok()?;
    let terrain = terrain.with_covers(covers?, tint?).with_imprints(imprints?);
    for warning in terrain.water_warnings() {
        errors.push_warning(file, &warning.path, &warning.message);
    }
    Some(terrain)
}

fn parse_heights(
    value: &Json,
    file: &str,
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<Vec<Vec<f64>>> {
    let rows_json = expect_array(value, file, "heights", errors)?;
    let Some(density) = Terrain::density_for(rows_json.len(), scene.height) else {
        errors.push(
            file,
            "heights",
            format!(
                "heights: {} строк, а по высоте сцены {} клеток нужно {}",
                rows_json.len(),
                scene.height,
                Terrain::allowed_rows(scene.height)
            ),
        );
        return None;
    };
    let (want_rows, want_columns) = (rows_json.len(), density * scene.width as usize + 1);
    let mut ok = true;
    let mut rows = Vec::with_capacity(want_rows);
    for (r, row_json) in rows_json.iter().enumerate() {
        let row_path = format!("heights[{r}]");
        let Some(row_arr) = expect_array(row_json, file, &row_path, errors) else {
            ok = false;
            continue;
        };
        if row_arr.len() != want_columns {
            errors.push(
                file,
                &row_path,
                format!(
                    "heights[{r}]: {} чисел, а при {density} точках на клетку (строк {want_rows}) и ширине сцены {} клеток нужно {density} × {} + 1 = {want_columns}",
                    row_arr.len(),
                    scene.width,
                    scene.width
                ),
            );
            ok = false;
            continue;
        }
        let mut row = Vec::with_capacity(want_columns);
        for (c, cell) in row_arr.iter().enumerate() {
            match expect_number(cell, file, &format!("{row_path}[{c}]"), errors) {
                Some(n) => row.push(n),
                None => ok = false,
            }
        }
        rows.push(row);
    }
    ok.then_some(rows)
}

/// «Редактор», `set_terrain`: рельеф сцены из `heights` — строки сетки подряд сверху вниз — и воды
/// `water` в виде записи файла рельефа. Проверка чисел и воды — как у файла рельефа, тексты — в том же виде.
pub fn terrain_from_numbers(
    scene: &SceneConfig,
    heights: &[f64],
    water: Option<&Json>,
) -> Result<Terrain, String> {
    let points = |d: usize| (d * scene.height as usize + 1) * (d * scene.width as usize + 1);
    let Some(density) = DENSITIES.into_iter().find(|&d| points(d) == heights.len()) else {
        return Err(format!(
            "heights: {} чисел, а сцене {} × {} клеток нужно (d × {} + 1) строк по (d × {} + 1) чисел, где d — точек на клетку: {}",
            heights.len(),
            scene.width,
            scene.height,
            scene.height,
            scene.width,
            DENSITIES
                .iter()
                .map(|&d| format!("{} при d = {d}", points(d)))
                .collect::<Vec<_>>()
                .join(", ")
        ));
    };
    let want_columns = density * scene.width as usize + 1;
    if let Some(index) = heights.iter().position(|h| !h.is_finite()) {
        return Err(format!(
            "heights[{}][{}]: ожидалось число, получено {}",
            index / want_columns,
            index % want_columns,
            heights[index]
        ));
    }
    let water = water
        .map(|value| parse_edit_water(value, scene))
        .transpose()?;
    let rows: Vec<Vec<f64>> = heights.chunks(want_columns).map(<[f64]>::to_vec).collect();
    Terrain::from_rows([scene.width, scene.height], &rows, water)
}

/// «Редактор», `terrain_readings`: сцена и рельеф игры из текстов `game.json`, файла рельефа и штампов
/// — теми же функциями разбора, что `load`, но без картинок, мира и видеокарты. `None` вместо рельефа,
/// если у игры нет `files.terrain`. Ошибки — как при загрузке; сцена плоская — тоже ошибка.
pub fn read_terrain(
    game_json: &str,
    terrain_text: Option<&str>,
    stamp_texts: &[(String, Option<String>)],
) -> Result<(SceneConfig, Option<Terrain>), Vec<GameError>> {
    let (config, _) = read_entry(game_json).map_err(|failure| failure.errors)?;
    if !config.scene.is_3d() {
        return Err(vec![GameError::new(
            "game.json",
            "scene",
            "рельеф есть только у трёхмерной сцены: у scene в game.json нет camera",
        )]);
    }
    let mut errors = ErrorSink::new();
    let stamps = parse_stamp_table(&config.files.stamps, stamp_texts, &mut errors);
    let terrain = match (&config.files.terrain, terrain_text) {
        (Some(path), Some(text)) => parse_terrain(
            text,
            path,
            &config.scene,
            &config.files.materials,
            None,
            &stamps,
            &mut errors,
        ),
        (Some(path), None) => {
            errors.push(
                path,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → terrain",
            );
            None
        }
        (None, _) => None,
    };
    let (errs, _) = errors.into_parts();
    if errs.is_empty() {
        Ok((config.scene, terrain))
    } else {
        Err(errs)
    }
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
            "timer" => PropKind::Timer,
            "text" => PropKind::Text,
            other => {
                errors.push(
                    file,
                    &path,
                    format!(
                        "неизвестный вид свойства \"{other}\": ожидался flag, number, time, timer или text"
                    ),
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
    view_height: Option<f64>,
    y_sort: bool,
    camera: Option<CameraConfig>,
    light: LightConfig,
    cell_pixels: Option<f64>,
    player_camera: Option<PlayerCameraConfig>,
    road_kinds: Vec<RoadKindDecl>,
    road_junctions: Vec<JunctionDecl>,
}

/// «Мир на экране» → «Камера игрока», требования 1–3, 23: `player_camera` — `zoom` из двух чисел,
/// `0 < min ≤ view_height ≤ max`, и необязательная точка `start` внутри сцены.
fn parse_player_camera(
    value: &Json,
    scene: (Option<u32>, Option<u32>),
    view_height: Option<f64>,
    errors: &mut ErrorSink,
) -> Option<PlayerCameraConfig> {
    const PATH: &str = "scene → player_camera";
    let obj = expect_object(value, "game.json", PATH, errors)?;
    reject_unknown_keys(obj, &["zoom", "start"], "game.json", PATH, errors);
    let zoom = parse_player_zoom(obj, view_height, errors);
    let start = match obj.get("start") {
        None => Some(None),
        Some(point) => parse_player_start(point, scene, errors).map(Some),
    };
    Some(PlayerCameraConfig {
        zoom: zoom?,
        start: start?,
    })
}

fn parse_player_zoom(
    obj: &serde_json::Map<String, Json>,
    view_height: Option<f64>,
    errors: &mut ErrorSink,
) -> Option<[f64; 2]> {
    const PATH: &str = "scene → player_camera → zoom";
    let value = require_field(obj, "zoom", "game.json", "scene → player_camera", errors)?;
    let [min, max] = parse_vec2(value, "game.json", PATH, errors)?;
    if min <= 0.0 {
        errors.push(
            "game.json",
            PATH,
            format!("zoom: самый близкий вид должен быть больше нуля, получено {min}"),
        );
        return None;
    }
    if min > max {
        errors.push(
            "game.json",
            PATH,
            format!("zoom: самый близкий вид {min} больше самого дальнего {max}"),
        );
        return None;
    }
    if let Some(view_height) = view_height
        && !(min..=max).contains(&view_height)
    {
        errors.push(
            "game.json",
            PATH,
            format!("zoom [{min}, {max}] не вмещает view_height {view_height}"),
        );
        return None;
    }
    Some([min, max])
}

fn parse_player_start(
    value: &Json,
    scene: (Option<u32>, Option<u32>),
    errors: &mut ErrorSink,
) -> Option<Vec2> {
    const PATH: &str = "scene → player_camera → start";
    let point = parse_vec2(value, "game.json", PATH, errors)?;
    let (Some(width), Some(height)) = scene else {
        return Some(point);
    };
    if (0.0..=f64::from(width)).contains(&point[0]) && (0.0..=f64::from(height)).contains(&point[1])
    {
        return Some(point);
    }
    errors.push(
        "game.json",
        PATH,
        format!(
            "start [{}, {}] вне сцены {width} × {height}",
            point[0], point[1]
        ),
    );
    None
}

/// «Трёхмерная сцена» → «Камера», требование 30: `camera` — объект с числом `pitch` от 30 до 90 включительно и
/// необязательной `projection`: `perspective` или `orthographic`.
fn parse_camera(value: &Json, errors: &mut ErrorSink) -> Option<CameraConfig> {
    let obj = expect_object(value, "game.json", "scene → camera", errors)?;
    reject_unknown_keys(
        obj,
        &["pitch", "projection"],
        "game.json",
        "scene → camera",
        errors,
    );
    let pitch_json = require_field(obj, "pitch", "game.json", "scene → camera", errors);
    let pitch = pitch_json
        .and_then(|v| expect_number(v, "game.json", "scene → camera → pitch", errors))
        .and_then(|pitch| {
            if (30.0..=90.0).contains(&pitch) {
                return Some(pitch);
            }
            errors.push(
                "game.json",
                "scene → camera → pitch",
                format!("pitch должен быть от 30 до 90 включительно, получено {pitch}"),
            );
            None
        });
    let projection = parse_projection(obj.get("projection"), errors);
    Some(CameraConfig {
        pitch: pitch?,
        projection: projection?,
    })
}

/// `projection` камеры: `perspective` или `orthographic`; нет ключа — `perspective`.
fn parse_projection(value: Option<&Json>, errors: &mut ErrorSink) -> Option<Projection> {
    let Some(value) = value else {
        return Some(Projection::Perspective);
    };
    let path = "scene → camera → projection";
    match expect_string(value, "game.json", path, errors)?.as_str() {
        "perspective" => Some(Projection::Perspective),
        "orthographic" => Some(Projection::Orthographic),
        other => {
            errors.push(
                "game.json",
                path,
                format!(
                    "projection должен быть \"perspective\" или \"orthographic\", получено \"{other}\""
                ),
            );
            None
        }
    }
}

/// «Трёхмерная сцена» → «Земля, свет и плоские объекты», требование 30: все три поля необязательны,
/// каждое в своём отрезке.
fn parse_light(value: &Json, errors: &mut ErrorSink) -> Option<LightConfig> {
    let obj = expect_object(value, "game.json", "scene → light", errors)?;
    reject_unknown_keys(
        obj,
        &[
            "sun_from",
            "sun_height",
            "sun_color",
            "sky_color",
            "shade_color",
            "shadow",
        ],
        "game.json",
        "scene → light",
        errors,
    );
    let defaults = LightConfig::default();
    let mut field = |key: &str, range: std::ops::RangeInclusive<f64>, default: f64| {
        let Some(v) = obj.get(key) else {
            return Some(default);
        };
        let path = format!("scene → light → {key}");
        let n = expect_number(v, "game.json", &path, errors)?;
        if range.contains(&n) {
            return Some(n);
        }
        errors.push(
            "game.json",
            &path,
            format!(
                "{key} должен быть от {} до {} включительно, получено {n}",
                range.start(),
                range.end()
            ),
        );
        None
    };
    let sun_from = field("sun_from", 0.0..=360.0, defaults.sun_from);
    let sun_height = field("sun_height", 10.0..=90.0, defaults.sun_height);
    let shadow = field("shadow", 0.0..=1.0, defaults.shadow);
    let mut tint = |key: &str, default: [f32; 3]| {
        let Some(v) = obj.get(key) else {
            return Some(default);
        };
        let path = format!("scene → light → {key}");
        let text = expect_string(v, "game.json", &path, errors)?;
        let Some([r, g, b, _]) = parse_hex_color(&text) else {
            errors.push(
                "game.json",
                &path,
                format!("цвет должен быть вида \"#rrggbb\", получено \"{text}\""),
            );
            return None;
        };
        Some([r, g, b])
    };
    let sun_color = tint("sun_color", defaults.sun_color);
    let sky_color = tint("sky_color", defaults.sky_color);
    let shade_color = tint("shade_color", defaults.shade_color);
    Some(LightConfig {
        sun_from: sun_from?,
        sun_height: sun_height?,
        shadow: shadow?,
        sun_color: sun_color?,
        sky_color: sky_color?,
        shade_color: shade_color?,
    })
}

fn parse_scene_config(value: &Json, errors: &mut ErrorSink) -> Option<SceneJsonConfig> {
    let obj = expect_object(value, "game.json", "scene", errors)?;
    reject_unknown_keys(
        obj,
        &[
            "width",
            "height",
            "background",
            "view_height",
            "y_sort",
            "camera",
            "light",
            "cell_pixels",
            "player_camera",
            "road_kinds",
            "road_junctions",
        ],
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
    // «Камера», требование 1: необязательный, число больше нуля.
    let view_height = match obj.get("view_height") {
        None => Some(None),
        Some(v) => match expect_number(v, "game.json", "scene → view_height", errors) {
            Some(n) if n > 0.0 => Some(Some(n)),
            Some(n) => {
                errors.push(
                    "game.json",
                    "scene → view_height",
                    format!("view_height должен быть больше нуля, получено {n}"),
                );
                None
            }
            None => None,
        },
    };
    // «Порядок рисования», требование 9: необязательный, true/false.
    let y_sort = match obj.get("y_sort") {
        None => Some(false),
        Some(v) => match v.as_bool() {
            Some(b) => Some(b),
            None => {
                errors.push(
                    "game.json",
                    "scene → y_sort",
                    format!("ожидался признак (true/false), получено {}", kind_name(v)),
                );
                None
            }
        },
    };
    let camera = match obj.get("camera") {
        None => Some(None),
        Some(v) => parse_camera(v, errors).map(Some),
    };
    let light = match obj.get("light") {
        None => Some(LightConfig::default()),
        Some(v) => parse_light(v, errors),
    };
    let three_d = matches!(camera, Some(Some(_)));
    if obj.contains_key("camera") && !obj.contains_key("view_height") {
        errors.push(
            "game.json",
            "scene → camera",
            "camera без view_height: в трёхмерной сцене view_height обязателен",
        );
    }
    if obj.contains_key("light") && !obj.contains_key("camera") {
        errors.push(
            "game.json",
            "scene → light",
            "light без camera: свет есть только в трёхмерной сцене",
        );
    }
    if three_d && y_sort == Some(true) {
        errors.push(
            "game.json",
            "scene → y_sort",
            "y_sort вместе с camera: порядок рисования в трёхмерной сцене задаёт глубина",
        );
    }
    let cell_pixels = parse_cell_pixels(obj, errors);
    let player_camera = match obj.get("player_camera") {
        None => Some(None),
        Some(_) if !obj.contains_key("view_height") => {
            errors.push(
                "game.json",
                "scene → player_camera",
                "player_camera без view_height: вид при сборке мира берётся из view_height",
            );
            None
        }
        Some(v) => parse_player_camera(v, (width, height), view_height.flatten(), errors).map(Some),
    };
    // «Вид дороги»: необязательна — нет ключа, нет и видов; только в трёхмерной сцене.
    let road_kinds = match obj.get("road_kinds") {
        None => Some(Vec::new()),
        Some(v) => parse_road_kinds(v, obj.contains_key("camera"), errors),
    };
    let road_junctions = match obj.get("road_junctions") {
        None => Some(Vec::new()),
        Some(v) => parse_road_junctions(v, obj.contains_key("camera"), errors),
    };
    Some(SceneJsonConfig {
        width: width?,
        height: height?,
        background: background?,
        view_height: view_height?,
        y_sort: y_sort?,
        camera: camera?,
        light: light?,
        cell_pixels: cell_pixels?,
        player_camera: player_camera?,
        road_kinds: road_kinds?,
        road_junctions: road_junctions?,
    })
}

/// «Редактор», «Формат игры»: `cell_pixels` — необязательное число больше нуля. Редактор читает его из
/// текста `game.json`; движку оно нужно для размера облака по точкам его картинки. Внешний `None` —
/// значение не годится, внутренний — поля нет.
fn parse_cell_pixels(
    scene: &serde_json::Map<String, Json>,
    errors: &mut ErrorSink,
) -> Option<Option<f64>> {
    let Some(value) = scene.get("cell_pixels") else {
        return Some(None);
    };
    match expect_number(value, "game.json", "scene → cell_pixels", errors) {
        Some(n) if n > 0.0 => Some(Some(n)),
        Some(n) => {
            errors.push(
                "game.json",
                "scene → cell_pixels",
                format!("cell_pixels должен быть больше нуля, получено {n}"),
            );
            None
        }
        None => None,
    }
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
    /// «Интерфейс игры» → «Текст»: настройки каждого шрифта `fonts`, в том же порядке.
    pub font_settings: Vec<FontSettings>,
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
    /// `(имя, путь)`, в порядке `files.tables` — «Таблицы данных»: только имя и путь, содержимое
    /// разбирается позже, вместе с остальными текстами второго захода (`parse_and_validate_tables`).
    /// Пусто, если `files.tables` не объявлена.
    pub tables: Vec<(String, String)>,
    /// `files.terrain` — «Рельеф»: необязательный путь к файлу высот земли трёхмерной сцены. `None`
    /// — земля ровная на высоте 0.
    pub terrain: Option<String>,
    /// `(имя, путь)`, в порядке объявления `files.stamps` — «Лепка рельефа»: номер штампа, на который
    /// ссылаются отпечатки файла рельефа. Пусто, если не объявлена.
    pub stamps: Vec<(String, String)>,
    /// `files.materials` — «Свет и материалы»: материалы рельефа в порядке объявления; этот порядок —
    /// номер материала, на который ссылаются слои `covers` файла рельефа. Пусто, если не объявлена.
    pub materials: Vec<MaterialDecl>,
    /// `files.models` — «Трёхмерная сцена» → «Модели и движения»: модели в порядке объявления. Пусто, если не
    /// объявлена.
    pub models: Vec<ModelDecl>,
    /// `files.motions` — «Движения»: необязательный путь к `motions.json`, движениям моделей с поправками. `None` —
    /// файла нет.
    pub motions: Option<String>,
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
    /// «Картинки» → «Кадры», требование 5: whether `frame_time` was declared — `false` for a
    /// plain frame set (`frames` alone, or with `frame_by`) always shows frame 0 to
    /// `render::atlas::frame_atlas_rect`, rather than cycling by elapsed time.
    pub animated: bool,
    /// «Картинки» → «Кадры», требования 1–3: frames per grid row; `None` — one row, as before.
    pub columns: Option<u32>,
    /// «Картинки» → «Картинка своего размера», требование 6: own size in cells; `None` — the
    /// image stretches onto the object's own rectangle, as before.
    pub size: Option<Vec2>,
    /// требование 6: the point of the own-size rectangle that coincides with the same point of
    /// the object's rectangle (shifted by `offset`). Meaningless without `size`, always `Center`
    /// then — `size` alone gates whether it's ever read.
    pub anchor: Anchor,
    /// требование 6: shift from `anchor`'s point, in cells. Meaningless without `size`, always
    /// `[0, 0]` then.
    pub offset: Vec2,
    /// «Картинки», требование 23: the frame is picked by this `number`-kind property on the
    /// object itself, instead of by elapsed time — `None` for a normal, time-driven strip (or a
    /// single-frame image). Resolved from `frame_by_name` once `properties.json` is parsed (see
    /// `resolve_image_frame_by`) — `game.json` alone doesn't know property names yet.
    pub frame_by: Option<PropertyId>,
    pub(crate) frame_by_name: Option<String>,
    /// «Картинки» → «Сглаживание», требования 1–2: необязательный, `true`/`false`, по умолчанию
    /// `false` — линейная выборка вместо выборки по точкам, everywhere this image is drawn.
    pub smooth: bool,
    /// «Картинки» → «Таблица картинок»: необязательный, `true`/`false`, по умолчанию `false` — точки
    /// картинки прибавляют свет к тому, что под ней, чёрное в ней невидимо.
    pub glow: bool,
    /// `frame_time` в секундах, как записан в файле; без `frame_time` — 0. Кадры объекта с `sway`
    /// идут своими часами в секундах, и округление до шагов (`frame_steps`) им не годится.
    pub frame_seconds: f64,
    /// «Картинки» → «Видео»: `path` на `.mp4` — видео двойной высоты (цвет сверху, маска снизу), а
    /// не картинка; кадров, столбцов и `frame_by` у него нет.
    pub video: bool,
    /// «Картинки» → «Видео на панели»: путь на `.mp4` с тем же роликом задом наперёд; бывает только у видео.
    pub reverse: Option<String>,
    /// «Картинки» → «Растягивание рамки»: поля `[слева, сверху, справа, снизу]` в точках картинки — у
    /// панели, кнопки и шкалы экрана она тянется девятью кусками.
    pub slice: Option<[u32; 4]>,
}

impl ImageDecl {
    /// «Ветер и частицы» → «Проверка перед запуском», требование 27: чем картинка не годится облакам;
    /// `None` — годится. Облако — одна картинка без кадров, с размером в клетках из `size` или из точек и
    /// `cell_pixels`.
    pub fn cloud_misfit(&self, cell_pixels: Option<f64>) -> Option<CloudMisfit> {
        if self.video {
            Some(CloudMisfit::Video)
        } else if self.animated || self.frame_by_name.is_some() || self.frames > 1 {
            Some(CloudMisfit::Frames)
        } else if self.anchor != Anchor::Center || self.offset != [0.0, 0.0] {
            Some(CloudMisfit::Placement)
        } else if self.size.is_none() && cell_pixels.is_none() {
            Some(CloudMisfit::NoSize)
        } else {
            None
        }
    }
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
/// У видео ответ `Video`: браузер взялся его играть, и здесь только размер файла, оба его слоя
/// вместе; сам проигрыватель движку отдаёт `wasm::Engine::load`.
#[derive(Debug, Clone)]
pub enum ImageVerdict {
    Ok {
        width: u32,
        height: u32,
        pixels: Vec<u8>,
    },
    /// `seconds` — длина ролика; `reverse` — ответ по файлу `reverse` его описания, `None` — страница его
    /// не присылала.
    Video {
        width: u32,
        height: u32,
        seconds: f64,
        reverse: Option<ReverseVerdict>,
    },
    Missing,
    Rejected,
}

/// Ответ страницы по файлу `reverse` видео — «Картинки» → «Видео на панели»: размер файла, оба слоя вместе,
/// и длина ролика в секундах.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ReverseVerdict {
    Ok {
        width: u32,
        height: u32,
        seconds: f64,
    },
    Missing,
    Rejected,
}

/// «Интерфейс игры» → «Текст»: ширина, толщина и признаки OpenType шрифта из `files.fonts`.
#[derive(Debug, Clone, PartialEq)]
pub struct FontSettings {
    /// Ширина в процентах обычной — один из шагов CSS `font-stretch`, `FONT_WIDTHS`.
    pub width: f32,
    /// Толщина от 1 до 1000, 400 — обычная.
    pub weight: u16,
    /// Включённые признаки OpenType, по четыре знака.
    pub features: Vec<[u8; 4]>,
}

impl Default for FontSettings {
    fn default() -> Self {
        FontSettings {
            width: 100.0,
            weight: 400,
            features: Vec::new(),
        }
    }
}

/// Шаги CSS `font-stretch`, в процентах обычной ширины.
pub const FONT_WIDTHS: [f32; 9] = [50.0, 62.5, 75.0, 87.5, 100.0, 112.5, 125.0, 150.0, 200.0];

/// Список двоичных файлов, которые стоит прочитать в третьем заходе загрузки — «Звук»
/// → «Загрузка: сначала лёгкое, потом тяжёлое»: шрифты, звуки и картинки читаются все, музыка —
/// только та, которую называет хоть один экран.
#[derive(Debug, Clone, Default)]
pub struct NeededMedia {
    pub fonts: Vec<(String, String)>,
    pub sounds: Vec<(String, String)>,
    pub music: Vec<(String, String)>,
    pub images: Vec<(String, String)>,
    /// `(имя видео, путь второго файла)` — «Картинки» → «Видео на панели»: страница читает и этот файл. У видео
    /// по наведению это его `reverse`, у видео по кругу — он сам: два проигрывателя одного файла играют по очереди.
    pub reverses: Vec<(String, String)>,
    /// Пути карт всех материалов: материал за материалом, в каждом `color`, `normal`, `roughness`,
    /// `height`, `ao`.
    pub materials: Vec<String>,
    /// Пути масок `covers` по порядку слоёв.
    pub masks: Vec<String>,
    /// `(имя, путь)` каждой модели `files.models`: у сжатой — путь облегчённой модели, а не исходного `.glb`.
    pub models: Vec<(String, String)>,
    /// Пути сжатых файлов картинок сжатых моделей — каждый один раз на игру, в порядке `ImportedPlan::textures`.
    pub model_textures: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct GameConfig {
    pub scene: SceneConfig,
    /// «Вид дороги»: `scene.road_kinds` — виды дорог игры; пусто, если не объявлены.
    pub road_kinds: Vec<RoadKindDecl>,
    /// «Вид дороги»: `scene.road_junctions` — модели перекрёстков и их рукава; пусто, если не объявлены.
    pub road_junctions: Vec<JunctionDecl>,
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
/// collects them all in one pass. `rules_json` plays no part in the answer (only a screen's own `music`
/// field decides which tracks are read — «Звук» → «Загрузка и проверка») and is accordingly not asked for here.
pub fn read_texts(
    config: &GameConfig,
    properties_json: Option<&str>,
    screens_json: Option<&str>,
    terrain_json: Option<&str>,
) -> NeededMedia {
    read_texts_with_imported(
        config,
        properties_json,
        None,
        screens_json,
        terrain_json,
        None,
    )
}

/// Same as `read_texts`, additionally accepting the text of `imported/index.json` — «Трёхмерная сцена» →
/// «Загрузка и проверка»: по каждой модели, у которой в нём есть запись, читаются облегчённая модель и сжатые файлы
/// её картинок, а не исходный `.glb`. `None` — файла нет, сжатых моделей нет. `scene_json` — «Видео на панели» →
/// «Выбор»: видео объектов и земли сцены второй раз не читаются, их не назовёт таблица выбора.
pub fn read_texts_with_imported(
    config: &GameConfig,
    properties_json: Option<&str>,
    scene_json: Option<&str>,
    screens_json: Option<&str>,
    terrain_json: Option<&str>,
    imported_index: Option<&str>,
) -> NeededMedia {
    let plan = ImportedPlan::new(&config.files.models, imported_index);
    let mut scratch = ErrorSink::new();
    let properties = match properties_json {
        Some(text) => parse_properties_json(text, &config.files.properties, &mut scratch),
        None => PropertyTable::new(),
    };
    let parsed_screens = match screens_json {
        Some(text) => {
            parse_screens_json(text, &config.files.screens, &properties, &mut scratch).screens
        }
        None => Vec::new(),
    };
    let referenced: std::collections::HashSet<&str> = parsed_screens
        .iter()
        .filter_map(|s| s.music.as_deref())
        .collect();
    let scene_images = match scene_json {
        Some(text) => {
            let scene = parse_scene_json(
                text,
                &config.files.scene,
                &properties,
                &config.files.images,
                &config.scene,
                &mut scratch,
            );
            scene_image_ids(&scene.objects, &scene.ground)
        }
        None => std::collections::HashSet::new(),
    };
    let looped = looped_video_names(&config.files.images, &parsed_screens, &scene_images);
    let music = config
        .files
        .music
        .iter()
        .filter(|(name, _)| referenced.contains(name.as_str()))
        .cloned()
        .collect();
    // «Текст»: два имени на один файл — файл читается один раз, под первым из имён.
    let mut fonts: Vec<(String, String)> = Vec::new();
    for (name, path) in &config.files.fonts {
        if !fonts.iter().any(|(_, known)| known == path) {
            fonts.push((name.clone(), path.clone()));
        }
    }
    NeededMedia {
        fonts,
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
        reverses: config
            .files
            .images
            .iter()
            .filter_map(|decl| {
                let path = if looped.contains(&decl.name.as_str()) {
                    decl.path.clone()
                } else {
                    decl.reverse.clone()?
                };
                Some((decl.name.clone(), path))
            })
            .collect(),
        // «Свет и материалы»: каждая карта каждого материала и каждая маска покрытий.
        materials: config
            .files
            .materials
            .iter()
            .flat_map(|material| material.maps().map(|(_, path)| path.to_string()))
            .collect(),
        masks: terrain_json.map(terrain_image_paths).unwrap_or_default(),
        models: config
            .files
            .models
            .iter()
            .enumerate()
            .map(|(at, decl)| {
                let path = plan.model_path(at).unwrap_or(&decl.path);
                (decl.name.clone(), path.to_string())
            })
            .collect(),
        model_textures: plan.textures().to_vec(),
    }
}

/// «Видео на панели»: картинки и видео, которые называют объекты сцены и слои земли.
fn scene_image_ids(
    objects: &[ParsedObject],
    ground: &[GroundLayer],
) -> std::collections::HashSet<ImageId> {
    let mut ids: std::collections::HashSet<ImageId> =
        ground.iter().map(|layer| layer.image).collect();
    for (_, value) in objects.iter().flat_map(|object| &object.values) {
        match value {
            Value::Image(id) => {
                ids.insert(*id);
            }
            Value::ImageList(list) => ids.extend(list),
            _ => {}
        }
    }
    ids
}

/// «Картинки» → «Видео на панели»: имена видео, которые панели экранов крутят по кругу, — `image` панели
/// без `play_on_hover` и ролик покоя `idle`, названные видео из `files.images`. `scene_images` — что называют
/// объекты сцены и земля.
fn looped_video_names<'a>(
    images: &'a [ImageDecl],
    screens: &[ParsedScreen],
    scene_images: &std::collections::HashSet<ImageId>,
) -> Vec<&'a str> {
    let mut names = Vec::new();
    let panels = screens
        .iter()
        .flat_map(|screen| &screen.elements)
        .flat_map(|(_, element)| match element {
            ParsedElementData::Panel {
                image_name,
                hover,
                idle_name,
                ..
            } => [
                image_name
                    .as_deref()
                    .filter(|_| hover.play_on_hover.is_none()),
                idle_name.as_deref(),
            ],
            _ => [None, None],
        })
        .flatten();
    for name in panels {
        if let Some(decl) = images.iter().find(|decl| decl.video && decl.name == name)
            && !names.contains(&decl.name.as_str())
        {
            names.push(decl.name.as_str());
        }
    }
    // «Выбор»: какое видео назовёт `{choice:…}`, знают таблицы данных, а они читаются позже этого захода, —
    // поэтому второй раз читается каждое видео, которое не стоит на панели, у объекта сцены или на земле: видео
    // выбора идёт по кругу вперёд, `reverse` у него не берётся, и больше нигде оно стоять не может.
    let names_a_choice = screens
        .iter()
        .flat_map(|screen| &screen.elements)
        .any(|(_, element)| {
            matches!(
                element,
                ParsedElementData::Panel {
                    choice_image: Some(_),
                    ..
                }
            )
        });
    if names_a_choice {
        let on_panels: std::collections::HashSet<&str> = screens
            .iter()
            .flat_map(|screen| &screen.elements)
            .filter_map(|(_, element)| match element {
                ParsedElementData::Panel {
                    image_name,
                    idle_name,
                    ..
                } => Some([image_name.as_deref(), idle_name.as_deref()]),
                _ => None,
            })
            .flatten()
            .flatten()
            .collect();
        for (index, decl) in images.iter().enumerate() {
            if decl.video
                && !on_panels.contains(decl.name.as_str())
                && !scene_images.contains(&index)
                && !names.contains(&decl.name.as_str())
            {
                names.push(decl.name.as_str());
            }
        }
    }
    names
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
                "tables",
                "terrain",
                "materials",
                "stamps",
                "models",
                "motions",
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
        // «Таблицы данных»: необязательна — нет ключа, нет и таблиц; та же форма «имя → путь»,
        // что у `files.fonts`, но с непустым именем — требование 22.
        let tables = match f.get("tables") {
            Some(v) => parse_named_paths(
                v,
                "game.json",
                "files → tables",
                "files → tables → пустое имя: имя таблицы — любая непустая строка",
                errors,
            )
            .unwrap_or_default(),
            None => Vec::new(),
        };
        // «Рельеф»: необязателен — нет ключа, нет и файла высот, земля ровная.
        let terrain = f
            .get("terrain")
            .and_then(|v| expect_string(v, "game.json", "files → terrain", errors));
        // «Свет и материалы»: необязательна — нет ключа, нет и материалов.
        let materials = f.get("materials").map(|v| {
            let order = declaration_order(text).materials;
            parse_materials_table(v, &order, "game.json", "files → materials", errors)
                .unwrap_or_default()
        });
        // «Трёхмерная сцена»: необязательна — нет ключа, нет и моделей; номер модели — порядок объявления.
        let models = f.get("models").map(|v| {
            let order = declaration_order(text).models;
            parse_models_table(v, &order, "game.json", "files → models", errors)
                .unwrap_or_default()
        });
        // «Движения»: необязателен — нет ключа, нет и файла поправок движений.
        let motions = f.get("motions").and_then(|v| {
            let path = expect_string(v, "game.json", "files → motions", errors)?;
            if path.to_ascii_lowercase().ends_with(".json") {
                return Some(path);
            }
            errors.push(
                "game.json",
                "files → motions",
                format!("{path} — движения берутся из файла .json; путь должен оканчиваться на \".json\""),
            );
            None
        });
        // «Лепка рельефа»: необязательна — нет ключа, нет и отпечатков штампов; номер штампа — порядок
        // объявления, а не алфавит.
        let stamps = f.get("stamps").map(|v| {
            let order = declaration_order(text).stamps;
            let mut stamps = parse_named_paths(
                v,
                "game.json",
                "files → stamps",
                "files → stamps → пустое имя: у штампа должно быть имя, которым его назовёт отпечаток",
                errors,
            )
            .unwrap_or_default();
            stamps.sort_by_key(|(name, _)| order.iter().position(|n| n == name));
            stamps
        });
        (
            properties, scene_path, rules, screens, fonts, sounds, music, images, code, tables,
            terrain, materials, stamps, models, motions,
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

    let mut scene = scene?;
    // Модель перекрёстка при равной цене выбирается по порядку записи, а не по алфавиту `serde_json::Map`.
    let junction_order = declaration_order(text).road_junctions;
    scene.road_junctions.sort_by_key(|junction| {
        junction_order
            .iter()
            .position(|name| name == &junction.name)
    });
    let max_objects = max_objects?;
    let random_seed = random_seed?;
    let (
        properties,
        scene_path,
        rules,
        screens,
        fonts,
        sounds,
        music,
        images,
        code,
        tables,
        terrain,
        materials,
        stamps,
        models,
        motions,
    ) = files?;
    if terrain.is_some() && scene.camera.is_none() {
        errors.push(
            "game.json",
            "files → terrain",
            "files.terrain есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
    }
    if materials.is_some() && scene.camera.is_none() {
        errors.push(
            "game.json",
            "files → materials",
            "files.materials есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
    }
    if models.is_some() && scene.camera.is_none() {
        errors.push(
            "game.json",
            "files → models",
            "files.models есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
    }
    if scene.camera.is_none() {
        for decl in models.as_deref().unwrap_or_default() {
            let keys = [
                ("solid", decl.solid.is_some()),
                ("cutaway", decl.cutaway.is_some()),
            ];
            for (key, _) in keys.into_iter().filter(|&(_, present)| present) {
                errors.push(
                    "game.json",
                    &format!("files → models → {} → {key}", decl.name),
                    format!("{key} есть только в трёхмерной сцене: у scene в game.json нет camera"),
                );
            }
        }
    }
    if stamps.is_some() && scene.camera.is_none() {
        errors.push(
            "game.json",
            "files → stamps",
            "files.stamps есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
    }
    validate_kind_models(
        &scene.road_kinds,
        models.as_deref().unwrap_or_default(),
        errors,
    );
    validate_junctions(
        &scene.road_junctions,
        &scene.road_kinds,
        models.as_deref().unwrap_or_default(),
        errors,
    );
    let (properties, scene_path, rules, screens, (fonts, font_settings)) =
        (properties?, scene_path?, rules?, screens?, fonts?);
    let start_screen = start_screen?;

    Some(GameConfig {
        scene: SceneConfig {
            width: scene.width,
            height: scene.height,
            background: scene.background,
            view_height: scene.view_height,
            y_sort: scene.y_sort,
            camera: scene.camera,
            light: scene.light,
            cell_pixels: scene.cell_pixels,
            player_camera: scene.player_camera,
        },
        road_kinds: scene.road_kinds,
        road_junctions: scene.road_junctions,
        random_seed,
        max_objects: max_objects as usize,
        files: FilePaths {
            properties,
            scene: scene_path,
            rules,
            screens,
            fonts,
            font_settings,
            sounds,
            music,
            images,
            code,
            tables,
            terrain,
            stamps: stamps.unwrap_or_default(),
            materials: materials.unwrap_or_default(),
            models: models.unwrap_or_default(),
            motions,
        },
        start_screen,
        win_screen,
        loss_screen,
    })
}

/// `files.tables`: the same shape as `files.fonts` — an object mapping a data table's name (used
/// by the game code as `tables.<name>`) to its JSON file's path inside the game's folder — but,
/// unlike a font's name, requirement 22 makes the table's name any non-empty string, so an empty
/// key is its own error (`empty_name`) rather than a silently accepted `read_entry` name. `files.stamps`
/// has the same shape.
fn parse_named_paths(
    value: &Json,
    file: &str,
    path: &str,
    empty_name: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<(String, String)>> {
    let obj = expect_object(value, file, path, errors)?;
    let mut tables = Vec::with_capacity(obj.len());
    for (name, path_json) in obj {
        if name.is_empty() {
            errors.push(file, path, empty_name);
            continue;
        }
        if let Some(table_path) = expect_string(path_json, file, &join(path, name), errors) {
            tables.push((name.clone(), table_path));
        }
    }
    Some(tables)
}

/// `files.fonts` разобранная: `(имя, путь)` и настройки каждого шрифта, в одном порядке.
type FontTable = (Vec<(String, String)>, Vec<FontSettings>);

/// `files.fonts`: an object mapping a font's name (used by `screens.json`'s `font` field) to its
/// path inside the game's folder, or to `{path, width, weight, features}` — «Интерфейс игры» →
/// «Текст». Order is preserved — it becomes the font's `FontId`.
fn parse_font_table(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<FontTable> {
    let obj = expect_object(value, file, path, errors)?;
    let mut fonts = Vec::with_capacity(obj.len());
    let mut settings = Vec::with_capacity(obj.len());
    for (name, entry) in obj {
        let entry_path = join(path, name);
        let parsed = if entry.is_object() {
            parse_font_object(entry, file, &entry_path, errors)
        } else {
            expect_string(entry, file, &entry_path, errors)
                .map(|font_path| (font_path, FontSettings::default()))
        };
        if let Some((font_path, font_settings)) = parsed {
            fonts.push((name.clone(), font_path));
            settings.push(font_settings);
        }
    }
    Some((fonts, settings))
}

/// Шрифт объектом: `path` обязателен, `width` — шаг CSS `font-stretch`, `weight` — от 1 до 1000,
/// `features` — список признаков OpenType из четырёх латинских букв или цифр.
fn parse_font_object(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<(String, FontSettings)> {
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(
        obj,
        &["path", "width", "weight", "features"],
        file,
        path,
        errors,
    );
    let font_path = require_field(obj, "path", file, path, errors)
        .and_then(|v| expect_string(v, file, &join(path, "path"), errors));
    let width = match obj.get("width") {
        None => Some(100.0),
        Some(v) => expect_number(v, file, &join(path, "width"), errors).and_then(|n| {
            let width = n as f32;
            if FONT_WIDTHS.contains(&width) && f64::from(width) == n {
                Some(width)
            } else {
                errors.push(
                    file,
                    &join(path, "width"),
                    format!(
                        "свойство \"width\" — ширина шрифта в процентах обычной, один из шагов 50, 62.5, 75, 87.5, 100, 112.5, 125, 150, 200; получено {n}"
                    ),
                );
                None
            }
        }),
    };
    let weight = match obj.get("weight") {
        None => Some(400),
        Some(v) => expect_number(v, file, &join(path, "weight"), errors).and_then(|n| {
            if (1.0..=1000.0).contains(&n) && n.fract() == 0.0 {
                Some(n as u16)
            } else {
                errors.push(
                    file,
                    &join(path, "weight"),
                    format!(
                        "свойство \"weight\" — толщина шрифта, целое число от 1 до 1000 (400 — обычная, 700 — жирная); получено {n}"
                    ),
                );
                None
            }
        }),
    };
    let features = match obj.get("features") {
        None => Some(Vec::new()),
        Some(v) => parse_font_features(v, file, &join(path, "features"), errors),
    };
    Some((
        font_path?,
        FontSettings {
            width: width?,
            weight: weight?,
            features: features?,
        },
    ))
}

/// `features` шрифта: список строк из четырёх латинских букв или цифр — `"tnum"`, `"zero"`.
fn parse_font_features(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<[u8; 4]>> {
    let items = expect_array(value, file, path, errors)?;
    let mut features = Vec::with_capacity(items.len());
    let mut ok = true;
    for (i, item) in items.iter().enumerate() {
        let tag = item.as_str().and_then(|s| {
            let bytes: [u8; 4] = s.as_bytes().try_into().ok()?;
            bytes.iter().all(u8::is_ascii_alphanumeric).then_some(bytes)
        });
        match tag {
            Some(tag) => features.push(tag),
            None => {
                errors.push(
                    file,
                    &join(path, &format!("[{i}]")),
                    format!(
                        "свойство \"features\" — список признаков OpenType из четырёх латинских букв или цифр, как \"tnum\"; получено {item}"
                    ),
                );
                ok = false;
            }
        }
    }
    ok.then_some(features)
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
            &[
                "path",
                "frames",
                "columns",
                "frame_time",
                "frame_by",
                "size",
                "anchor",
                "offset",
                "smooth",
                "glow",
                "slice",
                "reverse",
            ],
            file,
            &entry_path,
            errors,
        );
        let Some(image_path) = require_field(decl_obj, "path", file, &entry_path, errors)
            .and_then(|v| expect_string(v, file, &join(&entry_path, "path"), errors))
        else {
            continue;
        };
        let lower_path = image_path.to_ascii_lowercase();
        let video = lower_path.ends_with(VIDEO_EXTENSION);
        if !video && !lower_path.ends_with(".png") {
            errors.push(
                file,
                &join(&entry_path, "path"),
                format!(
                    "{image_path} — files.images берёт PNG и MP4; путь должен оканчиваться на \".png\" или \"{VIDEO_EXTENSION}\""
                ),
            );
            continue;
        }
        let Some(slice) = parse_slice(decl_obj, video, file, &entry_path, errors) else {
            continue;
        };
        if video && reject_video_frame_keys(decl_obj, file, &entry_path, errors) {
            continue;
        }
        let Some(reverse) = parse_reverse(decl_obj, video, file, &entry_path, errors) else {
            continue;
        };
        let frame_by_json = decl_obj.get("frame_by");
        // «Картинки», требование 23: `frame_by` требует `frames`; вместе с `frame_time` —
        // ошибка. Имя свойства разрешается позже, в `resolve_image_frame_by`, once
        // `properties.json` is parsed.
        if frame_by_json.is_some() && decl_obj.get("frame_time").is_some() {
            errors.push(
                file,
                &entry_path,
                "frame_by вместе с frame_time — ошибка: кадр выбирает либо время, либо свойство",
            );
            continue;
        }
        if frame_by_json.is_some() && decl_obj.get("frames").is_none() {
            errors.push(
                file,
                &entry_path,
                "frame_by задан без frames: числа кадров ленты",
            );
            continue;
        }
        let frame_by_name = match frame_by_json {
            Some(v) => expect_string(v, file, &join(&entry_path, "frame_by"), errors),
            None => None,
        };
        // «Картинки», требование 5: `frames` alone (or with `frame_by`) is no longer an error —
        // a bare frame/tile set, first frame shown until something else picks one.
        let (frames, frame_steps, animated) =
            match (decl_obj.get("frames"), decl_obj.get("frame_time")) {
                (None, None) => (1, 1, false),
                (None, Some(_)) => {
                    errors.push(
                    file,
                    &entry_path,
                    "frame_time задан без frames: оба поля нужны вместе, иначе не нужно ни одного",
                );
                    continue;
                }
                (Some(frames_json), frame_time_json) => {
                    let frames =
                        expect_number(frames_json, file, &join(&entry_path, "frames"), errors)
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
                    // Без `frame_time`: сам номер кадра шаг не считает, единица годится как
                    // заглушка — `animated: false` ниже говорит `atlas::frame_atlas_rect` не
                    // читать эту величину вовсе.
                    let frame_steps = match frame_time_json {
                        Some(frame_time_json) => expect_number(
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
                        })
                        .map(seconds_to_steps),
                        None => Some(1),
                    };
                    match (frames, frame_steps) {
                        (Some(frames), Some(frame_steps)) => {
                            (frames, frame_steps, frame_time_json.is_some())
                        }
                        _ => continue,
                    }
                }
            };
        // «Картинки», требование 1: `columns` — целое от 1, не больше `frames`, только вместе с
        // `frames`.
        let columns = match decl_obj.get("columns") {
            None => None,
            Some(v) => {
                if decl_obj.get("frames").is_none() {
                    errors.push(
                        file,
                        &entry_path,
                        "columns задан без frames: сетке нужно общее число кадров",
                    );
                    continue;
                }
                match expect_number(v, file, &join(&entry_path, "columns"), errors) {
                    Some(n) if n >= 1.0 && n.fract() == 0.0 && n <= frames as f64 => Some(n as u32),
                    Some(n) => {
                        errors.push(
                            file,
                            &join(&entry_path, "columns"),
                            format!(
                                "columns должен быть целым от 1 до frames ({frames}), получено {n}"
                            ),
                        );
                        continue;
                    }
                    None => continue,
                }
            }
        };
        // «Картинки», требование 6: `anchor`/`offset` требуют `size`; `size` — пара чисел
        // больше нуля.
        if decl_obj.get("size").is_none()
            && (decl_obj.get("anchor").is_some() || decl_obj.get("offset").is_some())
        {
            errors.push(
                file,
                &entry_path,
                "anchor или offset заданы без size: сдвигать от чего?",
            );
            continue;
        }
        let size = match decl_obj.get("size") {
            None => None,
            Some(v) => match parse_vec2(v, file, &join(&entry_path, "size"), errors) {
                Some(s) if s[0] > 0.0 && s[1] > 0.0 => Some(s),
                Some(s) => {
                    errors.push(
                        file,
                        &join(&entry_path, "size"),
                        format!(
                            "size должен быть парой чисел больше нуля, получено [{}, {}]",
                            s[0], s[1]
                        ),
                    );
                    continue;
                }
                None => continue,
            },
        };
        let anchor = match decl_obj.get("anchor") {
            None => Anchor::Center,
            Some(v) => {
                let Some(s) = expect_string(v, file, &join(&entry_path, "anchor"), errors) else {
                    continue;
                };
                let Some(a) = Anchor::parse(&s) else {
                    errors.push(
                        file,
                        &join(&entry_path, "anchor"),
                        format!("неизвестный якорь \"{s}\""),
                    );
                    continue;
                };
                a
            }
        };
        let offset = match decl_obj.get("offset") {
            None => [0.0, 0.0],
            Some(v) => match parse_vec2(v, file, &join(&entry_path, "offset"), errors) {
                Some(o) => o,
                None => continue,
            },
        };
        // «Картинки» → «Сглаживание», требования 1–2: необязательный, true/false.
        let smooth = parse_image_flag(decl_obj, "smooth", file, &entry_path, errors);
        let glow = parse_image_flag(decl_obj, "glow", file, &entry_path, errors);
        let (Some(smooth), Some(glow)) = (smooth, glow) else {
            continue;
        };
        out.push(ImageDecl {
            frame_by: None,
            frame_by_name,
            name: name.clone(),
            path: image_path,
            frames,
            frame_steps,
            animated,
            columns,
            size,
            anchor,
            offset,
            smooth,
            glow,
            frame_seconds: decl_obj
                .get("frame_time")
                .and_then(Json::as_f64)
                .unwrap_or(0.0),
            video,
            reverse,
            slice,
        });
    }
    Some(out)
}

/// «Картинки» → «Растягивание рамки»: `slice` — четыре целых от 0, только у картинки без `frames` и не у
/// видео. Внешний `None` — ошибка уже названа.
fn parse_slice(
    decl_obj: &serde_json::Map<String, Json>,
    video: bool,
    file: &str,
    entry_path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<[u32; 4]>> {
    let Some(value) = decl_obj.get("slice") else {
        return Some(None);
    };
    let path = join(entry_path, "slice");
    if video || decl_obj.contains_key("frames") {
        errors.push(
            file,
            &path,
            "свойство \"slice\" — поля рамки одной картинки: у картинки с кадрами frames и у видео их не бывает",
        );
        return None;
    }
    let margins: Option<Vec<u32>> = value.as_array().and_then(|items| {
        items
            .iter()
            .map(|item| {
                item.as_f64()
                    .filter(|n| *n >= 0.0 && n.fract() == 0.0 && *n <= f64::from(u32::MAX))
                    .map(|n| n as u32)
            })
            .collect()
    });
    match margins.as_deref() {
        Some(&[left, top, right, bottom]) => Some(Some([left, top, right, bottom])),
        _ => {
            errors.push(
                file,
                &path,
                format!(
                    "свойство \"slice\" — поля рамки [слева, сверху, справа, снизу], четыре целых числа от 0; получено {value}"
                ),
            );
            None
        }
    }
}

/// «Картинки» → «Видео»: путь на такое окончание — видео, а не PNG.
const VIDEO_EXTENSION: &str = ".mp4";

/// «Картинки» → «Видео на панели»: `reverse` — путь на `.mp4` с тем же роликом задом наперёд, только у видео.
/// Внешний `None` — ошибка уже названа.
fn parse_reverse(
    decl_obj: &serde_json::Map<String, Json>,
    video: bool,
    file: &str,
    entry_path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<String>> {
    let Some(value) = decl_obj.get("reverse") else {
        return Some(None);
    };
    let path = join(entry_path, "reverse");
    if !video {
        errors.push(
            file,
            &path,
            "reverse — тот же ролик задом наперёд: у картинки PNG его не бывает, только у видео",
        );
        return None;
    }
    let reverse = expect_string(value, file, &path, errors)?;
    if !reverse.to_ascii_lowercase().ends_with(VIDEO_EXTENSION) {
        errors.push(
            file,
            &path,
            format!(
                "{reverse} — reverse берёт MP4; путь должен оканчиваться на \"{VIDEO_EXTENSION}\""
            ),
        );
        return None;
    }
    Some(Some(reverse))
}

/// «Картинки» → «Видео»: у видео нет `frames`, `columns`, `frame_time` и `frame_by` — его кадры это
/// кадры ролика. Каждый названный ключ — своя ошибка; `true`, если хоть один назван.
fn reject_video_frame_keys(
    decl_obj: &serde_json::Map<String, Json>,
    file: &str,
    entry_path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let mut rejected = false;
    for key in ["frames", "columns", "frame_time", "frame_by"] {
        if decl_obj.contains_key(key) {
            errors.push(
                file,
                &join(entry_path, key),
                format!("{key} у видео не бывает: его кадры — кадры ролика"),
            );
            rejected = true;
        }
    }
    rejected
}

/// Необязательный признак описания картинки — `smooth`, `glow`: `true`/`false`, без ключа `false`.
fn parse_image_flag(
    decl_obj: &serde_json::Map<String, Json>,
    key: &str,
    file: &str,
    entry_path: &str,
    errors: &mut ErrorSink,
) -> Option<bool> {
    let Some(value) = decl_obj.get(key) else {
        return Some(false);
    };
    let flag = value.as_bool();
    if flag.is_none() {
        errors.push(
            file,
            &join(entry_path, key),
            format!(
                "ожидался признак (true/false), получено {}",
                kind_name(value)
            ),
        );
    }
    flag
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
        // «Свойство вида text», требование 20: строки не сравниваются — раньше такое условие
        // молча всегда было ложным, теперь это ошибка проверки.
        if properties.kind(prop) == PropKind::Text {
            errors.push(
                file,
                &join(path, "[0]"),
                format!("свойство \"{name}\" вида text (строка): условия строки не сравнивают"),
            );
            return None;
        }
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
        // «Формат игры»: время и таймер в файлах — секунды, а `World::number_like` отдаёт для
        // `Time`/`Timer` шаги (world.rs), так что порог сравнения переводится здесь же, при
        // разборе — это порог, а не длительность, так что `seconds_to_steps`'s минимум в один
        // шаг тут не к месту: сравнение с нулём должно остаться сравнением с нулём.
        let value = if matches!(properties.kind(prop), PropKind::Time | PropKind::Timer) {
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
        // «Правила игры», требование 6: `all`/`any`/`not`, вкладываются друг в друга на любую
        // глубину.
        if let Some(all) = obj.get("all") {
            let list =
                parse_condition_list(all, "all", properties, file, &join(path, "all"), errors)?;
            return Some(Condition::All(list));
        }
        if let Some(any) = obj.get("any") {
            let list =
                parse_condition_list(any, "any", properties, file, &join(path, "any"), errors)?;
            return Some(Condition::Any(list));
        }
        if let Some(not) = obj.get("not") {
            let inner = parse_condition(not, properties, file, &join(path, "not"), errors)?;
            return Some(Condition::Not(Box::new(inner)));
        }
        errors.push(
            file,
            path,
            "условие не той формы: ожидался outside_scene, сравнение, fewer_than, after_move_of, all, any или not"
                .to_string(),
        );
        return None;
    }
    errors.push(file, path, "условие не той формы".to_string());
    None
}

/// `all`/`any`'s own list of nested conditions — требование 6: «пустой список у all/any —
/// ошибка».
fn parse_condition_list(
    value: &Json,
    head: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<Condition>> {
    let arr = expect_array(value, file, path, errors)?;
    if arr.is_empty() {
        errors.push(file, path, format!("{head} не может быть пустым списком"));
        return None;
    }
    let mut list = Vec::with_capacity(arr.len());
    let mut ok = true;
    for (i, entry) in arr.iter().enumerate() {
        match parse_condition(
            entry,
            properties,
            file,
            &join(path, &format!("[{i}]")),
            errors,
        ) {
            Some(c) => list.push(c),
            None => ok = false,
        }
    }
    ok.then_some(list)
}

/// Whether `condition` contains an `after_move_of` node reachable without crossing an `any` or a
/// `not` (an `all` doesn't hide it) — «Правила игры», требование 7. `Err` when two or more such
/// nodes exist: `at_parent`/`from_parent` need exactly one to supply the parent from.
fn find_spawn_parent_of(condition: &Condition) -> Result<Option<Selector>, ()> {
    match condition {
        Condition::AfterMoveOf { of } => Ok(Some(of.clone())),
        Condition::All(list) => {
            let mut found: Option<Selector> = None;
            for c in list {
                if let Some(of) = find_spawn_parent_of(c)? {
                    if found.is_some() {
                        return Err(());
                    }
                    found = Some(of);
                }
            }
            Ok(found)
        }
        Condition::Any(_) | Condition::Not(_) => Ok(None),
        Condition::Compare { .. } | Condition::OutsideScene | Condition::FewerThan { .. } => {
            Ok(None)
        }
    }
}

/// «Правила игры», требование 7: сравнение и `outside_scene` запрещены у `spawn` на любой
/// глубине.
fn condition_forbids_spawn_leaf(condition: &Condition) -> bool {
    match condition {
        Condition::Compare { .. } | Condition::OutsideScene => true,
        Condition::FewerThan { .. } | Condition::AfterMoveOf { .. } => false,
        Condition::All(list) | Condition::Any(list) => {
            list.iter().any(condition_forbids_spawn_leaf)
        }
        Condition::Not(inner) => condition_forbids_spawn_leaf(inner),
    }
}

fn parse_spawn_condition(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<SpawnCondition> {
    let condition = parse_condition(value, properties, file, path, errors)?;
    if condition_forbids_spawn_leaf(&condition) {
        errors.push(
            file,
            path,
            "у «создать» условие не может содержать сравнение или outside_scene".to_string(),
        );
        return None;
    }
    let parent_of = match find_spawn_parent_of(&condition) {
        Ok(of) => of,
        Err(()) => {
            errors.push(
                file,
                path,
                "у «создать» может быть только один after_move_of вне any/not".to_string(),
            );
            return None;
        }
    };
    Some(SpawnCondition {
        condition,
        parent_of,
    })
}

/// Resolves an `add`/`set` "by"/"times" name — must name a property of kind `number`, on the
/// object the action itself changes, not this rule's other selectors («Правила игры»,
/// требование 11).
fn resolve_number_source(
    value: &Json,
    field: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<PropertyId> {
    let name = expect_string(value, file, &join(path, field), errors)?;
    let prop = resolve_property(&name, properties, file, &join(path, field), errors)?;
    if properties.kind(prop) != PropKind::Number {
        errors.push(
            file,
            &join(path, field),
            format!(
                "{field} должно называть свойство вида number, у \"{name}\" вид {}",
                properties.kind(prop).label()
            ),
        );
        return None;
    }
    Some(prop)
}

/// The number `add`/`set` writes — a constant, `{"table": [...], "by": "<свойство>"}` or
/// `{"value": <число>, "times": "<свойство>"}» — «Правила игры», требование 11. `target_kind` is
/// the property being changed: `time`/`timer` numbers are seconds in the file, converted to
/// steps here so nothing downstream sees seconds again.
fn parse_number_expr(
    value: &Json,
    target_kind: PropKind,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<NumberExpr> {
    let convert = |raw: f64| -> f64 {
        if matches!(target_kind, PropKind::Time | PropKind::Timer) {
            seconds_to_steps_delta(raw) as f64
        } else {
            raw
        }
    };
    if let Some(n) = value.as_f64() {
        return Some(NumberExpr::Const(convert(n)));
    }
    let obj = expect_object(value, file, path, errors)?;
    if obj.contains_key("table") {
        reject_unknown_keys(obj, &["table", "by"], file, path, errors);
        let table_json = require_field(obj, "table", file, path, errors)?;
        let table_arr = expect_array(table_json, file, &join(path, "table"), errors)?;
        if table_arr.is_empty() {
            errors.push(file, &join(path, "table"), "table не может быть пустым");
            return None;
        }
        let table_path = join(path, "table");
        let mut ok = true;
        let mut table = Vec::with_capacity(table_arr.len());
        for (i, v) in table_arr.iter().enumerate() {
            match expect_number(v, file, &join(&table_path, &format!("[{i}]")), errors) {
                Some(n) => table.push(convert(n)),
                None => ok = false,
            }
        }
        let by_json = require_field(obj, "by", file, path, errors)?;
        let by = resolve_number_source(by_json, "by", properties, file, path, errors)?;
        return ok.then_some(NumberExpr::Table { table, by });
    }
    if obj.contains_key("value") {
        reject_unknown_keys(obj, &["value", "times"], file, path, errors);
        let value_json = require_field(obj, "value", file, path, errors)?;
        let v = expect_number(value_json, file, &join(path, "value"), errors)?;
        let times_json = require_field(obj, "times", file, path, errors)?;
        let times = resolve_number_source(times_json, "times", properties, file, path, errors)?;
        return Some(NumberExpr::Multiplier {
            value: convert(v),
            times,
        });
    }
    errors.push(
        file,
        path,
        "ожидалось число, {table, by} или {value, times}".to_string(),
    );
    None
}

/// The number/time/timer check both `add` and `set` (its number form) need on the property their
/// number actually writes to.
fn require_number_kind(
    prop: PropertyId,
    name: &str,
    action: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<PropKind> {
    if !(height_fits_the_scene(prop, properties, file, path, errors)
        && depth_layer_fits_the_scene(prop, properties, file, path, errors))
    {
        return None;
    }
    let kind = properties.kind(prop);
    if matches!(kind, PropKind::Number | PropKind::Time | PropKind::Timer) {
        Some(kind)
    } else {
        errors.push(
            file,
            path,
            format!(
                "{action} применим только к числу, времени или таймеру, у \"{name}\" вид {}",
                kind.label()
            ),
        );
        None
    }
}

/// Числа, которые `expr` называет прямо в файле: само число или элементы таблицы; множитель их не даёт.
fn number_constants(expr: &NumberExpr) -> &[f64] {
    match expr {
        NumberExpr::Const(n) => std::slice::from_ref(n),
        NumberExpr::Table { table, .. } => table,
        NumberExpr::Multiplier { .. } => &[],
    }
}

/// `set`'s value — a `NumberExpr` for a `number`/`time`/`timer` property, a literal `Value`
/// otherwise («Правила игры», требование 11).
fn parse_set_value(
    value: &Json,
    prop: PropertyId,
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<SetValue> {
    let kind = properties.kind(prop);
    if matches!(kind, PropKind::Number | PropKind::Time | PropKind::Timer) {
        if !(height_fits_the_scene(prop, properties, file, path, errors)
            && depth_layer_fits_the_scene(prop, properties, file, path, errors))
        {
            return None;
        }
        let expr = parse_number_expr(value, kind, properties, file, path, errors)?;
        // «Мир на экране» → «Проверка перед запуском»: `parallax` меньше 0 — ошибка и в постоянных
        // правила, как в объекте, клавише и щелчке.
        if matches!(prop, property::PARALLAX | property::SWAY)
            && let Some(&n) = number_constants(&expr).iter().find(|&&n| n < 0.0)
        {
            if prop == property::SWAY {
                validate_sway(n, properties, file, path, errors);
            } else {
                validate_parallax(n, properties, file, path, errors);
            }
            return None;
        }
        let particle_like = property::PARTICLE_PROPERTIES.contains(&prop)
            || property::FIRE_PROPERTIES.contains(&prop)
            || prop == property::CLOUDS;
        if particle_like
            && !number_constants(&expr)
                .iter()
                .all(|&n| validate_particle_number(prop, n, properties, file, path, errors))
        {
            return None;
        }
        if RUN_JUMP_NUMBERS.contains(&prop)
            && !number_constants(&expr)
                .iter()
                .all(|&n| validate_run_jump_number(prop, n, properties, file, path, errors))
        {
            return None;
        }
        if DRIVE_NUMBERS.contains(&prop)
            && !number_constants(&expr)
                .iter()
                .all(|&n| validate_drive_number(prop, n, properties, file, path, errors))
        {
            return None;
        }
        if prop == property::CLIMB_SPEED
            && !number_constants(&expr)
                .iter()
                .all(|&n| validate_climb_speed(n, file, path, errors))
        {
            return None;
        }
        return Some(SetValue::Number(expr));
    }
    parse_scalar_value(value, prop, properties, images, file, path, errors).map(SetValue::Const)
}

/// `["shift", {"group":..., "by":[dx,dy], "blocked_by":..., "if_blocked":[...]}]` — «Правила
/// игры», требования 12, 15.
#[allow(clippy::too_many_arguments)]
fn parse_shift(
    arr: &[Json],
    properties: &PropertyTable,
    images: &[ImageDecl],
    sounds: &[(String, String)],
    music: &[(String, String)],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ShiftSpec> {
    let settings_json = require_index(arr, 1, file, path, errors)?;
    let obj = expect_object(settings_json, file, path, errors)?;
    reject_unknown_keys(
        obj,
        &["group", "by", "blocked_by", "if_blocked"],
        file,
        path,
        errors,
    );
    let group_json = require_field(obj, "group", file, path, errors)?;
    let group_obj = expect_object(group_json, file, &join(path, "group"), errors)?;
    let group = resolve_selector(group_obj, properties, file, &join(path, "group"), errors);
    let by_json = require_field(obj, "by", file, path, errors)?;
    let by = parse_vec2(by_json, file, &join(path, "by"), errors)?;
    let blocked_by = match obj.get("blocked_by") {
        Some(v) => {
            let bobj = expect_object(v, file, &join(path, "blocked_by"), errors)?;
            Some(resolve_selector(
                bobj,
                properties,
                file,
                &join(path, "blocked_by"),
                errors,
            ))
        }
        None => None,
    };
    if obj.contains_key("if_blocked") && blocked_by.is_none() {
        errors.push(
            file,
            path,
            "if_blocked без blocked_by: отменять нечего".to_string(),
        );
        return None;
    }
    let if_blocked = parse_common_actions(
        obj.get("if_blocked"),
        properties,
        images,
        sounds,
        music,
        file,
        &join(path, "if_blocked"),
        errors,
    );
    Some(ShiftSpec {
        group,
        by,
        blocked_by,
        if_blocked,
    })
}

/// `["turn", {"group":..., "around":..., "dir": 1, "blocked_by":..., "if_blocked":[...]}]` —
/// «Правила игры», требования 13, 15.
#[allow(clippy::too_many_arguments)]
fn parse_turn(
    arr: &[Json],
    properties: &PropertyTable,
    images: &[ImageDecl],
    sounds: &[(String, String)],
    music: &[(String, String)],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<TurnSpec> {
    let settings_json = require_index(arr, 1, file, path, errors)?;
    let obj = expect_object(settings_json, file, path, errors)?;
    reject_unknown_keys(
        obj,
        &["group", "around", "dir", "blocked_by", "if_blocked"],
        file,
        path,
        errors,
    );
    let group_json = require_field(obj, "group", file, path, errors)?;
    let group_obj = expect_object(group_json, file, &join(path, "group"), errors)?;
    let group = resolve_selector(group_obj, properties, file, &join(path, "group"), errors);
    let around_json = require_field(obj, "around", file, path, errors)?;
    let around_obj = expect_object(around_json, file, &join(path, "around"), errors)?;
    let around = resolve_selector(around_obj, properties, file, &join(path, "around"), errors);
    let dir_json = require_field(obj, "dir", file, path, errors)?;
    let dir_num = expect_number(dir_json, file, &join(path, "dir"), errors)?;
    let dir = if dir_num == 1.0 {
        TurnDir::Clockwise
    } else if dir_num == -1.0 {
        TurnDir::CounterClockwise
    } else {
        errors.push(
            file,
            &join(path, "dir"),
            format!("dir должен быть 1 или -1, получено {dir_num}"),
        );
        return None;
    };
    let blocked_by = match obj.get("blocked_by") {
        Some(v) => {
            let bobj = expect_object(v, file, &join(path, "blocked_by"), errors)?;
            Some(resolve_selector(
                bobj,
                properties,
                file,
                &join(path, "blocked_by"),
                errors,
            ))
        }
        None => None,
    };
    if obj.contains_key("if_blocked") && blocked_by.is_none() {
        errors.push(
            file,
            path,
            "if_blocked без blocked_by: отменять нечего".to_string(),
        );
        return None;
    }
    let if_blocked = parse_common_actions(
        obj.get("if_blocked"),
        properties,
        images,
        sounds,
        music,
        file,
        &join(path, "if_blocked"),
        errors,
    );
    Some(TurnSpec {
        group,
        around,
        dir,
        blocked_by,
        if_blocked,
    })
}

#[allow(clippy::too_many_arguments)]
fn parse_common_actions(
    value: Option<&Json>,
    properties: &PropertyTable,
    images: &[ImageDecl],
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
                images,
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

#[allow(clippy::too_many_arguments)]
fn parse_common_action(
    value: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
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
            let kind = require_number_kind(prop, &name, "add", properties, file, path, errors)?;
            let raw_json = require_index(arr, 2, file, path, errors)?;
            let expr =
                parse_number_expr(raw_json, kind, properties, file, &join(path, "[2]"), errors)?;
            Some(CommonAction::Add { prop, expr })
        }
        // «Правила игры», требование 10: `["set", "<свойство>", <значение>]` пишет всем текущим
        // носителям свойства.
        "set" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            let prop = resolve_property(&name, properties, file, &join(path, "[1]"), errors)?;
            let raw_value = require_index(arr, 2, file, path, errors)?;
            let value = parse_set_value(
                raw_value,
                prop,
                properties,
                images,
                file,
                &join(path, "[2]"),
                errors,
            )?;
            Some(CommonAction::SetAll { prop, value })
        }
        // требование 10: `["give", "<признак>", <отбор>]` — отбор обязателен.
        "give" => {
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
                        "give применим только к признаку, у \"{name}\" вид {}",
                        properties.kind(prop).label()
                    ),
                );
                return None;
            }
            if !depth_layer_fits_the_scene(prop, properties, file, path, errors)
                || !engine_alone_writes_ok(prop, file, path, errors)
            {
                return None;
            }
            let selector_json = require_index(arr, 2, file, path, errors)?;
            let selector_obj = expect_object(selector_json, file, &join(path, "[2]"), errors)?;
            let selector =
                resolve_selector(selector_obj, properties, file, &join(path, "[2]"), errors);
            Some(CommonAction::GiveWhere { prop, selector })
        }
        // требование 10: `["take", "<признак>"]` — снимает у всех текущих носителей.
        "take" => {
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
                        "take применим только к признаку, у \"{name}\" вид {}",
                        properties.kind(prop).label()
                    ),
                );
                return None;
            }
            if !depth_layer_fits_the_scene(prop, properties, file, path, errors)
                || !engine_alone_writes_ok(prop, file, path, errors)
            {
                return None;
            }
            Some(CommonAction::TakeAll { prop })
        }
        "shift" => parse_shift(arr, properties, images, sounds, music, file, path, errors)
            .map(CommonAction::Shift),
        "turn" => parse_turn(arr, properties, images, sounds, music, file, path, errors)
            .map(CommonAction::Turn),
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
            let kind = require_number_kind(prop, &name, "add", properties, file, path, errors)?;
            let raw_json = require_index(arr, 2, file, path, errors)?;
            let expr =
                parse_number_expr(raw_json, kind, properties, file, &join(path, "[2]"), errors)?;
            Some(CollideEffect::Add { prop, expr })
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
            let value = parse_set_value(
                raw_value,
                prop,
                properties,
                images,
                file,
                &join(path, "[2]"),
                errors,
            )?;
            Some(CollideEffect::Set { prop, value })
        }
        "shift" | "turn" => {
            errors.push(
                file,
                path,
                format!("{head} — действие группы, его место в do, не в effects"),
            );
            None
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
            if !depth_layer_fits_the_scene(prop, properties, file, path, errors)
                || !engine_alone_writes_ok(prop, file, path, errors)
            {
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
/// Every key of `obj` except `reserved` becomes a template field — shared by `template` itself,
/// a `pick_one` variant and a `pick_one` cell, which all "poverh"-layer the same kind of field
/// («Создание по форме», требования 19–20).
fn parse_field_overrides(
    obj: &serde_json::Map<String, Json>,
    reserved: &[&str],
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> (
    Vec<(PropertyId, TemplateValue)>,
    std::collections::HashSet<PropertyId>,
) {
    let mut fields = Vec::new();
    let mut broken_properties = std::collections::HashSet::new();
    for (name, field_value) in obj {
        if reserved.contains(&name.as_str()) {
            continue;
        }
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
            fields.push((prop, TemplateValue::FromParent(parent_prop)));
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
            fields.push((prop, TemplateValue::Const(v)));
        } else {
            broken_properties.insert(prop);
        }
    }
    (fields, broken_properties)
}

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
    let Some(obj) = expect_object(value, file, path, errors) else {
        return (Vec::new(), std::collections::HashSet::new());
    };
    let (template, broken_properties) =
        parse_field_overrides(obj, &[], properties, images, file, path, errors);
    let shape: std::collections::HashSet<PropertyId> = template.iter().map(|(p, _)| *p).collect();
    validate_image_fill(&shape, file, path, errors);
    validate_follow_mouse_shape(&shape, false, file, path, errors);
    validate_shape_fill(&shape, false, file, path, errors);
    validate_model_fill(&shape, false, file, path, errors);
    validate_deck_shape(&shape, false, file, path, errors);
    validate_road_shape(&shape, false, file, path, errors);
    validate_road_kind_shape(&shape, false, file, path, errors);
    validate_bend_shape(&shape, file, path, errors);
    validate_particles_shape(&shape, false, file, path, errors);
    validate_fire_shape(&shape, false, file, path, errors);
    validate_clouds_shape(&shape, false, file, path, errors);
    let is_layer = template.iter().any(|(prop, value)| {
        *prop == property::PARALLAX
            && matches!(value, TemplateValue::Const(Value::Number(n)) if *n != 1.0)
    });
    let present = template
        .iter()
        .filter(|(_, value)| !matches!(value, TemplateValue::Const(Value::Flag(false))))
        .map(|(prop, _)| *prop)
        .collect();
    validate_depth_layer_only_draws(is_layer, &present, file, path, errors);
    (template, broken_properties)
}

/// `[x, y]` or `{"at": [x, y], ...поля}` — «Создание по форме», требование 20.
fn parse_spawn_cell(
    value: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<SpawnCell> {
    if let Some(arr) = value.as_array()
        && arr.len() == 2
        && arr[0].is_number()
        && arr[1].is_number()
    {
        let at = parse_vec2(value, file, path, errors)?;
        return Some(SpawnCell {
            at,
            fields: Vec::new(),
        });
    }
    let obj = expect_object(value, file, path, errors)?;
    let at_json = require_field(obj, "at", file, path, errors)?;
    let at = parse_vec2(at_json, file, &join(path, "at"), errors)?;
    let (fields, broken) =
        parse_field_overrides(obj, &["at"], properties, images, file, path, errors);
    if !broken.is_empty() {
        return None;
    }
    Some(SpawnCell { at, fields })
}

/// One `pick_one` variant — требование 20: an object with a non-empty `cells`, plus fields of its
/// own layered over `template`.
fn parse_spawn_variant(
    value: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<SpawnVariant> {
    let obj = expect_object(value, file, path, errors)?;
    let cells_json = require_field(obj, "cells", file, path, errors)?;
    let cells_arr = expect_array(cells_json, file, &join(path, "cells"), errors)?;
    if cells_arr.is_empty() {
        errors.push(file, &join(path, "cells"), "cells не может быть пустым");
        return None;
    }
    let mut ok = true;
    let mut cells = Vec::with_capacity(cells_arr.len());
    for (i, c) in cells_arr.iter().enumerate() {
        match parse_spawn_cell(
            c,
            properties,
            images,
            file,
            &join(&join(path, "cells"), &format!("[{i}]")),
            errors,
        ) {
            Some(cell) => cells.push(cell),
            None => ok = false,
        }
    }
    let (fields, broken) =
        parse_field_overrides(obj, &["cells"], properties, images, file, path, errors);
    if !ok || !broken.is_empty() {
        return None;
    }
    Some(SpawnVariant { cells, fields })
}

/// `pick_one` — требование 20: a non-empty list of variants.
fn parse_pick_one(
    value: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<SpawnVariant>> {
    let arr = expect_array(value, file, path, errors)?;
    if arr.is_empty() {
        errors.push(file, path, "pick_one не может быть пустым списком");
        return None;
    }
    let mut ok = true;
    let mut variants = Vec::with_capacity(arr.len());
    for (i, v) in arr.iter().enumerate() {
        match parse_spawn_variant(
            v,
            properties,
            images,
            file,
            &join(path, &format!("[{i}]")),
            errors,
        ) {
            Some(variant) => variants.push(variant),
            None => ok = false,
        }
    }
    ok.then_some(variants)
}

/// Known keys across every rule kind — used to still catch a typo in a rule whose own `kind` is
/// missing or unrecognized, rather than let it hide behind the `kind` error until a later pass.
const ALL_RULE_KEYS: &[&str] = &[
    "kind", "for", "a", "b", "effects", "do", "when", "where", "template", "pick_one", "avoid",
    "solid", "one_way", "side", "ladders",
];

const DRIVE_IN_FLAT_SCENE: &str =
    "drive есть только в трёхмерной сцене: у scene в game.json нет camera";

const CLIMB_IN_FLAT_SCENE: &str =
    "climb есть только в трёхмерной сцене: у scene в game.json нет camera";

/// «Езда по дорогам», требование 9: `side` необязателен — `right`, `left` или `middle`, по умолчанию
/// `right`. `None` — значение не годится, ошибка уже записана.
fn parse_drive_side(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Side> {
    let Some(value) = obj.get("side") else {
        return Some(Side::Right);
    };
    let side_path = join(path, "side");
    let name = expect_string(value, file, &side_path, errors)?;
    match name.as_str() {
        "right" => Some(Side::Right),
        "left" => Some(Side::Left),
        "middle" => Some(Side::Middle),
        other => {
            errors.push(
                file,
                &side_path,
                format!(
                    "side должен быть \"right\", \"left\" или \"middle\", получено \"{other}\""
                ),
            );
            None
        }
    }
}

const RUN_JUMP_IN_3D_SCENE: &str =
    "run_jump есть только в плоской сцене: у scene в game.json есть camera";

/// Необязательный отбор правила под ключом `key`: `Some(None)` — ключа нет, `None` — он есть, но
/// не отбор.
fn parse_optional_selector(
    obj: &serde_json::Map<String, Json>,
    key: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<Selector>> {
    let Some(value) = obj.get(key) else {
        return Some(None);
    };
    let key_path = join(path, key);
    let selector_obj = expect_object(value, file, &key_path, errors)?;
    Some(Some(resolve_selector(
        selector_obj,
        properties,
        file,
        &key_path,
        errors,
    )))
}

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
        // «Ходьба», требование 22: `avoid` необязателен.
        Some("walk") => {
            reject_unknown_keys(obj, &["kind", "for", "avoid"], file, &path, errors);
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            let avoid = match obj.get("avoid") {
                Some(v) => {
                    let avoid_obj = expect_object(v, file, &join(&path, "avoid"), errors)?;
                    Some(resolve_selector(
                        avoid_obj,
                        properties,
                        file,
                        &join(&path, "avoid"),
                        errors,
                    ))
                }
                None => None,
            };
            Some((Rule::Walk { for_, avoid }, std::collections::HashSet::new()))
        }
        // «Бег и прыжок», требование 1: `solid` и `one_way` необязательны, но если названы — это отбор.
        Some("run_jump") => {
            reject_unknown_keys(
                obj,
                &["kind", "for", "solid", "one_way"],
                file,
                &path,
                errors,
            );
            if properties.three_d() {
                errors.push(file, &path, RUN_JUMP_IN_3D_SCENE);
                return None;
            }
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            let solid = parse_optional_selector(obj, "solid", properties, file, &path, errors)?;
            let one_way = parse_optional_selector(obj, "one_way", properties, file, &path, errors)?;
            Some((
                Rule::RunJump {
                    for_,
                    solid,
                    one_way,
                },
                std::collections::HashSet::new(),
            ))
        }
        // «Езда по дорогам», требование 9.
        Some("drive") => {
            reject_unknown_keys(obj, &["kind", "for", "side"], file, &path, errors);
            if !properties.three_d() {
                errors.push(file, &path, DRIVE_IN_FLAT_SCENE);
                return None;
            }
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            let side = parse_drive_side(obj, file, &path, errors)?;
            Some((Rule::Drive { for_, side }, std::collections::HashSet::new()))
        }
        // «Лазание»: `for` и `ladders` — отборы, оба обязательны.
        Some("climb") => {
            reject_unknown_keys(obj, &["kind", "for", "ladders"], file, &path, errors);
            if !properties.three_d() {
                errors.push(file, &path, CLIMB_IN_FLAT_SCENE);
                return None;
            }
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            let ladders_json = require_field(obj, "ladders", file, &path, errors)?;
            let ladders_path = join(&path, "ladders");
            let ladders_obj = expect_object(ladders_json, file, &ladders_path, errors)?;
            let ladders = resolve_selector(ladders_obj, properties, file, &ladders_path, errors);
            Some((
                Rule::Climb { for_, ladders },
                std::collections::HashSet::new(),
            ))
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
                images,
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
        // «Правила игры», требование 4: `for` обязателен, `when` необязателен (нет — верно
        // всегда), `do` обязателен.
        Some("check") => {
            reject_unknown_keys(obj, &["kind", "for", "when", "do"], file, &path, errors);
            let for_json = require_field(obj, "for", file, &path, errors)?;
            let for_obj = expect_object(for_json, file, &join(&path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(&path, "for"), errors);
            let when = match obj.get("when") {
                Some(v) => Some(parse_condition(
                    v,
                    properties,
                    file,
                    &join(&path, "when"),
                    errors,
                )?),
                None => None,
            };
            let do_json = require_field(obj, "do", file, &path, errors)?;
            let do_ = parse_common_actions(
                Some(do_json),
                properties,
                images,
                sounds,
                music,
                file,
                &join(&path, "do"),
                errors,
            );
            Some((
                Rule::Check { for_, when, do_ },
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
                images,
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
                &["kind", "when", "where", "template", "pick_one", "do"],
                file,
                &path,
                errors,
            );
            let when_json = require_field(obj, "when", file, &path, errors)?;
            let when =
                parse_spawn_condition(when_json, properties, file, &join(&path, "when"), errors)?;
            let where_json = obj.get("where");
            // «Создание по форме», требование 19: `at_parent`, `random_cell` или `{"at": [x,
            // y]}` — клетка сцены, названная буквально.
            let place = match where_json {
                Some(Json::String(s)) if s == "at_parent" => SpawnPlace::AtParent,
                Some(Json::String(s)) if s == "random_cell" => SpawnPlace::RandomCell,
                Some(Json::Object(o)) => {
                    reject_unknown_keys(o, &["at"], file, &join(&path, "where"), errors);
                    let at_json = require_field(o, "at", file, &join(&path, "where"), errors)?;
                    let at = parse_vec2(at_json, file, &join(&path, "where → at"), errors)?;
                    SpawnPlace::Cell(at)
                }
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
                        "ожидалось \"at_parent\", \"random_cell\" или {\"at\": [x, y]}".to_string(),
                    );
                    return None;
                }
            };
            if matches!(place, SpawnPlace::AtParent) && when.parent_of.is_none() {
                errors.push(
                    file,
                    &join(&path, "where"),
                    "at_parent недоступен без after_move_of в условии: у него нет родителя"
                        .to_string(),
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
            let pick_one = match obj.get("pick_one") {
                Some(v) => Some(parse_pick_one(
                    v,
                    properties,
                    images,
                    file,
                    &join(&path, "pick_one"),
                    errors,
                )?),
                None => None,
            };
            validate_spawned_animations(
                &template,
                pick_one.as_deref().unwrap_or_default(),
                properties,
                file,
                &path,
                errors,
            );
            // «Создание по форме», требование 20: поля идут template → вариант → клетка, и
            // `from_parent` может прийти с любого из трёх уровней, не только template — путь в
            // ошибке называет то поле, а не всегда template.
            if when.parent_of.is_none() {
                let find_from_parent = |fields: &[(PropertyId, TemplateValue)]| {
                    fields
                        .iter()
                        .find(|(_, v)| matches!(v, TemplateValue::FromParent(_)))
                        .map(|(prop, _)| *prop)
                };
                let from_parent_path = find_from_parent(&template)
                    .map(|prop| join(&join(&path, "template"), properties.name(prop)))
                    .or_else(|| {
                        pick_one.as_deref().and_then(|variants| {
                            variants.iter().enumerate().find_map(|(i, variant)| {
                                let variant_path =
                                    join(&join(&path, "pick_one"), &format!("[{i}]"));
                                if let Some(prop) = find_from_parent(&variant.fields) {
                                    return Some(join(&variant_path, properties.name(prop)));
                                }
                                let cells_path = join(&variant_path, "cells");
                                variant.cells.iter().enumerate().find_map(|(j, cell)| {
                                    find_from_parent(&cell.fields).map(|prop| {
                                        let cell_path = join(&cells_path, &format!("[{j}]"));
                                        join(&cell_path, properties.name(prop))
                                    })
                                })
                            })
                        })
                    });
                if let Some(from_parent_path) = from_parent_path {
                    errors.push(
                        file,
                        &from_parent_path,
                        "from_parent недоступен без after_move_of в условии: у него нет родителя"
                            .to_string(),
                    );
                }
            }
            let do_ = parse_common_actions(
                obj.get("do"),
                properties,
                images,
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
                    pick_one,
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
    let mut mark = |edit: &KeyEdit| {
        if matches!(edit.value, EditValue::Const(Value::Flag(false))) {
            removable.insert(edit.property);
        } else {
            maybe.insert(edit.property);
        }
    };
    if let Some(table) = &obj.keys {
        for binding in table.values() {
            for edit in binding.press.iter().chain(binding.release.iter()) {
                mark(edit);
            }
        }
    }
    // «Мышь в мире», требование 19: `on_click` пишет так же, как `press` клавиши — та же
    // widening-логика.
    if let Some(edits) = &obj.on_click {
        for edit in edits {
            mark(edit);
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
///
/// «Правила игры», требование 10: the common `["give", prop, selector]`/`["take", prop]`/
/// `["set", prop, value]` in a `check`/`delete`/`spawn`/`collide` rule's own `do` (or an
/// `if_blocked` nested in its `shift`/`turn`) widen shapes exactly as much as a collide effect
/// does — a `check` rule's `give` is just as real a source of a flag as a collide rule's, and a
/// selector elsewhere that only ever matches through it must not warn "заведомо не подходит ни
/// одному объекту". `GiveWhere` carries its own selector (`CommonAction::GiveWhere`'s `world.ids()
/// .filter(selector_matches)` in `step.rs`), unlike a collide effect's implicit "whichever object
/// played this side" — so it widens whatever shapes that selector itself picks out, not a rule's
/// own `for`/`a`/`b`. `TakeAll`/`SetAll(Flag(false))` touch *every* current holder in the world,
/// with no selector of their own, so they widen `removable` on every shape unconditionally.
/// `SetAll` to anything else only ever writes a holder that already has `prop`
/// (`execute_common_actions`'s `if world.has(id, *prop)` guard) — unlike a collide effect's own
/// `Set`, which writes the matched object whether or not it already carries `prop` — so it can
/// never make a selector newly satisfiable and is not tracked here. No reachability gate on the
/// enclosing rule's own selector, unlike the collide pass above: `check`/`delete`'s `for` and
/// `spawn`'s `when` are conditions, not a single selector these two sides can be read off of, and
/// running this pass regardless of reachability only ever widens `maybe`/`removable` further —
/// the safe direction for a warning that must not fire on a selector the game can actually reach.
/// Folded into the very same `while changed` loop as the collide pass, since either one's
/// widening in a pass can be what makes the other's selector newly matchable.
fn widen_shapes_by_rule_actions(shapes: &mut [PossibleShape], rules: &RuleSet) {
    fn collect_do_effects(
        actions: &[CommonAction],
        given: &mut Vec<(PropertyId, Selector)>,
        taken: &mut Vec<PropertyId>,
    ) {
        walk_common_actions(actions, &mut |action| match action {
            CommonAction::GiveWhere { prop, selector } => {
                given.push((*prop, selector.clone()));
            }
            CommonAction::TakeAll { prop } => taken.push(*prop),
            CommonAction::SetAll {
                prop,
                value: SetValue::Const(Value::Flag(false)),
            } => taken.push(*prop),
            _ => {}
        });
    }

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
                            value: SetValue::Const(Value::Flag(false)),
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

        // «Бег и прыжок»: `on_ground` ставит сам движок объектам под `run_jump` — отбор по нему
        // подходит им, хотя ни в файле, ни в правилах признака нет.
        for rule in &rules.rules {
            let Rule::RunJump { for_, .. } = rule else {
                continue;
            };
            for ps in shapes.iter_mut() {
                if matches_shape(for_, &ps.shape) {
                    changed |= ps.shape.maybe.insert(property::ON_GROUND);
                }
            }
        }

        // «Езда по дорогам»: `drive_to` убирает сам движок, когда машина доехала, — отбор по его
        // отсутствию находит стоящих, хотя ни одно правило файла признака не снимает.
        for rule in &rules.rules {
            let Rule::Drive { for_, .. } = rule else {
                continue;
            };
            for ps in shapes.iter_mut() {
                if matches_shape(for_, &ps.shape) {
                    changed |= ps.shape.maybe.insert(property::DRIVE_TO);
                    changed |= ps.shape.removable.insert(property::DRIVE_TO);
                }
            }
        }

        // «Лазание»: просьбу `climb` и признак `climbing` ставит и снимает сам движок — отбор по ним подходит
        // лезущим, хотя ни одно правило файла признака не ставит и не снимает.
        for rule in &rules.rules {
            let Rule::Climb { for_, .. } = rule else {
                continue;
            };
            for ps in shapes.iter_mut() {
                if matches_shape(for_, &ps.shape) {
                    for prop in [property::CLIMB, property::CLIMBING] {
                        changed |= ps.shape.maybe.insert(prop);
                        changed |= ps.shape.removable.insert(prop);
                    }
                }
            }
        }

        for rule in &rules.rules {
            let do_ = match rule {
                Rule::Check { do_, .. }
                | Rule::Delete { do_, .. }
                | Rule::Spawn { do_, .. }
                | Rule::Collide { do_, .. } => do_,
                Rule::Move { .. }
                | Rule::Walk { .. }
                | Rule::RunJump { .. }
                | Rule::Climb { .. }
                | Rule::Drive { .. } => continue,
            };
            let mut given = Vec::new();
            let mut taken = Vec::new();
            collect_do_effects(do_, &mut given, &mut taken);
            for (prop, selector) in &given {
                for ps in shapes.iter_mut() {
                    if matches_shape(selector, &ps.shape) {
                        changed |= ps.shape.maybe.insert(*prop);
                    }
                }
            }
            for prop in &taken {
                for ps in shapes.iter_mut() {
                    changed |= ps.shape.removable.insert(*prop);
                }
            }
        }
    }
}

/// "движок берёт все объекты, какие вообще могут существовать": scene objects plus one shape
/// per spawn template, each widened by every property the game could still hand it or take away
/// at runtime through `give`/`take`/`set` or a `keys` edit — see
/// `widen_shapes_by_rule_actions` and `keys_edited_properties`.
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
        if let Rule::Spawn {
            template, pick_one, ..
        } = rule
        {
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

            let mut maybe: std::collections::HashSet<PropertyId> = template
                .iter()
                .filter(|(p, tv)| {
                    properties.kind(*p) == PropKind::Flag
                        && matches!(tv, TemplateValue::FromParent(_))
                })
                .map(|(p, _)| *p)
                .collect();
            // «Создание по форме», требование 20: каждый вариант/клетка может добавить своё
            // поле поверх шаблона — какая именно фигура выпадет, решает `pick_one` во время
            // партии, так что здесь известно только «может появиться», не «появится всегда».
            if let Some(variants) = pick_one {
                for variant in variants {
                    maybe.extend(variant.fields.iter().map(|(p, _)| *p));
                    for cell in &variant.cells {
                        maybe.extend(cell.fields.iter().map(|(p, _)| *p));
                    }
                }
                maybe.retain(|p| !certain.contains(p));
            }

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
    widen_shapes_by_rule_actions(&mut shapes, rules);
    shapes
}

/// «Картинки» → «Отражение», требование 13: текст ошибки `flip_x` без картинки.
const FLIP_X_WITHOUT_IMAGE: &str = "flip_x задан объекту, которому картинка не достанется никак: ни в файле, ни клавишей, ни правилом, ни кодом";

/// «Слои глубины», требование 23: текст ошибки `repeat_x` без заливки.
const REPEAT_X_WITHOUT_FILL: &str = "repeat_x задан объекту, которому ни картинка, ни цвет не достанутся никак: ни в файле, ни клавишей, ни правилом, ни кодом";

/// «Картинки»: engine properties that only mean something on top of a picture — `opacity`
/// multiplies it, `flip_x` (требование 13) mirrors it — each with its own «картинки нет» message.
const NEEDS_IMAGE: [(PropertyId, &str); 2] = [
    (property::OPACITY, OPACITY_WITHOUT_IMAGE),
    (property::FLIP_X, FLIP_X_WITHOUT_IMAGE),
];

/// «Картинки»: `opacity` or `flip_x` declared on a scene object or spawn template needs `image`,
/// but not necessarily from the object's own file shape — a `keys` edit or a collide rule's
/// `set`/`give` can hand it `image` once the game is running (`possible_shapes`'s `maybe`), and a
/// `broken_properties` `image` is already reported by its own error, so its presence is
/// known-incomplete rather than known-absent (see `validate_image_fill`). Run once here, after
/// `possible_shapes` has widened every shape, instead of at parse time like the `color`+`image`
/// check, which needs no such widening. A property that reaches an object only at runtime (a
/// `keys` edit, a rule, or code) is not checked — «Крайние случаи»: the prestart check only looks
/// at the scene and templates. An object with `shape` is skipped: `validate_shape_fill` already
/// reports its `opacity`/`flip_x`. «Слои глубины», требование 23: тем же способом проверяется
/// `repeat_x`, но заливкой ему служит и картинка, и цвет, а слово `color` в тексте кода молчит
/// проверку так же, как слово `image`.
fn validate_reachable_image(shapes: &[PossibleShape], code: Option<&str>, errors: &mut ErrorSink) {
    // «Код игры»: слово `image` в тексте кода — код тоже мог дать объекту картинку, проверка
    // здесь бессильна отличить это от настоящей нехватки источника, так что просто не спорит.
    let code_gives_image = code.is_some_and(|c| code_mentions_word(c, "image"));
    let code_gives_fill = code_gives_image || code.is_some_and(|c| code_mentions_word(c, "color"));
    for ps in shapes {
        let reachable = |prop: PropertyId| {
            ps.shape.certain.contains(&prop)
                || ps.shape.maybe.contains(&prop)
                || ps.shape.broken_properties.contains(&prop)
        };
        let has_image = reachable(property::IMAGE);
        if !code_gives_fill
            && !has_image
            && !reachable(property::COLOR)
            && ps.shape.certain.contains(&property::REPEAT_X)
        {
            errors.push(&ps.file, &ps.path, REPEAT_X_WITHOUT_FILL);
        }
        if code_gives_image
            || has_image
            || ps.shape.certain.contains(&property::SHAPE)
            || ps.shape.certain.contains(&property::MODEL)
        {
            continue;
        }
        for (prop, message) in NEEDS_IMAGE {
            if ps.shape.certain.contains(&prop) {
                errors.push(&ps.file, &ps.path, message);
            }
        }
    }
}

/// «Бег и прыжок», требование 21: объект из `scene.json` под `move` и под `run_jump` двигался бы дважды.
/// Объект под `run_jump` получает `velocity` от правила, так что `move` по `velocity` берёт и того, у
/// кого её в файле нет.
fn validate_move_apart_from_run_jump(
    scene: &[ParsedObject],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    scene_file: &str,
    errors: &mut ErrorSink,
) {
    let labeled = rules.rules.iter().zip(rule_meta);
    for obj in scene {
        let selects = |selector: &Selector, owns_velocity: bool| {
            let has = |prop: &PropertyId| {
                obj.shape.contains(prop) || (owns_velocity && *prop == property::VELOCITY)
            };
            selector.has.iter().all(has) && !selector.without.iter().any(has)
        };
        let Some(run_jump) = labeled.clone().find_map(|(rule, meta)| match rule {
            Rule::RunJump { for_, .. } if selects(for_, false) => Some(meta.file_index),
            _ => None,
        }) else {
            continue;
        };
        let Some(movement) = labeled.clone().find_map(|(rule, meta)| match rule {
            Rule::Move { for_ } if selects(for_, true) => Some(meta.file_index),
            _ => None,
        }) else {
            continue;
        };
        errors.push(
            scene_file,
            &obj.path,
            format!(
                "объект{} попал и под move (rules[{movement}]), и под run_jump (rules[{run_jump}]): его двигало бы дважды",
                name_suffix(obj)
            ),
        );
    }
}

/// «Езда по дорогам», требование 30: объект из `scene.json` под `drive` и под `move`, `walk` или
/// `run_jump` двигался бы дважды.
fn validate_drive_apart_from_other_movers(
    scene: &[ParsedObject],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    scene_file: &str,
    errors: &mut ErrorSink,
) {
    let labeled = rules.rules.iter().zip(rule_meta);
    for obj in scene {
        let selects = |selector: &Selector| {
            selector.has.iter().all(|prop| obj.shape.contains(prop))
                && !selector.without.iter().any(|prop| obj.shape.contains(prop))
        };
        let Some(drive) = labeled.clone().find_map(|(rule, meta)| match rule {
            Rule::Drive { for_, .. } if selects(for_) => Some(meta.file_index),
            _ => None,
        }) else {
            continue;
        };
        let other = labeled.clone().find_map(|(rule, meta)| match rule {
            Rule::Move { for_ } if selects(for_) => Some(("move", meta.file_index)),
            Rule::Walk { for_, .. } if selects(for_) => Some(("walk", meta.file_index)),
            Rule::RunJump { for_, .. } if selects(for_) => Some(("run_jump", meta.file_index)),
            _ => None,
        });
        if let Some((kind, index)) = other {
            errors.push(
                scene_file,
                &obj.path,
                format!(
                    "объект{} попал и под {kind} (rules[{index}]), и под drive (rules[{drive}]): его двигало бы дважды",
                    name_suffix(obj)
                ),
            );
        }
    }
}

/// «Лазание» → «Проверка перед запуском»: модель объекта из `scene.json` под `climb` — со ступнями `feet` и кистями
/// `hands`: на перекладины ставят именно их.
fn validate_climbers_have_limbs(
    scene: &[ParsedObject],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    models: &[ModelDecl],
    scene_file: &str,
    errors: &mut ErrorSink,
) {
    let labeled = rules.rules.iter().zip(rule_meta);
    for obj in scene {
        let Some(Value::Text(name)) = find_value(&obj.values, property::MODEL) else {
            continue;
        };
        let Some(model) = models.iter().find(|decl| &decl.name == name) else {
            continue;
        };
        let missing: Vec<&str> = [
            ("feet", model.feet.is_none()),
            ("hands", model.hands.is_none()),
        ]
        .into_iter()
        .filter_map(|(key, absent)| absent.then_some(key))
        .collect();
        if missing.is_empty() {
            continue;
        }
        let selects = |selector: &Selector| {
            selector.has.iter().all(|prop| obj.shape.contains(prop))
                && !selector.without.iter().any(|prop| obj.shape.contains(prop))
        };
        let Some(climb) = labeled.clone().find_map(|(rule, meta)| match rule {
            Rule::Climb { for_, .. } if selects(for_) => Some(meta.file_index),
            _ => None,
        }) else {
            continue;
        };
        errors.push(
            scene_file,
            &obj.path,
            format!(
                "объект{} под climb (rules[{climb}]), а у его модели \"{name}\" в game.json нет {}: кисти и ступни на перекладины ставят по ним",
                name_suffix(obj),
                missing.join(" и ")
            ),
        );
    }
}

/// «Камера игрока», требование 24: клавиша камеры, названная в `keys` объекта сцены или экрана,
/// достаётся привязке — камера её не берёт. Предупреждение, не ошибка.
fn warn_taken_camera_keys(
    scene: &[ParsedObject],
    scene_file: &str,
    screens: &[Screen],
    screens_file: &str,
    errors: &mut ErrorSink,
) {
    for obj in scene {
        let Some(keys) = &obj.keys else {
            continue;
        };
        let owner = obj.name.as_deref().map_or_else(
            || format!("объекта {}", obj.path),
            |name| format!("объекта „{name}“"),
        );
        for code in camera::pan_key_codes().filter(|code| keys.contains_key(*code)) {
            errors.push_warning(
                scene_file,
                &join(&join(&obj.path, "keys"), code),
                format!("клавиша {code} у {owner}: камера её не берёт"),
            );
        }
    }
    for (index, screen) in screens.iter().enumerate() {
        for code in camera::pan_key_codes().filter(|code| screen.keys.contains_key(*code)) {
            errors.push_warning(
                screens_file,
                &join(&format!("screens[{index}]"), &join("keys", code)),
                format!(
                    "клавиша {code} у экрана „{}“: камера её не берёт",
                    screen.name
                ),
            );
        }
    }
}

/// «Езда по дорогам», требование 31: игра называет `road`, но дорог в ней не будет — в `scene.json`
/// их нет, ни шаблон, ни клавиша их не дают и в коде нет слова `road`.
fn warn_road_without_roads(
    used: &std::collections::HashSet<PropertyId>,
    shapes: &[PossibleShape],
    code: Option<&str>,
    rules_file: &str,
    errors: &mut ErrorSink,
) {
    let has_road = shapes.iter().any(|ps| {
        ps.shape.certain.contains(&property::ROAD) || ps.shape.maybe.contains(&property::ROAD)
    });
    if used.contains(&property::ROAD)
        && !has_road
        && !code.is_some_and(|c| code_mentions_word(c, "road"))
    {
        errors.push_warning(
            rules_file,
            "",
            "игра называет road, но ни одной дороги в scene.json нет и в коде нет слова road: машины поедут по прямой",
        );
    }
}

/// «Камера», требование 37: `camera_follows` в игре без `view_height` ни на что не влияет —
/// предупреждение, не ошибка.
fn validate_camera_follows_needs_view_height(
    shapes: &[PossibleShape],
    view_height: Option<f64>,
    errors: &mut ErrorSink,
) {
    if view_height.is_some() {
        return;
    }
    for ps in shapes {
        if ps.shape.certain.contains(&property::CAMERA_FOLLOWS)
            || ps.shape.maybe.contains(&property::CAMERA_FOLLOWS)
        {
            errors.push_warning(
                &ps.file,
                &ps.path,
                "camera_follows в игре без view_height: на отрисовку он не влияет",
            );
        }
    }
}

/// «Код игры» → «Проверка перед запуском»: whether `word` appears in `code`'s text as its own
/// identifier, not merely as a substring of a longer one — such a mention counts as "used" for
/// the «объявлено и не используется» warnings, and a mention of `image` specifically excuses
/// `validate_reachable_image`.
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

/// Calls `check` on every `run`'s function name `actions` (a `do` or an `if_blocked`) reaches,
/// including inside a `shift`/`turn`'s own `if_blocked` (`walk_common_actions`) — «Правила
/// игры», требование 17.
fn check_common_actions_run(
    actions: &[CommonAction],
    label: &str,
    check: &dyn Fn(&str, &str, &mut ErrorSink),
    errors: &mut ErrorSink,
) {
    walk_common_actions(actions, &mut |action| {
        if let CommonAction::Run(name) = action {
            check(name, label, errors);
        }
    });
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
                check_common_actions_run(do_, &join(&label, "do"), &check, errors);
            }
            Rule::Check { do_, .. } | Rule::Delete { do_, .. } | Rule::Spawn { do_, .. } => {
                check_common_actions_run(do_, &join(&label, "do"), &check, errors);
            }
            Rule::Move { .. }
            | Rule::Walk { .. }
            | Rule::RunJump { .. }
            | Rule::Climb { .. }
            | Rule::Drive { .. } => {}
        }
    }
}

/// `do_` from any rule kind that has one — `Move`/`Walk`/`RunJump`/`Climb`/`Drive` have none.
fn common_actions(rule: &Rule) -> &[CommonAction] {
    match rule {
        Rule::Collide { do_, .. }
        | Rule::Check { do_, .. }
        | Rule::Delete { do_, .. }
        | Rule::Spawn { do_, .. } => do_,
        Rule::Move { .. }
        | Rule::Walk { .. }
        | Rule::RunJump { .. }
        | Rule::Climb { .. }
        | Rule::Drive { .. } => &[],
    }
}

/// Visits every action `actions` reaches, including a `shift`/`turn`'s own `if_blocked` — a `do`
/// (or an `if_blocked`, which is itself a `do`) can hide another `shift`/`turn` inside that
/// `if_blocked`, and a validation that must see everything a rule's `do` can run (`play_sound` for
/// «unused sound»/«no toggle_sound», `give`/`take`/`set` for shape widening) needs the same
/// recursive reach `check_do_selectors` already gives selectors.
fn walk_common_actions<'a>(actions: &'a [CommonAction], visit: &mut impl FnMut(&'a CommonAction)) {
    for action in actions {
        visit(action);
        match action {
            CommonAction::Shift(spec) => walk_common_actions(&spec.if_blocked, visit),
            CommonAction::Turn(spec) => walk_common_actions(&spec.if_blocked, visit),
            _ => {}
        }
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
            Rule::Check { for_, when, .. } => {
                if let Some(cond) = when {
                    let mut props = Vec::new();
                    let mut wants_position_size = false;
                    collect_condition_requirements(cond, &mut props, &mut wants_position_size);
                    for prop in props {
                        require(
                            &candidates,
                            file,
                            for_,
                            prop,
                            properties,
                            &label,
                            &mut seen,
                            errors,
                        );
                    }
                    if wants_position_size {
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
                }
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
            // «Формат игры»: `fewer_than`/`after_move_of` не читают положение самого кандидата
            // на удаление — само условие спрашивает про отбор `of`, не про `for_`.
            Rule::Delete { for_, when, .. } => {
                let mut props = Vec::new();
                let mut wants_position_size = false;
                collect_condition_requirements(when, &mut props, &mut wants_position_size);
                for prop in props {
                    require(
                        &candidates,
                        file,
                        for_,
                        prop,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                }
                if wants_position_size {
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
            }
            Rule::Spawn {
                when,
                template,
                pick_one,
                ..
            } => {
                if let Some(of) = &when.parent_of {
                    for parent_prop in spawn_from_parent_fields(template, pick_one) {
                        require(
                            &candidates,
                            file,
                            of,
                            parent_prop,
                            properties,
                            &label,
                            &mut seen,
                            errors,
                        );
                    }
                }
            }
            // «Ходьба», требование 36: объекту под `walk` не хватает `position`, `size` или
            // `walk_speed` — `avoid`'s own candidates need neither: an obstacle without them is
            // simply not an obstacle (`step::apply_walk_rule`'s own `rect_of` already skips it).
            Rule::Walk { for_, .. } => {
                for prop in [property::POSITION, property::SIZE, property::WALK_SPEED] {
                    require(
                        &candidates,
                        file,
                        for_,
                        prop,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                }
            }
            // «Езда по дорогам», требование 30: объекту под `drive` не хватает `position`, `size` или
            // `drive_speed` — `drive_to` не обязателен: машина без него стоит.
            Rule::Drive { for_, .. } => {
                for prop in [property::POSITION, property::SIZE, property::DRIVE_SPEED] {
                    require(
                        &candidates,
                        file,
                        for_,
                        prop,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                }
            }
            // «Лазание»: объекту под `climb` не хватает `position`, `size` или `climb_speed`; `ladders` ничего не
            // требуют: объект без перекладин в модели просто не лестница.
            Rule::Climb { for_, .. } => {
                for prop in [property::POSITION, property::SIZE, property::CLIMB_SPEED] {
                    require(
                        &candidates,
                        file,
                        for_,
                        prop,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
                }
            }
            // «Бег и прыжок», требование 21: объекту под `run_jump` не хватает `position`, `size`,
            // `run_speed` или `jump_height` — `solid` и `one_way` ничего не требуют: объект без
            // `position` и `size` просто не твёрдое.
            Rule::RunJump { for_, .. } => {
                for prop in [
                    property::POSITION,
                    property::SIZE,
                    property::RUN_SPEED,
                    property::JUMP_HEIGHT,
                ] {
                    require(
                        &candidates,
                        file,
                        for_,
                        prop,
                        properties,
                        &label,
                        &mut seen,
                        errors,
                    );
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
        Condition::All(list) | Condition::Any(list) => {
            for c in list {
                mark_condition_used(used, c);
            }
        }
        Condition::Not(inner) => mark_condition_used(used, inner),
    }
}

/// «check», требование 4–9: every `Compare`/`OutsideScene` leaf in `condition` names a property
/// (or needs `position`/`size`) the checked object must carry — collected here, once, for both
/// `validate_property_sufficiency` and the "unused property" pass to share.
fn collect_condition_requirements(
    condition: &Condition,
    props: &mut Vec<PropertyId>,
    wants_position_size: &mut bool,
) {
    match condition {
        Condition::Compare { prop, .. } => props.push(*prop),
        Condition::OutsideScene => *wants_position_size = true,
        Condition::FewerThan { .. } | Condition::AfterMoveOf { .. } => {}
        Condition::All(list) | Condition::Any(list) => {
            for c in list {
                collect_condition_requirements(c, props, wants_position_size);
            }
        }
        Condition::Not(inner) => collect_condition_requirements(inner, props, wants_position_size),
    }
}

fn mark_spawn_condition_used(
    used: &mut std::collections::HashSet<PropertyId>,
    condition: &SpawnCondition,
) {
    mark_condition_used(used, &condition.condition);
}

fn mark_common_actions_used(
    used: &mut std::collections::HashSet<PropertyId>,
    actions: &[CommonAction],
) {
    walk_common_actions(actions, &mut |action| match action {
        CommonAction::Add { prop, .. }
        | CommonAction::SetAll { prop, .. }
        | CommonAction::TakeAll { prop } => {
            used.insert(*prop);
        }
        CommonAction::GiveWhere { prop, selector } => {
            used.insert(*prop);
            mark_selector_used(used, selector);
        }
        CommonAction::Shift(spec) => {
            mark_selector_used(used, &spec.group);
            if let Some(of) = &spec.blocked_by {
                mark_selector_used(used, of);
            }
        }
        CommonAction::Turn(spec) => {
            mark_selector_used(used, &spec.group);
            mark_selector_used(used, &spec.around);
            if let Some(of) = &spec.blocked_by {
                mark_selector_used(used, of);
            }
        }
        CommonAction::EndGame(_) | CommonAction::PlaySound(_) | CommonAction::Run(_) => {}
    });
}

/// From-parent fields across `template` and every `pick_one` variant/cell — «Создание по форме»,
/// требования 19–20.
fn spawn_from_parent_fields<'a>(
    template: &'a [(PropertyId, TemplateValue)],
    pick_one: &'a Option<Vec<SpawnVariant>>,
) -> impl Iterator<Item = PropertyId> + 'a {
    template
        .iter()
        .chain(pick_one.iter().flatten().flat_map(|v| {
            v.fields
                .iter()
                .chain(v.cells.iter().flat_map(|c| c.fields.iter()))
        }))
        .filter_map(|(_, tv)| match tv {
            TemplateValue::FromParent(p) => Some(*p),
            TemplateValue::Const(_) => None,
        })
}

/// Every property referenced anywhere in the loaded data: on a scene object (including a value
/// set by a `keys` edit), or by a rule's selector, condition, action or spawn template. Whether
/// the reference is itself valid is `validate_property_sufficiency`'s job, not this one — a
/// property named only inside a broken rule still counts as "used" here.
/// «Формат игры», требование 35: a screen element's own `{имя.свойство}` substitution counts as
/// used, too — before this, a property visible only on a screen got a false "not used" warning.
fn mark_screen_elements_used(used: &mut std::collections::HashSet<PropertyId>, screens: &[Screen]) {
    for screen in screens {
        for element in &screen.elements {
            if let Element::List { for_, sort_by, .. } = element {
                mark_selector_used(used, for_);
                used.extend(sort_by);
            }
            if let Some(selector) = element.selector() {
                mark_selector_used(used, selector);
            }
            for leaf in element.leaves() {
                if let Element::Bar { value, max, .. } = leaf {
                    used.insert(*value);
                    if let MaxSpec::Property(prop) = max {
                        used.insert(*prop);
                    }
                }
                let text = match leaf {
                    Element::Label { text, .. } | Element::Button { text, .. } => text.as_slice(),
                    Element::Panel { .. } | Element::Bar { .. } | Element::List { .. } => &[],
                };
                for part in text.iter().chain(leaf.tooltip().unwrap_or_default()) {
                    if let TextPart::Value { prop, .. } | TextPart::Own(prop) = part {
                        used.insert(*prop);
                    }
                }
            }
        }
    }
}

/// «Надписи и полоски в мире», требование 35: `for`, the color-by-list's `by`, and a bar's
/// `value`/`max` or a label's own substitutions all count as used.
fn mark_world_elements_used(
    used: &mut std::collections::HashSet<PropertyId>,
    elements: &[WorldElement],
) {
    for element in elements {
        mark_selector_used(used, &element.for_);
        if let WorldColor::Table { by, .. } = &element.color {
            used.insert(*by);
        }
        match &element.kind {
            WorldElementKind::Label { text, .. } => {
                for part in text {
                    if let WorldTextPart::Value(prop) = part {
                        used.insert(*prop);
                    }
                }
            }
            WorldElementKind::Bar { value, max, .. } => {
                used.insert(*value);
                if let MaxSpec::Property(p) = max {
                    used.insert(*p);
                }
            }
            WorldElementKind::Image {
                image: ImageRef::Own { property, .. },
            } => {
                used.insert(*property);
            }
            WorldElementKind::Image { .. } => {}
        }
    }
}

fn collect_used_properties(
    scene: &[ParsedObject],
    rules: &RuleSet,
    screens: &[Screen],
    world_elements: &[WorldElement],
) -> std::collections::HashSet<PropertyId> {
    let mut used = std::collections::HashSet::new();
    mark_screen_elements_used(&mut used, screens);
    mark_world_elements_used(&mut used, world_elements);
    for obj in scene {
        used.extend(obj.shape.iter().copied());
        if let Some(table) = &obj.keys {
            for binding in table.values() {
                used.extend(binding.press.iter().map(|e| e.property));
                used.extend(binding.release.iter().map(|e| e.property));
            }
        }
        if let Some(edits) = &obj.on_click {
            used.extend(edits.iter().map(|e| e.property));
        }
    }
    for rule in &rules.rules {
        match rule {
            Rule::Move { for_ } | Rule::Drive { for_, .. } => mark_selector_used(&mut used, for_),
            Rule::Climb { for_, ladders } => {
                mark_selector_used(&mut used, for_);
                mark_selector_used(&mut used, ladders);
            }
            Rule::Walk { for_, avoid } => {
                mark_selector_used(&mut used, for_);
                if let Some(avoid) = avoid {
                    mark_selector_used(&mut used, avoid);
                }
            }
            Rule::RunJump {
                for_,
                solid,
                one_way,
            } => {
                mark_selector_used(&mut used, for_);
                for selector in [solid, one_way].into_iter().flatten() {
                    mark_selector_used(&mut used, selector);
                }
            }
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
            Rule::Check { for_, when, do_ } => {
                mark_selector_used(&mut used, for_);
                if let Some(cond) = when {
                    mark_condition_used(&mut used, cond);
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
                pick_one,
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
                if let Some(variants) = pick_one {
                    for variant in variants {
                        for (prop, tv) in &variant.fields {
                            used.insert(*prop);
                            if let TemplateValue::FromParent(p) = tv {
                                used.insert(*p);
                            }
                        }
                        for cell in &variant.cells {
                            for (prop, tv) in &cell.fields {
                                used.insert(*prop);
                                if let TemplateValue::FromParent(p) = tv {
                                    used.insert(*p);
                                }
                            }
                        }
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
#[allow(clippy::too_many_arguments)]
fn validate_unused_properties(
    properties: &PropertyTable,
    properties_file: &str,
    scene: &[ParsedObject],
    rules: &RuleSet,
    screens: &[Screen],
    world_elements: &[WorldElement],
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let used = collect_used_properties(scene, rules, screens, world_elements);
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

/// «Ветер и частицы» → «Проверка перед запуском»: предупреждение, если у объекта сцены `clouds` больше
/// нуля, а `cloud_images` нет или он пуст, — облаков не будет.
fn warn_clouds_without_images(scene: &[ParsedObject], scene_file: &str, errors: &mut ErrorSink) {
    for obj in scene {
        let wanted = matches!(
            find_value(&obj.values, property::CLOUDS),
            Some(Value::Number(n)) if *n > 0.0
        );
        let chosen = matches!(
            find_value(&obj.values, property::CLOUD_IMAGES),
            Some(Value::ImageList(images)) if !images.is_empty()
        );
        if wanted && !chosen {
            errors.push_warning(
                scene_file,
                &join(&obj.path, "clouds"),
                "облаков нет: не выбраны картинки облаков",
            );
        }
    }
}

fn name_suffix(obj: &ParsedObject) -> String {
    match &obj.name {
        Some(name) => format!(" (имя \"{name}\")"),
        None => String::new(),
    }
}

/// «Формат игры»: предупреждение (игра идёт), если объект сцены целиком стоит за пределами
/// сцены — его прямоугольник не пересекается с `[0,0]..[width,height]` вовсе. Объект без
/// `position` или `size` не проверяется: его прямоугольник неизвестен. «Слои глубины», требование 25:
/// объект с `parallax`, отличным от 1, не проверяется — где его видно, решает камера; у `repeat_x`
/// проверяется только высота — по ширине копии закрывают окно.
fn validate_objects_within_scene(
    scene: &[ParsedObject],
    scene_file: &str,
    config: &SceneConfig,
    errors: &mut ErrorSink,
) {
    for obj in scene {
        if !matches!(
            find_value(&obj.values, property::PARALLAX),
            None | Some(Value::Number(1.0))
        ) {
            continue;
        }
        let repeats = matches!(
            find_value(&obj.values, property::REPEAT_X),
            Some(Value::Flag(true))
        );
        let (Some(pos), Some(Value::Vec2(size))) = (
            find_value(&obj.values, property::POSITION).and_then(|v| match v {
                Value::Vec2(xy) => Some([xy[0], xy[1]]),
                Value::Vec3(xyz) => Some([xyz[0], xyz[1]]),
                _ => None,
            }),
            find_value(&obj.values, property::SIZE),
        ) else {
            continue;
        };
        let pos = &pos;
        // «Трёхмерная сцена», требование 15: повёрнутый прямоугольник — сам, не охватывающий его.
        let rotation = match find_value(&obj.values, property::ROTATION) {
            Some(Value::Rotation(r)) if config.is_3d() => Some(*r),
            _ => None,
        };
        let on_scene = match rotation.filter(|r| r.angle() != 0.0) {
            None => {
                (repeats || (pos[0] + size[0] > 0.0 && pos[0] < config.width as f64))
                    && pos[1] + size[1] > 0.0
                    && pos[1] < config.height as f64
            }
            Some(turned) => {
                let scene_place =
                    Footprint::flat([0.0, 0.0], [config.width as f64, config.height as f64]);
                Footprint::rotated(*pos, *size, Some(turned)).overlaps(&scene_place)
            }
        };
        if on_scene {
            continue;
        }
        let turn = rotation
            .filter(|r| r.angle() != 0.0)
            .map_or_else(String::new, |r| {
                format!(", повёрнутый на {}° вокруг середины", r.angle())
            });
        errors.push_warning(
            scene_file,
            &join(&obj.path, "position"),
            format!(
                "объект{} стоит за пределами сцены: прямоугольник [{}, {}]..[{}, {}]{turn} не пересекается со сценой [0, 0]..[{}, {}]; ожидалось, что объект будет хотя бы частично на сцене",
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
/// `take`, `set`, `keys`; см. `widen_shapes_by_rule_actions`, `keys_edited_properties`).
/// Проверяются все отборы, которые документ называет отбором правила — `for` у «подвинуть» и
/// «удалить», `a`/`b` у «столкнуть», и у «создать» и «удалить» оба отбора, какими может быть их
/// условие: `of` внутри `fewer_than` и `of` внутри `after_move_of`. Документ не делает исключения
/// по виду отбора: пустой `fewer_than.of` делает условие «меньше N» всегда истинным и «создать»
/// сыплет объекты до `max_objects`, ровно то же по сути, что и пустой `for`. Пустой отбор (`has`
/// и `without` оба пусты) подходит всем и предупреждения не даёт.
/// The shared boolean `validate_selectors_not_empty`'s own `check` and
/// `validate_world_element_selectors_not_empty` both need — factored out so the two error-pushing
/// call sites (different files, different labels) don't duplicate the actual "can this ever match
/// anything" logic.
fn selector_is_satisfiable(
    selector: &Selector,
    shapes: &[PossibleShape],
    properties: &PropertyTable,
    code: Option<&str>,
) -> bool {
    if selector.has.is_empty() && selector.without.is_empty() {
        return true;
    }
    if shapes.iter().any(|ps| matches_shape(selector, &ps.shape)) {
        return true;
    }
    // «Код игры»: свойство, которым явно распоряжается код (не только сцена/правила), код и
    // решает, каким объектам его давать — статическому разбору форм это не видно, так что
    // предупреждение о заведомо пустом отборе тут скорее шумит, чем помогает.
    selector
        .has
        .iter()
        .chain(&selector.without)
        .any(|&p| code.is_some_and(|c| code_mentions_word(c, properties.name(p))))
}

fn validate_selectors_not_empty(
    shapes: &[PossibleShape],
    rules: &RuleSet,
    rule_meta: &[RuleMeta],
    rules_file: &str,
    properties: &PropertyTable,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let mut check = |selector: &Selector, label: &str, field: &str| {
        if selector_is_satisfiable(selector, shapes, properties, code) {
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
            Rule::Move { for_ } | Rule::Drive { for_, .. } => check(for_, &label, "for"),
            Rule::Climb { for_, ladders } => {
                check(for_, &label, "for");
                check(ladders, &label, "ladders");
            }
            Rule::Collide { a, b, do_, .. } => {
                check(a, &label, "a");
                check(b, &label, "b");
                check_do_selectors(do_, &label, "do", &mut check);
            }
            Rule::Check { for_, when, do_ } => {
                check(for_, &label, "for");
                if let Some(cond) = when {
                    check_condition_selectors(cond, &label, "when", &mut check);
                }
                check_do_selectors(do_, &label, "do", &mut check);
            }
            Rule::Delete { for_, when, do_ } => {
                check(for_, &label, "for");
                check_condition_selectors(when, &label, "when", &mut check);
                check_do_selectors(do_, &label, "do", &mut check);
            }
            Rule::Spawn { when, do_, .. } => {
                check_condition_selectors(&when.condition, &label, "when", &mut check);
                check_do_selectors(do_, &label, "do", &mut check);
            }
            Rule::Walk { for_, avoid } => {
                check(for_, &label, "for");
                if let Some(avoid) = avoid {
                    check(avoid, &label, "avoid");
                }
            }
            Rule::RunJump {
                for_,
                solid,
                one_way,
            } => {
                check(for_, &label, "for");
                if let Some(solid) = solid {
                    check(solid, &label, "solid");
                }
                if let Some(one_way) = one_way {
                    check(one_way, &label, "one_way");
                }
            }
        }
    }
}

/// «Надписи и полоски в мире», требование 34: `for` заведомо не подходит ни одному объекту — та
/// же проверка `validate_selectors_not_empty` использует для правил (`selector_is_satisfiable`),
/// со своим файлом (`screens.json`) и своими метками.
fn validate_world_element_selectors_not_empty(
    world_elements: &[WorldElement],
    shapes: &[PossibleShape],
    screens_file: &str,
    properties: &PropertyTable,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    for (i, element) in world_elements.iter().enumerate() {
        if selector_is_satisfiable(&element.for_, shapes, properties, code) {
            continue;
        }
        errors.push_warning(
            screens_file,
            &format!("world_elements[{i}] → for"),
            format!(
                "отбор {} заведомо не подходит ни одному объекту, какой может существовать в игре; ожидалось, что отбору будет соответствовать хотя бы один объект",
                describe_selector(&element.for_, properties)
            ),
        );
    }
}

/// «Интерфейс игры» → «Проверка данных перед запуском»: `for` элемента экрана или списка заведомо не
/// подходит ни одному объекту — предупреждение, как у правил.
fn validate_screen_selectors_not_empty(
    screens: &[Screen],
    shapes: &[PossibleShape],
    screens_file: &str,
    properties: &PropertyTable,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    for (screen_index, screen) in screens.iter().enumerate() {
        for (index, element) in screen.elements.iter().enumerate() {
            let selector = match element {
                Element::List { for_, .. } => Some(for_),
                other => other.selector(),
            };
            let Some(selector) = selector else {
                continue;
            };
            if selector_is_satisfiable(selector, shapes, properties, code) {
                continue;
            }
            errors.push_warning(
                screens_file,
                &format!("screens[{screen_index}] → elements[{index}] → for"),
                format!(
                    "отбор {} заведомо не подходит ни одному объекту, какой может существовать в игре; ожидалось, что отбору будет соответствовать хотя бы один объект",
                    describe_selector(selector, properties)
                ),
            );
        }
    }
}

/// «Список», требование 22: элемент строки выходит за строку — левее или выше её угла, правее списка или
/// ниже `row_height`.
fn warn_row_elements_outside_row(screens: &[Screen], screens_file: &str, errors: &mut ErrorSink) {
    for (screen_index, screen) in screens.iter().enumerate() {
        for (index, element) in screen.elements.iter().enumerate() {
            let Element::List {
                placement,
                row_height,
                row,
                ..
            } = element
            else {
                continue;
            };
            for (inner, part) in row.iter().enumerate() {
                let own = part.placement();
                let outside = own.offset[0] < 0.0
                    || own.offset[1] < 0.0
                    || own.offset[0] + own.size[0] > placement.size[0]
                    || own.offset[1] + own.size[1] > *row_height;
                if outside {
                    errors.push_warning(
                        screens_file,
                        &format!("screens[{screen_index}] → elements[{index}] → row[{inner}]"),
                        format!(
                            "элемент строки выходит за строку {} × {}; ожидалось, что элемент строки лежит внутри неё",
                            placement.size[0], row_height
                        ),
                    );
                }
            }
        }
    }
}

fn check_condition_selectors(
    condition: &Condition,
    label: &str,
    field: &str,
    check: &mut dyn FnMut(&Selector, &str, &str),
) {
    match condition {
        Condition::AfterMoveOf { of } => check(of, label, &join(field, "after_move_of")),
        Condition::FewerThan { of, .. } => check(of, label, &join(field, "fewer_than → of")),
        Condition::Compare { .. } | Condition::OutsideScene => {}
        Condition::All(list) | Condition::Any(list) => {
            for c in list {
                check_condition_selectors(c, label, field, check);
            }
        }
        Condition::Not(inner) => check_condition_selectors(inner, label, field, check),
    }
}

/// `shift`/`turn`'s own `group`/`blocked_by`/`around` — checked wherever a `do` (or an
/// `if_blocked`) can carry them.
fn check_do_selectors(
    actions: &[CommonAction],
    label: &str,
    field: &str,
    check: &mut dyn FnMut(&Selector, &str, &str),
) {
    walk_common_actions(actions, &mut |action| match action {
        CommonAction::GiveWhere { selector, .. } => check(selector, label, field),
        CommonAction::Shift(spec) => {
            check(&spec.group, label, &join(field, "shift → group"));
            if let Some(of) = &spec.blocked_by {
                check(of, label, &join(field, "shift → blocked_by"));
            }
        }
        CommonAction::Turn(spec) => {
            check(&spec.group, label, &join(field, "turn → group"));
            check(&spec.around, label, &join(field, "turn → around"));
            if let Some(of) = &spec.blocked_by {
                check(of, label, &join(field, "turn → blocked_by"));
            }
        }
        _ => {}
    });
}

// ---------------------------------------------------------------------------------------------
// screens.json — «Экраны и состояние» / «Интерфейс игры»
// ---------------------------------------------------------------------------------------------

fn expect_ui_color(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<[f32; 4]> {
    let s = expect_string(value, file, path, errors)?;
    parse_color(&s).or_else(|| {
        errors.push(
            file,
            path,
            format!("цвет должен быть вида \"#rrggbb\" или \"#rrggbbaa\", получено \"{s}\""),
        );
        None
    })
}

/// One `text` field after parsing: `{имя объекта.свойство}` becomes a resolved `TextPart::Value`,
/// everything else stays literal. «Интерфейс игры» → «Текст». `own` — элемент при объекте или в строке
/// списка: тогда `{свойство}` без имени объекта — `TextPart::Own`, свойство объекта элемента.
fn parse_rich_text(
    s: &str,
    properties: &PropertyTable,
    own: bool,
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
        let part = match inner.strip_prefix("choice:") {
            Some(reference) => TextPart::Choice(parse_choice_ref(reference, file, path, errors)?),
            None => match inner.find('.') {
                None if !inner.is_empty() && own => {
                    TextPart::Own(resolve_property(&inner, properties, file, path, errors)?)
                }
                None if !inner.is_empty() => {
                    errors.push(
                        file,
                        path,
                        format!(
                            "подстановка \"{{{inner}}}\" без имени объекта допустима только у элемента с for и в строке списка; у этого элемента нужна подстановка вида {{имя объекта.свойство}}"
                        ),
                    );
                    return None;
                }
                Some(dot) if dot > 0 && dot + 1 < inner.len() => TextPart::Value {
                    object_name: inner[..dot].to_string(),
                    prop: resolve_property(&inner[dot + 1..], properties, file, path, errors)?,
                },
                _ => {
                    errors.push(
                        file,
                        path,
                        format!(
                            "подстановка \"{{{inner}}}\" должна быть вида {{имя объекта.свойство}}"
                        ),
                    );
                    return None;
                }
            },
        };
        if !literal.is_empty() {
            parts.push(TextPart::Literal(std::mem::take(&mut literal)));
        }
        parts.push(part);
    }
    if !literal.is_empty() {
        parts.push(TextPart::Literal(literal));
    }
    Some(parts)
}

/// «Выбор»: `таблица` или `таблица.поле` внутри `{choice:…}`.
fn parse_choice_ref(
    reference: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ChoiceRef> {
    let (group, field) = match reference.split_once('.') {
        Some((group, field)) => (group, Some(field)),
        None => (reference, None),
    };
    if group.is_empty() || field.is_some_and(|field| field.is_empty() || field.contains('.')) {
        errors.push(
            file,
            path,
            format!(
                "подстановка \"{{choice:{reference}}}\" должна быть вида {{choice:таблица}} или {{choice:таблица.поле}}"
            ),
        );
        return None;
    }
    Some(ChoiceRef {
        group: group.to_string(),
        field: field.map(str::to_string),
    })
}

/// «Выбор»: `{choice:…}`, занимающая строку `text` целиком, — имя картинки панели или начальное значение
/// (`what`). Ошибка, если вокруг подстановки есть ещё что-то.
fn parse_whole_choice(
    text: &str,
    what: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ChoiceRef> {
    let reference = text
        .strip_prefix("{choice:")
        .and_then(|rest| rest.strip_suffix('}'))
        .filter(|reference| !reference.contains(['{', '}']));
    let Some(reference) = reference else {
        errors.push(
            file,
            path,
            format!("\"{text}\": подстановка {{choice:…}} в {what} занимает его целиком"),
        );
        return None;
    };
    parse_choice_ref(reference, file, path, errors)
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
    /// «Начальные значения у new_game», требование 29: the target screen, plus the raw
    /// `"<объект>.<свойство>"` → value pairs of an optional third element, still unresolved (no
    /// `properties`/scene in scope here) — `resolve_on_click` finishes the job.
    NewGame(String, Option<Vec<(String, Json)>>),
    Resume,
    Quit,
    ToggleSound,
    Exit,
    /// Имя функции кода, ещё не сверенное с файлом кода — `resolve_on_click` сверяет.
    Run(String),
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
            if arr.len() > 3 {
                errors.push(
                    file,
                    path,
                    format!(
                        "у команды \"new_game\" должно быть 1 или 2 параметра, получено {}",
                        arr.len() - 1
                    ),
                );
                return None;
            }
            let values = match arr.get(2) {
                Some(v) => {
                    let obj = expect_object(v, file, &join(path, "[2]"), errors)?;
                    Some(
                        obj.iter()
                            .map(|(k, v)| (k.clone(), v.clone()))
                            .collect::<Vec<_>>(),
                    )
                }
                None => None,
            };
            Some(ParsedCommand::NewGame(name, values))
        }
        "run" => {
            let name = expect_string(
                require_index(arr, 1, file, path, errors)?,
                file,
                &join(path, "[1]"),
                errors,
            )?;
            want_args(2, errors).then_some(ParsedCommand::Run(name))
        }
        "resume" => want_args(1, errors).then_some(ParsedCommand::Resume),
        "quit" => want_args(1, errors).then_some(ParsedCommand::Quit),
        "exit" => want_args(1, errors).then_some(ParsedCommand::Exit),
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
        /// `image: "{свойство}"` — имя картинки берёт свойство объекта элемента; `image_name` тогда пуст.
        own_image: Option<PropertyId>,
        opacity: Option<f32>,
        image_scale: Option<f32>,
        tooltip: Option<Vec<TextPart>>,
        for_: Option<Selector>,
        hover: HoverFields,
        choice: ChoiceFields,
        /// `image: "{choice:…}"` — имя картинки берёт выбранное место; `image_name` тогда пуст.
        choice_image: Option<ChoiceRef>,
        rest_at: Option<f32>,
        /// «Видео на панели»: имя ролика покоя.
        idle_name: Option<String>,
        /// «На экране»: свойства частиц панели-источника.
        particles: Option<Settings>,
    },
    /// «Шкала»: в коробке, как кнопка, — вид элемента не раздувается.
    Bar(Box<ParsedBar>),
    Label {
        placement: Placement,
        text: Vec<TextPart>,
        font_name: String,
        font_size: f32,
        color: [f32; 4],
        align: Align,
        wrap: bool,
        for_: Option<Selector>,
        hover: HoverFields,
    },
    Button {
        placement: Placement,
        text: Vec<TextPart>,
        /// Текст кнопки, как он записан в файле, — им ошибка называет кнопку.
        caption: String,
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
        image_scale: Option<f32>,
        tooltip: Option<Vec<TextPart>>,
        on_click: ParsedCommand,
        /// В коробке: кнопка — самый большой вид элемента.
        for_: Option<Box<Selector>>,
        hover: HoverFields,
        choice: ChoiceFields,
    },
    List {
        placement: Placement,
        for_: Selector,
        row_height: f32,
        gap: f32,
        sort_by: Option<PropertyId>,
        descending: bool,
        row: ParsedElements,
        scroll_color: [f32; 4],
    },
}

/// Разобранные элементы списка или экрана с номерами, под которыми они стоят в файле: элемент с ошибкой
/// разбора выпал, а ошибки следующих называют номер из файла.
type ParsedElements = Vec<(usize, ParsedElementData)>;

/// Где стоит элемент: прямо на экране (якорь и, может быть, отбор `for`) или в строке списка (сдвиг от
/// угла строки, без якоря и отбора).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Scope {
    Screen,
    Row,
}

/// Цвет полосы прокрутки списка по умолчанию, `#ffffff66`.
const DEFAULT_SCROLL_COLOR: [f32; 4] = [1.0, 1.0, 1.0, 0.4];

const PANEL_KEYS: &[&str] = &[
    "kind",
    "anchor",
    "offset",
    "size",
    "color",
    "image",
    "opacity",
    "image_scale",
    "tooltip",
    "for",
    "name",
    "show_on_hover",
    "play_on_hover",
    "choose",
    "needs_choice",
    "text_color_disabled",
    "rest_at",
    "idle",
];
const BAR_KEYS: &[&str] = &[
    "kind",
    "anchor",
    "offset",
    "size",
    "for",
    "value",
    "max",
    "color",
    "image",
    "back_color",
    "back_image",
    "image_scale",
    "tooltip",
    "name",
    "show_on_hover",
    "play_on_hover",
    "choose",
    "needs_choice",
    "text_color_disabled",
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
    "wrap",
    "for",
    "name",
    "show_on_hover",
    "play_on_hover",
    "choose",
    "needs_choice",
    "text_color_disabled",
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
    "image_scale",
    "tooltip",
    "on_click",
    "for",
    "name",
    "show_on_hover",
    "play_on_hover",
    "choose",
    "needs_choice",
    "text_color_disabled",
];
const LIST_KEYS: &[&str] = &[
    "kind",
    "anchor",
    "offset",
    "size",
    "for",
    "row_height",
    "gap",
    "sort_by",
    "descending",
    "row",
    "scroll_color",
    "name",
    "show_on_hover",
    "play_on_hover",
    "choose",
    "needs_choice",
    "text_color_disabled",
];

/// «Ветер и частицы» → «На экране»: свойства частиц панели — те же имена, что у свойств объекта.
const PARTICLE_KEYS: [&str; 13] = [
    "smoke",
    "smoke_height",
    "smoke_color",
    "sparks",
    "sparks_reach",
    "sparks_direction",
    "sparks_spread",
    "leaf_fall",
    "leaf_color",
    "fluff",
    "fluff_color",
    "butterflies",
    "butterfly_color",
];

/// «На экране»: свойства частиц бывают только у панели. Любой из них у надписи, кнопки, шкалы или
/// списка — ошибка со своим текстом.
fn reject_foreign_particle_keys(
    obj: &serde_json::Map<String, Json>,
    owner: HoverOwner,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    for key in PARTICLE_KEYS.iter().filter(|key| obj.contains_key(**key)) {
        errors.push(
            file,
            &join(path, key),
            format!(
                "у {} нет \"{key}\": свойства частиц бывают только у панели экрана",
                owner.genitive()
            ),
        );
    }
}

/// «На экране»: свойства частиц панели. `Some(None)` — их нет. Источник частиц стоит на экране всегда:
/// у панели с `for` и у панели в строке списка они — ошибка. Значения проверяются как у объекта.
/// Внешний `None` — ошибка уже названа.
fn parse_panel_particles(
    obj: &serde_json::Map<String, Json>,
    scope: Scope,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<Settings>> {
    let present: Vec<&str> = PARTICLE_KEYS
        .into_iter()
        .filter(|key| obj.contains_key(*key))
        .collect();
    if present.is_empty() {
        return Some(None);
    }
    let misplaced = if scope == Scope::Row {
        Some("в строке списка")
    } else if obj.contains_key("for") {
        Some("с for")
    } else {
        None
    };
    if let Some(place) = misplaced {
        for key in present {
            errors.push(
                file,
                &join(path, key),
                format!(
                    "у панели {place} нет \"{key}\": источник частиц стоит на экране всегда, а не при объекте"
                ),
            );
        }
        return None;
    }
    let mut settings = Settings::default();
    let mut ok = true;
    for key in present {
        ok &= parse_particle_field(
            &mut settings,
            key,
            &obj[key],
            properties,
            file,
            path,
            errors,
        );
    }
    ok.then_some(Some(settings))
}

/// Одно свойство частиц панели: число в пределах свойства объекта или цвет.
fn parse_particle_field(
    settings: &mut Settings,
    key: &str,
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let Some(prop) = properties.resolve(key) else {
        return false;
    };
    let key_path = join(path, key);
    if properties.kind(prop) == PropKind::Color {
        let Some([r, g, b, _]) = expect_ui_color(value, file, &key_path, errors) else {
            return false;
        };
        match prop {
            property::SMOKE_COLOR => settings.smoke_color = [r, g, b],
            property::LEAF_COLOR => settings.leaf_color = Some([r, g, b]),
            property::FLUFF_COLOR => settings.fluff_color = [r, g, b],
            property::BUTTERFLY_COLOR => settings.butterfly_color = Some([r, g, b]),
            _ => unreachable!("PARTICLE_KEYS: цвет частиц — одно из четырёх свойств цвета"),
        }
        return true;
    }
    let Some(n) = expect_number(value, file, &key_path, errors) else {
        return false;
    };
    if !particle_number_fits(prop, n, key, file, &key_path, errors) {
        return false;
    }
    match prop {
        property::SMOKE => settings.smoke = n,
        property::SMOKE_HEIGHT => settings.smoke_height = n,
        property::SPARKS => settings.sparks = n,
        property::SPARKS_REACH => settings.sparks_reach = n,
        property::SPARKS_DIRECTION => settings.sparks_direction = n,
        property::SPARKS_SPREAD => settings.sparks_spread = n,
        property::LEAF_FALL => settings.leaf_fall = n,
        property::FLUFF => settings.fluff = n,
        property::BUTTERFLIES => settings.butterflies = n,
        _ => unreachable!("PARTICLE_KEYS: число частиц — одно из девяти свойств-чисел"),
    }
    true
}

/// «Наведение»: `name`, `show_on_hover` и `play_on_hover` одного элемента, места ещё по именам.
#[derive(Debug, Clone, Default)]
struct HoverFields {
    name: Option<String>,
    show_on_hover: Option<String>,
    play_on_hover: Option<String>,
}

/// Вид элемента, у которого разбираются поля наведения.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HoverOwner {
    Panel,
    Label,
    Button,
    Bar,
    List,
}

impl HoverOwner {
    fn genitive(self) -> &'static str {
        match self {
            HoverOwner::Panel => "панели",
            HoverOwner::Label => "надписи",
            HoverOwner::Button => "кнопки",
            HoverOwner::Bar => "шкалы",
            HoverOwner::List => "списка",
        }
    }
}

/// «Наведение»: разбирает `name`, `show_on_hover` и `play_on_hover` элемента `owner`. `name` бывает у панели
/// и кнопки, `show_on_hover` — у панели и надписи, `play_on_hover` — у панели; все три — только у элемента
/// самого экрана, `name` — ещё и без отбора `for`. Внешний `None` — ошибка уже названа.
fn parse_hover_fields(
    obj: &serde_json::Map<String, Json>,
    owner: HoverOwner,
    scope: Scope,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<HoverFields> {
    let mut fields = HoverFields::default();
    let mut ok = true;
    for (key, allowed, who) in [
        (
            "name",
            matches!(owner, HoverOwner::Panel | HoverOwner::Button),
            "место наведения бывает только у панели и кнопки",
        ),
        (
            "show_on_hover",
            matches!(owner, HoverOwner::Panel | HoverOwner::Label),
            "показ по наведению бывает только у панели и надписи",
        ),
        (
            "play_on_hover",
            owner == HoverOwner::Panel,
            "видео по наведению играет только панель",
        ),
    ] {
        let Some(value) = obj.get(key) else {
            continue;
        };
        let key_path = join(path, key);
        let problem = if !allowed {
            Some(format!("у {} нет \"{key}\": {who}", owner.genitive()))
        } else if scope == Scope::Row {
            Some(format!(
                "у элемента строки списка нет \"{key}\": место наведения — элемент самого экрана"
            ))
        } else if key == "name" && obj.contains_key("for") {
            Some(
                "у элемента с for нет \"name\": место наведения стоит на экране всегда, а не при объекте"
                    .to_string(),
            )
        } else {
            None
        };
        if let Some(problem) = problem {
            errors.push(file, &key_path, problem);
            ok = false;
            continue;
        }
        let Some(text) = expect_string(value, file, &key_path, errors) else {
            ok = false;
            continue;
        };
        if text.is_empty() {
            errors.push(file, &key_path, format!("\"{key}\" не может быть пустым"));
            ok = false;
            continue;
        }
        match key {
            "name" => fields.name = Some(text),
            "show_on_hover" => fields.show_on_hover = Some(text),
            _ => fields.play_on_hover = Some(text),
        }
    }
    ok.then_some(fields)
}

/// «Выбор»: `choose`, `needs_choice` и `text_color_disabled` одного элемента.
#[derive(Debug, Clone, Default)]
struct ChoiceFields {
    choose: Option<String>,
    needs_choice: Option<String>,
    text_color_disabled: Option<[f32; 4]>,
}

/// «Выбор»: разбирает `choose` (панель или кнопка с `name`, не при объекте и не в строке списка),
/// `needs_choice` (только кнопка) и `text_color_disabled` (только при `needs_choice`). Таблицы здесь не
/// проверяются — их проверяет `resolve_element`. Внешний `None` — ошибка уже названа.
fn parse_choice_fields(
    obj: &serde_json::Map<String, Json>,
    owner: HoverOwner,
    scope: Scope,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ChoiceFields> {
    let mut fields = ChoiceFields::default();
    let mut ok = true;
    for key in ["choose", "needs_choice"] {
        let Some(value) = obj.get(key) else {
            continue;
        };
        let key_path = join(path, key);
        let problem = if key == "choose" && !matches!(owner, HoverOwner::Panel | HoverOwner::Button)
        {
            Some(format!(
                "у {} нет \"choose\": выбирать щелчком можно только панель или кнопку",
                owner.genitive()
            ))
        } else if key == "needs_choice" && owner != HoverOwner::Button {
            Some(format!(
                "у {} нет \"needs_choice\": запереть до выбора можно только кнопку",
                owner.genitive()
            ))
        } else if key == "choose" && scope == Scope::Row {
            Some(
                "у элемента строки списка нет \"choose\": выбираемое место стоит на экране всегда, а не при объекте"
                    .to_string(),
            )
        } else if key == "choose" && obj.contains_key("for") {
            Some(
                "у элемента с for нет \"choose\": выбираемое место стоит на экране всегда, а не при объекте"
                    .to_string(),
            )
        } else if key == "choose" && !obj.contains_key("name") {
            Some(
                "у \"choose\" нет \"name\": выбирается место, а место — панель или кнопка с name, ключом строки таблицы"
                    .to_string(),
            )
        } else {
            None
        };
        if let Some(problem) = problem {
            errors.push(file, &key_path, problem);
            ok = false;
            continue;
        }
        let Some(text) = expect_string(value, file, &key_path, errors) else {
            ok = false;
            continue;
        };
        if text.is_empty() {
            errors.push(file, &key_path, format!("\"{key}\" не может быть пустым"));
            ok = false;
            continue;
        }
        if key == "choose" {
            fields.choose = Some(text);
        } else {
            fields.needs_choice = Some(text);
        }
    }
    if let Some(value) = obj.get("text_color_disabled") {
        let key_path = join(path, "text_color_disabled");
        if !obj.contains_key("needs_choice") {
            errors.push(
                file,
                &key_path,
                "text_color_disabled у элемента без needs_choice: цвет запертой подписи нужен только запертой кнопке",
            );
            ok = false;
        } else {
            match expect_ui_color(value, file, &key_path, errors) {
                Some(color) => fields.text_color_disabled = Some(color),
                None => ok = false,
            }
        }
    }
    ok.then_some(fields)
}

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

/// «Видео на панели»: `idle` — ролик покоя, другое видео из `files.images`, только у панели с `play_on_hover`
/// (`hovers`); `video` — видео самой панели. Панель с `play_on_hover` без видео уже названа ошибкой, второй о
/// ролике покоя ей не нужно. Внешний `None` — ошибка уже названа.
fn resolve_panel_idle(
    name: Option<&str>,
    hovers: bool,
    video: Option<ImageId>,
    images: &[ImageDecl],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<ImageId>> {
    let Some(name) = name else {
        return Some(None);
    };
    if !hovers {
        errors.push(
            file,
            path,
            "idle у панели без видео по наведению: ролик покоя показывается, пока видео с play_on_hover стоит на первом кадре",
        );
        return None;
    }
    let image = resolve_image(name, images, file, path, errors)?;
    if !images[image].video {
        errors.push(
            file,
            path,
            format!(
                "\"{name}\" — картинка, а idle берёт видео: ролик покоя объявляется в files.images путём на .mp4"
            ),
        );
        return None;
    }
    if video == Some(image) {
        errors.push(
            file,
            path,
            format!(
                "\"{name}\" — видео самой панели: ролик покоя — другое видео, оно идёт, пока это стоит на первом кадре"
            ),
        );
        return None;
    }
    Some(Some(image))
}

/// «Видео на панели»: `rest_at` — секунда ролика, число не меньше нуля; верхнюю границу — длину ролика —
/// проверяет `validate_rest_at`, когда страница ответила по файлу. Внешний `None` — ошибка уже названа.
fn parse_rest_at(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<f32>> {
    let Some(v) = obj.get("rest_at") else {
        return Some(None);
    };
    let rest_path = join(path, "rest_at");
    let n = expect_number(v, file, &rest_path, errors)?;
    if n < 0.0 {
        errors.push(
            file,
            &rest_path,
            format!("свойство \"rest_at\" не может быть отрицательным, получено {n}"),
        );
        return None;
    }
    Some(Some(n as f32))
}

/// «Картинки» → «Растягивание рамки»: `image_scale` — число больше нуля, только у элемента с картинкой.
/// Внешний `None` — ошибка уже названа.
fn parse_image_scale(
    obj: &serde_json::Map<String, Json>,
    has_image: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<f32>> {
    let Some(v) = obj.get("image_scale") else {
        return Some(None);
    };
    let scale_path = join(path, "image_scale");
    let n = expect_number(v, file, &scale_path, errors)?;
    if n <= 0.0 {
        errors.push(
            file,
            &scale_path,
            format!("свойство \"image_scale\" должно быть больше нуля, получено {n}"),
        );
        return None;
    }
    if !has_image {
        errors.push(
            file,
            &scale_path,
            "свойство \"image_scale\" — толщина рамки картинки: у элемента без картинки его не бывает",
        );
        return None;
    }
    Some(Some(n as f32))
}

/// «Подсказка»: `tooltip` — текст с подстановками, как `text`; вид подсказок задаёт блок `tooltip` в
/// `screens.json`, без него подсказка — ошибка. Внешний `None` — ошибка уже названа.
fn parse_tooltip(
    obj: &serde_json::Map<String, Json>,
    context: ElementContext<'_>,
    own: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<Vec<TextPart>>> {
    let Some(v) = obj.get("tooltip") else {
        return Some(None);
    };
    let tooltip_path = join(path, "tooltip");
    let text = expect_string(v, file, &tooltip_path, errors)?;
    if !context.tooltips {
        errors.push(
            file,
            &tooltip_path,
            "у элемента есть \"tooltip\", а блока \"tooltip\" с видом подсказок в screens.json нет",
        );
        return None;
    }
    let parts = parse_rich_text(&text, context.properties, own, file, &tooltip_path, errors)?;
    if parts.iter().any(|part| matches!(part, TextPart::Choice(_))) {
        errors.push(
            file,
            &tooltip_path,
            "подстановка {choice:…} есть в тексте надписи и кнопки, а в подсказке её нет",
        );
        return None;
    }
    Some(Some(parts))
}

/// Что элементу экрана нужно знать о файле вокруг него: свойства игры и есть ли блок вида подсказок.
#[derive(Clone, Copy)]
struct ElementContext<'a> {
    properties: &'a PropertyTable,
    tooltips: bool,
}

/// «Шкала» после разбора, с картинками ещё по именам.
#[derive(Debug, Clone)]
struct ParsedBar {
    placement: Placement,
    for_: Option<Selector>,
    value: PropertyId,
    max: MaxSpec,
    color: Option<[f32; 4]>,
    image_name: Option<String>,
    back_color: Option<[f32; 4]>,
    back_image_name: Option<String>,
    image_scale: Option<f32>,
    tooltip: Option<Vec<TextPart>>,
}

fn parse_bar(
    obj: &serde_json::Map<String, Json>,
    scope: Scope,
    context: ElementContext<'_>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ParsedElementData> {
    reject_unknown_keys_except(obj, BAR_KEYS, &PARTICLE_KEYS, file, path, errors);
    reject_foreign_particle_keys(obj, HoverOwner::Bar, file, path, errors);
    let properties = context.properties;
    let (placement, for_) = parse_element_place(obj, scope, properties, file, path, errors)?;
    if scope == Scope::Screen && for_.is_none() {
        errors.push(
            file,
            path,
            "у шкалы на экране нужен \"for\": value и max она берёт у объекта отбора",
        );
        return None;
    }
    let value_json = require_field(obj, "value", file, path, errors)?;
    let value = resolve_number_source(value_json, "value", properties, file, path, errors)?;
    let max_json = require_field(obj, "max", file, path, errors)?;
    let max = parse_world_max(max_json, properties, file, &join(path, "max"), errors)?;
    let color = parse_optional_ui_color(obj, "color", file, path, errors)?;
    let image_name = parse_optional_string(obj, "image", file, path, errors)?;
    validate_fill_choice(color.is_some(), image_name.is_some(), file, path, errors);
    let back_color = parse_optional_ui_color(obj, "back_color", file, path, errors)?;
    let back_image_name = parse_optional_string(obj, "back_image", file, path, errors)?;
    if back_color.is_some() && back_image_name.is_some() {
        errors.push(
            file,
            path,
            "заданы и \"back_color\", и \"back_image\"; у подложки шкалы не больше одного из двух",
        );
    }
    let has_image = image_name.is_some() || back_image_name.is_some();
    let image_scale = parse_image_scale(obj, has_image, file, path, errors)?;
    let tooltip = parse_tooltip(obj, context, true, file, path, errors)?;
    Some(ParsedElementData::Bar(Box::new(ParsedBar {
        placement,
        for_,
        value,
        max,
        color,
        image_name,
        back_color,
        back_image_name,
        image_scale,
        tooltip,
    })))
}

/// Необязательный цвет `key`: `Some(None)` — его нет, внешний `None` — ошибка уже названа.
fn parse_optional_ui_color(
    obj: &serde_json::Map<String, Json>,
    key: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<[f32; 4]>> {
    match obj.get(key) {
        Some(v) => expect_ui_color(v, file, &join(path, key), errors).map(Some),
        None => Some(None),
    }
}

/// Необязательная строка `key`: `Some(None)` — её нет, внешний `None` — ошибка уже названа.
fn parse_optional_string(
    obj: &serde_json::Map<String, Json>,
    key: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<String>> {
    match obj.get(key) {
        Some(v) => expect_string(v, file, &join(path, key), errors).map(Some),
        None => Some(None),
    }
}

/// «Список», требование 6: элемент строки — панель, надпись или кнопка с `offset` от левого верхнего угла
/// строки и `size`, без `anchor` и `for`.
fn parse_row_placement(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Placement> {
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
        anchor: Anchor::TopLeft,
        offset: [offset[0] as f32, offset[1] as f32],
        size: [size[0] as f32, size[1] as f32],
    })
}

/// Место элемента и его отбор `for`: для элемента экрана — якорь и необязательный отбор, для элемента
/// строки — сдвиг от угла строки, а `anchor` и `for` — ошибка. Внешний `None` — ошибка уже названа.
fn parse_element_place(
    obj: &serde_json::Map<String, Json>,
    scope: Scope,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<(Placement, Option<Selector>)> {
    if scope == Scope::Row {
        let mut allowed = true;
        for (key, reason) in [
            (
                "anchor",
                "его место — offset от левого верхнего угла строки",
            ),
            (
                "for",
                "подстановка {свойство} в нём берёт свойство объекта строки",
            ),
        ] {
            if obj.contains_key(key) {
                errors.push(
                    file,
                    &join(path, key),
                    format!("у элемента строки списка нет \"{key}\": {reason}"),
                );
                allowed = false;
            }
        }
        return allowed
            .then(|| parse_row_placement(obj, file, path, errors))
            .flatten()
            .map(|placement| (placement, None));
    }
    let for_ = match obj.get("for") {
        None => None,
        Some(v) => {
            let for_path = join(path, "for");
            let for_obj = expect_object(v, file, &for_path, errors)?;
            Some(resolve_selector(
                for_obj, properties, file, &for_path, errors,
            ))
        }
    };
    let placement = parse_placement(obj, file, path, errors)?;
    Some((placement, for_))
}

/// `wrap` надписи: `true` или `false`, по умолчанию `false`.
fn parse_wrap(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<bool> {
    match obj.get("wrap") {
        None => Some(false),
        Some(v) => v.as_bool().or_else(|| {
            errors.push(
                file,
                &join(path, "wrap"),
                format!(
                    "свойство \"wrap\" должно быть true или false, получено {}",
                    kind_name(v)
                ),
            );
            None
        }),
    }
}

/// Свойство, по которому список упорядочен: число, время или таймер.
fn parse_sort_by(
    obj: &serde_json::Map<String, Json>,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<PropertyId>> {
    let Some(v) = obj.get("sort_by") else {
        return Some(None);
    };
    let sort_path = join(path, "sort_by");
    let name = expect_string(v, file, &sort_path, errors)?;
    let prop = resolve_property(&name, properties, file, &sort_path, errors)?;
    if !matches!(
        properties.kind(prop),
        PropKind::Number | PropKind::Time | PropKind::Timer
    ) {
        errors.push(
            file,
            &sort_path,
            format!(
                "свойство \"sort_by\" должно называть свойство вида number, time или timer, у \"{name}\" вид {}",
                properties.kind(prop).label()
            ),
        );
        return None;
    }
    Some(Some(prop))
}

fn parse_list(
    obj: &serde_json::Map<String, Json>,
    path: &str,
    file: &str,
    context: ElementContext<'_>,
    errors: &mut ErrorSink,
) -> Option<ParsedElementData> {
    let properties = context.properties;
    reject_unknown_keys_except(obj, LIST_KEYS, &PARTICLE_KEYS, file, path, errors);
    reject_foreign_particle_keys(obj, HoverOwner::List, file, path, errors);
    let placement = parse_placement(obj, file, path, errors)?;
    let for_json = require_field(obj, "for", file, path, errors)?;
    let for_path = join(path, "for");
    let for_obj = expect_object(for_json, file, &for_path, errors)?;
    let for_ = resolve_selector(for_obj, properties, file, &for_path, errors);
    let row_height_json = require_field(obj, "row_height", file, path, errors)?;
    let row_height_path = join(path, "row_height");
    let row_height = expect_number(row_height_json, file, &row_height_path, errors)?;
    if row_height <= 0.0 {
        errors.push(
            file,
            &row_height_path,
            format!("свойство \"row_height\" должно быть больше нуля, получено {row_height}"),
        );
        return None;
    }
    let gap = match obj.get("gap") {
        None => 0.0,
        Some(v) => {
            let gap = expect_number(v, file, &join(path, "gap"), errors)?;
            if gap < 0.0 {
                errors.push(
                    file,
                    &join(path, "gap"),
                    format!("свойство \"gap\" не может быть меньше нуля, получено {gap}"),
                );
                return None;
            }
            gap
        }
    };
    let sort_by = parse_sort_by(obj, properties, file, path, errors)?;
    let descending = match obj.get("descending") {
        None => false,
        Some(v) => v.as_bool().or_else(|| {
            errors.push(
                file,
                &join(path, "descending"),
                format!(
                    "свойство \"descending\" должно быть true или false, получено {}",
                    kind_name(v)
                ),
            );
            None
        })?,
    };
    let scroll_color = match obj.get("scroll_color") {
        Some(v) => expect_ui_color(v, file, &join(path, "scroll_color"), errors)?,
        None => DEFAULT_SCROLL_COLOR,
    };
    let row_json = require_field(obj, "row", file, path, errors)?;
    let row_path = join(path, "row");
    let row_items = expect_array(row_json, file, &row_path, errors)?;
    let row = row_items
        .iter()
        .enumerate()
        .filter_map(|(i, item)| {
            parse_element(
                item,
                &join(&row_path, &format!("[{i}]")),
                file,
                context,
                Scope::Row,
                errors,
            )
            .map(|element| (i, element))
        })
        .collect();
    Some(ParsedElementData::List {
        placement,
        for_,
        row_height: row_height as f32,
        gap: gap as f32,
        sort_by,
        descending,
        row,
        scroll_color,
    })
}

fn parse_element(
    value: &Json,
    path: &str,
    file: &str,
    context: ElementContext<'_>,
    scope: Scope,
    errors: &mut ErrorSink,
) -> Option<ParsedElementData> {
    let properties = context.properties;
    let obj = expect_object(value, file, path, errors)?;
    let kind_json = require_field(obj, "kind", file, path, errors)?;
    let kind = expect_string(kind_json, file, &join(path, "kind"), errors)?;
    match kind.as_str() {
        "panel" => {
            let known: Vec<&str> = PANEL_KEYS.iter().chain(&PARTICLE_KEYS).copied().collect();
            reject_unknown_keys(obj, &known, file, path, errors);
            let hover = parse_hover_fields(obj, HoverOwner::Panel, scope, file, path, errors);
            let choice = parse_choice_fields(obj, HoverOwner::Panel, scope, file, path, errors);
            let particles = parse_panel_particles(obj, scope, properties, file, path, errors);
            let (placement, for_) =
                parse_element_place(obj, scope, properties, file, path, errors)?;
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
            let image_scale = parse_image_scale(obj, image_name.is_some(), file, path, errors)?;
            let rest_at = parse_rest_at(obj, file, path, errors);
            let idle_name = match obj.get("idle") {
                Some(v) => expect_string(v, file, &join(path, "idle"), errors).map(Some),
                None => Some(None),
            };
            let own = scope == Scope::Row || for_.is_some();
            let tooltip = parse_tooltip(obj, context, own, file, path, errors)?;
            // «Выбор»: `{choice:…}` в `image` — имя картинки берёт выбранное место.
            let (image_name, choice_image) = match image_name {
                Some(name) if name.contains("{choice:") => {
                    let image_path = join(path, "image");
                    let choice_image =
                        parse_whole_choice(&name, "image панели", file, &image_path, errors)?;
                    (None, Some(choice_image))
                }
                other => (other, None),
            };
            // «Элемент при объекте»: `{свойство}` в `image` — только у панели, у которой есть объект.
            let (image_name, own_image) = match image_name {
                Some(name) if name.contains(['{', '}']) => {
                    let image_path = join(path, "image");
                    if !own {
                        errors.push(
                            file,
                            &image_path,
                            format!(
                                "\"{name}\": имя картинки из свойства берёт только панель при объекте — в строке списка или с for"
                            ),
                        );
                        return None;
                    }
                    match parse_image_name(name, properties, file, &image_path, errors)? {
                        ParsedImageRef::Own(property) => (None, Some(property)),
                        ParsedImageRef::Name(name) => (Some(name), None),
                    }
                }
                other => (other, None),
            };
            Some(ParsedElementData::Panel {
                placement,
                color,
                image_name,
                own_image,
                opacity,
                image_scale,
                tooltip,
                for_,
                hover: hover?,
                choice: choice?,
                choice_image,
                rest_at: rest_at?,
                idle_name: idle_name?,
                particles: particles?,
            })
        }
        "label" => {
            reject_unknown_keys_except(obj, LABEL_KEYS, &PARTICLE_KEYS, file, path, errors);
            reject_foreign_particle_keys(obj, HoverOwner::Label, file, path, errors);
            let hover = parse_hover_fields(obj, HoverOwner::Label, scope, file, path, errors);
            let choice = parse_choice_fields(obj, HoverOwner::Label, scope, file, path, errors);
            let (placement, for_) =
                parse_element_place(obj, scope, properties, file, path, errors)?;
            let text_json = require_field(obj, "text", file, path, errors)?;
            let text_str = expect_string(text_json, file, &join(path, "text"), errors)?;
            let own = scope == Scope::Row || for_.is_some();
            let text = parse_rich_text(
                &text_str,
                properties,
                own,
                file,
                &join(path, "text"),
                errors,
            )?;
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
            let wrap = parse_wrap(obj, file, path, errors)?;
            choice?;
            Some(ParsedElementData::Label {
                placement,
                text,
                font_name,
                font_size: font_size as f32,
                color,
                align,
                wrap,
                for_,
                hover: hover?,
            })
        }
        "button" => {
            reject_unknown_keys_except(obj, BUTTON_KEYS, &PARTICLE_KEYS, file, path, errors);
            reject_foreign_particle_keys(obj, HoverOwner::Button, file, path, errors);
            let hover = parse_hover_fields(obj, HoverOwner::Button, scope, file, path, errors);
            let choice = parse_choice_fields(obj, HoverOwner::Button, scope, file, path, errors);
            let (placement, for_) =
                parse_element_place(obj, scope, properties, file, path, errors)?;
            let text_json = require_field(obj, "text", file, path, errors)?;
            let text_str = expect_string(text_json, file, &join(path, "text"), errors)?;
            let own = scope == Scope::Row || for_.is_some();
            let text = parse_rich_text(
                &text_str,
                properties,
                own,
                file,
                &join(path, "text"),
                errors,
            )?;
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
            let image_scale = parse_image_scale(obj, image_name.is_some(), file, path, errors)?;
            let tooltip = parse_tooltip(obj, context, own, file, path, errors)?;
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
                caption: text_str,
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
                image_scale,
                tooltip,
                on_click,
                for_: for_.map(Box::new),
                hover: hover?,
                choice: choice?,
            })
        }
        "bar" => {
            let hover = parse_hover_fields(obj, HoverOwner::Bar, scope, file, path, errors);
            let choice = parse_choice_fields(obj, HoverOwner::Bar, scope, file, path, errors);
            let bar = parse_bar(obj, scope, context, file, path, errors);
            hover.and(choice).and(bar)
        }
        "list" if scope == Scope::Screen => {
            let hover = parse_hover_fields(obj, HoverOwner::List, scope, file, path, errors);
            let choice = parse_choice_fields(obj, HoverOwner::List, scope, file, path, errors);
            let list = parse_list(obj, path, file, context, errors);
            hover.and(choice).and(list)
        }
        "list" => {
            errors.push(
                file,
                &join(path, "kind"),
                "в строке списка нет вложенных списков: ожидался panel, label, button или bar",
            );
            None
        }
        other => {
            let expected = match scope {
                Scope::Screen => "panel, label, button, bar или list",
                Scope::Row => "panel, label, button или bar",
            };
            errors.push(
                file,
                &join(path, "kind"),
                format!("неизвестный вид элемента \"{other}\": ожидался {expected}"),
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
            // «Мышь в мире», требование 36: `MouseLeft` — не клавиша экрана: она достаётся миру
            // или интерфейсу по тому, куда пришёл щелчок, а не по имени, как клавиатурная клавиша.
            if code == "MouseLeft" {
                errors.push(
                    file,
                    &join(path, code),
                    "MouseLeft не может быть клавишей экрана".to_string(),
                );
                return None;
            }
            let cmd = parse_button_command(cmd_json, file, &join(path, code), "клавиши", errors)?;
            Some((code.clone(), cmd))
        })
        .collect()
}

struct ParsedScreen {
    name: String,
    world_runs: bool,
    elements: ParsedElements,
    keys: Vec<(String, ParsedCommand)>,
    /// Raw `music` name, not yet resolved against `files.music` — «Звук» →«Два вида звука». Resolution happens in `resolve_screens`, same as `font` on an element.
    music: Option<String>,
    /// «Ветер и частицы» → «На экране»: клетка частиц экрана в точках `screens.json`.
    cell_pixels: Option<f64>,
    /// «Ветер и частицы» → «На экране»: ровный ветер частиц экрана.
    wind: Vec2,
    path: String,
}

fn parse_screen(
    value: &Json,
    index: usize,
    file: &str,
    context: ElementContext<'_>,
    errors: &mut ErrorSink,
) -> Option<ParsedScreen> {
    let path = format!("screens[{index}]");
    let obj = expect_object(value, file, &path, errors)?;
    reject_unknown_keys(
        obj,
        &[
            "name",
            "world_runs",
            "elements",
            "keys",
            "music",
            "cell_pixels",
            "wind",
        ],
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
                        context,
                        Scope::Screen,
                        errors,
                    )
                    .map(|element| (i, element))
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
    let cell_pixels = obj
        .get("cell_pixels")
        .and_then(|v| parse_screen_cell_pixels(v, file, &join(&path, "cell_pixels"), errors));
    if !obj.contains_key("cell_pixels") {
        require_screen_cell_pixels(name.as_deref(), &path, &elements, file, errors);
    }
    let wind = obj
        .get("wind")
        .and_then(|v| parse_vec2(v, file, &join(&path, "wind"), errors))
        .unwrap_or_default();
    Some(ParsedScreen {
        name: name?,
        world_runs: world_runs?,
        elements,
        keys,
        music,
        cell_pixels,
        wind,
        path,
    })
}

/// `screens.json`'s two root lists — «Надписи и полоски в мире», требование 1: `world_elements`
/// is optional, and its absence changes nothing (`Default` gives an empty list, same as before it
/// existed).
#[derive(Default)]
struct ParsedScreensFile {
    screens: Vec<ParsedScreen>,
    world_elements: Vec<ParsedWorldElement>,
    /// «Подсказка»: блок вида подсказок, со шрифтом и картинкой ещё по именам.
    tooltip: Option<ParsedTooltipStyle>,
    /// «Раскладка экрана»: размер окна, под который расставлены элементы.
    ui_reference: Option<[f32; 2]>,
}

fn parse_screens_json(
    text: &str,
    file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> ParsedScreensFile {
    let Some(root) = parse_json_or_error(file, text, errors) else {
        return ParsedScreensFile::default();
    };
    let Some(obj) = expect_object(&root, file, "", errors) else {
        return ParsedScreensFile::default();
    };
    reject_unknown_keys(
        obj,
        &["screens", "world_elements", "tooltip", "ui_reference"],
        file,
        "",
        errors,
    );
    let tooltip = obj
        .get("tooltip")
        .and_then(|v| parse_tooltip_style(v, file, errors));
    let ui_reference = obj
        .get("ui_reference")
        .and_then(|v| parse_ui_reference(v, file, errors));
    let context = ElementContext {
        properties,
        tooltips: obj.contains_key("tooltip"),
    };
    let Some(screens_json) = obj.get("screens") else {
        errors.push(file, "", "отсутствует список экранов");
        return ParsedScreensFile::default();
    };
    let Some(arr) = expect_array(screens_json, file, "screens", errors) else {
        return ParsedScreensFile::default();
    };
    let screens: Vec<ParsedScreen> = arr
        .iter()
        .enumerate()
        .filter_map(|(i, v)| parse_screen(v, i, file, context, errors))
        .collect();
    let world_elements = match obj.get("world_elements") {
        None => Vec::new(),
        Some(v) => match expect_array(v, file, "world_elements", errors) {
            Some(arr) => arr
                .iter()
                .enumerate()
                .filter_map(|(i, v)| {
                    parse_world_element(
                        v,
                        &format!("world_elements[{i}]"),
                        file,
                        properties,
                        errors,
                    )
                })
                .collect(),
            None => Vec::new(),
        },
    };
    ParsedScreensFile {
        screens,
        world_elements,
        tooltip,
        ui_reference,
    }
}

/// «Ветер и частицы» → «На экране»: `cell_pixels` экрана — число больше нуля, сколько точек
/// `screens.json` в одной клетке частиц.
fn parse_screen_cell_pixels(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<f64> {
    let n = expect_number(value, file, path, errors)?;
    if n > 0.0 {
        return Some(n);
    }
    errors.push(
        file,
        path,
        format!(
            "свойство \"cell_pixels\" — сколько точек экрана в одной клетке частиц, число больше нуля; получено {n}"
        ),
    );
    None
}

/// «Ветер и частицы» → «На экране»: панель со свойствами частиц меряет их клетками экрана, а клетку
/// называет только `cell_pixels` самого экрана: без него частицам не во что мериться.
fn require_screen_cell_pixels(
    screen_name: Option<&str>,
    screen_path: &str,
    elements: &ParsedElements,
    file: &str,
    errors: &mut ErrorSink,
) {
    for (index, element) in elements {
        if matches!(
            element,
            ParsedElementData::Panel {
                particles: Some(_),
                ..
            }
        ) {
            errors.push(
                file,
                &join(&join(screen_path, "elements"), &format!("[{index}]")),
                format!(
                    "у панели свойства частиц, а у экрана \"{}\" нет cell_pixels: частицы меряются клетками, а клетку задаёт cell_pixels экрана — сколько точек экрана в одной клетке",
                    screen_name.unwrap_or_default()
                ),
            );
        }
    }
}

/// «Подсказка» после разбора: шрифт и картинка ещё по именам.
#[derive(Debug, Clone)]
struct ParsedTooltipStyle {
    color: Option<[f32; 4]>,
    image_name: Option<String>,
    image_scale: f32,
    font_name: String,
    font_size: f32,
    text_color: [f32; 4],
    padding: [f32; 2],
    gap: f32,
    delay: f64,
}

/// Подсказка ждёт курсор столько секунд, когда `delay` не задан.
const DEFAULT_TOOLTIP_DELAY: f64 = 0.5;

/// «Подсказка»: блок `tooltip` в `screens.json` — плашка `image` или `color`, `image_scale`, `font`,
/// `font_size`, `text_color`, `padding`, `gap`, `delay`.
fn parse_tooltip_style(
    value: &Json,
    file: &str,
    errors: &mut ErrorSink,
) -> Option<ParsedTooltipStyle> {
    let path = "tooltip";
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(
        obj,
        &[
            "image",
            "color",
            "image_scale",
            "font",
            "font_size",
            "text_color",
            "padding",
            "gap",
            "delay",
        ],
        file,
        path,
        errors,
    );
    let color = parse_optional_ui_color(obj, "color", file, path, errors);
    let image_name = parse_optional_string(obj, "image", file, path, errors);
    if let (Some(color), Some(image_name)) = (&color, &image_name) {
        validate_fill_choice(color.is_some(), image_name.is_some(), file, path, errors);
    }
    let has_image = image_name.as_ref().is_some_and(Option::is_some);
    let image_scale = parse_image_scale(obj, has_image, file, path, errors);
    let font_name = require_field(obj, "font", file, path, errors)
        .and_then(|v| expect_string(v, file, &join(path, "font"), errors));
    let font_size = match obj.get("font_size") {
        None => Some(16.0),
        Some(v) => expect_number(v, file, &join(path, "font_size"), errors).filter(|n| {
            let ok = *n > 0.0;
            if !ok {
                errors.push(
                    file,
                    &join(path, "font_size"),
                    format!("свойство \"font_size\" должно быть больше нуля, получено {n}"),
                );
            }
            ok
        }),
    };
    let text_color = parse_optional_ui_color(obj, "text_color", file, path, errors)
        .map(|c| c.unwrap_or([1.0, 1.0, 1.0, 1.0]));
    let padding = match obj.get("padding") {
        None => Some([0.0, 0.0]),
        Some(v) => parse_vec2(v, file, &join(path, "padding"), errors).and_then(|p| {
            if p[0] >= 0.0 && p[1] >= 0.0 {
                Some([p[0] as f32, p[1] as f32])
            } else {
                errors.push(
                    file,
                    &join(path, "padding"),
                    format!(
                        "свойство \"padding\" — отступы [по бокам, сверху и снизу], два числа от 0; получено [{}, {}]",
                        p[0], p[1]
                    ),
                );
                None
            }
        }),
    };
    let gap = parse_not_negative(obj, "gap", 0.0, file, path, errors);
    let delay = parse_not_negative(obj, "delay", DEFAULT_TOOLTIP_DELAY, file, path, errors);
    Some(ParsedTooltipStyle {
        color: color?,
        image_name: image_name?,
        image_scale: image_scale?.unwrap_or(1.0),
        font_name: font_name?,
        font_size: font_size? as f32,
        text_color: text_color?,
        padding: padding?,
        gap: gap? as f32,
        delay: delay?,
    })
}

/// Необязательное число `key` от 0, `default` — когда его нет. `None` — ошибка уже названа.
fn parse_not_negative(
    obj: &serde_json::Map<String, Json>,
    key: &str,
    default: f64,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<f64> {
    let Some(v) = obj.get(key) else {
        return Some(default);
    };
    let n = expect_number(v, file, &join(path, key), errors)?;
    if n < 0.0 {
        errors.push(
            file,
            &join(path, key),
            format!("свойство \"{key}\" не может быть меньше нуля, получено {n}"),
        );
        return None;
    }
    Some(n)
}

/// «Раскладка экрана»: `ui_reference` — пара чисел больше нуля.
fn parse_ui_reference(value: &Json, file: &str, errors: &mut ErrorSink) -> Option<[f32; 2]> {
    let size = parse_vec2(value, file, "ui_reference", errors)?;
    if size[0] > 0.0 && size[1] > 0.0 {
        return Some([size[0] as f32, size[1] as f32]);
    }
    errors.push(
        file,
        "ui_reference",
        format!(
            "свойство \"ui_reference\" — размер окна образца [ширина, высота], два числа больше нуля; получено [{}, {}]",
            size[0], size[1]
        ),
    );
    None
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

/// «Выбор»: строки таблицы группы по ключу места.
type ChoiceRows = serde_json::Map<String, Json>;

/// «Выбор»: таблицы данных, по которым проверяются `choose`, `needs_choice` и `{choice:…}`, и группы, на которые
/// ссылаются экраны. `declared` — `files.tables`, `tables` — те из них, что разобрались.
struct ChoiceChecker<'a> {
    declared: &'a [(String, String)],
    tables: &'a [(String, Json)],
    /// Группы, у которых есть место с `choose` хоть на одном экране.
    choosable: std::collections::HashSet<String>,
    /// Группы, на которые что-то ссылается, в порядке первой ссылки.
    used: Vec<String>,
    /// Таблицы, строение которых уже названо ошибкой.
    broken: std::collections::HashSet<String>,
}

impl<'a> ChoiceChecker<'a> {
    fn new(
        declared: &'a [(String, String)],
        tables: &'a [(String, Json)],
        screens: &[ParsedScreen],
    ) -> Self {
        let choosable = screens
            .iter()
            .flat_map(|screen| &screen.elements)
            .filter_map(|(_, element)| match element {
                ParsedElementData::Panel { choice, .. }
                | ParsedElementData::Button { choice, .. } => choice.choose.clone(),
                _ => None,
            })
            .collect();
        ChoiceChecker {
            declared,
            tables,
            choosable,
            used: Vec::new(),
            broken: std::collections::HashSet::new(),
        }
    }

    /// Строки таблицы группы. `None` — таблицы нет в `files.tables` или у неё не те строки (ошибка названа тут),
    /// либо таблица не разобралась (её ошибка названа раньше).
    fn rows(
        &mut self,
        group: &str,
        file: &str,
        path: &str,
        errors: &mut ErrorSink,
    ) -> Option<&'a ChoiceRows> {
        if !self.declared.iter().any(|(name, _)| name == group) {
            errors.push(
                file,
                path,
                format!(
                    "таблицы данных \"{group}\" нет в files.tables game.json: группа выбора названа по таблице данных"
                ),
            );
            return None;
        }
        if !self.used.iter().any(|used| used == group) {
            self.used.push(group.to_string());
        }
        let tables: &'a [(String, Json)] = self.tables;
        let (_, table) = tables.iter().find(|(name, _)| name == group)?;
        let rows = table
            .as_object()
            .filter(|rows| rows.values().all(Json::is_object));
        if rows.is_none() && self.broken.insert(group.to_string()) {
            errors.push(
                file,
                path,
                format!(
                    "таблица \"{group}\" — не объект со строками-объектами: группа выбора берёт места из ключей строк, а поля из самих строк"
                ),
            );
        }
        rows
    }

    /// `{choice:…}` называет таблицу, которая есть, и поле, которое есть хоть в одной её строке.
    fn reference(
        &mut self,
        choice: &ChoiceRef,
        file: &str,
        path: &str,
        errors: &mut ErrorSink,
    ) -> bool {
        let Some(rows) = self.rows(&choice.group, file, path, errors) else {
            return false;
        };
        let Some(field) = &choice.field else {
            return true;
        };
        let known = rows.values().any(|row| row.get(field).is_some());
        if !known {
            errors.push(
                file,
                path,
                format!(
                    "в строках таблицы \"{}\" нет поля \"{field}\": подстановка берёт поле строки выбранного места",
                    choice.group
                ),
            );
        }
        known
    }

    /// `choose` называет таблицу, которая есть, а `name` места — ключ её строки.
    fn place(
        &mut self,
        group: &str,
        place: &str,
        file: &str,
        group_path: &str,
        place_path: &str,
        errors: &mut ErrorSink,
    ) -> bool {
        let Some(rows) = self.rows(group, file, group_path, errors) else {
            return false;
        };
        let known = rows.contains_key(place);
        if !known {
            let keys = rows.keys().cloned().collect::<Vec<_>>().join(", ");
            errors.push(
                file,
                place_path,
                format!(
                    "имя места \"{place}\" — не ключ строки таблицы \"{group}\": choose выбирает строку таблицы по name; ключи: {keys}"
                ),
            );
        }
        known
    }

    /// `needs_choice` называет таблицу, которая есть, и группу, в которой что-то можно выбрать.
    fn needs(&mut self, group: &str, file: &str, path: &str, errors: &mut ErrorSink) -> bool {
        if self.rows(group, file, path, errors).is_none() {
            return false;
        }
        let choosable = self.choosable.contains(group);
        if !choosable {
            errors.push(
                file,
                path,
                format!(
                    "needs_choice называет группу \"{group}\", а ни у одной панели или кнопки ни на одном экране нет choose: \"{group}\": кнопка осталась бы запертой навсегда"
                ),
            );
        }
        choosable
    }

    /// Таблицы групп, на которые ссылаются экраны, с полями строк текстом.
    fn into_tables(self) -> std::collections::HashMap<String, ChoiceTable> {
        self.used
            .iter()
            .filter_map(|group| {
                let (_, table) = self.tables.iter().find(|(name, _)| name == group)?;
                let rows = table
                    .as_object()
                    .filter(|rows| rows.values().all(Json::is_object))?;
                let table = rows
                    .iter()
                    .filter_map(|(place, row)| {
                        let fields = row
                            .as_object()?
                            .iter()
                            .map(|(field, value)| (field.clone(), choice_text(value)))
                            .collect();
                        Some((place.clone(), fields))
                    })
                    .collect();
                Some((group.clone(), table))
            })
            .collect()
    }
}

/// «Выбор»: поле строки таблицы текстом подстановки — строка как есть, число без лишней дробной части
/// (`8`, а не `8.0`), признак как у свойства, остальное пусто.
fn choice_text(value: &Json) -> String {
    match value {
        Json::String(text) => text.clone(),
        Json::Number(number) => number
            .as_f64()
            .map(|number| format!("{number}"))
            .unwrap_or_default(),
        Json::Bool(true) => "да".to_string(),
        _ => String::new(),
    }
}

/// «Выбор»: каждая `{choice:…}` в тексте называет таблицу и поле, которые есть.
fn validate_choice_refs(
    text: &[TextPart],
    choices: &mut ChoiceChecker<'_>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    let mut ok = true;
    for part in text {
        if let TextPart::Choice(choice) = part {
            ok &= choices.reference(choice, file, path, errors);
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

/// «Начальные значения у new_game», требование 29: one `"<объект>.<свойство>"` → значение pair —
/// the object must be named in `scene.json`, the property known and settable by a plain value
/// (not `grid`/`keys`), the value in `scene.json`'s own form. Several scene objects can share a
/// name; which one actually gets it is a runtime question (`Game::new_game_with_values`), so only
/// the name itself is checked here, not a specific id.
#[allow(clippy::too_many_arguments)]
fn resolve_initial_value(
    key: &str,
    value_json: &Json,
    properties: &PropertyTable,
    images: &[ImageDecl],
    scene_names: &std::collections::HashSet<&str>,
    choices: &mut ChoiceChecker<'_>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<(String, PropertyId, InitialValue)> {
    let Some(dot) = key.find('.') else {
        errors.push(
            file,
            path,
            format!("ключ \"{key}\" должен быть вида \"объект.свойство\""),
        );
        return None;
    };
    let (object_name, prop_name) = (&key[..dot], &key[dot + 1..]);
    if object_name.is_empty() || prop_name.is_empty() {
        errors.push(
            file,
            path,
            format!("ключ \"{key}\" должен быть вида \"объект.свойство\""),
        );
        return None;
    }
    if !scene_names.contains(object_name) {
        errors.push(
            file,
            path,
            format!("в scene.json нет объекта с именем \"{object_name}\""),
        );
        return None;
    }
    let prop = resolve_property(prop_name, properties, file, path, errors)?;
    if let Some(text) = value_json.as_str().filter(|text| text.contains("{choice:")) {
        let choice = parse_whole_choice(text, "начальном значении", file, path, errors)?;
        if properties.kind(prop) != PropKind::Text {
            errors.push(
                file,
                path,
                format!(
                    "\"{text}\": у свойства \"{prop_name}\" вид {}, а выбор подставляется строкой в свойство вида text",
                    properties.kind(prop).label()
                ),
            );
            return None;
        }
        return choices
            .reference(&choice, file, path, errors)
            .then(|| (object_name.to_string(), prop, InitialValue::Choice(choice)));
    }
    let value = parse_scalar_value(value_json, prop, properties, images, file, path, errors)?;
    Some((object_name.to_string(), prop, InitialValue::Fixed(value)))
}

#[allow(clippy::too_many_arguments)]
fn resolve_on_click(
    cmd: &ParsedCommand,
    name_to_id: &std::collections::HashMap<String, ScreenId>,
    parsed_screens: &[ParsedScreen],
    properties: &PropertyTable,
    images: &[ImageDecl],
    scene_names: &std::collections::HashSet<&str>,
    choices: &mut ChoiceChecker<'_>,
    initial_values_table: &mut Vec<Vec<(String, PropertyId, InitialValue)>>,
    runs: &mut RunTable<'_>,
    origin: String,
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
        ParsedCommand::NewGame(name, raw_values) => {
            let id = resolve_button_target(name, name_to_id, file, path, subject, errors)?;
            if !parsed_screens[id].world_runs {
                errors.push(
                    file,
                    path,
                    format!("new_game ведёт на экран \"{name}\" без world_runs"),
                );
                return None;
            }
            let values_id = match raw_values {
                None => None,
                Some(raw) => {
                    let values_path = join(path, "[2]");
                    let mut ok = true;
                    let mut resolved = Vec::with_capacity(raw.len());
                    for (key, value_json) in raw {
                        match resolve_initial_value(
                            key,
                            value_json,
                            properties,
                            images,
                            scene_names,
                            choices,
                            file,
                            &join(&values_path, key),
                            errors,
                        ) {
                            Some(v) => resolved.push(v),
                            None => ok = false,
                        }
                    }
                    if !ok {
                        return None;
                    }
                    initial_values_table.push(resolved);
                    Some(initial_values_table.len() - 1)
                }
            };
            Some(ButtonCommand::NewGame(id, values_id))
        }
        ParsedCommand::Resume => Some(ButtonCommand::Resume),
        ParsedCommand::Quit => Some(ButtonCommand::Quit),
        ParsedCommand::ToggleSound => Some(ButtonCommand::ToggleSound),
        ParsedCommand::Exit => Some(ButtonCommand::Exit),
        ParsedCommand::Run(function) => runs
            .add(function, origin, file, path, errors)
            .map(ButtonCommand::Run),
    }
}

/// `["run", "<функция>"]` кнопок и клавиш экранов, сверенные с файлом кода по ходу разбора:
/// «Экраны и состояние» → «Проверка данных перед запуском». `declared` — функции файла кода; `None`,
/// когда файла нет или он не загрузился (его ошибка уже названа).
struct RunTable<'a> {
    calls: Vec<RunCall>,
    has_code: bool,
    declared: Option<&'a [String]>,
}

impl RunTable<'_> {
    fn add(
        &mut self,
        function: &str,
        origin: String,
        file: &str,
        path: &str,
        errors: &mut ErrorSink,
    ) -> Option<RunId> {
        if !self.has_code {
            errors.push(file, path, "run в игре без files.code".to_string());
            return None;
        }
        if let Some(declared) = self.declared
            && !declared.iter().any(|d| d == function)
        {
            errors.push(
                file,
                path,
                format!(
                    "функции \"{function}\" в коде нет; {}",
                    describe_declared_functions(declared)
                ),
            );
            return None;
        }
        self.calls.push(RunCall {
            function: function.to_string(),
            origin,
        });
        Some(self.calls.len() - 1)
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
            // «Картинки» → «Видео»: видео рисует только объект плоской сцены.
            if images[image].video {
                errors.push(
                    file,
                    image_path,
                    format!(
                        "\"{name}\" объявлена видео: кнопка и шкала её не берут; видео играет только панель"
                    ),
                );
                return None;
            }
            // «Картинки», требование 23: `frame_by` — у картинки панели или кнопки — ошибка.
            if images[image].frame_by.is_some() {
                errors.push(
                    file,
                    image_path,
                    format!("\"{name}\" объявлена с frame_by: панель и кнопка её не берут"),
                );
                return None;
            }
            // «Картинки», требование 12: `size` — та же ошибка, что и `frame_by`.
            if images[image].size.is_some() {
                errors.push(
                    file,
                    image_path,
                    format!("\"{name}\" объявлена с size: панель и кнопка её не берут"),
                );
                return None;
            }
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

/// «Наведение»: имена мест наведения одного экрана и номера их элементов.
type HoverPlaces = std::collections::HashMap<String, usize>;

/// «Наведение»: места наведения экрана — панели и кнопки с `name`. Имя единственное на экране.
fn screen_hover_places(
    screen: &ParsedScreen,
    elements_path: &str,
    file: &str,
    errors: &mut ErrorSink,
) -> HoverPlaces {
    let mut places = HoverPlaces::new();
    for (index, (file_index, data)) in screen.elements.iter().enumerate() {
        let name = match data {
            ParsedElementData::Panel { hover, .. } | ParsedElementData::Button { hover, .. } => {
                hover.name.as_deref()
            }
            _ => None,
        };
        let Some(name) = name else {
            continue;
        };
        if places.insert(name.to_string(), index).is_some() {
            errors.push(
                file,
                &join(&join(elements_path, &format!("[{file_index}]")), "name"),
                format!(
                    "имя места наведения \"{name}\" повторяется на экране \"{}\": имя единственное на экране",
                    screen.name
                ),
            );
        }
    }
    places
}

/// «Наведение»: место, названное в `key` (`show_on_hover` или `play_on_hover`) элемента по пути `path`.
/// `Some(None)` — поля нет, внешний `None` — места с таким именем на экране нет, ошибка названа.
fn resolve_hover_place(
    name: Option<&str>,
    key: &str,
    places: &HoverPlaces,
    screen_name: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<usize>> {
    let Some(name) = name else {
        return Some(None);
    };
    match places.get(name) {
        Some(&index) => Some(Some(index)),
        None => {
            errors.push(
                file,
                &join(path, key),
                format!(
                    "на экране \"{screen_name}\" нет места наведения \"{name}\": name есть у панели или кнопки этого экрана"
                ),
            );
            None
        }
    }
}

/// «Видео на панели»: заливка панели с `play_on_hover` — видео `image`; у видео панели в `files.images`
/// обязателен `reverse`.
fn resolve_panel_video(
    image: ImageId,
    opacity: Option<f32>,
    images: &[ImageDecl],
    file: &str,
    image_path: &str,
    errors: &mut ErrorSink,
) -> Option<Fill> {
    let decl = &images[image];
    if decl.reverse.is_none() {
        errors.push(
            file,
            image_path,
            format!(
                "видео \"{}\" играет панель, а в files.images у него нет reverse: нужен путь к тому же ролику задом наперёд",
                decl.name
            ),
        );
        return None;
    }
    Some(Fill::Image {
        image,
        opacity: opacity.unwrap_or(1.0),
    })
}

#[allow(clippy::too_many_arguments)]
fn resolve_element(
    data: &ParsedElementData,
    path: &str,
    file: &str,
    scene_file: &str,
    screen_name: &str,
    fonts: &[(String, String)],
    images: &[ImageDecl],
    properties: &PropertyTable,
    scene_names: &std::collections::HashSet<&str>,
    name_to_id: &std::collections::HashMap<String, ScreenId>,
    parsed_screens: &[ParsedScreen],
    places: &HoverPlaces,
    choices: &mut ChoiceChecker<'_>,
    initial_values_table: &mut Vec<Vec<(String, PropertyId, InitialValue)>>,
    runs: &mut RunTable<'_>,
    errors: &mut ErrorSink,
) -> Option<Element> {
    match data {
        ParsedElementData::Panel {
            placement,
            color,
            image_name,
            own_image,
            opacity,
            image_scale,
            tooltip,
            for_,
            hover,
            choice,
            choice_image,
            rest_at,
            idle_name,
            particles,
        } => {
            let image_path = join(path, "image");
            let show_on_hover = resolve_hover_place(
                hover.show_on_hover.as_deref(),
                "show_on_hover",
                places,
                screen_name,
                file,
                path,
                errors,
            );
            let play_on_hover = resolve_hover_place(
                hover.play_on_hover.as_deref(),
                "play_on_hover",
                places,
                screen_name,
                file,
                path,
                errors,
            );
            let video = image_name.as_deref().and_then(|name| {
                images
                    .iter()
                    .position(|decl| decl.video && decl.name == name)
            });
            let choice_ok = choice_image
                .as_ref()
                .is_none_or(|image| choices.reference(image, file, &image_path, errors));
            let choose_ok = choice.choose.as_deref().is_none_or(|group| {
                choices.place(
                    group,
                    hover.name.as_deref().unwrap_or_default(),
                    file,
                    &join(path, "choose"),
                    &join(path, "name"),
                    errors,
                )
            });
            let fill = match (choice_image, own_image, video) {
                (Some(choice), _, _) => Some(PanelFill::Choice {
                    choice: choice.clone(),
                    names: images.iter().map(|decl| decl.name.clone()).collect(),
                    opacity: opacity.unwrap_or(1.0),
                }),
                (None, Some(property), _) => Some(PanelFill::Own {
                    image: ImageRef::Own {
                        property: *property,
                        names: images.iter().map(|decl| decl.name.clone()).collect(),
                    },
                    opacity: opacity.unwrap_or(1.0),
                }),
                (None, None, Some(image)) if hover.play_on_hover.is_some() => {
                    resolve_panel_video(image, *opacity, images, file, &image_path, errors)
                        .map(PanelFill::Fixed)
                }
                // «Видео на панели»: видео без `play_on_hover` крутится по кругу, `reverse` ему не нужен.
                (None, None, Some(image)) => Some(PanelFill::Fixed(Fill::Image {
                    image,
                    opacity: opacity.unwrap_or(1.0),
                })),
                (None, None, None) => resolve_fill(
                    *color,
                    image_name.as_deref(),
                    *opacity,
                    None,
                    images,
                    file,
                    &image_path,
                    errors,
                )
                .map(PanelFill::Fixed),
            };
            let plays_nothing = hover.play_on_hover.is_some() && video.is_none();
            if plays_nothing {
                errors.push(
                    file,
                    &join(path, "play_on_hover"),
                    "play_on_hover у панели без видео: играть нечего, в image нужна картинка, объявленная в files.images путём на .mp4",
                );
            }
            let rests_nowhere = rest_at.is_some() && video.is_none();
            if rests_nowhere {
                errors.push(
                    file,
                    &join(path, "rest_at"),
                    "rest_at у панели без видео: позы покоя нет, в image нужна картинка, объявленная в files.images путём на .mp4",
                );
            }
            let rests_in_a_loop =
                rest_at.is_some() && video.is_some() && hover.play_on_hover.is_none();
            if rests_in_a_loop {
                errors.push(
                    file,
                    &join(path, "rest_at"),
                    "rest_at у панели с видео по кругу: поза покоя нужна только видео по наведению, а это идёт без play_on_hover",
                );
            }
            let idle = resolve_panel_idle(
                idle_name.as_deref(),
                hover.play_on_hover.is_some(),
                video,
                images,
                file,
                &join(path, "idle"),
                errors,
            );
            let tooltip_ok =
                validate_tooltip_refs(tooltip, scene_names, file, scene_file, path, errors);
            let (fill, show_on_hover, play_on_hover, idle) =
                (fill?, show_on_hover?, play_on_hover?, idle?);
            (tooltip_ok
                && choice_ok
                && choose_ok
                && !plays_nothing
                && !rests_nowhere
                && !rests_in_a_loop)
                .then(|| Element::Panel {
                    placement: *placement,
                    fill,
                    image_scale: image_scale.unwrap_or(1.0),
                    tooltip: tooltip.clone(),
                    for_: for_.clone(),
                    name: hover.name.clone(),
                    choose: choice.choose.clone(),
                    show_on_hover,
                    play_on_hover,
                    rest_at: *rest_at,
                    idle,
                    particles: *particles,
                })
        }
        ParsedElementData::Bar(bar) => {
            resolve_bar(bar, path, file, scene_file, images, scene_names, errors)
        }
        ParsedElementData::Label {
            placement,
            text,
            font_name,
            font_size,
            color,
            align,
            wrap,
            for_,
            hover,
        } => {
            let show_on_hover = resolve_hover_place(
                hover.show_on_hover.as_deref(),
                "show_on_hover",
                places,
                screen_name,
                file,
                path,
                errors,
            );
            let font = resolve_font(font_name, fonts, file, &join(path, "font"), errors);
            let text_ok = validate_text_refs(
                text,
                scene_names,
                file,
                scene_file,
                &join(path, "text"),
                errors,
            );
            let choice_ok = validate_choice_refs(text, choices, file, &join(path, "text"), errors);
            let (font, show_on_hover) = (font?, show_on_hover?);
            if !text_ok || !choice_ok {
                return None;
            }
            Some(Element::Label {
                placement: *placement,
                text: text.clone(),
                font,
                font_size: *font_size,
                color: *color,
                align: *align,
                wrap: *wrap,
                for_: for_.clone(),
                show_on_hover,
            })
        }
        ParsedElementData::Button {
            placement,
            text,
            caption,
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
            image_scale,
            tooltip,
            on_click,
            for_,
            hover,
            choice,
        } => {
            let font = resolve_font(font_name, fonts, file, &join(path, "font"), errors);
            let tooltip_ok =
                validate_tooltip_refs(tooltip, scene_names, file, scene_file, path, errors);
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
                properties,
                images,
                scene_names,
                choices,
                initial_values_table,
                runs,
                format!("экран \"{screen_name}\", кнопка \"{caption}\""),
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
            let choice_ok = validate_choice_refs(text, choices, file, &join(path, "text"), errors);
            let choose_ok = choice.choose.as_deref().is_none_or(|group| {
                choices.place(
                    group,
                    hover.name.as_deref().unwrap_or_default(),
                    file,
                    &join(path, "choose"),
                    &join(path, "name"),
                    errors,
                )
            });
            let needs_ok = choice.needs_choice.as_deref().is_none_or(|group| {
                choices.needs(group, file, &join(path, "needs_choice"), errors)
            });
            if !text_ok || !tooltip_ok || !choice_ok || !choose_ok || !needs_ok {
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
                image_scale: image_scale.unwrap_or(1.0),
                tooltip: tooltip.clone(),
                on_click,
                for_: for_.as_deref().cloned(),
                name: hover.name.clone(),
                choose: choice.choose.clone(),
                needs_choice: choice.needs_choice.clone(),
                text_color_disabled: choice.text_color_disabled,
            })
        }
        ParsedElementData::List {
            placement,
            for_,
            row_height,
            gap,
            sort_by,
            descending,
            row,
            scroll_color,
        } => {
            let row_path = join(path, "row");
            let resolved: Vec<Option<Element>> = row
                .iter()
                .map(|(i, item)| {
                    resolve_element(
                        item,
                        &join(&row_path, &format!("[{i}]")),
                        file,
                        scene_file,
                        screen_name,
                        fonts,
                        images,
                        properties,
                        scene_names,
                        name_to_id,
                        parsed_screens,
                        places,
                        choices,
                        initial_values_table,
                        runs,
                        errors,
                    )
                })
                .collect();
            let row = resolved.into_iter().collect::<Option<Vec<_>>>()?;
            Some(Element::List {
                placement: *placement,
                for_: for_.clone(),
                row_height: *row_height,
                gap: *gap,
                sort_by: *sort_by,
                descending: *descending,
                row,
                scroll_color: *scroll_color,
            })
        }
    }
}

/// «Подсказка»: подстановки `{имя объекта.свойство}` в подсказке называют объекты из сцены, как в `text`.
fn validate_tooltip_refs(
    tooltip: &Option<Vec<TextPart>>,
    scene_names: &std::collections::HashSet<&str>,
    file: &str,
    scene_file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> bool {
    tooltip.as_ref().is_none_or(|text| {
        validate_text_refs(
            text,
            scene_names,
            file,
            scene_file,
            &join(path, "tooltip"),
            errors,
        )
    })
}

/// «Шкала»: заливка и подложка по именам картинок — как у панели.
fn resolve_bar(
    bar: &ParsedBar,
    path: &str,
    file: &str,
    scene_file: &str,
    images: &[ImageDecl],
    scene_names: &std::collections::HashSet<&str>,
    errors: &mut ErrorSink,
) -> Option<Element> {
    let fill = resolve_fill(
        bar.color,
        bar.image_name.as_deref(),
        None,
        None,
        images,
        file,
        &join(path, "image"),
        errors,
    );
    let back = match (bar.back_color, bar.back_image_name.as_deref()) {
        (None, None) => Some(None),
        (color, image) => resolve_fill(
            color,
            image,
            None,
            None,
            images,
            file,
            &join(path, "back_image"),
            errors,
        )
        .map(Some),
    };
    let tooltip_ok =
        validate_tooltip_refs(&bar.tooltip, scene_names, file, scene_file, path, errors);
    let (fill, back) = (fill?, back?);
    tooltip_ok.then(|| Element::Bar {
        placement: bar.placement,
        value: bar.value,
        max: bar.max,
        fill,
        back,
        image_scale: bar.image_scale.unwrap_or(1.0),
        tooltip: bar.tooltip.clone(),
        for_: bar.for_.clone(),
    })
}

/// «Подсказка»: вид подсказок со шрифтом и картинкой, найденными по именам.
fn resolve_tooltip_style(
    parsed: &ParsedTooltipStyle,
    fonts: &[(String, String)],
    images: &[ImageDecl],
    file: &str,
    errors: &mut ErrorSink,
) -> Option<TooltipStyle> {
    let font = resolve_font(&parsed.font_name, fonts, file, "tooltip → font", errors);
    let fill = resolve_fill(
        parsed.color,
        parsed.image_name.as_deref(),
        None,
        None,
        images,
        file,
        "tooltip → image",
        errors,
    );
    Some(TooltipStyle {
        fill: fill?,
        image_scale: parsed.image_scale,
        font: font?,
        font_size: parsed.font_size,
        text_color: parsed.text_color,
        padding: parsed.padding,
        gap: parsed.gap,
        delay: parsed.delay,
    })
}

// ---------------------------------------------------------------------------------------------
// world_elements — «Надписи и полоски в мире»
// ---------------------------------------------------------------------------------------------

const WORLD_LABEL_KEYS: &[&str] = &[
    "kind",
    "for",
    "anchor",
    "offset",
    "size",
    "color",
    "text",
    "font",
    "font_size",
    "align",
];
const WORLD_IMAGE_KEYS: &[&str] = &["kind", "for", "anchor", "offset", "size", "image"];
const WORLD_BAR_KEYS: &[&str] = &[
    "kind",
    "for",
    "anchor",
    "offset",
    "size",
    "color",
    "value",
    "max",
    "back_color",
];

/// «Надписи и полоски в мире», требование 6: one `text` field after parsing — `{свойство}`
/// becomes a resolved `WorldTextPart::Value`, everything else stays literal. Unlike a screen's own
/// `{имя объекта.свойство}` (`parse_rich_text`), a dot here is an error: a world element only ever
/// substitutes its own object's property.
fn parse_world_text(
    s: &str,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<WorldTextPart>> {
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
        if inner.is_empty() {
            errors.push(
                file,
                path,
                format!("подстановка \"{{}}\" в \"{s}\" должна называть свойство"),
            );
            return None;
        }
        if inner.contains('.') {
            errors.push(
                file,
                path,
                format!(
                    "подстановка \"{{{inner}}}\" над объектом в мире называет только его собственное свойство, без точки"
                ),
            );
            return None;
        }
        let prop = resolve_property(&inner, properties, file, path, errors)?;
        if !literal.is_empty() {
            parts.push(WorldTextPart::Literal(std::mem::take(&mut literal)));
        }
        parts.push(WorldTextPart::Value(prop));
    }
    if !literal.is_empty() {
        parts.push(WorldTextPart::Literal(literal));
    }
    Some(parts)
}

/// «Надписи и полоски в мире», требование 5: `color`/`back_color`'s non-list form — a plain
/// string is `expect_ui_color`'s own `#rrggbb`/`#rrggbbaa`; `{"table": [...], "by": "<свойство>"}`
/// picks from a list by a `number`-kind property, mirroring `parse_number_expr`'s own `Table`
/// variant for rules.
fn parse_world_color(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<WorldColor> {
    if let Some(s) = value.as_str() {
        return parse_color(s).map(WorldColor::Solid).or_else(|| {
            errors.push(
                file,
                path,
                format!(
                    "цвет должен быть вида \"#rrggbb\", \"#rrggbbaa\" или {{table, by}}, получено \"{s}\""
                ),
            );
            None
        });
    }
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(obj, &["table", "by"], file, path, errors);
    let table_json = require_field(obj, "table", file, path, errors)?;
    let table_arr = expect_array(table_json, file, &join(path, "table"), errors)?;
    if table_arr.is_empty() {
        errors.push(file, &join(path, "table"), "список цветов пуст");
        return None;
    }
    let table_path = join(path, "table");
    let mut ok = true;
    let mut colors = Vec::with_capacity(table_arr.len());
    for (i, c) in table_arr.iter().enumerate() {
        match expect_ui_color(c, file, &join(&table_path, &format!("[{i}]")), errors) {
            Some(color) => colors.push(color),
            None => ok = false,
        }
    }
    let by_json = require_field(obj, "by", file, path, errors)?;
    let by = resolve_number_source(by_json, "by", properties, file, path, errors)?;
    ok.then_some(WorldColor::Table { colors, by })
}

/// «Надписи и полоски в мире», требование 51: `max` — a positive constant, or a `number`-kind
/// property named by a string, exactly like `value` (`resolve_number_source`) but also accepting a
/// plain literal.
fn parse_world_max(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<MaxSpec> {
    if let Some(n) = value.as_f64() {
        if n <= 0.0 {
            errors.push(
                file,
                path,
                format!("max должен быть больше нуля, получено {n}"),
            );
            return None;
        }
        return Some(MaxSpec::Const(n as f32));
    }
    let name = expect_string(value, file, path, errors)?;
    let prop = resolve_property(&name, properties, file, path, errors)?;
    if properties.kind(prop) != PropKind::Number {
        errors.push(
            file,
            path,
            format!(
                "max должен быть числом или называть свойство вида number, у \"{name}\" вид {}",
                properties.kind(prop).label()
            ),
        );
        return None;
    }
    Some(MaxSpec::Property(prop))
}

/// «Надписи, полоски и картинки в мире», требование 1: `image` — имя картинки из `files.images` или
/// `{свойство}`, занимающее строку целиком: имя берётся из свойства самого объекта. Так же — `image` панели
/// при объекте («Элемент при объекте»).
#[derive(Debug, Clone)]
enum ParsedImageRef {
    Name(String),
    Own(PropertyId),
}

fn parse_world_image(
    value: &Json,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ParsedImageRef> {
    let text = expect_string(value, file, path, errors)?;
    parse_image_name(text, properties, file, path, errors)
}

fn parse_image_name(
    text: String,
    properties: &PropertyTable,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ParsedImageRef> {
    let Some(inner) = text
        .strip_prefix('{')
        .and_then(|rest| rest.strip_suffix('}'))
    else {
        if text.contains(['{', '}']) {
            errors.push(
                file,
                path,
                format!(
                    "\"{text}\": подстановка {{свойство}} должна занимать имя картинки целиком"
                ),
            );
            return None;
        }
        return Some(ParsedImageRef::Name(text));
    };
    if inner.is_empty() || inner.contains(['{', '}', '.']) {
        errors.push(
            file,
            path,
            format!(
                "подстановка \"{text}\" называет одно собственное свойство объекта, без точки и скобок внутри"
            ),
        );
        return None;
    }
    let property = resolve_property(inner, properties, file, path, errors)?;
    if properties.kind(property) != PropKind::Text {
        errors.push(
            file,
            path,
            format!(
                "подстановка \"{text}\" называет не строковое свойство: у \"{inner}\" вид {}, а имя картинки берётся из свойства вида text",
                properties.kind(property).label()
            ),
        );
        return None;
    }
    Some(ParsedImageRef::Own(property))
}

/// Parsed, but with a `label`'s font still a name (`ParsedScreen`/`ParsedElementData`'s own
/// pattern) — `files.fonts` isn't known yet at parse time.
#[derive(Debug, Clone)]
enum ParsedWorldElementKind {
    Label {
        text: Vec<WorldTextPart>,
        font_name: String,
        font_size: Option<f32>,
        align: Align,
    },
    Bar {
        value: PropertyId,
        max: MaxSpec,
        back_color: Option<[f32; 4]>,
    },
    Image {
        image: ParsedImageRef,
    },
}

#[derive(Debug, Clone)]
struct ParsedWorldElement {
    for_: Selector,
    placement: Placement,
    color: WorldColor,
    kind: ParsedWorldElementKind,
}

fn parse_world_element(
    value: &Json,
    path: &str,
    file: &str,
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) -> Option<ParsedWorldElement> {
    let obj = expect_object(value, file, path, errors)?;
    let kind_json = require_field(obj, "kind", file, path, errors)?;
    let kind = expect_string(kind_json, file, &join(path, "kind"), errors)?;
    match kind.as_str() {
        "label" => {
            reject_unknown_keys(obj, WORLD_LABEL_KEYS, file, path, errors);
            let for_obj_json = require_field(obj, "for", file, path, errors)?;
            let for_obj = expect_object(for_obj_json, file, &join(path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(path, "for"), errors);
            let placement = parse_placement(obj, file, path, errors)?;
            let color_json = require_field(obj, "color", file, path, errors)?;
            let color =
                parse_world_color(color_json, properties, file, &join(path, "color"), errors)?;
            let text_json = require_field(obj, "text", file, path, errors)?;
            let text_str = expect_string(text_json, file, &join(path, "text"), errors)?;
            let text = parse_world_text(&text_str, properties, file, &join(path, "text"), errors)?;
            let font_json = require_field(obj, "font", file, path, errors)?;
            let font_name = expect_string(font_json, file, &join(path, "font"), errors)?;
            let font_size = match obj.get("font_size") {
                Some(v) => {
                    let n = expect_number(v, file, &join(path, "font_size"), errors)?;
                    if n <= 0.0 {
                        errors.push(
                            file,
                            &join(path, "font_size"),
                            format!("font_size должен быть больше нуля, получено {n}"),
                        );
                        return None;
                    }
                    Some(n as f32)
                }
                None => None,
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
            Some(ParsedWorldElement {
                for_,
                placement,
                color,
                kind: ParsedWorldElementKind::Label {
                    text,
                    font_name,
                    font_size,
                    align,
                },
            })
        }
        "bar" => {
            reject_unknown_keys(obj, WORLD_BAR_KEYS, file, path, errors);
            let for_obj_json = require_field(obj, "for", file, path, errors)?;
            let for_obj = expect_object(for_obj_json, file, &join(path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(path, "for"), errors);
            let placement = parse_placement(obj, file, path, errors)?;
            let color_json = require_field(obj, "color", file, path, errors)?;
            let color =
                parse_world_color(color_json, properties, file, &join(path, "color"), errors)?;
            let value_json = require_field(obj, "value", file, path, errors)?;
            let value = resolve_number_source(value_json, "value", properties, file, path, errors)?;
            let max_json = require_field(obj, "max", file, path, errors)?;
            let max = parse_world_max(max_json, properties, file, &join(path, "max"), errors)?;
            let back_color = match obj.get("back_color") {
                Some(v) => Some(expect_ui_color(v, file, &join(path, "back_color"), errors)?),
                None => None,
            };
            Some(ParsedWorldElement {
                for_,
                placement,
                color,
                kind: ParsedWorldElementKind::Bar {
                    value,
                    max,
                    back_color,
                },
            })
        }
        "image" => {
            reject_unknown_keys(obj, WORLD_IMAGE_KEYS, file, path, errors);
            let for_obj_json = require_field(obj, "for", file, path, errors)?;
            let for_obj = expect_object(for_obj_json, file, &join(path, "for"), errors)?;
            let for_ = resolve_selector(for_obj, properties, file, &join(path, "for"), errors);
            let placement = parse_placement(obj, file, path, errors)?;
            let image_json = require_field(obj, "image", file, path, errors)?;
            let image =
                parse_world_image(image_json, properties, file, &join(path, "image"), errors)?;
            Some(ParsedWorldElement {
                for_,
                placement,
                color: WorldColor::Solid([1.0; 4]),
                kind: ParsedWorldElementKind::Image { image },
            })
        }
        other => {
            errors.push(
                file,
                &join(path, "kind"),
                format!(
                    "неизвестный вид элемента в мире \"{other}\": ожидался label, bar или image"
                ),
            );
            None
        }
    }
}

/// Resolves one parsed world element's font name (the one thing font-independent parsing can't
/// finish) — mirrors `resolve_element`. «Надписи и полоски в мире», требование 3: `font_size`
/// defaults to the element's own height once `placement.size` is known.
fn resolve_world_element(
    parsed: &ParsedWorldElement,
    fonts: &[(String, String)],
    images: &[ImageDecl],
    image_names: &Arc<[String]>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<WorldElement> {
    match &parsed.kind {
        ParsedWorldElementKind::Bar {
            value,
            max,
            back_color,
        } => Some(WorldElement {
            for_: parsed.for_.clone(),
            placement: parsed.placement,
            color: parsed.color.clone(),
            kind: WorldElementKind::Bar {
                value: *value,
                max: *max,
                back_color: *back_color,
            },
        }),
        ParsedWorldElementKind::Label {
            text,
            font_name,
            font_size,
            align,
        } => {
            let font = resolve_font(font_name, fonts, file, &join(path, "font"), errors)?;
            Some(WorldElement {
                for_: parsed.for_.clone(),
                placement: parsed.placement,
                color: parsed.color.clone(),
                kind: WorldElementKind::Label {
                    text: text.clone(),
                    font,
                    font_size: font_size.unwrap_or(parsed.placement.size[1]),
                    align: *align,
                },
            })
        }
        ParsedWorldElementKind::Image { image } => {
            let image = match image {
                ParsedImageRef::Name(name) => ImageRef::Fixed(resolve_image(
                    name,
                    images,
                    file,
                    &join(path, "image"),
                    errors,
                )?),
                ParsedImageRef::Own(property) => ImageRef::Own {
                    property: *property,
                    names: Arc::clone(image_names),
                },
            };
            Some(WorldElement {
                for_: parsed.for_.clone(),
                placement: parsed.placement,
                color: parsed.color.clone(),
                kind: WorldElementKind::Image { image },
            })
        }
    }
}

fn resolve_world_elements(
    parsed: &[ParsedWorldElement],
    fonts: &[(String, String)],
    images: &[ImageDecl],
    file: &str,
    errors: &mut ErrorSink,
) -> Vec<WorldElement> {
    let image_names: Arc<[String]> = images.iter().map(|decl| decl.name.clone()).collect();
    parsed
        .iter()
        .enumerate()
        .filter_map(|(i, p)| {
            resolve_world_element(
                p,
                fonts,
                images,
                &image_names,
                file,
                &format!("world_elements[{i}]"),
                errors,
            )
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// files.tables — «Таблицы данных»
// ---------------------------------------------------------------------------------------------

/// Every place `null` sits inside `value`, as a `join()`-style path from the root (`""`) —
/// «Таблицы данных», требование 23: `null` on any depth is an error naming the table's own file
/// and place, same convention `locate::locate` already resolves for every other file.
fn collect_json_nulls(value: &Json, path: &str, out: &mut Vec<String>) {
    match value {
        Json::Null => out.push(path.to_string()),
        Json::Array(arr) => {
            for (i, v) in arr.iter().enumerate() {
                collect_json_nulls(v, &join(path, &format!("[{i}]")), out);
            }
        }
        Json::Object(obj) => {
            for (k, v) in obj {
                collect_json_nulls(v, &join(path, k), out);
            }
        }
        _ => {}
    }
}

/// «Таблицы данных», требования 22–24: `declared` — `files.tables`' name→path pairs, in file
/// order; `table_texts` — what the page (or `load_game_from_texts_with_tables`, for tests)
/// actually fetched for each name, `None` meaning it could not. A missing file, JSON that doesn't
/// parse, or `null` anywhere inside it are each prestart errors; a table that parses cleanly
/// becomes one `(имя, разобранный JSON)` entry, in declared order — «Код игры»: this same order is
/// what makes `tables`' own key order (and so `pairs` in Lua) the same on every load.
fn parse_and_validate_tables(
    declared: &[(String, String)],
    table_texts: &[(String, Option<String>)],
    errors: &mut ErrorSink,
) -> Vec<(String, Json)> {
    let mut out = Vec::with_capacity(declared.len());
    for (name, path) in declared {
        let text = table_texts
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, t)| t.as_deref());
        let Some(text) = text else {
            errors.push(
                path,
                "",
                format!(
                    "файл не найден; ожидался JSON-файл таблицы данных, названный в game.json → files → tables → {name}"
                ),
            );
            continue;
        };
        let Some(value) = parse_json_or_error(path, text, errors) else {
            continue;
        };
        let mut nulls = Vec::new();
        collect_json_nulls(&value, "", &mut nulls);
        if !nulls.is_empty() {
            for null_path in &nulls {
                errors.push(path, null_path, "null не допускается в таблице данных");
            }
            errors.fill_locations(path, text);
            continue;
        }
        errors.fill_locations(path, text);
        out.push((name.clone(), value));
    }
    out
}

/// «Таблицы данных», требование 24: имя таблицы не встречается в тексте кода игры, поиском по
/// границам слова — та же проверка, что у неиспользуемого свойства (`code_mentions_word`); таблицу,
/// на которую ссылается экран (`choose`, `needs_choice`, `{choice:…}`), читает движок, и она
/// используется без кода. В игре без `files.code` предупреждение выходит по остальным объявленным таблицам.
fn validate_unused_tables(
    tables: &[(String, String)],
    code: Option<&str>,
    used_by_screens: Option<&std::collections::HashMap<String, ChoiceTable>>,
    errors: &mut ErrorSink,
) {
    for (name, _path) in tables {
        if used_by_screens.is_some_and(|used| used.contains_key(name))
            || code.is_some_and(|c| code_mentions_word(c, name))
        {
            continue;
        }
        errors.push_warning(
            "game.json",
            &format!("files → tables → {name}"),
            format!(
                "таблица \"{name}\" объявлена, но не встречается в тексте кода; ожидалось, что объявленная таблица где-то используется"
            ),
        );
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
    properties: &PropertyTable,
    has_code: bool,
    declared_functions: Option<&[String]>,
    declared_tables: &[(String, String)],
    tables: &[(String, Json)],
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

    let mut initial_values: Vec<Vec<(String, PropertyId, InitialValue)>> = Vec::new();
    let mut choices = ChoiceChecker::new(declared_tables, tables, parsed);
    let mut runs = RunTable {
        calls: Vec::new(),
        has_code,
        declared: declared_functions,
    };
    let screens: Vec<Screen> = parsed
        .iter()
        .map(|screen| {
            let elements_path = join(&screen.path, "elements");
            let places = screen_hover_places(screen, &elements_path, file, errors);
            let elements = screen
                .elements
                .iter()
                .filter_map(|(i, data)| {
                    resolve_element(
                        data,
                        &join(&elements_path, &format!("[{i}]")),
                        file,
                        scene_file,
                        &screen.name,
                        fonts,
                        images,
                        properties,
                        &scene_names,
                        &name_to_id,
                        parsed,
                        &places,
                        &mut choices,
                        &mut initial_values,
                        &mut runs,
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
                        properties,
                        images,
                        &scene_names,
                        &mut choices,
                        &mut initial_values,
                        &mut runs,
                        format!("экран \"{}\", клавиша \"{code}\"", screen.name),
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
                cell_pixels: screen.cell_pixels,
                wind: screen.wind,
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
        let buttons = screen
            .elements
            .iter()
            .flat_map(Element::leaves)
            .filter_map(|element| match element {
                Element::Button { on_click, .. } => Some(on_click),
                _ => None,
            });
        for cmd in buttons.chain(screen.keys.values()) {
            match cmd {
                ButtonCommand::ShowScreen(target) | ButtonCommand::NewGame(target, _) => {
                    reachable.insert(*target);
                }
                ButtonCommand::Resume
                | ButtonCommand::Quit
                | ButtonCommand::ToggleSound
                | ButtonCommand::Exit
                | ButtonCommand::Run(_) => {}
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
        initial_values,
        choices: choices.into_tables(),
        // Filled in by `load_rest_with_tables` once this resolves — «Надписи и полоски в мире»
        // resolve separately (`resolve_world_elements`), since they need no scene/screen-name
        // cross-checks `resolve_screens` itself runs.
        world_elements: Vec::new(),
        runs: runs.calls,
        // Как `world_elements`: вид подсказок и масштаб интерфейса ставит `load_rest_with_tables`.
        tooltip: None,
        ui_reference: None,
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
/// «Текст»: байты файла шрифта `path` — страница читает файл один раз, под первым именем, что ведёт на
/// него, а годятся байты под любым из этих имён.
pub fn font_file_bytes<'a>(
    fonts: &[(String, String)],
    font_bytes: &'a [(String, Option<Vec<u8>>)],
    path: &str,
) -> Option<&'a Vec<u8>> {
    font_bytes
        .iter()
        .find(|(name, bytes)| bytes.is_some() && fonts.iter().any(|(n, p)| n == name && p == path))
        .and_then(|(_, bytes)| bytes.as_ref())
}

fn validate_font_files(
    fonts: &[(String, String)],
    font_bytes: &[(String, Option<Vec<u8>>)],
    errors: &mut ErrorSink,
) {
    for (name, path) in fonts {
        let bytes = font_file_bytes(fonts, font_bytes, path);
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

/// «Картинки», требование 23: resolves every declared image's `frame_by` name against
/// `properties` — must name a property of kind `number`. `game.json` alone (where `files.images`
/// lives) doesn't know property names, so this runs once `properties.json` has been parsed,
/// before anything downstream reads `ImageDecl::frame_by`.
fn resolve_image_frame_by(
    images: &mut [ImageDecl],
    properties: &PropertyTable,
    errors: &mut ErrorSink,
) {
    for decl in images {
        let Some(name) = decl.frame_by_name.take() else {
            continue;
        };
        let field_path = format!("files → images → {} → frame_by", decl.name);
        let Some(prop) = resolve_property(&name, properties, "game.json", &field_path, errors)
        else {
            continue;
        };
        if properties.kind(prop) != PropKind::Number {
            errors.push(
                "game.json",
                &field_path,
                format!(
                    "frame_by должно называть свойство вида number, у \"{name}\" вид {}",
                    properties.kind(prop).label()
                ),
            );
            continue;
        }
        decl.frame_by = Some(prop);
    }
}

/// «Картинки» → «Загрузка и проверка»: every declared image is read and checked whether or not
/// anything names it — same treatment `validate_font_files`/`validate_sound_files` give fonts and
/// sounds, unlike an unreferenced track (`validate_unreferenced_tracks`), which isn't even read.
fn validate_image_files(
    images: &[ImageDecl],
    image_data: &[(String, ImageVerdict)],
    looped: &[&str],
    errors: &mut ErrorSink,
) {
    for decl in images {
        let field_path = format!("files → images → {}", decl.name);
        let verdict = image_data
            .iter()
            .find(|(n, _)| n == &decl.name)
            .map(|(_, v)| v);
        if decl.video {
            let twin = looped.contains(&decl.name.as_str());
            validate_video_file(decl, verdict, twin, &field_path, errors);
            continue;
        }
        match verdict {
            Some(ImageVerdict::Video { .. }) => errors.push(
                "game.json",
                &field_path,
                format!(
                    "{} — страница ответила видео, а картинка объявлена PNG",
                    decl.path
                ),
            ),
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
                // «Картинки», требование 3: с `columns` — ширина делится на столбцы, высота на
                // число строк сетки; без него — ширина делится на `frames`, как раньше.
                match decl.columns {
                    Some(columns) => {
                        if width % columns != 0 {
                            errors.push(
                                "game.json",
                                &field_path,
                                format!(
                                    "{} — ширина {width} не делится на columns ({columns}) нацело, остаток {}",
                                    decl.path,
                                    width % columns
                                ),
                            );
                        }
                        let rows = decl.frames.div_ceil(columns);
                        if height % rows != 0 {
                            errors.push(
                                "game.json",
                                &field_path,
                                format!(
                                    "{} — высота {height} не делится на число строк сетки ({rows}) нацело, остаток {}",
                                    decl.path,
                                    height % rows
                                ),
                            );
                        }
                    }
                    None => {
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
                }
                validate_slice_fits(decl, *width, *height, &field_path, errors);
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

/// «Картинки» → «Загрузка и проверка»: поля `slice` слева и справа в сумме меньше ширины файла, сверху и
/// снизу — высоты: у краёв и середины остаются точки.
fn validate_slice_fits(
    decl: &ImageDecl,
    width: u32,
    height: u32,
    field_path: &str,
    errors: &mut ErrorSink,
) {
    let Some([left, top, right, bottom]) = decl.slice else {
        return;
    };
    let path = join(field_path, "slice");
    if u64::from(left) + u64::from(right) >= u64::from(width) {
        errors.push(
            "game.json",
            &path,
            format!(
                "{} — поля \"slice\" слева и справа ({left} + {right}) должны быть меньше ширины картинки {width}",
                decl.path
            ),
        );
    }
    if u64::from(top) + u64::from(bottom) >= u64::from(height) {
        errors.push(
            "game.json",
            &path,
            format!(
                "{} — поля \"slice\" сверху и снизу ({top} + {bottom}) должны быть меньше высоты картинки {height}",
                decl.path
            ),
        );
    }
}

/// Наибольшая ширина и высота кадра видео, как у картинки: лист атласа — 2048 × 2048 точек.
const MAX_VIDEO_FRAME_SIDE: u32 = 2048;

/// «Картинки» → «Загрузка и проверка»: файл видео — страница нашла его и браузер берётся его играть;
/// высота файла чётная (верхняя половина — цвет, нижняя — маска), кадр — ширина файла на половину
/// высоты — не шире и не выше листа атласа.
fn validate_video_file(
    decl: &ImageDecl,
    verdict: Option<&ImageVerdict>,
    looped: bool,
    field_path: &str,
    errors: &mut ErrorSink,
) {
    let message = match verdict {
        Some(ImageVerdict::Video {
            width,
            height,
            seconds,
            reverse,
        }) => video_size_problem(&decl.path, *width, *height).or_else(|| {
            if looped {
                return loop_twin_problem(
                    &decl.path,
                    [*width, *height],
                    *seconds,
                    reverse.as_ref(),
                );
            }
            let reverse_path = decl.reverse.as_deref()?;
            reverse_problem(reverse_path, [*width, *height], *seconds, reverse.as_ref())
        }),
        Some(ImageVerdict::Ok { .. }) => Some(format!(
            "{} — страница ответила картинкой, а объявлено видео",
            decl.path
        )),
        Some(ImageVerdict::Rejected) => Some(format!(
            "{} — исполнитель (браузер) не берётся играть это видео",
            decl.path
        )),
        Some(ImageVerdict::Missing) | None => Some(format!(
            "файл \"{}\" не найден; ожидалось видео MP4, названное в game.json → files → images",
            decl.path
        )),
    };
    if let Some(message) = message {
        errors.push("game.json", field_path, message);
    }
}

/// Насколько файл `reverse` может быть длиннее или короче ролика: один кадр при 24 кадрах в секунду, как у
/// роликов, которые режет `tools/art`.
const REVERSE_LENGTH_SLACK_SECONDS: f64 = 1.0 / 24.0 + 1e-6;

/// «Картинки» → «Видео на панели»: что не так с файлом `reverse` видео размера `size` и длины `seconds`:
/// его нет или браузер не играет его, он другого размера, длиннее или короче больше чем на кадр.
fn reverse_problem(
    path: &str,
    size: [u32; 2],
    seconds: f64,
    verdict: Option<&ReverseVerdict>,
) -> Option<String> {
    match verdict {
        Some(ReverseVerdict::Ok {
            width,
            height,
            seconds: reverse_seconds,
        }) => {
            if [*width, *height] != size {
                Some(format!(
                    "{path} — reverse размером {width}×{height}, а ролик {}×{}: обратный файл должен быть того же размера",
                    size[0], size[1]
                ))
            } else if (reverse_seconds - seconds).abs() > REVERSE_LENGTH_SLACK_SECONDS {
                Some(format!(
                    "{path} — reverse длиной {reverse_seconds:.3} с, а ролик {seconds:.3} с: обратный файл не должен отличаться по длине больше чем на кадр"
                ))
            } else {
                None
            }
        }
        Some(ReverseVerdict::Rejected) => Some(format!(
            "{path} — исполнитель (браузер) не берётся играть этот reverse"
        )),
        Some(ReverseVerdict::Missing) | None => Some(format!(
            "файл reverse \"{path}\" не найден; ожидалось видео MP4 — тот же ролик задом наперёд"
        )),
    }
}

/// «Картинки» → «Видео на панели»: что не так со вторым проигрывателем видео `path` по кругу: страница читает тот
/// же файл второй раз, и браузер должен взяться играть его так же, с тем же размером и длиной.
fn loop_twin_problem(
    path: &str,
    size: [u32; 2],
    seconds: f64,
    verdict: Option<&ReverseVerdict>,
) -> Option<String> {
    match verdict {
        Some(ReverseVerdict::Ok {
            width,
            height,
            seconds: twin_seconds,
        }) if [*width, *height] == size
            && (twin_seconds - seconds).abs() <= REVERSE_LENGTH_SLACK_SECONDS =>
        {
            None
        }
        _ => Some(format!(
            "{path} — видео по кругу играют два проигрывателя по очереди, а браузер не взялся играть этот файл вторым проигрывателем"
        )),
    }
}

/// Что не так с размером файла видео `width` × `height`.
fn video_size_problem(path: &str, width: u32, height: u32) -> Option<String> {
    if width == 0 || height == 0 {
        return Some(format!(
            "{path} — ширина и высота видео должны быть больше нуля, получено {width}×{height}"
        ));
    }
    if !height.is_multiple_of(2) {
        return Some(format!(
            "{path} — высота видео {height} нечётная: цвет и маска занимают по половине высоты"
        ));
    }
    let frame_height = height / 2;
    (width > MAX_VIDEO_FRAME_SIDE || frame_height > MAX_VIDEO_FRAME_SIDE).then(|| {
        format!(
            "{path} — кадр видео {width}×{frame_height} шире или выше {MAX_VIDEO_FRAME_SIDE}×{MAX_VIDEO_FRAME_SIDE}"
        )
    })
}

/// «Картинки» → «Видео»: видео рисует только плоская сцена — в трёхмерной любое объявленное видео
/// ошибка.
fn validate_videos_in_flat_scene(images: &[ImageDecl], errors: &mut ErrorSink) {
    for decl in images.iter().filter(|decl| decl.video) {
        errors.push(
            "game.json",
            &format!("files → images → {}", decl.name),
            format!(
                "{} — видео в трёхмерной сцене не рисуется: видео берёт только плоская сцена",
                decl.path
            ),
        );
    }
}

fn mark_fill_used(used: &mut std::collections::HashSet<ImageId>, fill: &Fill) {
    if let Fill::Image { image, .. } = fill {
        used.insert(*image);
    }
}

/// Картинки, которые называет значение: `image` одну, `cloud_images` — все из списка; строка — ту
/// из `text_names`, чьё имя она несёт (имя для `{свойство}` у картинки над объектом в мире).
fn mark_value_used(
    used: &mut std::collections::HashSet<ImageId>,
    value: &Value,
    text_names: &[ImageDecl],
) {
    match value {
        Value::Image(id) => {
            used.insert(*id);
        }
        Value::ImageList(ids) => used.extend(ids),
        Value::Text(name) => used.extend(text_names.iter().position(|decl| decl.name == *name)),
        _ => {}
    }
}

fn mark_key_table_used(
    used: &mut std::collections::HashSet<ImageId>,
    table: &KeyTable,
    text_names: &[ImageDecl],
) {
    for binding in table.values() {
        for edit in binding.press.iter().chain(&binding.release) {
            if let EditValue::Const(value) = &edit.value {
                mark_value_used(used, value, text_names);
            }
        }
    }
}

/// «Картинки»: an image the scene/rules/screens never name — collected across an object's own
/// `image` property, its key bindings' `press`/`release` edits, a spawn template's `image` field
/// (a collide effect's `set` counts too, the only other place a `Value::Image` can come from),
/// every panel/button fill, and (требование 22) every `ground` layer's own tile set.
fn collect_used_images(
    scene: &[ParsedObject],
    rules: &RuleSet,
    screens: &[Screen],
    world_elements: &[WorldElement],
    ground: &[GroundLayer],
    images: &[ImageDecl],
) -> std::collections::HashSet<ImageId> {
    let mut used = std::collections::HashSet::new();
    // Картинку над объектом в мире по `{свойство}` называет значение свойства, а не поле: все строки
    // игры, совпавшие с именем картинки, считаются её названием.
    let named_by_property = world_elements.iter().any(|element| {
        matches!(
            &element.kind,
            WorldElementKind::Image {
                image: ImageRef::Own { .. }
            }
        )
    }) || screens.iter().any(|screen| {
        screen
            .elements
            .iter()
            .flat_map(Element::leaves)
            .any(|element| {
                matches!(
                    element,
                    Element::Panel {
                        fill: PanelFill::Own { .. },
                        ..
                    }
                )
            })
    });
    let text_names = if named_by_property { images } else { &[] };
    for element in world_elements {
        if let WorldElementKind::Image {
            image: ImageRef::Fixed(image),
        } = &element.kind
        {
            used.insert(*image);
        }
    }
    for layer in ground {
        used.insert(layer.image);
    }
    for obj in scene {
        for (_, v) in &obj.values {
            mark_value_used(&mut used, v, text_names);
        }
        if let Some(table) = &obj.keys {
            mark_key_table_used(&mut used, table, text_names);
        }
        if let Some(edits) = &obj.on_click {
            for edit in edits {
                if let EditValue::Const(value) = &edit.value {
                    mark_value_used(&mut used, value, text_names);
                }
            }
        }
    }
    let mark_template = |used: &mut std::collections::HashSet<ImageId>,
                         template: &[(PropertyId, TemplateValue)]| {
        for (_, tv) in template {
            if let TemplateValue::Const(value) = tv {
                mark_value_used(used, value, text_names);
            }
        }
    };
    let mark_do = |used: &mut std::collections::HashSet<ImageId>, do_: &[CommonAction]| {
        walk_common_actions(do_, &mut |action| {
            if let CommonAction::SetAll {
                value: SetValue::Const(value),
                ..
            } = action
            {
                mark_value_used(used, value, text_names);
            }
        });
    };
    for rule in &rules.rules {
        match rule {
            Rule::Spawn {
                template,
                pick_one,
                do_,
                ..
            } => {
                mark_template(&mut used, template);
                if let Some(variants) = pick_one {
                    for variant in variants {
                        mark_template(&mut used, &variant.fields);
                        for cell in &variant.cells {
                            mark_template(&mut used, &cell.fields);
                        }
                    }
                }
                mark_do(&mut used, do_);
            }
            Rule::Collide {
                effects_a,
                effects_b,
                do_,
                ..
            } => {
                for effect in effects_a.iter().chain(effects_b) {
                    if let CollideEffect::Set {
                        value: SetValue::Const(value),
                        ..
                    } = effect
                    {
                        mark_value_used(&mut used, value, text_names);
                    }
                }
                mark_do(&mut used, do_);
            }
            Rule::Check { do_, .. } | Rule::Delete { do_, .. } => mark_do(&mut used, do_),
            Rule::Move { .. }
            | Rule::Walk { .. }
            | Rule::RunJump { .. }
            | Rule::Climb { .. }
            | Rule::Drive { .. } => {}
        }
    }
    for screen in screens {
        for element in screen.elements.iter().flat_map(Element::leaves) {
            match element {
                Element::Panel {
                    fill: PanelFill::Fixed(fill),
                    idle,
                    ..
                } => {
                    mark_fill_used(&mut used, fill);
                    used.extend(*idle);
                }
                Element::Panel {
                    fill: PanelFill::Own { .. } | PanelFill::Choice { .. },
                    ..
                } => {}
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
                Element::Bar { fill, back, .. } => {
                    mark_fill_used(&mut used, fill);
                    if let Some(back) = back {
                        mark_fill_used(&mut used, back);
                    }
                }
                Element::Label { .. } | Element::List { .. } => {}
            }
        }
    }
    used
}

/// «Картинки» → «Видео на панели»: одно видео — на одной панели. Видео панели — по наведению или по
/// кругу, — которое называет ещё и другая панель, объект мира, картинка над объектом или земля, — ошибка
/// использования. Видео из поля таблицы выбора, которое читает панель с `image: "{choice:…}"`, — такое же.
#[allow(clippy::too_many_arguments)]
fn validate_panel_videos(
    images: &[ImageDecl],
    scene: &[ParsedObject],
    rules: &RuleSet,
    config: &ScreensConfig,
    world_elements: &[WorldElement],
    ground: &[GroundLayer],
    screens_file: &str,
    errors: &mut ErrorSink,
) {
    let outside_screens = collect_used_images(scene, rules, &[], world_elements, ground, images);
    let mut played = std::collections::HashSet::new();
    for (screen_index, screen) in config.screens.iter().enumerate() {
        for (element_index, element) in screen.elements.iter().enumerate() {
            let Element::Panel { fill, idle, .. } = element else {
                continue;
            };
            let field =
                |key: &str| format!("screens[{screen_index}].elements[{element_index}].{key}");
            let (mut videos, group): (Vec<(ImageId, String)>, Option<&str>) = match fill {
                PanelFill::Fixed(Fill::Image { image, .. }) => {
                    (vec![(*image, field("image"))], None)
                }
                PanelFill::Choice { choice, names, .. } => (
                    config
                        .choice_images(choice, names)
                        .into_iter()
                        .map(|image| (image, field("image")))
                        .collect(),
                    Some(choice.group.as_str()),
                ),
                _ => continue,
            };
            videos.extend(idle.map(|idle| (idle, field("idle"))));
            let mut seen = Vec::new();
            for (image, path) in videos {
                if !images[image].video || seen.contains(&image) {
                    continue;
                }
                seen.push(image);
                let name = &images[image].name;
                let source = match group {
                    Some(group) => format!("берётся из таблицы выбора \"{group}\""),
                    None => "играет панель".to_string(),
                };
                if outside_screens.contains(&image) {
                    errors.push(
                        screens_file,
                        &path,
                        format!(
                            "видео \"{name}\" {source}, а его называет ещё и объект мира, картинка над объектом или земля: одно видео — на одной панели"
                        ),
                    );
                } else if !played.insert(image) {
                    let message = match group {
                        Some(_) => format!(
                            "видео \"{name}\" {source}, а его уже играет другая панель: одно видео — на одной панели"
                        ),
                        None => format!(
                            "видео \"{name}\" уже играет другая панель: одно видео — на одной панели"
                        ),
                    };
                    errors.push(screens_file, &path, message);
                }
            }
        }
    }
}

/// «Картинки» → «Видео на панели»: ролик покоя `idle` того же размера, что видео панели; размеры называют
/// ответы страницы. Видео без ответа уже названо ошибкой файла.
fn validate_idle_size(
    screens: &[Screen],
    images: &[ImageDecl],
    image_data: &[(String, ImageVerdict)],
    screens_file: &str,
    errors: &mut ErrorSink,
) {
    let size_of = |image: ImageId| match image_data
        .iter()
        .find(|(name, _)| *name == images[image].name)
    {
        Some((_, ImageVerdict::Video { width, height, .. })) => Some([*width, *height]),
        _ => None,
    };
    for (screen_index, screen) in screens.iter().enumerate() {
        for (element_index, element) in screen.elements.iter().enumerate() {
            let Element::Panel {
                fill: PanelFill::Fixed(Fill::Image { image, .. }),
                idle: Some(idle),
                ..
            } = element
            else {
                continue;
            };
            let (Some(size), Some(idle_size)) = (size_of(*image), size_of(*idle)) else {
                continue;
            };
            if size != idle_size {
                errors.push(
                    screens_file,
                    &format!("screens[{screen_index}].elements[{element_index}].idle"),
                    format!(
                        "ролик покоя \"{}\" размером {}×{}, а видео панели \"{}\" {}×{}: ролик покоя делается того же размера, от первого кадра видео панели",
                        images[*idle].name,
                        idle_size[0],
                        idle_size[1],
                        images[*image].name,
                        size[0],
                        size[1]
                    ),
                );
            }
        }
    }
}

/// «Картинки» → «Видео на панели»: `rest_at` панели не длиннее её ролика; длину называет ответ страницы по
/// файлу. Ролик без ответа уже назван ошибкой файла.
fn validate_rest_at(
    screens: &[Screen],
    images: &[ImageDecl],
    image_data: &[(String, ImageVerdict)],
    screens_file: &str,
    errors: &mut ErrorSink,
) {
    for (screen_index, screen) in screens.iter().enumerate() {
        for (element_index, element) in screen.elements.iter().enumerate() {
            let Element::Panel {
                fill: PanelFill::Fixed(Fill::Image { image, .. }),
                rest_at: Some(rest_at),
                ..
            } = element
            else {
                continue;
            };
            let name = &images[*image].name;
            let Some((_, ImageVerdict::Video { seconds, .. })) =
                image_data.iter().find(|(n, _)| n == name)
            else {
                continue;
            };
            if *rest_at > *seconds as f32 {
                errors.push(
                    screens_file,
                    &format!("screens[{screen_index}].elements[{element_index}].rest_at"),
                    format!(
                        "rest_at {rest_at} с длиннее ролика \"{name}\" в {seconds:.3} с: поза покоя лежит внутри ролика"
                    ),
                );
            }
        }
    }
}

/// «Наведение»: предупреждение (игра идёт) о месте наведения, на которое не откликается ни один элемент
/// того же экрана — ни `show_on_hover`, ни `play_on_hover` с его именем.
fn warn_unanswered_hover_places(screens: &[Screen], screens_file: &str, errors: &mut ErrorSink) {
    for (screen_index, screen) in screens.iter().enumerate() {
        let answered = answered_places(screen);
        for (element_index, element) in screen.elements.iter().enumerate() {
            let Some(name) = element.place_name() else {
                continue;
            };
            if element.choose().is_some() {
                continue;
            }
            if !answered.contains(&element_index) {
                errors.push_warning(
                    screens_file,
                    &format!("screens[{screen_index}].elements[{element_index}].name"),
                    format!(
                        "место наведения \"{name}\" на экране \"{}\": на него не откликается ни один элемент; ожидался show_on_hover или play_on_hover с этим именем",
                        screen.name
                    ),
                );
            }
        }
    }
}

/// «Наведение»: номера мест экрана, на которые откликается хоть один элемент — `show_on_hover` или
/// `play_on_hover` с их именем.
fn answered_places(screen: &Screen) -> std::collections::HashSet<usize> {
    screen
        .elements
        .iter()
        .flat_map(|element| [element.show_on_hover(), element.play_on_hover()])
        .flatten()
        .collect()
}

/// «Выбор»: группы, которые называют подстановки `{choice:…}` текста.
fn text_choice_groups(text: &[TextPart]) -> impl Iterator<Item = &str> {
    text.iter().filter_map(|part| match part {
        TextPart::Choice(choice) => Some(choice.group.as_str()),
        _ => None,
    })
}

/// «Выбор»: группы, которые что-то читает: подпись кнопки или надпись, картинка панели, `needs_choice` или
/// начальное значение `new_game`.
fn read_choice_groups(config: &ScreensConfig) -> std::collections::HashSet<&str> {
    let mut read = std::collections::HashSet::new();
    for element in config
        .screens
        .iter()
        .flat_map(|screen| &screen.elements)
        .flat_map(Element::leaves)
    {
        match element {
            Element::Label { text, .. } => read.extend(text_choice_groups(text)),
            Element::Button {
                text, needs_choice, ..
            } => {
                read.extend(text_choice_groups(text));
                read.extend(needs_choice.as_deref());
            }
            Element::Panel {
                fill: PanelFill::Choice { choice, .. },
                ..
            } => {
                read.insert(choice.group.as_str());
            }
            _ => {}
        }
    }
    for (_, _, value) in config.initial_values.iter().flatten() {
        if let InitialValue::Choice(choice) = value {
            read.insert(choice.group.as_str());
        }
    }
    read
}

/// «Выбор»: предупреждение (игра идёт) о месте с `choose`, на которое не откликается ни один элемент его
/// экрана и группу которого никто не читает.
fn warn_unread_choose_places(config: &ScreensConfig, screens_file: &str, errors: &mut ErrorSink) {
    let read = read_choice_groups(config);
    for (screen_index, screen) in config.screens.iter().enumerate() {
        let answered = answered_places(screen);
        for (element_index, element) in screen.elements.iter().enumerate() {
            let (Some(group), Some(name)) = (element.choose(), element.place_name()) else {
                continue;
            };
            if answered.contains(&element_index) || read.contains(group) {
                continue;
            }
            errors.push_warning(
                screens_file,
                &format!("screens[{screen_index}].elements[{element_index}].choose"),
                format!(
                    "место выбора \"{name}\" группы \"{group}\" на экране \"{}\": на него не откликается ни один элемент, и группу не читает ни надпись, ни картинка панели, ни needs_choice, ни new_game; ожидался show_on_hover, play_on_hover или подстановка {{choice:{group}}}",
                    screen.name
                ),
            );
        }
    }
}

/// «Картинки»: предупреждение (игра идёт), если объявленная картинка не встречается ни на одном
/// объекте сцены, ни в одном шаблоне создания, ни на одном элементе экрана.
#[allow(clippy::too_many_arguments)]
fn validate_unused_images(
    images: &[ImageDecl],
    scene: &[ParsedObject],
    rules: &RuleSet,
    screens: &[Screen],
    tooltip: Option<&TooltipStyle>,
    world_elements: &[WorldElement],
    ground: &[GroundLayer],
    choice_images: &[ImageId],
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let mut used = collect_used_images(scene, rules, screens, world_elements, ground, images);
    used.extend(choice_images);
    if let Some(tooltip) = tooltip {
        mark_fill_used(&mut used, &tooltip.fill);
    }
    for (i, decl) in images.iter().enumerate() {
        if !used.contains(&i) && !code.is_some_and(|c| code_mentions_word(c, &decl.name)) {
            errors.push_warning(
                "game.json",
                &format!("files → images → {}", decl.name),
                format!(
                    "картинка \"{}\" объявлена, но её не называет ни один объект, ни один шаблон создания, ни один элемент экрана, ни код и его таблицы",
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
        walk_common_actions(common_actions(rule), &mut |action| {
            if let CommonAction::PlaySound(id) = action {
                used.insert(*id);
            }
        });
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
        let button_bound = screen
            .elements
            .iter()
            .flat_map(Element::leaves)
            .any(|element| {
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
        let mut found = false;
        walk_common_actions(common_actions(rule), &mut |action| {
            found |= matches!(action, CommonAction::PlaySound(_));
        });
        found
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
    skip_media_validation: bool,
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_tables(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        &[],
    )
}

/// Same as `load_rest`, additionally accepting `files.tables`' texts, by name, the same shape
/// `read_texts` gives the other second-round files — «Таблицы данных», требование 25. The wasm
/// layer's real `load()` calls this one; `load_rest` stays the plain five-file entry point every
/// existing test already uses, unaffected by a game with no tables at all.
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_tables(
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
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_terrain(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        table_texts,
        None,
    )
}

/// Same as `load_rest_with_tables`, additionally accepting the text of `files.terrain` —
/// «Рельеф», требование 47: файл высот читается тем же заходом, что таблицы данных. `None` — файла
/// нет на месте; без `files.terrain` в `game.json` текст не нужен.
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_terrain(
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
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
    terrain_text: Option<&str>,
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_materials(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        table_texts,
        terrain_text,
        &[],
        &[],
    )
}

/// Same as `load_rest_with_terrain`, additionally accepting what the page answered for each map of
/// each material and for each cover mask — «Свет и материалы»: `material_data` and `mask_data` are
/// keyed by the file's path, like `image_data` is by the image's name, numbered by
/// `read_texts`'s `materials` and `masks`.
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_materials(
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
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
    terrain_text: Option<&str>,
    material_data: &[(String, ImageVerdict)],
    mask_data: &[(String, ImageVerdict)],
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_stamps(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        table_texts,
        terrain_text,
        material_data,
        mask_data,
        &[],
    )
}

/// Same as `load_rest_with_materials`, additionally accepting the texts of `files.stamps`, by name,
/// the same shape as `table_texts` — «Лепка рельефа»: `None` — файла штампа нет на месте.
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_stamps(
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
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
    terrain_text: Option<&str>,
    material_data: &[(String, ImageVerdict)],
    mask_data: &[(String, ImageVerdict)],
    stamp_texts: &[(String, Option<String>)],
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_models(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        table_texts,
        terrain_text,
        material_data,
        mask_data,
        stamp_texts,
        &[],
    )
}

/// Same as `load_rest_with_stamps`, additionally accepting what the page answered for each model of
/// `files.models`, by name — «Трёхмерная сцена» → «Модели и движения»: байты файла `.glb` и разжатые картинки
/// из него (`ModelFile`).
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_models(
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
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
    terrain_text: Option<&str>,
    material_data: &[(String, ImageVerdict)],
    mask_data: &[(String, ImageVerdict)],
    stamp_texts: &[(String, Option<String>)],
    model_files: &[(String, ModelFile)],
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_imports(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        table_texts,
        terrain_text,
        material_data,
        mask_data,
        stamp_texts,
        model_files,
        None,
        &[],
    )
}

/// Same as `load_rest_with_models`, additionally accepting the text of `imported/index.json` and the bytes of the
/// compressed files `read_texts_with_imported` named, by path — «Трёхмерная сцена» → «Загрузка и проверка»: a model
/// the index lists is read from its light file, `None` bytes or none at all is a file the page did not find.
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_imports(
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
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
    terrain_text: Option<&str>,
    material_data: &[(String, ImageVerdict)],
    mask_data: &[(String, ImageVerdict)],
    stamp_texts: &[(String, Option<String>)],
    model_files: &[(String, ModelFile)],
    imported_index: Option<&str>,
    imported_textures: &[(String, Option<Vec<u8>>)],
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    load_rest_with_motions(
        game_json,
        config,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        font_bytes,
        sound_bytes,
        music_verdicts,
        image_data,
        code_json,
        skip_media_validation,
        table_texts,
        terrain_text,
        material_data,
        mask_data,
        stamp_texts,
        model_files,
        imported_index,
        imported_textures,
        None,
    )
}

/// Same as `load_rest_with_imports`, additionally accepting the text of `files.motions` — «Движения»: `None` — файла
/// нет на месте; без `files.motions` в `game.json` текст не нужен.
#[allow(clippy::too_many_arguments)]
pub fn load_rest_with_motions(
    game_json: &str,
    mut config: GameConfig,
    properties_json: Option<&str>,
    scene_json: Option<&str>,
    rules_json: Option<&str>,
    screens_json: Option<&str>,
    font_bytes: &[(String, Option<Vec<u8>>)],
    sound_bytes: &[(String, Option<Vec<u8>>)],
    music_verdicts: &[(String, MusicVerdict)],
    image_data: &[(String, ImageVerdict)],
    code_json: Option<&str>,
    skip_media_validation: bool,
    table_texts: &[(String, Option<String>)],
    terrain_text: Option<&str>,
    material_data: &[(String, ImageVerdict)],
    mask_data: &[(String, ImageVerdict)],
    stamp_texts: &[(String, Option<String>)],
    model_files: &[(String, ModelFile)],
    imported_index: Option<&str>,
    imported_textures: &[(String, Option<Vec<u8>>)],
    motions_text: Option<&str>,
) -> Result<(Game, ScreensConfig, Vec<GameError>, Vec<ImageDecl>), LoadFailure> {
    let mut errors = ErrorSink::new();

    let mut properties = match properties_json {
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
    properties.set_three_d(config.scene.is_3d());
    resolve_image_frame_by(&mut config.files.images, &properties, &mut errors);
    properties.set_cloud_misfits(
        config
            .files
            .images
            .iter()
            .map(|decl| decl.cloud_misfit(config.scene.cell_pixels))
            .collect(),
    );
    if config.scene.is_3d() {
        validate_videos_in_flat_scene(&config.files.images, &mut errors);
    }
    let stamp_table = parse_stamp_table(&config.files.stamps, stamp_texts, &mut errors);
    match (&config.files.terrain, terrain_text) {
        (Some(path), Some(text)) if config.scene.is_3d() => {
            let masks = (!skip_media_validation).then_some(mask_data);
            let materials = &config.files.materials;
            if let Some(terrain) = parse_terrain(
                text,
                path,
                &config.scene,
                materials,
                masks,
                &stamp_table,
                &mut errors,
            ) {
                properties.set_terrain(terrain);
            }
        }
        (Some(path), None) => errors.push(
            path,
            "",
            "файл не найден; ожидался JSON-файл, названный в game.json → files → terrain",
        ),
        _ => {}
    }
    properties.set_stamps(stamp_table);
    let imported = ImportedPlan::new(&config.files.models, imported_index);
    let motions = match (&config.files.motions, motions_text) {
        (Some(path), Some(text)) => parse_motions(text, path, &config.files.models, &mut errors),
        (Some(path), None) => {
            errors.push(
                path,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → motions",
            );
            MotionSet::default()
        }
        (None, _) => MotionSet::default(),
    };
    let models = load_models(
        &config.files.models,
        model_files,
        &imported,
        imported_textures,
        skip_media_validation,
        &motions,
        &mut errors,
    );
    properties.set_road_kinds(build_road_kinds(
        &config.road_kinds,
        &config.road_junctions,
        &models,
        &mut errors,
    ));
    properties.set_models(models);

    let ParsedSceneFile {
        objects: scene_objects,
        ground,
        wind,
    } = match scene_json {
        Some(text) => parse_scene_json(
            text,
            &config.files.scene,
            &properties,
            &config.files.images,
            &config.scene,
            &mut errors,
        ),
        None => {
            errors.push(
                &config.files.scene,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → scene",
            );
            ParsedSceneFile::default()
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

    // «Таблицы данных», требования 22–24: разобраны и провалидированы раньше прогона кода —
    // пункт 27 требует `tables` уже при первом выполнении файла, включая этот одноразовый прогон
    // проверки.
    let tables_json = parse_and_validate_tables(&config.files.tables, table_texts, &mut errors);
    // A table with its own error drops out of `tables_json`; running the code check without it
    // would add a second, misleading error («attempt to index a nil value») for the same fault.
    let tables_broken = tables_json.len() != config.files.tables.len();

    // «Код игры» → «Проверка перед запуском»: файл кода выполняется один раз здесь, в свежем,
    // одноразовом исполнителе — мира ещё нет, а `rng`/`messages` одноразовые: `print` и
    // `math.random` этого прогона в игру не идут (пункт 3). Успешный прогон отдаёт только имена
    // объявленных функций — сам исполнитель отбрасывается; настоящий, на партию, строит `Game::
    // new`/`new_game` заново («каждая партия создаёт свежий исполнитель»).
    let image_names: Vec<String> = config.files.images.iter().map(|d| d.name.clone()).collect();
    let parser = object_parser(config.files.images.clone());
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
        (Some(_), Some(_)) if tables_broken => None,
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
                &tables_json,
                Rc::default(),
                parser.clone(),
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

    let ParsedScreensFile {
        screens: parsed_screens,
        world_elements: parsed_world_elements,
        tooltip: parsed_tooltip,
        ui_reference,
    } = match screens_json {
        Some(text) => parse_screens_json(text, &config.files.screens, &properties, &mut errors),
        None => {
            errors.push(
                &config.files.screens,
                "",
                "файл не найден; ожидался JSON-файл, названный в game.json → files → screens",
            );
            ParsedScreensFile::default()
        }
    };
    let world_elements = resolve_world_elements(
        &parsed_world_elements,
        &config.files.fonts,
        &config.files.images,
        &config.files.screens,
        &mut errors,
    );
    let tooltip = parsed_tooltip.as_ref().and_then(|parsed| {
        resolve_tooltip_style(
            parsed,
            &config.files.fonts,
            &config.files.images,
            &config.files.screens,
            &mut errors,
        )
    });
    // «Тесты по записанному вводу», требование 33: файлы картинок, шрифтов и звуков не читаются —
    // проверки, которым нужен сам файл, пропускаются; JSON и код игры проверяются как обычно.
    if !skip_media_validation {
        validate_font_files(&config.files.fonts, font_bytes, &mut errors);
        validate_sound_files(&config.files.sounds, sound_bytes, &mut errors);
        validate_image_files(
            &config.files.images,
            image_data,
            &looped_video_names(
                &config.files.images,
                &parsed_screens,
                &scene_image_ids(&scene_objects, &ground),
            ),
            &mut errors,
        );
        validate_material_files(&config.files.materials, material_data, &mut errors);
    }
    // «Звук» → «Загрузка и проверка»: captured from the raw parse,
    // before `resolve_screens` resolves each screen's `music` name, so an unknown or malformed
    // name still counts as "referenced" here — its own error comes from `resolve_music` regardless.
    let referenced_music: std::collections::HashSet<String> = parsed_screens
        .iter()
        .filter_map(|s| s.music.clone())
        .collect();
    if !skip_media_validation {
        validate_music_files(
            &config.files.music,
            &referenced_music,
            music_verdicts,
            &mut errors,
        );
    }
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
        &properties,
        config.files.code.is_some(),
        declared_functions.as_deref(),
        &config.files.tables,
        &tables_json,
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
    validate_move_apart_from_run_jump(
        &scene_objects,
        &rules,
        &rule_meta,
        &config.files.scene,
        &mut errors,
    );
    validate_drive_apart_from_other_movers(
        &scene_objects,
        &rules,
        &rule_meta,
        &config.files.scene,
        &mut errors,
    );
    validate_climbers_have_limbs(
        &scene_objects,
        &rules,
        &rule_meta,
        &config.files.models,
        &config.files.scene,
        &mut errors,
    );
    validate_reachable_image(&shapes, code_json, &mut errors);
    validate_camera_follows_needs_view_height(&shapes, config.scene.view_height, &mut errors);
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
        let screens_for_usage: &[Screen] = screens_config
            .as_ref()
            .map(|sc| sc.screens.as_slice())
            .unwrap_or(&[]);
        validate_unused_properties(
            &properties,
            &config.files.properties,
            &scene_objects,
            &rules,
            screens_for_usage,
            &world_elements,
            code_json,
            &mut errors,
        );
        if config.scene.player_camera.is_some() {
            warn_taken_camera_keys(
                &scene_objects,
                &config.files.scene,
                screens_for_usage,
                &config.files.screens,
                &mut errors,
            );
        }
        warn_road_without_roads(
            &collect_used_properties(&scene_objects, &rules, screens_for_usage, &world_elements),
            &shapes,
            code_json,
            &config.files.rules,
            &mut errors,
        );
        validate_objects_within_scene(
            &scene_objects,
            &config.files.scene,
            &config.scene,
            &mut errors,
        );
        warn_clouds_without_images(&scene_objects, &config.files.scene, &mut errors);
        validate_selectors_not_empty(
            &shapes,
            &rules,
            &rule_meta,
            &config.files.rules,
            &properties,
            code_json,
            &mut errors,
        );
        validate_world_element_selectors_not_empty(
            &world_elements,
            &shapes,
            &config.files.screens,
            &properties,
            code_json,
            &mut errors,
        );
        validate_screen_selectors_not_empty(
            screens_for_usage,
            &shapes,
            &config.files.screens,
            &properties,
            code_json,
            &mut errors,
        );
        warn_row_elements_outside_row(screens_for_usage, &config.files.screens, &mut errors);
        warn_unanswered_hover_places(screens_for_usage, &config.files.screens, &mut errors);
        validate_unused_sounds(&config.files.sounds, &rules, code_json, &mut errors);
        validate_unreferenced_tracks(&config.files.music, &referenced_music, &mut errors);
        validate_unused_tables(
            &config.files.tables,
            code_json,
            screens_config.as_ref().map(|sc| &sc.choices),
            &mut errors,
        );
        let covers = properties
            .terrain()
            .map_or(&[][..], |terrain| terrain.covers());
        warn_unused_materials(&config.files.materials, covers, &mut errors);
        let imprints = properties
            .terrain()
            .map_or(&[][..], |terrain| terrain.imprints());
        warn_unused_stamps(&config.files.stamps, imprints, &mut errors);
        let mut named_models = named_in_scene(&scene_objects, property::MODEL);
        named_models.extend(
            config
                .road_kinds
                .iter()
                .flat_map(|kind| kind.pieces.iter().cloned()),
        );
        named_models.extend(
            config
                .road_junctions
                .iter()
                .map(|junction| junction.name.clone()),
        );
        named_models.extend(
            config
                .files
                .models
                .iter()
                .filter_map(|decl| decl.cutaway.clone()),
        );
        // «Таблицы данных»: таблицы читает код игры, и имя модели, картинки или вида дороги в таблице —
        // такое же упоминание, как в тексте кода. Без кода таблицы читать некому.
        let code_and_tables = code_json.map(|code| {
            let mut text = code.to_string();
            for table in table_texts.iter().filter_map(|(_, table)| table.as_deref()) {
                text.push('\n');
                text.push_str(table);
            }
            text
        });
        warn_unused_models(
            &config.files.models,
            &named_models,
            rules_json,
            code_and_tables.as_deref(),
            &mut errors,
        );
        let named_motions = named_in_scene(&scene_objects, property::ANIMATION);
        warn_unused_motions(
            &motions,
            &config.files.models,
            &named_motions,
            rules_json,
            code_and_tables.as_deref(),
            &mut errors,
        );
        warn_unused_road_kinds(
            &config.road_kinds,
            &named_in_scene(&scene_objects, property::ROAD_KIND),
            rules_json,
            code_and_tables.as_deref(),
            &mut errors,
        );
        // `screens_config` is `Some` whenever no error has been pushed — `resolve_screens` only
        // returns `None` by failing to resolve `start_screen`, which always pushes one.
        if let Some(sc) = &screens_config {
            validate_toggle_sound_presence(&rules, &sc.screens, &config.files.screens, &mut errors);
            warn_unread_choose_places(sc, &config.files.screens, &mut errors);
            validate_panel_videos(
                &config.files.images,
                &scene_objects,
                &rules,
                sc,
                &world_elements,
                &ground,
                &config.files.screens,
                &mut errors,
            );
            if !skip_media_validation {
                validate_rest_at(
                    &sc.screens,
                    &config.files.images,
                    image_data,
                    &config.files.screens,
                    &mut errors,
                );
                validate_idle_size(
                    &sc.screens,
                    &config.files.images,
                    image_data,
                    &config.files.screens,
                    &mut errors,
                );
            }
            validate_unused_images(
                &config.files.images,
                &scene_objects,
                &rules,
                &sc.screens,
                tooltip.as_ref(),
                &world_elements,
                &ground,
                &sc.choice_panel_images(),
                code_and_tables.as_deref(),
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
    if let (Some(path), Some(text)) = (&config.files.terrain, terrain_text) {
        errors.fill_locations(path, text);
    }
    if let (Some(path), Some(text)) = (&config.files.motions, motions_text) {
        errors.fill_locations(path, text);
    }

    let (errs, warnings) = errors.into_parts();
    if !errs.is_empty() {
        return Err(LoadFailure {
            errors: errs,
            warnings,
        });
    }
    let mut screens_config = screens_config.expect("no errors means screens.json resolved");
    screens_config.world_elements = world_elements;
    screens_config.tooltip = tooltip;
    screens_config.ui_reference = ui_reference;

    let scene_specs: Vec<ObjectSpec> = scene_objects
        .iter()
        .map(|parsed| ObjectSpec {
            values: parsed.values.clone(),
            grid: parsed.grid,
            keys: parsed.keys.clone(),
            on_click: parsed.on_click.clone(),
        })
        .collect();

    // «Экраны и состояние»: миру родиться только если у стартового экрана поднят world_runs —
    // на экране меню его просто нет, пока игрок не нажмёт «Играть».
    let start_is_live = screens_config.screens[screens_config.start_screen].world_runs;
    let world = if start_is_live {
        game::world_from_scene(&properties, &scene_specs)
    } else {
        World::new(&properties)
    };

    let sound_count = config.files.sounds.len();
    let game = Game::new(
        properties,
        world,
        rules,
        config.scene,
        config.max_objects,
        ground,
        wind,
        config.random_seed,
        scene_specs,
        sound_count,
        code_json.map(str::to_string),
        config.files.code.clone().unwrap_or_default(),
        image_names,
        sound_names,
        start_is_live,
        tables_json,
        parser,
    );
    // «Картинки», требование 23: already resolved at the top of this function (`resolve_image_
    // frame_by`, against `config.files.images` in place) — handed back rather than making a
    // caller (`wasm::Engine::load`) clone `config.files.images` before this call and resolve that
    // separate copy a second time once this one returns.
    Ok((game, screens_config, warnings, config.files.images))
}

/// «Тесты по записанному вводу», требования 30–31: parses a replay file's `values` (and a
/// `new_game`'s own third element, same form) — `{"<объект>.<свойство>": <значение>, ...}`, the
/// value written exactly as `scene.json` would — into `(имя объекта, свойство, значение)`
/// triples, ready for `Game::new_game_with_values`. Public only for `tests/replays.rs`, the one
/// caller outside this module with no other way to reach `parse_scalar_value`.
pub fn parse_initial_values(
    values_json: &Json,
    properties: &PropertyTable,
) -> Result<Vec<(String, PropertyId, Value)>, String> {
    let obj = values_json
        .as_object()
        .ok_or_else(|| "values должен быть объектом".to_string())?;
    let mut out = Vec::with_capacity(obj.len());
    for (key, value_json) in obj {
        let dot = key
            .find('.')
            .ok_or_else(|| format!("ключ \"{key}\" должен быть вида \"объект.свойство\""))?;
        let (object_name, prop_name) = (&key[..dot], &key[dot + 1..]);
        let prop = properties
            .resolve(prop_name)
            .ok_or_else(|| format!("неизвестное свойство \"{prop_name}\""))?;
        let mut scratch = ErrorSink::new();
        let value = parse_scalar_value(
            value_json,
            prop,
            properties,
            &[],
            "values",
            key,
            &mut scratch,
        )
        .ok_or_else(|| format!("не удалось разобрать значение \"{key}\""))?;
        out.push((object_name.to_string(), prop, value));
    }
    Ok(out)
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
    load_game_from_texts_with_tables(
        game_json,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        code_json,
        &[],
    )
}

/// Same as `load_game_from_texts_with_code`, additionally passing `files.tables`' texts — for
/// tests that exercise «Таблицы данных» without the wasm layer's three-round handshake. `tables`
/// is `(имя, текст)`, the same shape `load_rest_with_tables` wants.
pub fn load_game_from_texts_with_tables(
    game_json: &str,
    properties_json: &str,
    scene_json: &str,
    rules_json: &str,
    screens_json: &str,
    code_json: Option<&str>,
    tables: &[(String, Option<String>)],
) -> Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure> {
    load_game_from_texts_with_terrain(
        game_json,
        properties_json,
        scene_json,
        rules_json,
        screens_json,
        code_json,
        tables,
        None,
    )
}

/// Same as `load_game_from_texts_with_tables`, additionally passing the text of `files.terrain` —
/// for tests that exercise «Рельеф» without the wasm layer's three-round handshake.
#[allow(clippy::too_many_arguments)]
pub fn load_game_from_texts_with_terrain(
    game_json: &str,
    properties_json: &str,
    scene_json: &str,
    rules_json: &str,
    screens_json: &str,
    code_json: Option<&str>,
    tables: &[(String, Option<String>)],
    terrain_json: Option<&str>,
) -> Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure> {
    let (config, entry_warnings) = read_entry(game_json)?;
    match load_rest_with_terrain(
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
        false,
        tables,
        terrain_json,
    ) {
        Ok((game, screens, warnings, _images)) => {
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

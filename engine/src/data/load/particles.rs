//! «Ветер и частицы» → «Частицы», «Проверка перед запуском»: файл видов `files.particles` — таблица
//! «имя → вид», её проверка и предупреждение о виде, который никто не называет.

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::core::keys::EditValue;
use crate::core::particles::{ParticleKind, ParticleLook, ParticleShape, ParticleTable, Span};
use crate::core::property::{self, PropertyId};
use crate::core::rules::{CollideEffect, CommonAction, Rule, RuleSet, SetValue, TemplateValue};
use crate::core::value::Value;

use super::{
    ErrorSink, ImageDecl, ParsedObject, code_mentions_word, expect_array, expect_number,
    expect_object, expect_string, join, kind_name, parse_json_or_error, reject_unknown_keys,
    require_field, resolve_image, walk_common_actions,
};

const KEYS: &[&str] = &[
    "image",
    "shape",
    "rate",
    "lifetime",
    "size",
    "grow",
    "speed",
    "direction",
    "spread",
    "gravity",
    "opacity",
    "spin",
    "wobble",
    "wind",
];

/// Таблица видов из текста файла `file`; вид с ошибкой остаётся в ней без описания.
pub(super) fn parse_particle_table(
    text: &str,
    file: &str,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> ParticleTable {
    match parse_json_or_error(file, text, errors) {
        Some(root) => parse_particle_value(&root, file, images, errors),
        None => ParticleTable::default(),
    }
}

fn parse_particle_value(
    root: &Json,
    file: &str,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> ParticleTable {
    let Some(table) = expect_object(root, file, "", errors) else {
        return ParticleTable::default();
    };
    let mut kinds = Vec::with_capacity(table.len());
    for (name, value) in table {
        if name.is_empty() {
            errors.push(
                file,
                "",
                "пустое имя вида: у вида должно быть имя, которым его назовёт источник",
            );
            continue;
        }
        kinds.push((name.clone(), parse_kind(value, file, name, images, errors)));
    }
    ParticleTable::new(kinds)
}

/// «Редактор», `set_wind_particles`: виды, которые прислала страница, проверенные, как файл при
/// загрузке. Ошибка — текстом: место поля и что с ним не так.
pub fn parse_edit_particles(value: &Json, images: &[ImageDecl]) -> Result<ParticleTable, String> {
    let mut errors = ErrorSink::new();
    let table = parse_particle_value(value, "", images, &mut errors);
    let (errs, _) = errors.into_parts();
    if errs.is_empty() {
        return Ok(table);
    }
    let lines: Vec<String> = errs
        .iter()
        .map(|e| match e.path.is_empty() {
            true => e.message.clone(),
            false => format!("{}: {}", e.path, e.message),
        })
        .collect();
    Err(lines.join("; "))
}

fn parse_kind(
    value: &Json,
    file: &str,
    name: &str,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> Option<ParticleKind> {
    let obj = expect_object(value, file, name, errors)?;
    reject_unknown_keys(obj, KEYS, file, name, errors);
    let field = |key: &str| join(name, key);
    let look = parse_look(obj, file, name, images, errors);
    let rate = require_field(obj, "rate", file, name, errors)
        .and_then(|v| above_zero(v, "rate", file, &field("rate"), errors));
    let lifetime = require_field(obj, "lifetime", file, name, errors)
        .and_then(|v| parse_span(v, "lifetime", true, file, &field("lifetime"), errors));
    let size = require_field(obj, "size", file, name, errors)
        .and_then(|v| parse_span(v, "size", true, file, &field("size"), errors));
    let grow = optional(obj, "grow", 1.0, |v| {
        above_zero(v, "grow", file, &field("grow"), errors)
    });
    let speed = optional(obj, "speed", Span::single(0.0), |v| {
        parse_span(v, "speed", false, file, &field("speed"), errors)
    });
    let direction = optional(obj, "direction", 0.0, |v| {
        expect_number(v, file, &field("direction"), errors)
    });
    let spread = optional(obj, "spread", 0.0, |v| {
        within(v, "spread", 0.0, 180.0, file, &field("spread"), errors)
    });
    let gravity = optional(obj, "gravity", 0.0, |v| {
        expect_number(v, file, &field("gravity"), errors)
    });
    let opacity = optional(obj, "opacity", vec![1.0], |v| {
        parse_opacity(v, file, &field("opacity"), errors)
    });
    let spin = match obj.get("spin") {
        None => Some(None),
        Some(v) => parse_span(v, "spin", false, file, &field("spin"), errors).map(Some),
    };
    let wobble = optional(obj, "wobble", 0.0, |v| {
        at_least_zero(v, "wobble", file, &field("wobble"), errors)
    });
    let wind = optional(obj, "wind", 1.0, |v| {
        within(v, "wind", 0.0, 1.0, file, &field("wind"), errors)
    });
    Some(ParticleKind {
        look: look?,
        rate: rate?,
        lifetime: lifetime?,
        size: size?,
        grow: grow?,
        speed: speed?,
        direction: direction?,
        spread: spread?,
        gravity: gravity?,
        opacity: opacity?,
        spin: spin?,
        wobble: wobble?,
        wind: wind?,
    })
}

fn optional<T>(
    obj: &serde_json::Map<String, Json>,
    key: &str,
    default: T,
    parse: impl FnOnce(&Json) -> Option<T>,
) -> Option<T> {
    match obj.get(key) {
        None => Some(default),
        Some(value) => parse(value),
    }
}

/// `image` или `shape`, не оба; без обоих — мягкая точка.
fn parse_look(
    obj: &serde_json::Map<String, Json>,
    file: &str,
    name: &str,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> Option<ParticleLook> {
    match (obj.get("image"), obj.get("shape")) {
        (Some(_), Some(_)) => {
            errors.push(file, name, "у вида и image, и shape — оставьте одно");
            None
        }
        (Some(image), None) => {
            parse_image(image, file, &join(name, "image"), images, errors).map(ParticleLook::Image)
        }
        (None, Some(shape)) => {
            parse_shape(shape, file, &join(name, "shape"), errors).map(ParticleLook::Shape)
        }
        (None, None) => Some(ParticleLook::Shape(ParticleShape::Dot)),
    }
}

fn parse_shape(
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ParticleShape> {
    let name = expect_string(value, file, path, errors)?;
    if let Some(shape) = ParticleShape::from_name(&name) {
        return Some(shape);
    }
    let allowed: Vec<&str> = ParticleShape::NAMES
        .iter()
        .map(|(known, _)| *known)
        .collect();
    errors.push(
        file,
        path,
        format!(
            "неизвестный встроенный рисунок частиц \"{name}\"; ожидалось одно из: {}",
            allowed.join(", ")
        ),
    );
    None
}

fn parse_image(
    value: &Json,
    file: &str,
    path: &str,
    images: &[ImageDecl],
    errors: &mut ErrorSink,
) -> Option<usize> {
    let name = expect_string(value, file, path, errors)?;
    let id = resolve_image(&name, images, file, path, errors)?;
    let decl = &images[id];
    for (present, key) in [
        (
            decl.frame_by.is_some() || decl.frame_by_name.is_some(),
            "frame_by",
        ),
        (decl.size.is_some(), "size"),
    ] {
        if present {
            errors.push(
                file,
                path,
                format!(
                    "картинка \"{name}\" объявлена с {key}: частица рисуется своим размером и кадром, {key} ей не подходит"
                ),
            );
            return None;
        }
    }
    Some(id)
}

fn above_zero(
    value: &Json,
    key: &str,
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
        format!("{key} должен быть больше нуля, получено {n}"),
    );
    None
}

fn at_least_zero(
    value: &Json,
    key: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<f64> {
    let n = expect_number(value, file, path, errors)?;
    if n >= 0.0 {
        return Some(n);
    }
    errors.push(
        file,
        path,
        format!("{key} не может быть меньше нуля, получено {n}"),
    );
    None
}

fn within(
    value: &Json,
    key: &str,
    low: f64,
    high: f64,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<f64> {
    let n = expect_number(value, file, path, errors)?;
    if (low..=high).contains(&n) {
        return Some(n);
    }
    errors.push(
        file,
        path,
        format!("{key} должен быть от {low} до {high} включительно, получено {n}"),
    );
    None
}

/// `lifetime`, `size`, `speed`, `spin`: число или пара «от и до»; `positive` — края больше нуля.
fn parse_span(
    value: &Json,
    key: &str,
    positive: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Span> {
    let edge = |value: &Json, path: &str, errors: &mut ErrorSink| match positive {
        true => above_zero(value, key, file, path, errors),
        false => expect_number(value, file, path, errors),
    };
    let Some(pair) = value.as_array() else {
        if !value.is_number() {
            errors.push(
                file,
                path,
                format!(
                    "{key}: ожидалось число или пара чисел «от и до», получено {}",
                    kind_name(value)
                ),
            );
            return None;
        }
        return edge(value, path, errors).map(Span::single);
    };
    if pair.len() != 2 {
        errors.push(
            file,
            path,
            format!(
                "{key}: пара «от и до» — ровно два числа, получено {}",
                pair.len()
            ),
        );
        return None;
    }
    let from = edge(&pair[0], &join(path, "[0]"), errors);
    let to = edge(&pair[1], &join(path, "[1]"), errors);
    let (from, to) = (from?, to?);
    if from > to {
        errors.push(
            file,
            path,
            format!("{key}: в паре «от и до» первое число больше второго: [{from}, {to}]"),
        );
        return None;
    }
    Some(Span { from, to })
}

/// `opacity`: число от 0 до 1 или непустой список таких чисел.
fn parse_opacity(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<Vec<f64>> {
    if value.is_number() {
        return within(value, "opacity", 0.0, 1.0, file, path, errors).map(|n| vec![n]);
    }
    let points = expect_array(value, file, path, errors)?;
    if points.is_empty() {
        errors.push(file, path, "opacity: список точек не может быть пустым");
        return None;
    }
    let parsed: Vec<Option<f64>> = points
        .iter()
        .enumerate()
        .map(|(index, point)| {
            within(
                point,
                "opacity",
                0.0,
                1.0,
                file,
                &join(path, &format!("[{index}]")),
                errors,
            )
        })
        .collect();
    parsed.into_iter().collect()
}

fn mark_text(used: &mut HashSet<String>, prop: PropertyId, value: &Value) {
    if prop == property::PARTICLES
        && let Value::Text(name) = value
    {
        used.insert(name.clone());
    }
}

fn mark_set_value(used: &mut HashSet<String>, prop: PropertyId, value: &SetValue) {
    if let SetValue::Const(value) = value {
        mark_text(used, prop, value);
    }
}

fn mark_template(used: &mut HashSet<String>, template: &[(PropertyId, TemplateValue)]) {
    for (prop, value) in template {
        if let TemplateValue::Const(value) = value {
            mark_text(used, *prop, value);
        }
    }
}

fn mark_actions(used: &mut HashSet<String>, actions: &[CommonAction]) {
    walk_common_actions(actions, &mut |action| {
        if let CommonAction::SetAll { prop, value } = action {
            mark_set_value(used, *prop, value);
        }
    });
}

/// Имена видов, которые называют объекты сцены, их клавиши и щелчки, шаблоны и правила.
fn collect_used_particles(scene: &[ParsedObject], rules: &RuleSet) -> HashSet<String> {
    let mut used = HashSet::new();
    for object in scene {
        for (prop, value) in &object.values {
            mark_text(&mut used, *prop, value);
        }
        let key_edits = object
            .keys
            .iter()
            .flat_map(|table| table.values())
            .flat_map(|binding| binding.press.iter().chain(&binding.release));
        for edit in key_edits.chain(object.on_click.iter().flatten()) {
            if let EditValue::Const(value) = &edit.value {
                mark_text(&mut used, edit.property, value);
            }
        }
    }
    for rule in &rules.rules {
        match rule {
            Rule::Spawn {
                template,
                pick_one,
                do_,
                ..
            } => {
                mark_template(&mut used, template);
                for variant in pick_one.iter().flatten() {
                    mark_template(&mut used, &variant.fields);
                    for cell in &variant.cells {
                        mark_template(&mut used, &cell.fields);
                    }
                }
                mark_actions(&mut used, do_);
            }
            Rule::Collide {
                effects_a,
                effects_b,
                do_,
                ..
            } => {
                for effect in effects_a.iter().chain(effects_b) {
                    if let CollideEffect::Set { prop, value } = effect {
                        mark_set_value(&mut used, *prop, value);
                    }
                }
                mark_actions(&mut used, do_);
            }
            Rule::Check { do_, .. } | Rule::Delete { do_, .. } => mark_actions(&mut used, do_),
            Rule::Move { .. } | Rule::Walk { .. } => {}
        }
    }
    used
}

/// Вид объявлен и не назван ни в сцене, ни в правилах, ни в тексте кода — только предупреждение.
pub(super) fn warn_unused_particles(
    file: &str,
    table: &ParticleTable,
    scene: &[ParsedObject],
    rules: &RuleSet,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    let used = collect_used_particles(scene, rules);
    for name in table.names() {
        if !used.contains(name) && !code.is_some_and(|code| code_mentions_word(code, name)) {
            errors.push_warning(
                file,
                name,
                format!("вид частиц {name} объявлен и не используется"),
            );
        }
    }
}

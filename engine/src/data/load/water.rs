//! «Рельеф» → «Вода», «Проверка перед запуском»: запись `water` файла рельефа — море, озёра, реки,
//! прозрачность и рябь. Тот же разбор проверяет воду, которую присылает `set_terrain`.

use serde_json::Value as Json;

use crate::core::scene::SceneConfig;
use crate::core::value::Vec2;
use crate::core::water::{Lake, River, WaterSpec};

use super::{
    ErrorSink, expect_array, expect_number, expect_object, expect_string, join, parse_hex_color,
    parse_vec2, reject_unknown_keys, require_field,
};

pub(super) fn not_a_water_color(s: &str) -> String {
    format!("цвет должен быть вида \"#rrggbb\", получено \"{s}\"")
}

/// «Редактор», `set_terrain`: вода, которую прислала страница, проверенная, как в файле рельефа.
/// Ошибка — текстом: путь к полю и что с ним не так.
pub(super) fn parse_edit_water(value: &Json, scene: &SceneConfig) -> Result<WaterSpec, String> {
    let mut errors = ErrorSink::new();
    let water = parse_water(value, "", scene, &mut errors);
    let (errs, _) = errors.into_parts();
    match errs.first() {
        Some(error) => Err(format!("{}: {}", error.path, error.message)),
        None => water.ok_or_else(|| "вода не подходит".to_string()),
    }
}

/// Запись `water`: `color` обязателен, и есть хотя бы одно из `level`, `lakes`, `rivers`.
pub(super) fn parse_water(
    value: &Json,
    file: &str,
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<WaterSpec> {
    let obj = expect_object(value, file, "water", errors)?;
    reject_unknown_keys(
        obj,
        &["color", "level", "lakes", "rivers", "clarity", "ripple"],
        file,
        "water",
        errors,
    );
    let color = require_field(obj, "color", file, "water", errors)
        .and_then(|v| expect_string(v, file, "water → color", errors))
        .and_then(|s| {
            parse_hex_color(&s).or_else(|| {
                errors.push(file, "water → color", not_a_water_color(&s));
                None
            })
        });
    let level = optional(obj.get("level"), |v| {
        expect_number(v, file, "water → level", errors)
    });
    let clarity = optional(obj.get("clarity"), |v| {
        expect_positive(v, file, "water → clarity", errors)
    });
    let ripple = optional(obj.get("ripple"), |v| {
        expect_positive(v, file, "water → ripple", errors)
    });
    let lakes = optional_list(
        obj.get("lakes"),
        file,
        "water → lakes",
        errors,
        |item, path, errors| parse_lake(item, file, path, scene, errors),
    );
    let rivers = optional_list(
        obj.get("rivers"),
        file,
        "water → rivers",
        errors,
        |item, path, errors| parse_river(item, file, path, scene, errors),
    );
    let no_bodies =
        lakes.as_ref().is_some_and(Vec::is_empty) && rivers.as_ref().is_some_and(Vec::is_empty);
    if obj.get("level").is_none() && no_bodies {
        errors.push(
            file,
            "water",
            "нет ни \"level\", ни \"lakes\", ни \"rivers\": в воде нет ни моря, ни озёр, ни рек",
        );
        return None;
    }
    Some(WaterSpec {
        color: color?,
        level: level?,
        lakes: lakes?,
        rivers: rivers?,
        clarity: clarity?,
        ripple: ripple?,
    })
}

/// Необязательное поле: `Some(None)` — его нет, `Some(Some(v))` — есть и верно, `None` — есть и неверно.
fn optional<T>(value: Option<&Json>, parse: impl FnOnce(&Json) -> Option<T>) -> Option<Option<T>> {
    match value {
        None => Some(None),
        Some(value) => parse(value).map(Some),
    }
}

/// Необязательный список записей: каждая проверяется `parse` по своему пути; одна неверная делает
/// неверным весь список, но остальные всё равно проверяются.
fn optional_list<T>(
    value: Option<&Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
    mut parse: impl FnMut(&Json, &str, &mut ErrorSink) -> Option<T>,
) -> Option<Vec<T>> {
    let Some(value) = value else {
        return Some(Vec::new());
    };
    let items = expect_array(value, file, path, errors)?;
    let parsed: Vec<Option<T>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| parse(item, &format!("{path}[{index}]"), errors))
        .collect();
    parsed.into_iter().collect()
}

fn expect_positive(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<f64> {
    let n = expect_number(value, file, path, errors)?;
    if n <= 0.0 {
        errors.push(
            file,
            path,
            format!("ожидалось число больше нуля, получено {n}"),
        );
        return None;
    }
    Some(n)
}

/// Место в пределах сцены: от 0 до её размера по каждой оси, границы включены.
fn on_scene(place: Vec2, scene: &SceneConfig) -> bool {
    (0.0..=f64::from(scene.width)).contains(&place[0])
        && (0.0..=f64::from(scene.height)).contains(&place[1])
}

fn outside_scene(place: Vec2, scene: &SceneConfig) -> String {
    format!(
        "точка ({}, {}) вне сцены: ожидалось от 0 до {} по x и от 0 до {} по y",
        place[0], place[1], scene.width, scene.height
    )
}

fn parse_lake(
    value: &Json,
    file: &str,
    path: &str,
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<Lake> {
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(obj, &["at", "level"], file, path, errors);
    let at_path = join(path, "at");
    let at = require_field(obj, "at", file, path, errors)
        .and_then(|v| parse_vec2(v, file, &at_path, errors))
        .filter(|place| {
            let inside = on_scene(*place, scene);
            if !inside {
                errors.push(file, &at_path, outside_scene(*place, scene));
            }
            inside
        });
    let level = require_field(obj, "level", file, path, errors)
        .and_then(|v| expect_number(v, file, &join(path, "level"), errors));
    Some(Lake {
        at: at?,
        level: level?,
    })
}

fn parse_river(
    value: &Json,
    file: &str,
    path: &str,
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<River> {
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(obj, &["points"], file, path, errors);
    let points_path = join(path, "points");
    let items = require_field(obj, "points", file, path, errors)
        .and_then(|v| expect_array(v, file, &points_path, errors))?;
    if items.len() < 2 {
        errors.push(
            file,
            &points_path,
            format!("у реки не меньше двух точек, получено {}", items.len()),
        );
        return None;
    }
    let mut points: Vec<[f64; 4]> = Vec::with_capacity(items.len());
    let mut ok = true;
    for (index, item) in items.iter().enumerate() {
        let point_path = format!("{points_path}[{index}]");
        match parse_river_point(item, file, &point_path, scene, errors) {
            Some(point) => {
                if let Some(before) = points.last().filter(|before| point[2] > before[2]) {
                    errors.push(
                        file,
                        &point_path,
                        format!(
                            "точка реки выше предыдущей: высота {} против {}; к устью гладь не поднимается",
                            point[2], before[2]
                        ),
                    );
                    ok = false;
                }
                points.push(point);
            }
            None => ok = false,
        }
    }
    ok.then_some(River { points })
}

/// Точка реки `[x, y, h, w]`: место в сцене, высота глади и ширина русла больше нуля.
fn parse_river_point(
    value: &Json,
    file: &str,
    path: &str,
    scene: &SceneConfig,
    errors: &mut ErrorSink,
) -> Option<[f64; 4]> {
    let items = expect_array(value, file, path, errors)?;
    if items.len() != 4 {
        errors.push(
            file,
            path,
            format!(
                "ожидались четыре числа [x, y, высота глади, ширина русла], элементов: {}",
                items.len()
            ),
        );
        return None;
    }
    let numbers: Vec<Option<f64>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| expect_number(item, file, &format!("{path}[{index}]"), errors))
        .collect();
    let [Some(x), Some(y), Some(height), Some(width)] = numbers[..] else {
        return None;
    };
    if !on_scene([x, y], scene) {
        errors.push(file, path, outside_scene([x, y], scene));
        return None;
    }
    if width <= 0.0 {
        errors.push(
            file,
            &format!("{path}[3]"),
            format!("ширина русла должна быть больше нуля, получено {width}"),
        );
        return None;
    }
    Some([x, y, height, width])
}

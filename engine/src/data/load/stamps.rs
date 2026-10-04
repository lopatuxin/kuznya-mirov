//! «Лепка рельефа» → «Отпечатки», «Проверка перед запуском»: файлы штампов `files.stamps` и список отпечатков
//! `stamps` файла рельефа.

use std::sync::Arc;

use serde_json::Value as Json;

use crate::core::imprints::{Imprint, Stamp, StampTable};
use crate::core::value::{Rotation, Vec2};

use super::{
    ErrorSink, expect_array, expect_number, expect_object, expect_string, join,
    parse_json_or_error, parse_vec2, reject_unknown_keys, require_field,
};

/// Штампы `declared` — пары «имя, путь» в порядке объявления — из текстов `texts`, которые страница
/// прочитала по именам; `None` — файла не нашлось. Штамп с ошибкой остаётся в таблице пустым.
pub(super) fn parse_stamp_table(
    declared: &[(String, String)],
    texts: &[(String, Option<String>)],
    errors: &mut ErrorSink,
) -> StampTable {
    let mut entries = Vec::with_capacity(declared.len());
    for (name, path) in declared {
        let text = texts
            .iter()
            .find(|(n, _)| n == name)
            .and_then(|(_, t)| t.as_deref());
        let stamp = match text {
            Some(text) => {
                let stamp = parse_stamp(text, path, errors);
                errors.fill_locations(path, text);
                stamp
            }
            None => {
                errors.push(
                    path,
                    "",
                    format!(
                        "файл не найден; ожидался JSON-файл штампа, названный в game.json → files → stamps → {name}"
                    ),
                );
                None
            }
        };
        entries.push((name.clone(), stamp.map(Arc::new)));
    }
    StampTable::new(entries)
}

fn parse_stamp(text: &str, file: &str, errors: &mut ErrorSink) -> Option<Stamp> {
    let root = parse_json_or_error(file, text, errors)?;
    let obj = expect_object(&root, file, "", errors)?;
    reject_unknown_keys(obj, &["heights"], file, "", errors);
    let rows_json = require_field(obj, "heights", file, "", errors)
        .and_then(|v| expect_array(v, file, "heights", errors))?;
    parse_stamp_rows(rows_json, file, errors).map(|rows| Stamp::new(&rows))
}

/// Строки штампа: числа от 0 до 1, строки одной длины, точек не меньше двух на две.
fn parse_stamp_rows(
    rows_json: &[Json],
    file: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<Vec<f64>>> {
    let mut ok = true;
    let mut width: Option<usize> = None;
    let mut rows = Vec::with_capacity(rows_json.len());
    for (r, row_json) in rows_json.iter().enumerate() {
        let row_path = format!("heights[{r}]");
        let Some(cells) = expect_array(row_json, file, &row_path, errors) else {
            ok = false;
            continue;
        };
        let first = *width.get_or_insert(cells.len());
        if cells.len() != first {
            errors.push(
                file,
                &row_path,
                format!(
                    "heights[{r}]: {} чисел, а в первой строке {first}: строки штампа одной длины",
                    cells.len()
                ),
            );
            ok = false;
        }
        let mut row = Vec::with_capacity(cells.len());
        for (c, cell) in cells.iter().enumerate() {
            match parse_stamp_height(cell, file, &format!("{row_path}[{c}]"), errors) {
                Some(height) => row.push(height),
                None => ok = false,
            }
        }
        rows.push(row);
    }
    let width = width.unwrap_or(0);
    if ok && (rows.len() < 2 || width < 2) {
        errors.push(
            file,
            "heights",
            format!(
                "heights: {} строк по {width} чисел, а точек нужно не меньше двух на две",
                rows.len()
            ),
        );
        ok = false;
    }
    ok.then_some(rows)
}

fn parse_stamp_height(value: &Json, file: &str, path: &str, errors: &mut ErrorSink) -> Option<f64> {
    let height = expect_number(value, file, path, errors)?;
    if !(0.0..=1.0).contains(&height) {
        errors.push(
            file,
            path,
            format!("высота штампа — число от 0 до 1, получено {height}"),
        );
        return None;
    }
    Some(height)
}

/// `stamps` файла рельефа: список отпечатков.
pub(super) fn parse_imprints(
    value: &Json,
    file: &str,
    table: &StampTable,
    errors: &mut ErrorSink,
) -> Option<Vec<Imprint>> {
    let items = expect_array(value, file, "stamps", errors)?;
    parse_imprint_items(items, file, table, errors)
}

fn parse_imprint_items(
    items: &[Json],
    file: &str,
    table: &StampTable,
    errors: &mut ErrorSink,
) -> Option<Vec<Imprint>> {
    let mut ok = true;
    let mut imprints = Vec::with_capacity(items.len());
    for (index, item) in items.iter().enumerate() {
        match parse_imprint(item, file, &format!("stamps[{index}]"), table, errors) {
            Some(imprint) => imprints.push(imprint),
            None => ok = false,
        }
    }
    ok.then_some(imprints)
}

/// «Редактор», `set_terrain`: отпечатки из списка, который прислала страница, проверенные, как в файле
/// рельефа. Ошибка — текстом: путь к полю и что с ним не так.
pub fn parse_edit_imprints(items: &[Json], table: &StampTable) -> Result<Vec<Imprint>, String> {
    let mut errors = ErrorSink::new();
    let imprints = parse_imprint_items(items, "", table, &mut errors);
    let (errs, _) = errors.into_parts();
    match errs.first() {
        Some(error) => Err(format!("{}: {}", error.path, error.message)),
        None => imprints.ok_or_else(|| "отпечатки не подходят".to_string()),
    }
}

fn parse_imprint(
    value: &Json,
    file: &str,
    path: &str,
    table: &StampTable,
    errors: &mut ErrorSink,
) -> Option<Imprint> {
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(
        obj,
        &["stamp", "position", "size", "height", "rotation"],
        file,
        path,
        errors,
    );
    let stamp_path = join(path, "stamp");
    let stamp = require_field(obj, "stamp", file, path, errors)
        .and_then(|v| expect_string(v, file, &stamp_path, errors))
        .and_then(|name| match table.find(&name) {
            Some((index, Some(shape))) => Some((index, shape.clone())),
            Some((_, None)) => None,
            None => {
                errors.push(
                    file,
                    &stamp_path,
                    format!("штамп \"{name}\" не объявлен в game.json → files → stamps"),
                );
                None
            }
        });
    let position = require_field(obj, "position", file, path, errors)
        .and_then(|v| parse_vec2(v, file, &join(path, "position"), errors));
    let size = require_field(obj, "size", file, path, errors)
        .and_then(|v| parse_vec2(v, file, &join(path, "size"), errors))
        .filter(|size| positive_size(*size, file, &join(path, "size"), errors));
    let height = require_field(obj, "height", file, path, errors)
        .and_then(|v| expect_number(v, file, &join(path, "height"), errors))
        .filter(|&height| nonzero_height(height, file, &join(path, "height"), errors));
    let rotation = match obj.get("rotation") {
        None => Rotation::from_degrees(0.0),
        Some(v) => {
            expect_number(v, file, &join(path, "rotation"), errors).and_then(Rotation::from_degrees)
        }
    };
    let (stamp, shape) = stamp?;
    Some(Imprint {
        stamp,
        shape,
        position: position?,
        size: size?,
        height: height?,
        rotation: rotation?,
    })
}

fn positive_size(size: Vec2, file: &str, path: &str, errors: &mut ErrorSink) -> bool {
    let positive = size[0] > 0.0 && size[1] > 0.0;
    if !positive {
        errors.push(
            file,
            path,
            format!(
                "size — ширина и глубина в клетках, числа больше нуля, получено [{}, {}]",
                size[0], size[1]
            ),
        );
    }
    positive
}

fn nonzero_height(height: f64, file: &str, path: &str, errors: &mut ErrorSink) -> bool {
    let nonzero = height != 0.0;
    if !nonzero {
        errors.push(
            file,
            path,
            format!(
                "height — число не ноль: больше нуля поднимает землю, меньше — вдавливает, получено {}",
                height.abs()
            ),
        );
    }
    nonzero
}

/// Штамп объявлен и не поставлен ни одним отпечатком — только предупреждение.
pub(super) fn warn_unused_stamps(
    declared: &[(String, String)],
    imprints: &[Imprint],
    errors: &mut ErrorSink,
) {
    for (index, (name, _)) in declared.iter().enumerate() {
        if imprints.iter().all(|imprint| imprint.stamp != index) {
            errors.push_warning(
                "game.json",
                &format!("files → stamps → {name}"),
                format!(
                    "штамп \"{name}\" объявлен и не поставлен ни одним отпечатком файла рельефа"
                ),
            );
        }
    }
}

//! «Трёхмерная сцена» → «Вид дороги»: таблица `scene.road_kinds` и проверки перед запуском — кто и чем кусок
//! дороги, какие модели в нём названы, названа ли вид где-нибудь.

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::core::model::ModelTable;
use crate::core::road_kinds::{NodeModel, PieceModel, RoadKind, RoadKindTable};

use super::models::ModelDecl;
use super::{
    ErrorSink, code_mentions_word, expect_object, join, kind_name, reject_unknown_keys,
    require_field,
};

const FILE: &str = "game.json";
const PATH: &str = "scene → road_kinds";

/// Один вид `scene.road_kinds`: имя, имена моделей сменных вариантов прямого куска и имя модели узла.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadKindDecl {
    pub name: String,
    pub pieces: Vec<String>,
    pub node: String,
}

/// `scene.road_kinds`: таблица «имя вида → описание». Описание с ошибкой пропускается, его ошибки уже записаны.
pub(super) fn parse_road_kinds(
    value: &Json,
    three_d: bool,
    errors: &mut ErrorSink,
) -> Option<Vec<RoadKindDecl>> {
    if !three_d {
        errors.push(
            FILE,
            PATH,
            "\"road_kinds\" есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
        return None;
    }
    let Some(table) = value.as_object() else {
        errors.push(
            FILE,
            PATH,
            "\"road_kinds\" — ожидалась таблица \"имя вида → описание\", как files.models",
        );
        return None;
    };
    let mut out = Vec::with_capacity(table.len());
    for (name, decl) in table {
        if name.is_empty() {
            errors.push(
                FILE,
                PATH,
                "\"road_kinds\" → пустое имя: у вида должно быть имя, которым его назовёт \"road_kind\"",
            );
            continue;
        }
        if let Some(decl) = parse_kind(name, decl, errors) {
            out.push(decl);
        }
    }
    Some(out)
}

fn parse_kind(name: &str, value: &Json, errors: &mut ErrorSink) -> Option<RoadKindDecl> {
    let path = join(PATH, name);
    let obj = expect_object(value, FILE, &path, errors)?;
    reject_unknown_keys(obj, &["piece", "node"], FILE, &path, errors);
    let piece = require_field(obj, "piece", FILE, &path, errors)
        .and_then(|value| parse_pieces(value, &join(&path, "piece"), errors));
    let node = require_field(obj, "node", FILE, &path, errors)
        .and_then(|value| model_name(value, "node", &join(&path, "node"), errors));
    Some(RoadKindDecl {
        name: name.to_string(),
        pieces: piece?,
        node: node?,
    })
}

/// Имя модели в поле `key` вида: строка.
fn model_name(value: &Json, key: &str, path: &str, errors: &mut ErrorSink) -> Option<String> {
    let name = value.as_str().map(str::to_string);
    if name.is_none() {
        errors.push(
            FILE,
            path,
            format!(
                "\"{key}\" — ожидалось имя модели строкой, получено {}",
                kind_name(value)
            ),
        );
    }
    name
}

/// `piece` — имя модели или непустой список имён.
fn parse_pieces(value: &Json, path: &str, errors: &mut ErrorSink) -> Option<Vec<String>> {
    let Some(items) = value.as_array() else {
        return match value {
            Json::String(_) => model_name(value, "piece", path, errors).map(|name| vec![name]),
            _ => {
                errors.push(
                    FILE,
                    path,
                    format!(
                        "\"piece\" — ожидалось имя модели или список имён, получено {}",
                        kind_name(value)
                    ),
                );
                None
            }
        };
    };
    if items.is_empty() {
        errors.push(
            FILE,
            path,
            "\"piece\" — пустой список: нужен хотя бы один вариант прямого куска",
        );
        return None;
    }
    let names: Vec<Option<String>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| model_name(item, "piece", &join(path, &format!("[{index}]")), errors))
        .collect();
    names.into_iter().collect()
}

/// Каждая названная в видах модель объявлена в `files.models`.
pub(super) fn validate_kind_models(
    kinds: &[RoadKindDecl],
    declared: &[ModelDecl],
    errors: &mut ErrorSink,
) {
    let known = declared
        .iter()
        .map(|decl| format!("\"{}\"", decl.name))
        .collect::<Vec<_>>()
        .join(", ");
    let check = |model: &str, path: String, errors: &mut ErrorSink| {
        if declared.iter().any(|decl| decl.name == model) {
            return;
        }
        let message = if declared.is_empty() {
            format!("модели \"{model}\" нет; модели не объявлены")
        } else {
            format!("модели \"{model}\" нет; объявлены {known}")
        };
        errors.push(FILE, &path, message);
    };
    for kind in kinds {
        let path = join(PATH, &kind.name);
        for (index, piece) in kind.pieces.iter().enumerate() {
            let at = if kind.pieces.len() == 1 {
                join(&path, "piece")
            } else {
                join(&join(&path, "piece"), &format!("[{index}]"))
            };
            check(piece, at, errors);
        }
        check(&kind.node, join(&path, "node"), errors);
    }
}

/// Размеры названных моделей: у модели куска — по X и по Z, у модели узла — по X; нулевой размер — ошибка.
/// Возвращает таблицу видов; вид с ошибкой в неё не входит.
pub(super) fn build_road_kinds(
    kinds: &[RoadKindDecl],
    models: &ModelTable,
    errors: &mut ErrorSink,
) -> RoadKindTable {
    let mut out = Vec::with_capacity(kinds.len());
    for kind in kinds {
        let path = join(PATH, &kind.name);
        let mut fits = true;
        let mut pieces = Vec::with_capacity(kind.pieces.len());
        for name in &kind.pieces {
            match models
                .index_of(name)
                .and_then(|index| Some((index, models.get(index)?)))
            {
                Some((index, model)) if model.size[0] > 0.0 && model.size[1] > 0.0 => {
                    pieces.push(PieceModel {
                        model: index,
                        width: model.size[0],
                        length: model.size[1],
                    });
                }
                Some(_) => {
                    errors.push(
                        FILE,
                        &join(&path, "piece"),
                        format!(
                            "\"piece\" — у модели \"{name}\" размер по X или Z равен нулю: кусок лежит плоско, ширина — размер по X, длина — по Z"
                        ),
                    );
                    fits = false;
                }
                None => fits = false,
            }
        }
        let node = match models
            .index_of(&kind.node)
            .and_then(|index| Some((index, models.get(index)?)))
        {
            Some((index, model)) if model.size[0] > 0.0 => Some(NodeModel {
                model: index,
                width: model.size[0],
            }),
            Some(_) => {
                errors.push(
                    FILE,
                    &join(&path, "node"),
                    format!(
                        "\"node\" — у модели \"{}\" размер по X равен нулю: ширина узла — размер по X",
                        kind.node
                    ),
                );
                None
            }
            None => None,
        };
        if let (true, Some(node)) = (fits, node) {
            out.push(RoadKind {
                name: kind.name.clone(),
                pieces,
                node,
            });
        }
    }
    RoadKindTable::new(out)
}

/// Вид объявлен и нигде не назван — только предупреждение. Назван — значит, его `road_kind` стоит у объекта
/// `scene.json` или в привязке его клавиши (`named`), его имя стоит строкой в `rules.json` или встречается
/// словом в тексте кода.
pub(super) fn warn_unused_road_kinds(
    kinds: &[RoadKindDecl],
    named: &HashSet<String>,
    rules: Option<&str>,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    for kind in kinds {
        let quoted = format!("\"{}\"", kind.name);
        if named.contains(&kind.name)
            || rules.is_some_and(|rules| rules.contains(&quoted))
            || code.is_some_and(|code| code_mentions_word(code, &kind.name))
        {
            continue;
        }
        errors.push_warning(
            FILE,
            &join(PATH, &kind.name),
            format!(
                "вид дороги \"{}\" объявлен и нигде не назван ни в scene.json, ни в rules.json, ни в тексте кода",
                kind.name
            ),
        );
    }
}

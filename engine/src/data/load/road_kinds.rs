//! «Трёхмерная сцена» → «Вид дороги»: таблицы `scene.road_kinds` и `scene.road_junctions` и проверки перед
//! запуском — кто и чем кусок дороги, какие модели названы видами и перекрёстками, из каких рукавов состоит
//! перекрёсток, названа ли вид где-нибудь.

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::core::model::ModelTable;
use crate::core::road_kinds::{JunctionArm, JunctionModel, PieceModel, RoadKind, RoadKindTable};

use super::models::ModelDecl;
use super::{
    ErrorSink, code_mentions_word, expect_array, expect_number, expect_object, expect_string, join,
    kind_name, reject_unknown_keys, require_field,
};

const FILE: &str = "game.json";
const PATH: &str = "scene → road_kinds";
const JUNCTIONS_PATH: &str = "scene → road_junctions";

/// Полный круг в градусах: направления рукавов считаются по модулю круга.
const FULL_TURN_DEGREES: f64 = 360.0;

/// Один вид `scene.road_kinds`: имя и имена моделей сменных вариантов прямого куска.
#[derive(Debug, Clone, PartialEq)]
pub struct RoadKindDecl {
    pub name: String,
    pub pieces: Vec<String>,
}

/// Рукав модели перекрёстка `scene.road_junctions`: направление в градусах, имя вида, расстояние от середины до
/// устья и длина гаснущей полосы перед ним.
#[derive(Debug, Clone, PartialEq)]
pub struct JunctionArmDecl {
    pub dir: f64,
    pub kind: String,
    pub reach: f64,
    pub fade: f64,
}

/// Одна запись `scene.road_junctions`: имя модели из `files.models` и её рукава.
#[derive(Debug, Clone, PartialEq)]
pub struct JunctionDecl {
    pub name: String,
    pub arms: Vec<JunctionArmDecl>,
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
    reject_unknown_keys(obj, &["piece"], FILE, &path, errors);
    let piece = require_field(obj, "piece", FILE, &path, errors)
        .and_then(|value| parse_pieces(value, &join(&path, "piece"), errors));
    Some(RoadKindDecl {
        name: name.to_string(),
        pieces: piece?,
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

/// `scene.road_junctions`: таблица «имя модели → описание». Запись с ошибкой пропускается, её ошибки уже
/// записаны.
pub(super) fn parse_road_junctions(
    value: &Json,
    three_d: bool,
    errors: &mut ErrorSink,
) -> Option<Vec<JunctionDecl>> {
    if !three_d {
        errors.push(
            FILE,
            JUNCTIONS_PATH,
            "\"road_junctions\" есть только в трёхмерной сцене: у scene в game.json нет camera",
        );
        return None;
    }
    let Some(table) = value.as_object() else {
        errors.push(
            FILE,
            JUNCTIONS_PATH,
            "\"road_junctions\" — ожидалась таблица \"имя модели → описание\", как files.models",
        );
        return None;
    };
    Some(
        table
            .iter()
            .filter_map(|(name, decl)| parse_junction(name, decl, errors))
            .collect(),
    )
}

fn parse_junction(name: &str, value: &Json, errors: &mut ErrorSink) -> Option<JunctionDecl> {
    let path = join(JUNCTIONS_PATH, name);
    let obj = expect_object(value, FILE, &path, errors)?;
    reject_unknown_keys(obj, &["arms"], FILE, &path, errors);
    let arms_path = join(&path, "arms");
    let items = require_field(obj, "arms", FILE, &path, errors)
        .and_then(|value| expect_array(value, FILE, &arms_path, errors))?;
    if items.is_empty() {
        errors.push(
            FILE,
            &arms_path,
            "\"arms\" — пустой список: у перекрёстка должен быть хотя бы один рукав",
        );
        return None;
    }
    let arms: Vec<Option<JunctionArmDecl>> = items
        .iter()
        .enumerate()
        .map(|(index, item)| parse_arm(item, &join(&arms_path, &format!("[{index}]")), errors))
        .collect();
    let arms: Vec<JunctionArmDecl> = arms.into_iter().collect::<Option<_>>()?;
    let mut distinct = true;
    for (index, arm) in arms.iter().enumerate() {
        if arms[..index]
            .iter()
            .any(|known| same_direction(known.dir, arm.dir))
        {
            errors.push(
                FILE,
                &join(&arms_path, &format!("[{index}]")),
                format!(
                    "\"dir\" — направление {} уже занято другим рукавом этой модели",
                    arm.dir
                ),
            );
            distinct = false;
        }
    }
    distinct.then(|| JunctionDecl {
        name: name.to_string(),
        arms,
    })
}

/// Совпадают ли два направления в градусах с точностью до полного круга.
fn same_direction(a: f64, b: f64) -> bool {
    let apart = (a - b).rem_euclid(FULL_TURN_DEGREES);
    apart < 1e-9 || FULL_TURN_DEGREES - apart < 1e-9
}

/// Рукав: числа `dir`, `reach` (больше 0) и `fade` (от 0 до `reach`, не включая его) и имя вида.
fn parse_arm(value: &Json, path: &str, errors: &mut ErrorSink) -> Option<JunctionArmDecl> {
    let obj = expect_object(value, FILE, path, errors)?;
    reject_unknown_keys(obj, &["dir", "kind", "reach", "fade"], FILE, path, errors);
    let number = |key: &str, errors: &mut ErrorSink| {
        let value = require_field(obj, key, FILE, path, errors)?;
        expect_number(value, FILE, &join(path, key), errors)
    };
    let dir = number("dir", errors);
    let kind = require_field(obj, "kind", FILE, path, errors)
        .and_then(|value| expect_string(value, FILE, &join(path, "kind"), errors));
    let reach = number("reach", errors);
    let fade = number("fade", errors);
    let (dir, kind, reach, fade) = (dir?, kind?, reach?, fade?);
    if reach <= 0.0 {
        errors.push(
            FILE,
            &join(path, "reach"),
            format!("\"reach\" — ожидалось число больше 0, получено {reach}"),
        );
        return None;
    }
    if !(0.0..reach).contains(&fade) {
        errors.push(
            FILE,
            &join(path, "fade"),
            format!(
                "\"fade\" — ожидалось число от 0 до \"reach\" ({reach}), не включая его, получено {fade}"
            ),
        );
        return None;
    }
    Some(JunctionArmDecl {
        dir,
        kind,
        reach,
        fade,
    })
}

/// Каждая названная в видах модель объявлена в `files.models`.
pub(super) fn validate_kind_models(
    kinds: &[RoadKindDecl],
    declared: &[ModelDecl],
    errors: &mut ErrorSink,
) {
    for kind in kinds {
        let path = join(PATH, &kind.name);
        for (index, piece) in kind.pieces.iter().enumerate() {
            let at = if kind.pieces.len() == 1 {
                join(&path, "piece")
            } else {
                join(&join(&path, "piece"), &format!("[{index}]"))
            };
            check_declared(piece, &at, declared, errors);
        }
    }
}

fn check_declared(model: &str, path: &str, declared: &[ModelDecl], errors: &mut ErrorSink) {
    if declared.iter().any(|decl| decl.name == model) {
        return;
    }
    let message = if declared.is_empty() {
        format!("модели \"{model}\" нет; модели не объявлены")
    } else {
        let known = declared
            .iter()
            .map(|decl| format!("\"{}\"", decl.name))
            .collect::<Vec<_>>()
            .join(", ");
        format!("модели \"{model}\" нет; объявлены {known}")
    };
    errors.push(FILE, path, message);
}

/// Каждая модель перекрёстка объявлена в `files.models`, а вид каждого рукава — в `scene.road_kinds`.
pub(super) fn validate_junctions(
    junctions: &[JunctionDecl],
    kinds: &[RoadKindDecl],
    declared: &[ModelDecl],
    errors: &mut ErrorSink,
) {
    let known = kinds
        .iter()
        .map(|kind| format!("\"{}\"", kind.name))
        .collect::<Vec<_>>()
        .join(", ");
    for junction in junctions {
        let path = join(JUNCTIONS_PATH, &junction.name);
        check_declared(&junction.name, &path, declared, errors);
        for (index, arm) in junction.arms.iter().enumerate() {
            if kinds.iter().any(|kind| kind.name == arm.kind) {
                continue;
            }
            let at = join(&join(&join(&path, "arms"), &format!("[{index}]")), "kind");
            let message = if kinds.is_empty() {
                format!(
                    "\"kind\" — вида дороги \"{}\" нет; виды не объявлены",
                    arm.kind
                )
            } else {
                format!(
                    "\"kind\" — вида дороги \"{}\" нет; объявлены {known}",
                    arm.kind
                )
            };
            errors.push(FILE, &at, message);
        }
    }
}

/// Размеры названных моделей кусков: ширина по X и длина по Z, нулевой размер — ошибка; и модели перекрёстков с
/// рукавами, чьи виды получили номера. Возвращает таблицу; вид или перекрёсток с ошибкой в неё не входит.
pub(super) fn build_road_kinds(
    kinds: &[RoadKindDecl],
    junctions: &[JunctionDecl],
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
        if fits {
            out.push(RoadKind {
                name: kind.name.clone(),
                pieces,
            });
        }
    }
    let models_of_junctions = junctions
        .iter()
        .filter_map(|junction| {
            let model = models.index_of(&junction.name)?;
            let arms = junction
                .arms
                .iter()
                .map(|arm| {
                    let kind = out.iter().position(|kind| kind.name == arm.kind)?;
                    Some(JunctionArm {
                        dir: arm.dir.rem_euclid(FULL_TURN_DEGREES).to_radians(),
                        kind,
                        reach: arm.reach,
                        fade: arm.fade,
                        width: out[kind].width(),
                    })
                })
                .collect::<Option<_>>()?;
            Some(JunctionModel { model, arms })
        })
        .collect();
    RoadKindTable::new(out, models_of_junctions)
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
                "вид дороги \"{}\" объявлен и нигде не назван ни в scene.json, ни в rules.json, ни в тексте кода и его таблицах",
                kind.name
            ),
        );
    }
}

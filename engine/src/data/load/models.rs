//! «Трёхмерная сцена» → «Модели и движения»: таблица `files.models` и проверка файлов моделей, которые
//! страница уже прочитала и чьи картинки разжала.

use std::collections::HashSet;
use std::sync::Arc;

use serde_json::Value as Json;

use crate::core::keys::{EditValue, KeyEdit};
use crate::core::model::{Image, Look, Model, ModelImage, ModelTable};
use crate::core::property::PropertyId;
use crate::core::skeleton::{Leg, Skeleton};
use crate::core::solid::SolidGrid;
use crate::core::value::Value;
use crate::data::glb::{self, FileImage};

use super::imported::{ImportedPlan, PackedFault, PackedFiles, resolve_uri};
use super::materials::pixel_problem;
use super::motions::{self, MotionSet};
use super::{
    ErrorSink, ImageVerdict, ParsedObject, code_mentions_word, expect_object, expect_string,
    find_value, join, kind_name, reject_unknown_keys, require_field,
};

/// Одна модель `files.models`: имя, путь файла `.glb` от папки игры, как она освещена и, у модели со скелетом,
/// движения стойки, ходьбы и лазания, кости ступней, кость начала верха тела и две кисти.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelDecl {
    pub name: String,
    pub path: String,
    pub look: Look,
    /// `solid`: есть ли ключ и его значение.
    pub solid: Option<bool>,
    /// `cutaway`: имя модели среза.
    pub cutaway: Option<String>,
    pub idle: Option<String>,
    pub walk: Option<String>,
    pub climb: Option<String>,
    pub feet: Option<[String; 2]>,
    pub spine: Option<String>,
    pub hands: Option<[String; 2]>,
}

/// Ответ страницы по файлу одной модели: его байты, если файл нашёлся, и по каждой картинке внутри него —
/// её номер в файле и вердикт, как у картинок игры.
#[derive(Debug, Clone, Default)]
pub struct ModelFile {
    pub bytes: Option<Vec<u8>>,
    pub images: Vec<(usize, ImageVerdict)>,
}

/// `files.models`: таблица «имя → описание»; порядок объявления `order` — номер модели. Описание с
/// ошибкой пропускается, его ошибки уже записаны.
pub(super) fn parse_models_table(
    value: &Json,
    order: &[String],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<ModelDecl>> {
    let Some(obj) = value.as_object() else {
        errors.push(
            file,
            path,
            "files.models — ожидалась таблица \"имя → описание\", как files.images",
        );
        return None;
    };
    let mut out = Vec::with_capacity(obj.len());
    for (name, decl_json) in obj {
        if name.is_empty() {
            errors.push(
                file,
                path,
                "files.models → пустое имя: у модели должно быть имя, которым её назовёт свойство model",
            );
            continue;
        }
        if let Some(decl) = parse_model_decl(name, decl_json, file, &join(path, name), errors) {
            out.push(decl);
        }
    }
    out.sort_by_key(|decl| order.iter().position(|name| name == &decl.name));
    let declared: Vec<&String> = obj.keys().collect();
    check_cutaways(&out, &declared, file, path, errors);
    Some(out)
}

fn parse_model_decl(
    name: &str,
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<ModelDecl> {
    let obj = expect_object(value, file, path, errors)?;
    let known = [
        "path", "look", "solid", "cutaway", "idle", "walk", "climb", "feet", "spine", "hands",
    ];
    reject_unknown_keys(obj, &known, file, path, errors);
    if !obj.keys().all(|key| known.contains(&key.as_str())) {
        return None;
    }
    let motion = |key: &str, errors: &mut ErrorSink| -> Option<Option<String>> {
        match obj.get(key) {
            None => Some(None),
            Some(value) => expect_string(value, file, &join(path, key), errors).map(Some),
        }
    };
    let (idle, walk, climb) = (
        motion("idle", errors),
        motion("walk", errors),
        motion("climb", errors),
    );
    let feet = match obj.get("feet") {
        None => Some(None),
        Some(value) => {
            parse_bone_pair(value, "ступни", file, &join(path, "feet"), errors).map(Some)
        }
    };
    let spine = motion("spine", errors);
    let look = parse_look(obj.get("look"), file, &join(path, "look"), errors);
    let solid = parse_solid(obj.get("solid"), file, &join(path, "solid"), errors);
    let cutaway = match obj.get("cutaway") {
        None => Some(None),
        Some(value) => expect_string(value, file, &join(path, "cutaway"), errors).map(Some),
    };
    let hands = match obj.get("hands") {
        None => Some(None),
        Some(value) => {
            parse_bone_pair(value, "кисти", file, &join(path, "hands"), errors).map(Some)
        }
    };
    let field = join(path, "path");
    let model_path = expect_string(
        require_field(obj, "path", file, path, errors)?,
        file,
        &field,
        errors,
    )?;
    if !model_path.to_ascii_lowercase().ends_with(".glb") {
        errors.push(
            file,
            &field,
            format!(
                "{model_path} — модель берётся из файла .glb; путь должен оканчиваться на \".glb\""
            ),
        );
        return None;
    }
    Some(ModelDecl {
        name: name.to_string(),
        path: model_path,
        look: look?,
        solid: solid?,
        cutaway: cutaway?,
        idle: idle?,
        walk: walk?,
        climb: climb?,
        feet: feet?,
        spine: spine?,
        hands: hands?,
    })
}

/// «Свет и материалы» → «Рисованный свет и картины»: `look` — `lit` или `picture`; нет ключа — `lit`.
fn parse_look(
    value: Option<&Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Look> {
    let Some(value) = value else {
        return Some(Look::Lit);
    };
    match expect_string(value, file, path, errors)?.as_str() {
        "lit" => Some(Look::Lit),
        "picture" => Some(Look::Picture),
        other => {
            errors.push(
                file,
                path,
                format!("look должен быть \"lit\" или \"picture\", получено \"{other}\""),
            );
            None
        }
    }
}

/// «Трёхмерная сцена» → «Здания»: `solid` — `true` или `false`.
fn parse_solid(
    value: Option<&Json>,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<bool>> {
    let Some(value) = value else {
        return Some(None);
    };
    let flag = value.as_bool();
    if flag.is_none() {
        errors.push(
            file,
            path,
            format!(
                "solid должен быть true или false, получено {}",
                kind_name(value)
            ),
        );
    }
    flag.map(Some)
}

/// «Трёхмерная сцена» → «Здания»: `cutaway` есть только у твёрдой модели и называет другую модель из
/// `files.models`.
fn check_cutaways(
    decls: &[ModelDecl],
    declared: &[&String],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) {
    for decl in decls {
        let Some(cutaway) = &decl.cutaway else {
            continue;
        };
        let field = join(&join(path, &decl.name), "cutaway");
        if decl.solid != Some(true) {
            errors.push(
                file,
                &field,
                format!(
                    "cutaway у модели \"{}\" без solid: модель среза есть только у твёрдой модели (solid: true)",
                    decl.name
                ),
            );
        }
        if cutaway == &decl.name {
            errors.push(
                file,
                &field,
                format!(
                    "cutaway модели \"{}\" называет её саму; нужна другая модель — здание без стен и крыши",
                    decl.name
                ),
            );
        } else if !declared.contains(&cutaway) {
            let known = declared
                .iter()
                .map(|name| format!("\"{name}\""))
                .collect::<Vec<_>>()
                .join(", ");
            errors.push(
                file,
                &field,
                format!("cutaway \"{cutaway}\": такой модели нет; объявлены {known}"),
            );
        }
    }
}

/// `feet` и `hands`: имена костей левой и правой ступни или кисти; `whose` — «ступни» или «кисти».
fn parse_bone_pair(
    value: &Json,
    whose: &str,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<[String; 2]> {
    let names = value
        .as_array()
        .filter(|items| items.len() == 2)
        .and_then(|items| {
            items
                .iter()
                .map(|item| item.as_str().map(str::to_string))
                .collect::<Option<Vec<String>>>()
        });
    let Some(names) = names else {
        errors.push(
            file,
            path,
            format!(
                "ожидались имена двух костей, левой и правой {whose}: [\"левая\", \"правая\"], получено {}",
                kind_name(value)
            ),
        );
        return None;
    };
    names.try_into().ok()
}

/// Картинка файла модели из ответа страницы.
fn model_image(file: &ModelFile, index: usize) -> Result<Image, String> {
    match file.images.iter().find(|(known, _)| *known == index) {
        Some((
            _,
            ImageVerdict::Ok {
                width,
                height,
                pixels,
            },
        )) => match pixel_problem("картинка", *width, *height, pixels.len()) {
            Some(problem) => Err(problem),
            None => Ok(Image {
                width: *width,
                height: *height,
                pixels: pixels.clone(),
            }),
        },
        Some((_, ImageVerdict::Rejected | ImageVerdict::Video { .. })) => Err(format!(
            "картинка {index} внутри модели не разжалась: исполнитель (браузер) не берётся разжимать этот файл"
        )),
        Some((_, ImageVerdict::Missing)) | None => Err(format!(
            "страница не отдала картинку {index} из файла модели"
        )),
    }
}

/// Все модели игры из ответов страницы `files`, по имени модели, в порядке объявления. Сжатая модель (`plan`)
/// читается из своего облегчённого файла, а её картинки — из сжатых файлов `packed`; остальные — из исходных
/// файлов с картинками, что разжала страница. Модель, чей файл не читается, остаётся пустой заглушкой, а причина
/// записывается ошибкой; без ошибки заглушкой остаётся модель, чьего файла нет, когда файлы не читаются
/// (`skip_media`) — и тогда же нет предупреждения о несжатой модели.
pub(super) fn load_models(
    decls: &[ModelDecl],
    files: &[(String, ModelFile)],
    plan: &ImportedPlan,
    packed: &[(String, Option<Vec<u8>>)],
    skip_media: bool,
    motions: &MotionSet,
    errors: &mut ErrorSink,
) -> ModelTable {
    let mut packed = PackedFiles::new(packed);
    let mut models: Vec<Model> = decls
        .iter()
        .enumerate()
        .map(|(at, decl)| {
            let field = format!("files → models → {}", decl.name);
            let imported = plan.model_path(at);
            let path = imported.unwrap_or(&decl.path);
            let loaded = files
                .iter()
                .find(|(name, _)| name == &decl.name)
                .and_then(|(_, file)| Some((file, file.bytes.as_deref()?)));
            let Some((file, bytes)) = loaded else {
                if !skip_media {
                    errors.push(
                        "game.json",
                        &field,
                        format!(
                            "файл \"{path}\" не найден; ожидалась модель, названная в game.json → files → models"
                        ),
                    );
                }
                return Model::placeholder(&decl.name);
            };
            let parsed = glb::parse_model_images(&decl.name, bytes, |source| match source {
                FileImage::Embedded(index) => model_image(file, index).map(ModelImage::Pixels),
                FileImage::Linked { uri, .. } => {
                    let packed_path = resolve_uri(path, uri);
                    packed
                        .get(&packed_path)
                        .map(ModelImage::Packed)
                        .map_err(|fault| packed_problem(&fault, &packed_path, &decl.name))
                }
            });
            match parsed {
                Ok(mut model) => {
                    attach_motion(decl, &mut model, &field, motions, errors);
                    model.look = decl.look;
                    attach_building(decl, &mut model, &field, errors);
                    if imported.is_none() && !skip_media && !model.images.is_empty() {
                        errors.push_warning(
                            "game.json",
                            &field,
                            format!(
                                "модель {} не сжата: её картинки занимают на видеокарте в 4 раза больше; служба сжатия стенда сожмёт её сама",
                                decl.name
                            ),
                        );
                    }
                    model
                }
                Err(message) => {
                    errors.push("game.json", &field, format!("{path} — {message}"));
                    Model::placeholder(&decl.name)
                }
            }
        })
        .collect();
    for (model, decl) in models.iter_mut().zip(decls) {
        model.building.cutaway = decl
            .cutaway
            .as_ref()
            .and_then(|name| decls.iter().position(|other| &other.name == name));
    }
    ModelTable::new(models)
}

/// «Трёхмерная сцена» → «Здания»: твёрдая модель получает сетку столбов; срез без узлов `inside` включиться не
/// сможет.
fn attach_building(decl: &ModelDecl, model: &mut Model, field: &str, errors: &mut ErrorSink) {
    if decl.solid == Some(true) {
        model.building.solid = Some(Arc::new(SolidGrid::build(&model.parts, model.size)));
    }
    if decl.cutaway.is_some() && model.building.inside.is_empty() {
        errors.push_warning(
            "game.json",
            &join(field, "cutaway"),
            format!(
                "у модели {} есть cutaway, а узлов inside в её файле нет: срез не включится никогда",
                decl.name
            ),
        );
    }
}

/// Ключи скелета из описания `decl` и движения `motions.json` модели ложатся на её скелет, потом по именам из движений
/// файла и `motions.json` встают стойка, ходьба и лазание; чего в файле нет — ошибка.
fn attach_motion(
    decl: &ModelDecl,
    model: &mut Model,
    field: &str,
    motions: &MotionSet,
    errors: &mut ErrorSink,
) {
    let placeholder = model.parts.is_empty();
    let Some(skeleton) = &mut model.skeleton else {
        for (key, named) in [
            ("idle", decl.idle.is_some()),
            ("walk", decl.walk.is_some()),
            ("climb", decl.climb.is_some()),
            ("feet", decl.feet.is_some()),
            ("spine", decl.spine.is_some()),
            ("hands", decl.hands.is_some()),
        ] {
            if named {
                errors.push(
                    "game.json",
                    &join(field, key),
                    format!("в файле модели {} нет скелета", decl.name),
                );
            }
        }
        for motion in motions
            .decls
            .iter()
            .filter(|motion| motion.model == decl.name)
        {
            if !placeholder {
                errors.push(
                    &motions.file,
                    &join(&motion.name, "model"),
                    format!("в файле модели {} нет скелета", decl.name),
                );
            }
        }
        return;
    };
    if let Some(feet) = &decl.feet {
        skeleton.legs = legs_of(skeleton, feet, &join(field, "feet"), errors);
    }
    if let Some(name) = &decl.spine {
        skeleton.spine = bone_named(skeleton, name, &join(field, "spine"), errors);
    }
    if let Some(hands) = &decl.hands {
        skeleton.hands = hands_of(skeleton, hands, &join(field, "hands"), errors);
    }
    motions::attach(skeleton, decl, motions, errors);
    skeleton.idle = named_clip(skeleton, "idle", &decl.idle, field, errors);
    skeleton.walk = named_clip(skeleton, "walk", &decl.walk, field, errors);
    skeleton.climb = named_clip(skeleton, "climb", &decl.climb, field, errors);
}

fn bone_named(
    skeleton: &Skeleton,
    name: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<usize> {
    let found = skeleton.bone_named(name);
    if found.is_none() {
        errors.push(
            "game.json",
            path,
            format!("в скелете модели нет кости \"{name}\""),
        );
    }
    found
}

/// Две кисти и над каждой предплечье и плечо: `arms_out` поворачивает руку от плеча.
fn hands_of(
    skeleton: &Skeleton,
    hands: &[String; 2],
    path: &str,
    errors: &mut ErrorSink,
) -> Option<[usize; 2]> {
    let found = hands
        .clone()
        .map(|name| bone_named(skeleton, &name, path, errors));
    let [Some(left), Some(right)] = found else {
        return None;
    };
    if left == right {
        errors.push(
            "game.json",
            path,
            "левая и правая кисть — одна и та же кость",
        );
        return None;
    }
    let shoulder = |hand: usize| {
        skeleton.bones[hand]
            .parent
            .and_then(|elbow| skeleton.bones[elbow].parent)
    };
    if shoulder(left).is_none() || shoulder(right).is_none() {
        errors.push(
            "game.json",
            path,
            "над костью кисти должны быть предплечье и плечо",
        );
        return None;
    }
    Some([left, right])
}

fn named_clip(
    skeleton: &Skeleton,
    key: &str,
    name: &Option<String>,
    field: &str,
    errors: &mut ErrorSink,
) -> Option<usize> {
    let name = name.as_deref()?;
    let found = skeleton.clip_named(name);
    if found.is_none() {
        let known: Vec<String> = skeleton
            .clips
            .iter()
            .map(|clip| format!("\"{}\"", clip.name))
            .collect();
        errors.push(
            "game.json",
            &join(field, key),
            format!(
                "в файле модели нет движения \"{name}\"; в нём {}",
                if known.is_empty() {
                    "движений нет".to_string()
                } else {
                    known.join(", ")
                }
            ),
        );
    }
    found
}

fn legs_of(
    skeleton: &Skeleton,
    feet: &[String; 2],
    path: &str,
    errors: &mut ErrorSink,
) -> Option<[Leg; 2]> {
    let joints = feet.clone().map(|name| {
        let joint = skeleton.joint_named(&name);
        if joint.is_none() {
            errors.push(
                "game.json",
                path,
                format!("в скелете модели нет кости \"{name}\""),
            );
        }
        joint
    });
    let [Some(left), Some(right)] = joints else {
        return None;
    };
    if left == right {
        errors.push(
            "game.json",
            path,
            "левая и правая ступня — одна и та же кость",
        );
        return None;
    }
    let legs = [left, right].map(|joint| skeleton.leg_of(joint));
    let [Some(left_leg), Some(right_leg)] = legs else {
        errors.push(
            "game.json",
            path,
            "над костью ступни должны быть голень, бедро и таз",
        );
        return None;
    };
    if left_leg.pelvis != right_leg.pelvis {
        errors.push(
            "game.json",
            path,
            format!(
                "бёдра ног растут из разных костей (\"{}\" и \"{}\"): у обеих ног должен быть один таз",
                skeleton.bones[left_leg.pelvis].name, skeleton.bones[right_leg.pelvis].name
            ),
        );
        return None;
    }
    Some([left_leg, right_leg])
}

/// Ошибка требования 17: сжатый файл картинки модели не найден или не читается.
fn packed_problem(fault: &PackedFault, path: &str, model: &str) -> String {
    match fault {
        PackedFault::Missing => format!(
            "сжатая картинка {path} модели {model} не найдена или пуста — служба сжатия пересожмёт её, если исходная модель на месте"
        ),
        PackedFault::Broken(reason) => format!(
            "сжатая картинка {path} модели {model} испорчена — служба сжатия пересожмёт её, если исходная модель на месте. Причина: {reason}"
        ),
    }
}

/// Имена, которые объекты `scene.json` дают текстовому свойству `prop` (`model`, `road_kind`): в значении
/// объекта или в привязке его клавиши или щелчка.
pub(super) fn named_in_scene(objects: &[ParsedObject], prop: PropertyId) -> HashSet<String> {
    let is_named_edit = |edit: &&KeyEdit| edit.property == prop;
    let mut named = HashSet::new();
    for object in objects {
        if let Some(Value::Text(name)) = find_value(&object.values, prop) {
            named.insert(name.clone());
        }
        let bindings = object.keys.iter().flat_map(|keys| keys.values());
        let edits = bindings
            .flat_map(|binding| binding.press.iter().chain(&binding.release))
            .chain(object.on_click.iter().flatten());
        for edit in edits.filter(is_named_edit) {
            if let EditValue::Const(Value::Text(name)) = &edit.value {
                named.insert(name.clone());
            }
        }
    }
    named
}

/// Модель объявлена и нигде не названа — только предупреждение. Названа — значит, её `model` стоит у объекта
/// `scene.json` или в привязке его клавиши (`named`), её имя стоит строкой в `rules.json` или встречается
/// словом в тексте кода.
pub(super) fn warn_unused_models(
    decls: &[ModelDecl],
    named: &HashSet<String>,
    rules: Option<&str>,
    code: Option<&str>,
    errors: &mut ErrorSink,
) {
    for decl in decls {
        let quoted = format!("\"{}\"", decl.name);
        if named.contains(&decl.name)
            || rules.is_some_and(|rules| rules.contains(&quoted))
            || code.is_some_and(|code| code_mentions_word(code, &decl.name))
        {
            continue;
        }
        errors.push_warning(
            "game.json",
            &format!("files → models → {}", decl.name),
            format!(
                "модель \"{}\" объявлена и нигде не названа ни в scene.json, ни в rules.json, ни в тексте кода и его таблицах",
                decl.name
            ),
        );
    }
}

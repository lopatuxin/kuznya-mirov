//! «Трёхмерная сцена» → «Модели и движения»: таблица `files.models` и проверка файлов моделей, которые
//! страница уже прочитала и чьи картинки разжала.

use std::collections::HashSet;

use serde_json::Value as Json;

use crate::core::keys::{EditValue, KeyEdit};
use crate::core::model::{Image, Model, ModelImage, ModelTable};
use crate::core::property::PropertyId;
use crate::core::value::Value;
use crate::data::glb::{self, FileImage};

use super::imported::{ImportedPlan, PackedFault, PackedFiles, resolve_uri};
use super::materials::pixel_problem;
use super::{
    ErrorSink, ImageVerdict, ParsedObject, code_mentions_word, expect_object, expect_string,
    find_value, join, reject_unknown_keys, require_field,
};

/// Ключи описания, которые появятся вместе со скелетом и движениями моделей.
const MOTION_KEYS: [&str; 3] = ["idle", "walk", "feet"];

/// Одна модель `files.models`: имя и путь файла `.glb` от папки игры.
#[derive(Debug, Clone, PartialEq)]
pub struct ModelDecl {
    pub name: String,
    pub path: String,
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
    for key in MOTION_KEYS {
        if obj.contains_key(key) {
            errors.push(
                file,
                &join(path, key),
                format!("{key}: движения моделей появятся в фазе 27"),
            );
        }
    }
    let known: Vec<&str> = ["path"].into_iter().chain(MOTION_KEYS).collect();
    reject_unknown_keys(obj, &known, file, path, errors);
    if !obj.keys().all(|key| known.contains(&key.as_str()))
        || MOTION_KEYS.iter().any(|key| obj.contains_key(*key))
    {
        return None;
    }
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
    })
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
    errors: &mut ErrorSink,
) -> ModelTable {
    let mut packed = PackedFiles::new(packed);
    let models = decls
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
                Ok(model) => {
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
    ModelTable::new(models)
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

//! «Свет и материалы» → «Материал» и «Покрытия рельефа»: таблица `files.materials`, слои `covers`
//! файла рельефа и проверка карт и масок, которые страница уже разжала.

use std::collections::HashSet;
use std::fmt;

use serde::Deserialize;
use serde::de::{Deserializer, IgnoredAny, MapAccess, Visitor};
use serde_json::Value as Json;

use crate::core::terrain::{Cover, MAX_COVERS};

use super::{
    ErrorSink, ImageVerdict, expect_array, expect_number, expect_object, expect_string, join,
    reject_unknown_keys, require_field,
};

/// Стороны квадратной карты материала, которые берёт движок, в точках.
const MAP_SIDES: [u32; 3] = [512, 1024, 2048];

/// Один материал `files.materials`: `size` клеток сцены покрывает карта по ширине, остальное — пути
/// карт от папки игры.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterialDecl {
    pub name: String,
    pub size: f64,
    pub color: String,
    pub normal: String,
    pub roughness: String,
    pub height: String,
    pub ao: Option<String>,
}

impl MaterialDecl {
    /// Ключ и путь каждой карты в порядке `color`, `normal`, `roughness`, `height`, `ao`; `ao` — если
    /// задана.
    pub fn maps(&self) -> impl Iterator<Item = (&'static str, &str)> {
        [
            ("color", &self.color),
            ("normal", &self.normal),
            ("roughness", &self.roughness),
            ("height", &self.height),
        ]
        .into_iter()
        .chain(self.ao.as_ref().map(|path| ("ao", path)))
        .map(|(key, path)| (key, path.as_str()))
    }
}

/// Ключи объекта в том порядке, в каком они записаны: `serde_json::Map` держит их по алфавиту.
struct KeyOrder(Vec<String>);

impl<'de> Deserialize<'de> for KeyOrder {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<KeyOrder, D::Error> {
        struct Keys;

        impl<'de> Visitor<'de> for Keys {
            type Value = KeyOrder;

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("объект")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<KeyOrder, A::Error> {
                let mut keys = Vec::new();
                while let Some((key, IgnoredAny)) = map.next_entry::<String, IgnoredAny>()? {
                    keys.push(key);
                }
                Ok(KeyOrder(keys))
            }
        }

        deserializer.deserialize_map(Keys)
    }
}

/// Имена материалов `files.materials` в порядке записи в `game.json`; порядок — номер материала.
/// Пусто, если таблицы нет.
pub(super) fn declaration_order(game_json: &str) -> Vec<String> {
    #[derive(Deserialize)]
    struct Files {
        materials: Option<KeyOrder>,
    }
    #[derive(Deserialize)]
    struct Root {
        files: Option<Files>,
    }
    serde_json::from_str::<Root>(game_json)
        .ok()
        .and_then(|root| root.files?.materials)
        .map(|order| order.0)
        .unwrap_or_default()
}

fn has_extension(path: &str, extensions: &[&str]) -> bool {
    let lower = path.to_ascii_lowercase();
    extensions
        .iter()
        .any(|extension| lower.ends_with(extension))
}

/// `files.materials`: таблица «имя → описание»; порядок объявления `order` — номер материала.
/// Описание с ошибкой пропускается, его ошибки уже записаны.
pub(super) fn parse_materials_table(
    value: &Json,
    order: &[String],
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Vec<MaterialDecl>> {
    let Some(obj) = value.as_object() else {
        errors.push(
            file,
            path,
            "files.materials — ожидалась таблица \"имя → описание\", как files.images",
        );
        return None;
    };
    let mut out = Vec::with_capacity(obj.len());
    for (name, decl_json) in obj {
        if name.is_empty() {
            errors.push(
                file,
                path,
                "files.materials → пустое имя: у материала должно быть имя, которым его назовёт слой covers",
            );
            continue;
        }
        if let Some(decl) = parse_material(name, decl_json, file, &join(path, name), errors) {
            out.push(decl);
        }
    }
    out.sort_by_key(|decl| order.iter().position(|name| name == &decl.name));
    Some(out)
}

fn parse_material(
    name: &str,
    value: &Json,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<MaterialDecl> {
    let obj = expect_object(value, file, path, errors)?;
    reject_unknown_keys(
        obj,
        &["size", "color", "normal", "roughness", "height", "ao"],
        file,
        path,
        errors,
    );
    let size = require_field(obj, "size", file, path, errors)
        .and_then(|v| expect_number(v, file, &join(path, "size"), errors))
        .filter(|&n| {
            let positive = n > 0.0;
            if !positive {
                errors.push(
                    file,
                    &join(path, "size"),
                    format!("size должен быть числом больше нуля, получено {n}"),
                );
            }
            positive
        });
    let mut map = |key: &str, required: bool| parse_map(obj, key, required, file, path, errors);
    let color = map("color", true);
    let normal = map("normal", true);
    let roughness = map("roughness", true);
    let height = map("height", true);
    let ao = map("ao", false);
    Some(MaterialDecl {
        name: name.to_string(),
        size: size?,
        color: color.flatten()?,
        normal: normal.flatten()?,
        roughness: roughness.flatten()?,
        height: height.flatten()?,
        ao: ao?,
    })
}

/// Путь одной карты: `Some(None)` — необязательной карты нет, `None` — путь с ошибкой.
fn parse_map(
    obj: &serde_json::Map<String, Json>,
    key: &str,
    required: bool,
    file: &str,
    path: &str,
    errors: &mut ErrorSink,
) -> Option<Option<String>> {
    let value = if required {
        Some(require_field(obj, key, file, path, errors)?)
    } else {
        obj.get(key)
    };
    let Some(value) = value else {
        return Some(None);
    };
    let field = join(path, key);
    let text = expect_string(value, file, &field, errors)?;
    if !has_extension(&text, &[".png", ".jpg"]) {
        errors.push(
            file,
            &field,
            format!(
                "{text} — карта материала берётся из PNG или JPEG; путь должен оканчиваться на \".png\" или \".jpg\""
            ),
        );
        return None;
    }
    Some(Some(text))
}

/// Картинки файла рельефа, которые страница должна прочитать: маски покрытий в порядке слоёв, за
/// ними карта цвета `tint`, если она названа. Файл, который не читается, и `covers`, которых в нём
/// нет, дают пустой список масок: настоящая ошибка придёт из `load`.
pub fn terrain_image_paths(terrain_json: &str) -> Vec<String> {
    let Ok(root) = serde_json::from_str::<Json>(terrain_json) else {
        return Vec::new();
    };
    let mut paths: Vec<String> = root
        .get("covers")
        .and_then(Json::as_array)
        .map(|layers| {
            layers
                .iter()
                .filter_map(|layer| layer.get("mask")?.as_str())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    if let Some(tint) = root.get("tint").and_then(Json::as_str) {
        paths.push(tint.to_string());
    }
    paths
}

/// `covers` файла рельефа: слои снизу вверх. `masks` — что страница ответила по каждой маске;
/// `None` — файлы масок не читаются и не проверяются.
pub(super) fn parse_covers(
    value: &Json,
    file: &str,
    materials: &[MaterialDecl],
    masks: Option<&[(String, ImageVerdict)]>,
    errors: &mut ErrorSink,
) -> Option<Vec<Cover>> {
    let layers = expect_array(value, file, "covers", errors)?;
    if layers.is_empty() {
        errors.push(
            file,
            "covers",
            "covers пуст: нужен хотя бы один слой — нижний, он лежит на всём рельефе",
        );
        return None;
    }
    let mut sound = layers.len() <= MAX_COVERS;
    if !sound {
        errors.push(
            file,
            "covers",
            format!(
                "covers: {} слоёв, а больше {MAX_COVERS} не бывает",
                layers.len()
            ),
        );
    }
    let mut covers: Vec<Cover> = Vec::with_capacity(layers.len());
    let mut mask_count = 0;
    for (index, layer) in layers.iter().enumerate() {
        match parse_cover(index, layer, file, materials, masks, mask_count, errors) {
            Some(cover) => {
                mask_count += usize::from(cover.mask.is_some());
                covers.push(cover);
            }
            None => sound = false,
        }
    }
    sound.then_some(covers)
}

/// Слой `index`; `mask_index` — сколько масок уже названо ниже.
fn parse_cover(
    index: usize,
    value: &Json,
    file: &str,
    materials: &[MaterialDecl],
    masks: Option<&[(String, ImageVerdict)]>,
    mask_index: usize,
    errors: &mut ErrorSink,
) -> Option<Cover> {
    let path = format!("covers[{index}]");
    let obj = expect_object(value, file, &path, errors)?;
    reject_unknown_keys(obj, &["material", "mask"], file, &path, errors);
    let material = require_field(obj, "material", file, &path, errors)
        .and_then(|v| expect_string(v, file, &join(&path, "material"), errors))
        .and_then(|name| {
            let found = materials.iter().position(|m| m.name == name);
            if found.is_none() {
                errors.push(
                    file,
                    &join(&path, "material"),
                    format!("материал \"{name}\" не объявлен в game.json → files → materials"),
                );
            }
            found
        });
    let mask_path = join(&path, "mask");
    let mask = match (index, obj.get("mask")) {
        (0, None) => Some(None),
        (0, Some(_)) => {
            errors.push(
                file,
                &mask_path,
                "у нижнего слоя маски нет: он лежит на всём рельефе",
            );
            None
        }
        (_, None) => {
            errors.push(
                file,
                &path,
                "слой поверх нижнего без маски: отсутствует обязательная настройка \"mask\"",
            );
            None
        }
        (_, Some(v)) => parse_mask(v, file, &mask_path, masks, errors).map(|()| Some(mask_index)),
    };
    Some(Cover {
        material: material?,
        mask: mask?,
    })
}

fn parse_mask(
    value: &Json,
    file: &str,
    path: &str,
    masks: Option<&[(String, ImageVerdict)]>,
    errors: &mut ErrorSink,
) -> Option<()> {
    parse_terrain_image(
        value,
        file,
        path,
        "маска",
        "серая PNG-картинка маски, названная в covers",
        masks,
        errors,
    )
}

/// «Свет и материалы» → «Карта цвета»: `tint` файла рельефа — PNG, который страница прочитала и
/// разжала. `masks` — ответы страницы по картинкам рельефа; `None` — файлы не проверяются.
pub(super) fn parse_tint(
    value: &Json,
    file: &str,
    masks: Option<&[(String, ImageVerdict)]>,
    errors: &mut ErrorSink,
) -> Option<()> {
    parse_terrain_image(
        value,
        file,
        "tint",
        "карта цвета",
        "PNG-картинка карты цвета, названная в tint",
        masks,
        errors,
    )
}

/// Картинка файла рельефа — маска или карта цвета: путь на `.png`, и страница этот файл нашла и
/// разжала. `what` — чем картинка зовётся в тексте ошибки, `expected` — что ожидалось на месте
/// ненайденного файла.
fn parse_terrain_image(
    value: &Json,
    file: &str,
    path: &str,
    what: &str,
    expected: &str,
    masks: Option<&[(String, ImageVerdict)]>,
    errors: &mut ErrorSink,
) -> Option<()> {
    let text = expect_string(value, file, path, errors)?;
    if !has_extension(&text, &[".png"]) {
        errors.push(
            file,
            path,
            format!("{text} — {what} берётся только из PNG; путь должен оканчиваться на \".png\""),
        );
        return None;
    }
    let Some(verdicts) = masks else {
        return Some(());
    };
    let verdict = verdicts.iter().find(|(p, _)| p == &text).map(|(_, v)| v);
    let problem = match verdict {
        Some(ImageVerdict::Ok {
            width,
            height,
            pixels,
        }) => pixel_problem(&text, *width, *height, pixels.len()),
        Some(ImageVerdict::Rejected) => Some(format!(
            "{text} — исполнитель (браузер) не берётся разжимать этот файл"
        )),
        Some(ImageVerdict::Missing) | None => {
            Some(format!("файл \"{text}\" не найден; ожидалась {expected}"))
        }
    };
    if let Some(message) = problem {
        errors.push(file, path, message);
        return None;
    }
    Some(())
}

/// Что не так с точками, которые страница отдала вместо файла `path`.
fn pixel_problem(path: &str, width: u32, height: u32, bytes: usize) -> Option<String> {
    if width == 0 || height == 0 {
        return Some(format!(
            "{path} — ширина и высота картинки должны быть больше нуля, получено {width}×{height}"
        ));
    }
    let expected = width as usize * height as usize * 4;
    (bytes != expected).then(|| {
        format!(
            "{path} — страница отдала {bytes} байт точек, ожидалось {expected} ({width}×{height}×4)"
        )
    })
}

/// Все карты всех материалов: файл нашёлся и разжат, карта — квадрат 512, 1024 или 2048 точек, и
/// все карты игры одного размера.
pub(super) fn validate_material_files(
    materials: &[MaterialDecl],
    data: &[(String, ImageVerdict)],
    errors: &mut ErrorSink,
) {
    let mut reference: Option<(u32, &str)> = None;
    for material in materials {
        for (key, path) in material.maps() {
            let field = format!("files → materials → {} → {key}", material.name);
            let verdict = data.iter().find(|(p, _)| p == path).map(|(_, v)| v);
            let message = match verdict {
                Some(ImageVerdict::Ok {
                    width,
                    height,
                    pixels,
                }) => pixel_problem(path, *width, *height, pixels.len())
                    .or_else(|| side_problem(path, *width, *height, &mut reference)),
                Some(ImageVerdict::Rejected) => Some(format!(
                    "{path} — исполнитель (браузер) не берётся разжимать этот файл"
                )),
                Some(ImageVerdict::Missing) | None => Some(format!(
                    "файл \"{path}\" не найден; ожидалась карта материала, названная в game.json → files → materials"
                )),
            };
            if let Some(message) = message {
                errors.push("game.json", &field, message);
            }
        }
    }
}

/// Карта — квадрат допустимой стороны и той же, что первая карта игры.
fn side_problem<'a>(
    path: &'a str,
    width: u32,
    height: u32,
    reference: &mut Option<(u32, &'a str)>,
) -> Option<String> {
    if width != height || !MAP_SIDES.contains(&width) {
        return Some(format!(
            "{path} — карта {width}×{height}: нужен квадрат 512, 1024 или 2048 точек"
        ));
    }
    match *reference {
        Some((side, first)) if side != width => Some(format!(
            "{path} — карта {width}×{height}, а карта {first} — {side}×{side}: все карты материалов игры одного размера"
        )),
        Some(_) => None,
        None => {
            *reference = Some((width, path));
            None
        }
    }
}

/// Материал объявлен и не назван ни в одном слое `covers` — только предупреждение.
pub(super) fn warn_unused_materials(
    materials: &[MaterialDecl],
    covers: &[Cover],
    errors: &mut ErrorSink,
) {
    let used: HashSet<usize> = covers.iter().map(|cover| cover.material).collect();
    for (index, material) in materials.iter().enumerate() {
        if !used.contains(&index) {
            errors.push_warning(
                "game.json",
                &format!("files → materials → {}", material.name),
                format!(
                    "материал \"{}\" объявлен и не назван ни в одном слое covers файла рельефа",
                    material.name
                ),
            );
        }
    }
}

//! Модель `.glb` → облегчённая модель без байтов картинок и список картинок, которые надо сжать.

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result, anyhow, bail};
use gltf::Glb;
use gltf::binary::Header;
use serde_json::{Value, json};

use crate::texture::{self, Encoding, Role};

const COMPRESSIONS: [&str; 2] = ["KHR_draco_mesh_compression", "EXT_meshopt_compression"];

/// Картинка для сжатия: имя файла без расширения, роль и исходные байты.
pub struct TextureJob {
    pub stem: String,
    pub role: Role,
    pub encoding: Encoding,
    pub source: Vec<u8>,
}

/// `textures` идёт по картинкам облегчённой модели: `None` — на картинку не ссылается ни один материал.
pub struct Lightened {
    pub glb: Vec<u8>,
    pub textures: Vec<Option<TextureJob>>,
}

#[derive(Clone, Copy)]
enum Place {
    BaseColor,
    MetalRough,
    Normal,
    Occlusion,
}

impl Place {
    const ALL: [Place; 4] = [
        Place::BaseColor,
        Place::MetalRough,
        Place::Normal,
        Place::Occlusion,
    ];

    fn role(self) -> Role {
        match self {
            Place::BaseColor => Role::Color,
            Place::MetalRough | Place::Occlusion => Role::Data,
            Place::Normal => Role::Normal,
        }
    }

    fn pointer(self, material: usize) -> String {
        let tail = match self {
            Place::BaseColor => "pbrMetallicRoughness/baseColorTexture",
            Place::MetalRough => "pbrMetallicRoughness/metallicRoughnessTexture",
            Place::Normal => "normalTexture",
            Place::Occlusion => "occlusionTexture",
        };
        format!("/materials/{material}/{tail}/index")
    }
}

struct Slot {
    pointer: String,
    texture: usize,
    role: Role,
}

/// Облегчённая модель `bytes`, которая ляжет в `imported/<light_path>`; её картинки названы относительным
/// `uri` на `imported/textures/`.
pub fn lighten(bytes: &[u8], light_path: &str) -> Result<Lightened> {
    let glb = Glb::from_slice(bytes).context("не читается как glTF одним файлом")?;
    let mut root: Value = serde_json::from_slice(&glb.json).context("JSON модели не читается")?;
    reject_compression(&root)?;
    let bin: &[u8] = glb.bin.as_deref().unwrap_or(&[]);
    check_buffers(&root)?;

    let mut images = array(&root, "images");
    let mut textures = array(&root, "textures");
    let image_views: Vec<Option<usize>> = images
        .iter()
        .map(|image| index(image.get("bufferView")))
        .collect();

    let mut jobs: Vec<Option<TextureJob>> = (0..images.len()).map(|_| None).collect();
    let mut taken = vec![false; images.len()];
    let mut owner: HashMap<(usize, Role), usize> = HashMap::new();
    let mut copies: HashMap<(usize, usize), usize> = HashMap::new();
    for slot in texture_slots(&root)? {
        let source = textures
            .get(slot.texture)
            .ok_or_else(|| anyhow!("материал называет несуществующую текстуру {}", slot.texture))?
            .get("source")
            .and_then(Value::as_u64)
            .ok_or_else(|| anyhow!("у текстуры {} нет картинки", slot.texture))?
            as usize;
        if source >= images.len() {
            bail!(
                "текстура {} называет несуществующую картинку {source}",
                slot.texture
            );
        }
        let light_image = match owner.get(&(source, slot.role)) {
            Some(&light_image) => light_image,
            None => {
                let light_image = if taken[source] {
                    images.push(images[source].clone());
                    jobs.push(None);
                    images.len() - 1
                } else {
                    taken[source] = true;
                    source
                };
                jobs[light_image] = Some(job(&images[source], bin, &root, source, slot.role)?);
                owner.insert((source, slot.role), light_image);
                light_image
            }
        };
        if light_image != source {
            let copy = *copies
                .entry((slot.texture, light_image))
                .or_insert_with(|| {
                    let mut copy = textures[slot.texture].clone();
                    copy["source"] = json!(light_image);
                    textures.push(copy);
                    textures.len() - 1
                });
            *root
                .pointer_mut(&slot.pointer)
                .ok_or_else(|| anyhow!("материал потерял ссылку на текстуру"))? = json!(copy);
        }
    }

    let uri_up = "../".repeat(light_path.matches('/').count());
    for (image, job) in images.iter_mut().zip(&jobs) {
        let fields = image.as_object_mut().context("картинка не объект")?;
        fields.remove("bufferView");
        match job {
            Some(job) => {
                fields.insert(
                    "uri".into(),
                    json!(format!("{uri_up}textures/{}.ktx2", job.stem)),
                );
                fields.insert("mimeType".into(), json!("image/ktx2"));
            }
            None => {
                fields.insert("uri".into(), json!(""));
                fields.remove("mimeType");
            }
        }
    }
    let new_bin = drop_image_views(&mut root, bin, &image_views)?;
    if !images.is_empty() {
        root["images"] = Value::Array(images);
    }
    if !textures.is_empty() {
        root["textures"] = Value::Array(textures);
    }

    let light = Glb {
        header: Header {
            magic: *b"glTF",
            version: 2,
            length: 0,
        },
        json: Cow::Owned(serde_json::to_vec(&root)?),
        bin: new_bin.map(Cow::Owned),
    };
    Ok(Lightened {
        glb: light
            .to_vec()
            .context("облегчённая модель не записывается")?,
        textures: jobs,
    })
}

fn array(root: &Value, key: &str) -> Vec<Value> {
    root.get(key)
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn index(value: Option<&Value>) -> Option<usize> {
    value.and_then(Value::as_u64).map(|number| number as usize)
}

fn reject_compression(root: &Value) -> Result<()> {
    for list in ["extensionsUsed", "extensionsRequired"] {
        let names = root.get(list).and_then(Value::as_array);
        let found = COMPRESSIONS
            .into_iter()
            .find(|name| names.is_some_and(|names| names.iter().any(|n| n.as_str() == Some(name))));
        if let Some(name) = found {
            bail!("сжатие {name} движок не читает; выгрузите модель без сжатия");
        }
    }
    Ok(())
}

fn check_buffers(root: &Value) -> Result<()> {
    let buffers = array(root, "buffers");
    if buffers.len() > 1 || buffers.iter().any(|buffer| buffer.get("uri").is_some()) {
        bail!("буфер лежит вне файла: нужен glTF одним файлом");
    }
    if array(root, "bufferViews")
        .iter()
        .any(|view| index(view.get("buffer")) != Some(0))
    {
        bail!("кусок буфера ссылается не на буфер файла");
    }
    Ok(())
}

fn texture_slots(root: &Value) -> Result<Vec<Slot>> {
    let mut slots = Vec::new();
    for material in 0..array(root, "materials").len() {
        for place in Place::ALL {
            let pointer = place.pointer(material);
            let Some(value) = root.pointer(&pointer) else {
                continue;
            };
            let texture = index(Some(value))
                .ok_or_else(|| anyhow!("материал {material}: номер текстуры не число"))?;
            slots.push(Slot {
                pointer,
                texture,
                role: place.role(),
            });
        }
    }
    Ok(slots)
}

fn job(image: &Value, bin: &[u8], root: &Value, number: usize, role: Role) -> Result<TextureJob> {
    let mime = image.get("mimeType").and_then(Value::as_str).unwrap_or("");
    let encoding = Encoding::from_mime(mime)
        .ok_or_else(|| anyhow!("картинка {number}: формат {mime:?} — не PNG и не JPEG"))?;
    let view = index(image.get("bufferView"))
        .ok_or_else(|| anyhow!("картинка {number} лежит вне файла"))?;
    let source = view_bytes(root, bin, view)?.to_vec();
    Ok(TextureJob {
        stem: texture::file_stem(&source, role),
        role,
        encoding,
        source,
    })
}

fn view_bytes<'a>(root: &Value, bin: &'a [u8], view: usize) -> Result<&'a [u8]> {
    let view = root
        .pointer(&format!("/bufferViews/{view}"))
        .ok_or_else(|| anyhow!("нет куска буфера {view}"))?;
    let offset = index(view.get("byteOffset")).unwrap_or(0);
    let length = index(view.get("byteLength")).context("у куска буфера нет длины")?;
    offset
        .checked_add(length)
        .and_then(|end| bin.get(offset..end))
        .context("кусок буфера выходит за буфер файла")
}

/// Убирает из буфера куски картинок, на которые не ссылается ни одна сетка; остальные куски ложатся подряд
/// по границе четырёх байт, номера кусков в `accessors` пересчитываются.
fn drop_image_views(
    root: &mut Value,
    bin: &[u8],
    image_views: &[Option<usize>],
) -> Result<Option<Vec<u8>>> {
    let views = array(root, "bufferViews");
    let mut accessors = array(root, "accessors");
    let used: HashSet<usize> = accessors
        .iter()
        .flat_map(|accessor| {
            [
                accessor.get("bufferView"),
                accessor.pointer("/sparse/indices/bufferView"),
                accessor.pointer("/sparse/values/bufferView"),
            ]
        })
        .filter_map(index)
        .collect();
    let dropped: HashSet<usize> = image_views
        .iter()
        .flatten()
        .copied()
        .filter(|view| !used.contains(view))
        .collect();

    let mut new_bin = Vec::new();
    let mut new_views = Vec::new();
    let mut renumbered: HashMap<usize, usize> = HashMap::new();
    for (old, view) in views.iter().enumerate() {
        if dropped.contains(&old) {
            continue;
        }
        new_bin.resize(new_bin.len().next_multiple_of(4), 0);
        let mut view = view.clone();
        view["byteOffset"] = json!(new_bin.len());
        new_bin.extend_from_slice(view_bytes(root, bin, old)?);
        renumbered.insert(old, new_views.len());
        new_views.push(view);
    }
    for accessor in &mut accessors {
        for pointer in [
            "/bufferView",
            "/sparse/indices/bufferView",
            "/sparse/values/bufferView",
        ] {
            if let Some(slot) = accessor.pointer_mut(pointer) {
                let old = index(Some(slot)).context("номер куска буфера не число")?;
                *slot = json!(
                    renumbered
                        .get(&old)
                        .context("accessor ссылается на убранный кусок буфера")?
                );
            }
        }
    }

    let object = root.as_object_mut().context("модель не объект")?;
    if !accessors.is_empty() {
        object.insert("accessors".into(), Value::Array(accessors));
    }
    if new_views.is_empty() {
        object.remove("bufferViews");
        object.remove("buffers");
        return Ok(None);
    }
    object.insert("bufferViews".into(), Value::Array(new_views));
    object.insert("buffers".into(), json!([{ "byteLength": new_bin.len() }]));
    Ok(Some(new_bin))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{POSITIONS, Spec, material, png};

    fn images(count: u8) -> Vec<Vec<u8>> {
        (0..count).map(|shade| png(8, 8, shade)).collect()
    }

    fn positions_of(glb: &[u8]) -> Vec<[f32; 3]> {
        let document = gltf::Gltf::from_slice(glb).expect("light model is glTF");
        let primitive = document
            .meshes()
            .next()
            .and_then(|mesh| mesh.primitives().next())
            .expect("a primitive");
        let blob = document.blob.as_deref();
        primitive
            .reader(|_| blob)
            .read_positions()
            .expect("positions")
            .collect()
    }

    fn root_of(glb: &[u8]) -> Value {
        serde_json::from_slice(&Glb::from_slice(glb).expect("glb").json).expect("json")
    }

    #[test]
    fn light_model_is_gltf_without_picture_bytes_and_with_the_same_mesh() {
        let spec = Spec::new(
            images(2),
            vec![0, 1],
            material(Some(0), Some(1), None, None),
        );
        let source = spec.build();
        let light = lighten(&source, "models/a.glb").unwrap();

        assert!(!light.glb.windows(4).any(|window| window == b"\x89PNG"));
        assert!(light.glb.len() < source.len());
        assert_eq!(positions_of(&light.glb), POSITIONS);
        let root = root_of(&light.glb);
        assert_eq!(root["materials"], spec.materials);
        assert_eq!(root["images"].as_array().unwrap().len(), 2);
        for image in root["images"].as_array().unwrap() {
            assert_eq!(image["mimeType"], "image/ktx2");
            assert!(image.get("bufferView").is_none());
            assert!(image["uri"].as_str().unwrap().starts_with("../textures/"));
        }
    }

    #[test]
    fn uri_climbs_out_of_every_folder_of_the_model() {
        let spec = Spec::new(images(1), vec![0], material(Some(0), None, None, None));
        let light = lighten(&spec.build(), "models/deep/er/a.glb").unwrap();
        let uri = root_of(&light.glb)["images"][0]["uri"]
            .as_str()
            .unwrap()
            .to_string();
        assert!(
            uri.starts_with("../../../textures/") && uri.ends_with(".ktx2"),
            "{uri}"
        );
    }

    #[test]
    fn picture_in_two_roles_becomes_two_images_and_two_textures() {
        let spec = Spec::new(images(1), vec![0], material(Some(0), None, Some(0), None));
        let light = lighten(&spec.build(), "models/a.glb").unwrap();
        let roles: Vec<Role> = light
            .textures
            .iter()
            .flatten()
            .map(|job| job.role)
            .collect();
        assert_eq!(roles, [Role::Color, Role::Normal]);
        assert_ne!(
            light.textures[0].as_ref().unwrap().stem,
            light.textures[1].as_ref().unwrap().stem
        );
        let root = root_of(&light.glb);
        let base = root["materials"][0]["pbrMetallicRoughness"]["baseColorTexture"]["index"]
            .as_u64()
            .unwrap();
        let normal = root["materials"][0]["normalTexture"]["index"]
            .as_u64()
            .unwrap();
        assert_ne!(base, normal);
        assert_eq!(root["textures"][base as usize]["source"], 0);
        assert_eq!(root["textures"][normal as usize]["source"], 1);
    }

    #[test]
    fn roughness_and_occlusion_share_one_file() {
        let spec = Spec::new(images(1), vec![0], material(None, Some(0), None, Some(0)));
        let light = lighten(&spec.build(), "models/a.glb").unwrap();
        assert_eq!(light.textures.len(), 1);
        assert_eq!(light.textures[0].as_ref().unwrap().role, Role::Data);
    }

    #[test]
    fn picture_no_material_uses_is_not_compressed() {
        let spec = Spec::new(images(2), vec![0, 1], material(Some(1), None, None, None));
        let light = lighten(&spec.build(), "models/a.glb").unwrap();
        assert!(light.textures[0].is_none() && light.textures[1].is_some());
        assert_eq!(root_of(&light.glb)["images"][0]["uri"], "");
    }

    #[test]
    fn model_without_pictures_keeps_its_mesh() {
        let spec = Spec::new(Vec::new(), Vec::new(), Value::Null);
        let light = lighten(&spec.build(), "models/a.glb").unwrap();
        assert!(light.textures.is_empty());
        assert_eq!(positions_of(&light.glb), POSITIONS);
    }

    #[test]
    fn draco_picture_format_and_external_buffer_are_refused() {
        let mut draco = Spec::new(Vec::new(), Vec::new(), Value::Null);
        draco.extensions_used = vec!["KHR_draco_mesh_compression"];
        assert!(lighten(&draco.build(), "models/a.glb").is_err());

        let mut webp = Spec::new(images(1), vec![0], material(Some(0), None, None, None));
        webp.mime = "image/webp";
        assert!(lighten(&webp.build(), "models/a.glb").is_err());

        let external = Spec::new(Vec::new(), Vec::new(), Value::Null).build();
        let mut root = root_of(&external);
        root["buffers"][0]["uri"] = json!("a.bin");
        let glb = Glb {
            header: Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: Cow::Owned(serde_json::to_vec(&root).unwrap()),
            bin: None,
        };
        assert!(lighten(&glb.to_vec().unwrap(), "models/a.glb").is_err());
        assert!(lighten(b"this is not a model", "models/a.glb").is_err());
    }
}

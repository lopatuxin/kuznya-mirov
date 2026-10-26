//! Помощники тестов: маленькие `.glb`, картинки и временные папки.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

use image::codecs::png::PngEncoder;
use image::{ExtendedColorType, ImageEncoder};
use serde_json::{Value, json};

pub fn png(width: u32, height: u32, shade: u8) -> Vec<u8> {
    let mut raw = Vec::new();
    for y in 0..height {
        for x in 0..width {
            raw.extend_from_slice(&[shade, (x * 7) as u8, (y * 9) as u8, 255]);
        }
    }
    let mut out = Vec::new();
    PngEncoder::new(&mut out)
        .write_image(&raw, width, height, ExtendedColorType::Rgba8)
        .expect("png encodes");
    out
}

pub const POSITIONS: [[f32; 3]; 3] = [[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]];

/// Модель из одного треугольника. Куски картинок идут в буфере раньше вершин, чтобы после их удаления номер
/// куска вершин менялся.
pub struct Spec {
    pub images: Vec<Vec<u8>>,
    pub mime: &'static str,
    pub textures: Vec<usize>,
    pub materials: Value,
    pub extensions_used: Vec<&'static str>,
}

impl Spec {
    pub fn new(images: Vec<Vec<u8>>, textures: Vec<usize>, materials: Value) -> Spec {
        Spec {
            images,
            mime: "image/png",
            textures,
            materials,
            extensions_used: Vec::new(),
        }
    }

    pub fn build(&self) -> Vec<u8> {
        let mut bin = Vec::new();
        let mut views = Vec::new();
        for image in &self.images {
            views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": image.len()}));
            bin.extend_from_slice(image);
            bin.resize(bin.len().next_multiple_of(4), 0);
        }
        views.push(json!({"buffer": 0, "byteOffset": bin.len(), "byteLength": 36}));
        for position in POSITIONS {
            for coordinate in position {
                bin.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        let mut root = json!({
            "asset": {"version": "2.0"},
            "scene": 0,
            "scenes": [{"nodes": [0]}],
            "nodes": [{"mesh": 0}],
            "meshes": [{"primitives": [{"attributes": {"POSITION": 0}}]}],
            "accessors": [{
                "bufferView": self.images.len(),
                "componentType": 5126,
                "count": 3,
                "type": "VEC3",
                "min": [0.0, 0.0, 0.0],
                "max": [1.0, 1.0, 0.0],
            }],
            "bufferViews": views,
            "buffers": [{"byteLength": bin.len()}],
        });
        if !self.images.is_empty() {
            root["images"] = Value::Array(
                (0..self.images.len())
                    .map(|index| json!({"bufferView": index, "mimeType": self.mime}))
                    .collect(),
            );
            root["textures"] = Value::Array(
                self.textures
                    .iter()
                    .map(|&source| json!({"source": source}))
                    .collect(),
            );
        }
        if !self.materials.is_null() {
            root["materials"] = self.materials.clone();
            root["meshes"][0]["primitives"][0]["material"] = json!(0);
        }
        if !self.extensions_used.is_empty() {
            root["extensionsUsed"] = json!(self.extensions_used);
        }
        gltf::Glb {
            header: gltf::binary::Header {
                magic: *b"glTF",
                version: 2,
                length: 0,
            },
            json: std::borrow::Cow::Owned(serde_json::to_vec(&root).expect("json")),
            bin: Some(std::borrow::Cow::Owned(bin)),
        }
        .to_vec()
        .expect("glb")
    }
}

/// Материал с картинками по ролям: цвет, шероховатость, мелкий рельеф, затенение (номера текстур).
pub fn material(
    color: Option<usize>,
    rough: Option<usize>,
    normal: Option<usize>,
    occlusion: Option<usize>,
) -> Value {
    let mut pbr = json!({});
    if let Some(index) = color {
        pbr["baseColorTexture"] = json!({"index": index});
    }
    if let Some(index) = rough {
        pbr["metallicRoughnessTexture"] = json!({"index": index});
    }
    let mut material = json!({"pbrMetallicRoughness": pbr});
    if let Some(index) = normal {
        material["normalTexture"] = json!({"index": index});
    }
    if let Some(index) = occlusion {
        material["occlusionTexture"] = json!({"index": index});
    }
    json!([material])
}

pub struct TempDir(pub PathBuf);

impl TempDir {
    pub fn new() -> TempDir {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "importer-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).expect("temp dir");
        TempDir(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

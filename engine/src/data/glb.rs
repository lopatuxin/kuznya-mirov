//! «Трёхмерная сцена» → «Модели и движения», «Загрузка и проверка»: разбор модели `.glb` в `core::model`.
//! Картинки внутри файла сюда приходят уже разжатыми страницей; узлы с их сдвигами, поворотами и
//! масштабами собираются в одно тело, сетки с одним материалом — в одну часть, оси файла ложатся в оси
//! сцены (+X вправо, +Y вверх → вверх, +Z к игроку → вниз по сцене). Скелеты, движения и расширения
//! `KHR_materials_*` и `KHR_texture_transform` игнорируются: модель рисуется в покое.

use std::collections::HashMap;

use gltf::buffer::Source as BufferSource;
use gltf::image::Source as ImageSource;
use gltf::mesh::Mode;
use gltf::texture::{MagFilter, MinFilter, WrappingMode};
use gltf::{Gltf, Node};
use serde_json::Value as Json;

use crate::core::model::{
    AlphaMode, DEFAULT_ALPHA_CUTOFF, Image, Material, Model, ModelVertex, Part, Sampler, Texture,
    Wrap,
};

type Vec3 = [f32; 3];
/// Матрица 4 × 4 по столбцам, как в glTF.
type Mat4 = [[f32; 4]; 4];

/// Сжатие сеток, которого движок не читает.
const COMPRESSIONS: [&str; 2] = ["KHR_draco_mesh_compression", "EXT_meshopt_compression"];

/// Склеивает матрицы: сначала `inner`, затем `outer`.
fn multiply(outer: &Mat4, inner: &Mat4) -> Mat4 {
    let mut out = [[0.0; 4]; 4];
    for (column, out_column) in out.iter_mut().enumerate() {
        for (row, cell) in out_column.iter_mut().enumerate() {
            *cell = (0..4).map(|k| outer[k][row] * inner[column][k]).sum();
        }
    }
    out
}

fn transform_point(m: &Mat4, p: Vec3) -> Vec3 {
    std::array::from_fn(|row| m[0][row] * p[0] + m[1][row] * p[1] + m[2][row] * p[2] + m[3][row])
}

fn transform_direction(m: &Mat4, v: Vec3) -> Vec3 {
    std::array::from_fn(|row| m[0][row] * v[0] + m[1][row] * v[1] + m[2][row] * v[2])
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn normalized(v: Vec3) -> Vec3 {
    let length = dot(v, v).sqrt();
    if length > 1e-20 {
        v.map(|c| c / length)
    } else {
        [0.0, 0.0, 1.0]
    }
}

fn xyz(column: &[f32; 4]) -> Vec3 {
    [column[0], column[1], column[2]]
}

/// Определитель верхней матрицы 3 × 3: отрицательный у зеркального узла.
fn determinant(m: &Mat4) -> f32 {
    dot(xyz(&m[0]), cross(xyz(&m[1]), xyz(&m[2])))
}

/// Нормаль в мире по нормали в узле: обратная транспонированная матрица, с учётом знака определителя.
fn transform_normal(m: &Mat4, determinant: f32, n: Vec3) -> Vec3 {
    let (c0, c1, c2) = (xyz(&m[0]), xyz(&m[1]), xyz(&m[2]));
    let (a, b, c) = (cross(c1, c2), cross(c2, c0), cross(c0, c1));
    let sign = if determinant < 0.0 { -1.0 } else { 1.0 };
    normalized(std::array::from_fn(|axis| {
        sign * (n[0] * a[axis] + n[1] * b[axis] + n[2] * c[axis])
    }))
}

/// Вершина части в осях файла, пока оси не легли в оси сцены.
#[derive(Clone, Copy)]
struct FileVertex {
    position: Vec3,
    normal: Vec3,
    uv: [f32; 2],
    tangent: [f32; 4],
    color: [f32; 4],
}

/// Читает `bytes` как модель `name`. `image` отдаёт картинку по её номеру в файле — страница уже разжала
/// каждую; она нужна, только если на неё ссылается материал части.
pub fn parse_model(
    name: &str,
    bytes: &[u8],
    image: impl FnMut(usize) -> Result<Image, String>,
) -> Result<Model, String> {
    let glb = read_container(bytes)?;
    if let Some(compression) = compression_used(&glb.json) {
        return Err(format!(
            "сжатие {compression} движок не читает; выгрузите модель без сжатия"
        ));
    }
    check_alpha_cutoffs(&glb.json)?;
    let document = Gltf::from_slice(&without_ignored_requirements(&glb.json))
        .map_err(|e| format!("не читается как glTF: {e}"))?;
    if document
        .buffers()
        .any(|buffer| matches!(buffer.source(), BufferSource::Uri(_)))
    {
        return Err("буфер лежит вне файла: нужен glTF одним файлом".to_string());
    }
    let blob = glb.bin.as_deref();
    let scene = document
        .default_scene()
        .or_else(|| document.scenes().next())
        .ok_or("в файле нет сцены с сеткой")?;
    let mut builder = Builder {
        blob,
        materials: Vec::new(),
        material_of: HashMap::new(),
        images: Vec::new(),
        image_of: HashMap::new(),
        parts: Vec::new(),
        load_image: image,
    };
    let identity: Mat4 = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    for node in scene.nodes() {
        builder.visit(&node, &identity)?;
    }
    builder.finish(name)
}

struct Container {
    json: Vec<u8>,
    bin: Option<Vec<u8>>,
}

/// Разбирает оболочку `.glb`: заголовок и куски. Файл, что не `.glb` (`.gltf` в тексте, чужой формат),
/// не принимается.
fn read_container(bytes: &[u8]) -> Result<Container, String> {
    if !bytes.starts_with(b"glTF") {
        return Err("это не glTF одним файлом (.glb)".to_string());
    }
    let declared = bytes
        .get(8..12)
        .map(|length| u32::from_le_bytes([length[0], length[1], length[2], length[3]]))
        .ok_or("файл обрывается в заголовке")?;
    if declared < 12 || declared as usize > bytes.len() {
        return Err("длина файла в заголовке не сходится с настоящей".to_string());
    }
    let glb = gltf::Glb::from_slice(bytes).map_err(|e| format!("не читается как glTF: {e}"))?;
    Ok(Container {
        json: glb.json.into_owned(),
        bin: glb.bin.map(std::borrow::Cow::into_owned),
    })
}

/// Имя сжатия, которое использует файл, если использует.
fn compression_used(json: &[u8]) -> Option<&'static str> {
    let root: Json = serde_json::from_slice(json).ok()?;
    let used = root.get("extensionsUsed")?.as_array()?;
    COMPRESSIONS
        .into_iter()
        .find(|name| used.iter().any(|used| used.as_str() == Some(name)))
}

/// «Свет и материалы», прозрачность материала: `alphaCutoff` каждого материала — число от 0 до 1. Библиотека
/// glTF порог не проверяет, а значение не числом назвала бы без имени поля.
fn check_alpha_cutoffs(json: &[u8]) -> Result<(), String> {
    let Ok(root) = serde_json::from_slice::<Json>(json) else {
        return Ok(());
    };
    let Some(materials) = root.get("materials").and_then(Json::as_array) else {
        return Ok(());
    };
    for (index, material) in materials.iter().enumerate() {
        let Some(cutoff) = material.get("alphaCutoff") else {
            continue;
        };
        if !cutoff.as_f64().is_some_and(|c| (0.0..=1.0).contains(&c)) {
            return Err(format!(
                "материал {index}: \"alphaCutoff\" должен быть числом от 0 до 1, получено {cutoff}"
            ));
        }
    }
    Ok(())
}

/// Расширения, что движок пропускает: библиотека отвергла бы файл, где они в `extensionsRequired`.
fn is_ignored_extension(name: &str) -> bool {
    name == "KHR_texture_transform" || name.starts_with("KHR_materials_")
}

/// Текст JSON файла без пропускаемых расширений в `extensionsRequired`; без них файл читается тем же
/// путём, что и с ними в `extensionsUsed`. Текст, что не разбирается, отдаётся как есть — его ошибку
/// назовёт библиотека.
fn without_ignored_requirements(json: &[u8]) -> Vec<u8> {
    let Ok(mut root) = serde_json::from_slice::<Json>(json) else {
        return json.to_vec();
    };
    if let Some(Json::Array(required)) = root.get_mut("extensionsRequired") {
        required.retain(|name| !name.as_str().is_some_and(is_ignored_extension));
    }
    serde_json::to_vec(&root).unwrap_or_else(|_| json.to_vec())
}

struct Builder<'a, F> {
    blob: Option<&'a [u8]>,
    materials: Vec<Material>,
    material_of: HashMap<Option<usize>, usize>,
    images: Vec<Image>,
    image_of: HashMap<usize, usize>,
    parts: Vec<(usize, Vec<FileVertex>, Vec<[u32; 3]>)>,
    load_image: F,
}

impl<F: FnMut(usize) -> Result<Image, String>> Builder<'_, F> {
    fn visit(&mut self, node: &Node<'_>, parent: &Mat4) -> Result<(), String> {
        let matrix = multiply(parent, &node.transform().matrix());
        if let Some(mesh) = node.mesh() {
            for primitive in mesh.primitives() {
                self.add_primitive(&primitive, &matrix)?;
            }
        }
        for child in node.children() {
            self.visit(&child, &matrix)?;
        }
        Ok(())
    }

    fn add_primitive(
        &mut self,
        primitive: &gltf::Primitive<'_>,
        matrix: &Mat4,
    ) -> Result<(), String> {
        let blob = self.blob;
        let reader = primitive.reader(|_| blob);
        let Some(positions) = reader.read_positions() else {
            return Ok(());
        };
        let positions: Vec<Vec3> = positions.collect();
        let indices: Vec<u32> = match reader.read_indices() {
            Some(indices) => indices.into_u32().collect(),
            None => (0..positions.len() as u32).collect(),
        };
        if indices.iter().any(|&i| i as usize >= positions.len()) {
            return Err("номер вершины за пределами вершин сетки".to_string());
        }
        let mut triangles = triangles_of(primitive.mode(), &indices);
        if triangles.is_empty() {
            return Ok(());
        }

        let determinant = determinant(matrix);
        if determinant < 0.0 {
            for triangle in &mut triangles {
                triangle.swap(1, 2);
            }
        }
        let normals: Option<Vec<Vec3>> = reader.read_normals().map(Iterator::collect);
        let uvs: Vec<[f32; 2]> = reader
            .read_tex_coords(0)
            .map(|uvs| uvs.into_f32().collect())
            .unwrap_or_default();
        let colors: Vec<[f32; 4]> = reader
            .read_colors(0)
            .map(|colors| colors.into_rgba_f32().collect())
            .unwrap_or_default();
        let tangents: Option<Vec<[f32; 4]>> = reader.read_tangents().map(Iterator::collect);
        let material = primitive.material();
        let wants_tangents = material.normal_texture().is_some();

        let mut vertices: Vec<FileVertex> = positions
            .iter()
            .enumerate()
            .map(|(at, &position)| FileVertex {
                position: transform_point(matrix, position),
                normal: normals
                    .as_ref()
                    .and_then(|normals| normals.get(at))
                    .map_or([0.0, 0.0, 1.0], |&n| {
                        transform_normal(matrix, determinant, n)
                    }),
                uv: uvs.get(at).copied().unwrap_or([0.0; 2]),
                tangent: tangents
                    .as_ref()
                    .and_then(|tangents| tangents.get(at))
                    .map_or([1.0, 0.0, 0.0, 1.0], |&t| {
                        let xyz = normalized(transform_direction(matrix, [t[0], t[1], t[2]]));
                        let w = if determinant < 0.0 { -t[3] } else { t[3] };
                        [xyz[0], xyz[1], xyz[2], w]
                    }),
                color: colors.get(at).copied().unwrap_or([1.0; 4]),
            })
            .collect();

        if normals.is_none() {
            (vertices, triangles) = split_with_flat_normals(&vertices, &triangles);
        }
        if wants_tangents && tangents.is_none() {
            fill_tangents(&mut vertices, &triangles);
        }

        let material = self.material_index(&material)?;
        self.parts.push((material, vertices, triangles));
        Ok(())
    }

    /// Номер материала части в списке модели; материал собирается при первом упоминании.
    fn material_index(&mut self, material: &gltf::Material<'_>) -> Result<usize, String> {
        if let Some(&index) = self.material_of.get(&material.index()) {
            return Ok(index);
        }
        let converted = self.convert_material(material)?;
        self.materials.push(converted);
        let index = self.materials.len() - 1;
        self.material_of.insert(material.index(), index);
        Ok(index)
    }

    fn convert_material(&mut self, material: &gltf::Material<'_>) -> Result<Material, String> {
        if material.index().is_none() {
            return Ok(Material::plain());
        }
        let pbr = material.pbr_metallic_roughness();
        let base_texture = pbr
            .base_color_texture()
            .map(|info| self.texture(&info.texture()))
            .transpose()?;
        let metal_rough_texture = pbr
            .metallic_roughness_texture()
            .map(|info| self.texture(&info.texture()))
            .transpose()?;
        let normal_texture = material
            .normal_texture()
            .map(|normal| Ok::<_, String>((self.texture(&normal.texture())?, normal.scale())))
            .transpose()?;
        let occlusion_texture = material
            .occlusion_texture()
            .map(|occlusion| {
                Ok::<_, String>((self.texture(&occlusion.texture())?, occlusion.strength()))
            })
            .transpose()?;
        Ok(Material {
            base_color: pbr.base_color_factor(),
            base_texture,
            metallic: pbr.metallic_factor(),
            roughness: pbr.roughness_factor(),
            metal_rough_texture,
            normal_texture,
            occlusion_texture,
            alpha_mode: match material.alpha_mode() {
                gltf::material::AlphaMode::Opaque => AlphaMode::Opaque,
                gltf::material::AlphaMode::Mask => AlphaMode::Mask,
                gltf::material::AlphaMode::Blend => AlphaMode::Blend,
            },
            alpha_cutoff: material.alpha_cutoff().unwrap_or(DEFAULT_ALPHA_CUTOFF),
        })
    }

    fn texture(&mut self, texture: &gltf::Texture<'_>) -> Result<Texture, String> {
        let source = texture.source();
        let file_index = source.index();
        let image = match self.image_of.get(&file_index) {
            Some(&image) => image,
            None => {
                let label = file_index;
                match source.source() {
                    ImageSource::View { mime_type, .. }
                        if mime_type == "image/png" || mime_type == "image/jpeg" => {}
                    ImageSource::View { mime_type, .. } => {
                        return Err(format!(
                            "картинка {label} внутри модели — {mime_type}, а берутся только PNG и JPEG"
                        ));
                    }
                    ImageSource::Uri { .. } => {
                        return Err(format!(
                            "картинка {label} лежит вне файла: нужен glTF одним файлом"
                        ));
                    }
                }
                let decoded = (self.load_image)(file_index)?;
                self.images.push(decoded);
                self.image_of.insert(file_index, self.images.len() - 1);
                self.images.len() - 1
            }
        };
        Ok(Texture {
            image,
            sampler: sampler_of(&texture.sampler()),
        })
    }

    /// Собирает модель: части по материалам, вершины в осях сцены, середина основания в начале.
    fn finish(self, name: &str) -> Result<Model, String> {
        let mut by_material: Vec<Option<Part>> = (0..self.materials.len()).map(|_| None).collect();
        for (material, vertices, triangles) in self.parts {
            let part = by_material[material].get_or_insert_with(|| Part {
                vertices: Vec::new(),
                indices: Vec::new(),
                material,
            });
            let base = part.vertices.len() as u32;
            part.vertices.extend(vertices.iter().map(to_scene_axes));
            part.indices
                .extend(triangles.iter().flatten().map(|index| index + base));
        }
        let mut parts: Vec<Part> = by_material.into_iter().flatten().collect();
        let mut low = [f32::INFINITY; 3];
        let mut high = [f32::NEG_INFINITY; 3];
        for vertex in parts.iter().flat_map(|part| &part.vertices) {
            for (axis, &coordinate) in vertex.position.iter().enumerate() {
                low[axis] = low[axis].min(coordinate);
                high[axis] = high[axis].max(coordinate);
            }
        }
        if parts.is_empty() || low.iter().chain(&high).any(|c| !c.is_finite()) {
            return Err("в файле нет ни одной сетки с треугольниками".to_string());
        }
        let shift = [(low[0] + high[0]) / 2.0, (low[1] + high[1]) / 2.0, low[2]];
        for vertex in parts.iter_mut().flat_map(|part| &mut part.vertices) {
            for (coordinate, shift) in vertex.position.iter_mut().zip(shift) {
                *coordinate -= shift;
            }
        }
        Ok(Model {
            name: name.to_string(),
            parts,
            materials: self.materials,
            images: self.images,
            size: std::array::from_fn(|axis| f64::from(high[axis] - low[axis])),
            origin: [-shift[0], -shift[1]],
        })
    }
}

/// Оси файла в оси сцены: x → x, z → y (к игроку), y → z (вверх). Обмен осей — отражение, поэтому знак
/// у `tangent.w` меняется: `cross(normal, tangent) × w` по-прежнему даёт направление «вверх» карты.
fn to_scene_axes(vertex: &FileVertex) -> ModelVertex {
    let swap = |v: Vec3| [v[0], v[2], v[1]];
    let tangent = swap([vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]]);
    ModelVertex {
        position: swap(vertex.position),
        normal: swap(vertex.normal),
        uv: vertex.uv,
        tangent: [tangent[0], tangent[1], tangent[2], -vertex.tangent[3]],
        color: vertex.color,
    }
}

/// Треугольники сетки списком по три номера; точки и линии пропускаются.
fn triangles_of(mode: Mode, indices: &[u32]) -> Vec<[u32; 3]> {
    match mode {
        Mode::Triangles => indices.as_chunks::<3>().0.to_vec(),
        Mode::TriangleStrip => (2..indices.len())
            .map(|at| {
                if at.is_multiple_of(2) {
                    [indices[at - 2], indices[at - 1], indices[at]]
                } else {
                    [indices[at - 1], indices[at - 2], indices[at]]
                }
            })
            .collect(),
        Mode::TriangleFan => (2..indices.len())
            .map(|at| [indices[0], indices[at - 1], indices[at]])
            .collect(),
        Mode::Points | Mode::Lines | Mode::LineLoop | Mode::LineStrip => Vec::new(),
    }
}

/// Нет нормалей — они считаются по треугольникам: у каждого треугольника свои вершины и одна нормаль.
fn split_with_flat_normals(
    vertices: &[FileVertex],
    triangles: &[[u32; 3]],
) -> (Vec<FileVertex>, Vec<[u32; 3]>) {
    let mut out = Vec::with_capacity(triangles.len() * 3);
    for triangle in triangles {
        let corners = triangle.map(|index| vertices[index as usize]);
        let normal = normalized(cross(
            sub(corners[1].position, corners[0].position),
            sub(corners[2].position, corners[0].position),
        ));
        out.extend(corners.map(|corner| FileVertex { normal, ..corner }));
    }
    let split = (0..triangles.len() as u32)
        .map(|at| [3 * at, 3 * at + 1, 3 * at + 2])
        .collect();
    (out, split)
}

/// Есть карта мелкого рельефа, но нет касательных — они считаются по картам вершин треугольников: вдоль `u`,
/// а `w` — по тому, куда у карты «вверх» (против `v`, как у файла).
fn fill_tangents(vertices: &mut [FileVertex], triangles: &[[u32; 3]]) {
    let mut along_u = vec![[0.0f32; 3]; vertices.len()];
    let mut along_up = vec![[0.0f32; 3]; vertices.len()];
    for triangle in triangles {
        let [a, b, c] = triangle.map(|index| vertices[index as usize]);
        let (e1, e2) = (sub(b.position, a.position), sub(c.position, a.position));
        let (du1, dv1) = (b.uv[0] - a.uv[0], b.uv[1] - a.uv[1]);
        let (du2, dv2) = (c.uv[0] - a.uv[0], c.uv[1] - a.uv[1]);
        let determinant = du1 * dv2 - du2 * dv1;
        if determinant.abs() < 1e-20 {
            continue;
        }
        let r = 1.0 / determinant;
        let u: Vec3 = std::array::from_fn(|k| (e1[k] * dv2 - e2[k] * dv1) * r);
        let up: Vec3 = std::array::from_fn(|k| -(e2[k] * du1 - e1[k] * du2) * r);
        for &index in triangle {
            for (sum, add) in along_u[index as usize].iter_mut().zip(u) {
                *sum += add;
            }
            for (sum, add) in along_up[index as usize].iter_mut().zip(up) {
                *sum += add;
            }
        }
    }
    for (at, vertex) in vertices.iter_mut().enumerate() {
        let n = vertex.normal;
        let t = normalized(sub(along_u[at], n.map(|c| c * dot(n, along_u[at]))));
        let w = if dot(cross(n, t), along_up[at]) < 0.0 {
            -1.0
        } else {
            1.0
        };
        vertex.tangent = [t[0], t[1], t[2], w];
    }
}

fn wrap_of(mode: WrappingMode) -> Wrap {
    match mode {
        WrappingMode::ClampToEdge => Wrap::Clamp,
        WrappingMode::MirroredRepeat => Wrap::Mirror,
        WrappingMode::Repeat => Wrap::Repeat,
    }
}

/// Сглаживание `sampler`: ближе — по `magFilter`, дальше — по `minFilter`; без них линейно.
fn sampler_of(sampler: &gltf::texture::Sampler<'_>) -> Sampler {
    Sampler {
        wrap: [wrap_of(sampler.wrap_s()), wrap_of(sampler.wrap_t())],
        mag_linear: !matches!(sampler.mag_filter(), Some(MagFilter::Nearest)),
        min_linear: !matches!(
            sampler.min_filter(),
            Some(
                MinFilter::Nearest
                    | MinFilter::NearestMipmapNearest
                    | MinFilter::NearestMipmapLinear
            )
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Одна сетка тестового файла: вершины, номера и необязательные карты вершин.
    #[derive(Default, Clone)]
    struct TestMesh {
        positions: Vec<[f32; 3]>,
        indices: Vec<u16>,
        normals: Option<Vec<[f32; 3]>>,
        uvs: Option<Vec<[f32; 2]>>,
        tangents: Option<Vec<[f32; 4]>>,
        colors: Option<Vec<[f32; 4]>>,
        material: Option<usize>,
    }

    /// Ящик `low`–`high` из двенадцати треугольников, обход против часовой стрелки снаружи.
    fn cube(low: [f32; 3], high: [f32; 3], material: Option<usize>) -> TestMesh {
        let corner = |bits: usize| {
            [
                if bits & 1 == 0 { low[0] } else { high[0] },
                if bits & 2 == 0 { low[1] } else { high[1] },
                if bits & 4 == 0 { low[2] } else { high[2] },
            ]
        };
        let positions = (0..8).map(corner).collect();
        let indices = vec![
            0, 4, 6, 0, 6, 2, // −x
            1, 3, 7, 1, 7, 5, // +x
            0, 1, 5, 0, 5, 4, // −y
            2, 6, 7, 2, 7, 3, // +y
            0, 2, 3, 0, 3, 1, // −z
            4, 5, 7, 4, 7, 6, // +z
        ];
        TestMesh {
            positions,
            indices,
            material,
            ..TestMesh::default()
        }
    }

    fn padded(mut bytes: Vec<u8>, with: u8) -> Vec<u8> {
        while !bytes.len().is_multiple_of(4) {
            bytes.push(with);
        }
        bytes
    }

    fn floats(values: impl IntoIterator<Item = f32>) -> Vec<u8> {
        values.into_iter().flat_map(f32::to_le_bytes).collect()
    }

    /// Собирает `.glb`: сетки, узлы (`nodes` — текст списка JSON), материалы и остальные поля корня.
    fn glb_file(meshes: &[TestMesh], nodes: &str, materials: &str, extra_root: &str) -> Vec<u8> {
        let mut bin = Vec::new();
        let mut views = Vec::new();
        let mut accessors = Vec::new();
        let mut mesh_json = Vec::new();
        let mut add = |bin: &mut Vec<u8>,
                       bytes: Vec<u8>,
                       component: u32,
                       kind: &str,
                       count: usize,
                       extra: &str| {
            let offset = bin.len();
            bin.extend(&bytes);
            while !bin.len().is_multiple_of(4) {
                bin.push(0);
            }
            views.push(format!(
                r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{}}}"#,
                bytes.len()
            ));
            accessors.push(format!(
                r#"{{"bufferView":{},"componentType":{component},"count":{count},"type":"{kind}"{extra}}}"#,
                views.len() - 1
            ));
            accessors.len() - 1
        };
        for mesh in meshes {
            let count = mesh.positions.len();
            let low: Vec<f32> = (0..3)
                .map(|a| {
                    mesh.positions
                        .iter()
                        .map(|p| p[a])
                        .fold(f32::INFINITY, f32::min)
                })
                .collect();
            let high: Vec<f32> = (0..3)
                .map(|a| {
                    mesh.positions
                        .iter()
                        .map(|p| p[a])
                        .fold(f32::NEG_INFINITY, f32::max)
                })
                .collect();
            let bounds = format!(r#","min":{low:?},"max":{high:?}"#);
            let position = add(
                &mut bin,
                floats(mesh.positions.iter().flatten().copied()),
                5126,
                "VEC3",
                count,
                &bounds,
            );
            let mut attributes = format!(r#""POSITION":{position}"#);
            if let Some(normals) = &mesh.normals {
                let at = add(
                    &mut bin,
                    floats(normals.iter().flatten().copied()),
                    5126,
                    "VEC3",
                    count,
                    "",
                );
                attributes += &format!(r#","NORMAL":{at}"#);
            }
            if let Some(uvs) = &mesh.uvs {
                let at = add(
                    &mut bin,
                    floats(uvs.iter().flatten().copied()),
                    5126,
                    "VEC2",
                    count,
                    "",
                );
                attributes += &format!(r#","TEXCOORD_0":{at}"#);
            }
            if let Some(tangents) = &mesh.tangents {
                let at = add(
                    &mut bin,
                    floats(tangents.iter().flatten().copied()),
                    5126,
                    "VEC4",
                    count,
                    "",
                );
                attributes += &format!(r#","TANGENT":{at}"#);
            }
            if let Some(colors) = &mesh.colors {
                let at = add(
                    &mut bin,
                    floats(colors.iter().flatten().copied()),
                    5126,
                    "VEC4",
                    count,
                    "",
                );
                attributes += &format!(r#","COLOR_0":{at}"#);
            }
            let index_bytes: Vec<u8> = mesh.indices.iter().flat_map(|i| i.to_le_bytes()).collect();
            let indices = add(
                &mut bin,
                index_bytes,
                5123,
                "SCALAR",
                mesh.indices.len(),
                "",
            );
            let material = mesh
                .material
                .map_or_else(String::new, |m| format!(r#","material":{m}"#));
            mesh_json.push(format!(
                r#"{{"primitives":[{{"attributes":{{{attributes}}},"indices":{indices}{material}}}]}}"#
            ));
        }
        let materials = if materials.is_empty() {
            String::new()
        } else {
            format!(r#","materials":{materials}"#)
        };
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],"nodes":{nodes},"meshes":[{}],"buffers":[{{"byteLength":{}}}],"bufferViews":[{}],"accessors":[{}]{materials}{extra_root}}}"#,
            mesh_json.join(","),
            bin.len(),
            views.join(","),
            accessors.join(","),
        );
        let json = padded(json.into_bytes(), b' ');
        let bin = padded(bin, 0);
        let total = 12 + 8 + json.len() + 8 + bin.len();
        let mut file = Vec::new();
        file.extend(b"glTF");
        file.extend(2u32.to_le_bytes());
        file.extend((total as u32).to_le_bytes());
        file.extend((json.len() as u32).to_le_bytes());
        file.extend(0x4E4F534Au32.to_le_bytes());
        file.extend(json);
        file.extend((bin.len() as u32).to_le_bytes());
        file.extend(0x004E4942u32.to_le_bytes());
        file.extend(bin);
        file
    }

    fn no_images(index: usize) -> Result<Image, String> {
        Err(format!("картинки {index} в тесте нет"))
    }

    fn parse(file: &[u8]) -> Result<Model, String> {
        parse_model("test", file, no_images)
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-5
    }

    #[test]
    fn nested_nodes_with_scale_and_translation_are_composed_into_one_body() {
        let file = glb_file(
            &[cube([0.0; 3], [1.0; 3], None)],
            r#"[{"mesh":0,"scale":[2,3,4],"translation":[10,0,0],"children":[1]},{"mesh":0,"translation":[0,1,0]}]"#,
            "",
            "",
        );
        let model = parse(&file).expect("модель читается");
        // Родитель: x 10…12, y 0…3, z 0…4; потомок встаёт в тех же осях родителя, сдвинутый на 1 по y и
        // растянутый вместе с ним: y 3…6.
        assert!(close(model.size[0], 2.0), "{:?}", model.size);
        assert!(
            close(model.size[1], 4.0),
            "по оси z файла: {:?}",
            model.size
        );
        assert!(
            close(model.size[2], 6.0),
            "по оси y файла: {:?}",
            model.size
        );
    }

    #[test]
    fn the_base_midpoint_of_the_box_is_the_origin_and_the_bottom_is_at_zero() {
        // Основание не на нуле: файл стоит от −3 до −1 по y.
        let file = glb_file(
            &[cube([2.0, -3.0, 5.0], [6.0, -1.0, 9.0], None)],
            r#"[{"mesh":0}]"#,
            "",
            "",
        );
        let model = parse(&file).expect("модель читается");
        let vertices: Vec<[f32; 3]> = model.parts[0].vertices.iter().map(|v| v.position).collect();
        let low = |axis: usize| {
            vertices
                .iter()
                .map(|p| p[axis])
                .fold(f32::INFINITY, f32::min)
        };
        let high = |axis: usize| {
            vertices
                .iter()
                .map(|p| p[axis])
                .fold(f32::NEG_INFINITY, f32::max)
        };
        assert!(close(f64::from(low(0)), -2.0) && close(f64::from(high(0)), 2.0));
        assert!(close(f64::from(low(1)), -2.0) && close(f64::from(high(1)), 2.0));
        assert!(close(f64::from(low(2)), 0.0) && close(f64::from(high(2)), 2.0));
        assert_eq!(model.size, [4.0, 4.0, 2.0]);
    }

    #[test]
    fn the_file_axes_lie_in_the_scene_as_plus_x_right_plus_z_toward_the_player_and_plus_y_up() {
        let mut arrow = cube([0.0; 3], [0.1; 3], None);
        arrow.positions = vec![
            [0.0, 0.0, 0.0],
            [0.0, 0.0, 2.0],
            [1.0, 0.0, 0.0],
            [0.0, 3.0, 0.0],
        ];
        arrow.indices = vec![0, 1, 2, 0, 2, 3];
        let file = glb_file(&[arrow], r#"[{"mesh":0}]"#, "", "");
        let model = parse(&file).expect("модель читается");
        // Размер: по x файла 1, по z файла 2 → глубина сцены, по y файла 3 → высота.
        assert_eq!(model.size, [1.0, 2.0, 3.0]);
        let positions: Vec<[f32; 3]> = model.parts[0].vertices.iter().map(|v| v.position).collect();
        let origin = positions
            .iter()
            .find(|p| p[0] < 0.0 && p[1] < 0.0 && p[2] < 0.01)
            .copied()
            .expect("вершина в начале файла");
        let along_z = positions
            .iter()
            .find(|p| p[1] > 0.0)
            .copied()
            .expect("вершина на +Z файла");
        assert!(along_z[1] > origin[1], "+Z файла — ниже по сцене, к игроку");
        let along_x = positions
            .iter()
            .find(|p| p[0] > 0.0)
            .copied()
            .expect("вершина на +X файла");
        assert!(along_x[0] > origin[0], "+X файла — правее");
        let up = positions
            .iter()
            .find(|p| p[2] > 2.0)
            .copied()
            .expect("вершина на +Y файла");
        assert!(close(f64::from(up[2]), 3.0), "+Y файла — вверх");
    }

    #[test]
    fn the_material_factors_vertex_colors_and_the_plain_default_are_read() {
        let mut painted = cube([0.0; 3], [1.0; 3], Some(0));
        painted.colors = Some(vec![[0.5, 0.25, 1.0, 1.0]; 8]);
        let bare = cube([2.0, 0.0, 0.0], [3.0, 1.0, 1.0], None);
        let file = glb_file(
            &[painted, bare],
            r#"[{"mesh":0,"children":[1]},{"mesh":1}]"#,
            r#"[{"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.4,0.6,1],"metallicFactor":0.6,"roughnessFactor":0.25}}]"#,
            "",
        );
        let model = parse(&file).expect("модель читается");
        assert_eq!(model.materials.len(), 2);
        let first = &model.materials[model.parts[0].material];
        assert_eq!(first.base_color, [0.2, 0.4, 0.6, 1.0]);
        assert_eq!((first.metallic, first.roughness), (0.6, 0.25));
        assert_eq!(model.parts[0].vertices[0].color, [0.5, 0.25, 1.0, 1.0]);
        let plain = &model.materials[model.parts[1].material];
        assert_eq!(*plain, Material::plain());
        assert_eq!(
            model.parts[1].vertices[0].color, [1.0; 4],
            "без цвета вершин — белый"
        );
    }

    /// Файл из ящиков с материалами `materials` (JSON-список), у каждого ящика — свой номер материала.
    fn parse_with_materials(materials: &str, count: usize) -> Result<Model, String> {
        let meshes: Vec<TestMesh> = (0..count)
            .map(|k| {
                cube(
                    [k as f32 * 2.0, 0.0, 0.0],
                    [k as f32 * 2.0 + 1.0, 1.0, 1.0],
                    Some(k),
                )
            })
            .collect();
        let children: Vec<String> = (1..count).map(|k| k.to_string()).collect();
        let nodes = format!(
            r#"[{{"mesh":0,"children":[{}]}}{}]"#,
            children.join(","),
            (1..count)
                .map(|k| format!(r#",{{"mesh":{k}}}"#))
                .collect::<String>()
        );
        parse(&glb_file(&meshes, &nodes, materials, ""))
    }

    /// «Свет и материалы», прозрачность материала: `alphaMode` и `alphaCutoff` читаются из файла; без записи —
    /// сплошной материал и порог 0,5.
    #[test]
    fn the_alpha_mode_and_cutoff_are_read_and_default_to_opaque_and_one_half() {
        let model = parse_with_materials(
            r#"[{"pbrMetallicRoughness":{}},
                {"alphaMode":"MASK","alphaCutoff":0.3,"pbrMetallicRoughness":{}},
                {"alphaMode":"MASK","pbrMetallicRoughness":{}},
                {"alphaMode":"BLEND","pbrMetallicRoughness":{"baseColorFactor":[1,1,1,0.35]}},
                {"alphaMode":"OPAQUE","alphaCutoff":0.9,"pbrMetallicRoughness":{}}]"#,
            5,
        )
        .expect("модель читается");
        let modes: Vec<(AlphaMode, f32)> = model
            .parts
            .iter()
            .map(|part| {
                let material = &model.materials[part.material];
                (material.alpha_mode, material.alpha_cutoff)
            })
            .collect();
        assert_eq!(
            modes,
            [
                (AlphaMode::Opaque, 0.5),
                (AlphaMode::Mask, 0.3),
                (AlphaMode::Mask, 0.5),
                (AlphaMode::Blend, 0.5),
                (AlphaMode::Opaque, 0.9),
            ]
        );
        assert_eq!(model.materials[3].base_color[3], 0.35);
        assert!(model.has_blend());
        assert_eq!(Material::plain().alpha_mode, AlphaMode::Opaque);
    }

    #[test]
    fn an_alpha_cutoff_outside_zero_to_one_or_not_a_number_is_an_error_naming_the_field() {
        for cutoff in ["1.5", "-0.1", r#""half""#] {
            let materials = format!(
                r#"[{{"alphaMode":"MASK","alphaCutoff":{cutoff},"pbrMetallicRoughness":{{}}}}]"#
            );
            let error = parse_with_materials(&materials, 1).expect_err("порог не годится");
            assert!(
                error.contains("\"alphaCutoff\"") && error.contains("от 0 до 1"),
                "{cutoff}: {error}"
            );
        }
        let edge = r#"[{"alphaMode":"MASK","alphaCutoff":1,"pbrMetallicRoughness":{}}]"#;
        assert!(
            parse_with_materials(edge, 1).is_ok(),
            "границы отрезка годятся"
        );
    }

    #[test]
    fn parts_with_one_material_are_one_part_and_other_materials_stay_apart() {
        let file = glb_file(
            &[
                cube([0.0; 3], [1.0; 3], Some(0)),
                cube([2.0, 0.0, 0.0], [3.0, 1.0, 1.0], Some(0)),
                cube([4.0, 0.0, 0.0], [5.0, 1.0, 1.0], Some(1)),
            ],
            r#"[{"mesh":0,"children":[1,2]},{"mesh":1},{"mesh":2}]"#,
            r#"[{"pbrMetallicRoughness":{}},{"pbrMetallicRoughness":{"baseColorFactor":[1,0,0,1]}}]"#,
            "",
        );
        let model = parse(&file).expect("модель читается");
        assert_eq!(model.parts.len(), 2);
        assert_eq!(model.parts[0].vertices.len(), 72);
        assert_eq!(model.parts[0].indices.len(), 72);
        assert!(model.parts[0].indices.iter().all(|&i| i < 72));
        assert_ne!(model.parts[0].material, model.parts[1].material);
    }

    #[test]
    fn missing_normals_are_flat_per_triangle_and_point_outward() {
        let file = glb_file(&[cube([0.0; 3], [1.0; 3], None)], r#"[{"mesh":0}]"#, "", "");
        let model = parse(&file).expect("модель читается");
        let part = &model.parts[0];
        assert_eq!(
            part.vertices.len(),
            36,
            "у каждого треугольника свои вершины"
        );
        let centre = [0.0f32, 0.0, 0.5];
        for triangle in part.indices.chunks(3) {
            let corners: Vec<&ModelVertex> = triangle
                .iter()
                .map(|&i| &part.vertices[i as usize])
                .collect();
            let middle: Vec<f32> = (0..3)
                .map(|a| corners.iter().map(|v| v.position[a]).sum::<f32>() / 3.0)
                .collect();
            let outward: f32 = (0..3)
                .map(|a| (middle[a] - centre[a]) * corners[0].normal[a])
                .sum();
            assert!(outward > 0.0, "нормаль внутрь: {corners:?}");
        }
    }

    #[test]
    fn a_mirrored_node_keeps_the_normals_outward() {
        let file = glb_file(
            &[cube([1.0, 0.0, 0.0], [2.0, 1.0, 1.0], None)],
            r#"[{"mesh":0,"scale":[-1,1,1]}]"#,
            "",
            "",
        );
        let model = parse(&file).expect("модель читается");
        let part = &model.parts[0];
        // Куб зеркально лежит от x = −2 до −1 файла: середина по x сцены −1,5 (до сдвига к началу).
        let low = part
            .vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::INFINITY, f32::min);
        let high = part
            .vertices
            .iter()
            .map(|v| v.position[0])
            .fold(f32::NEG_INFINITY, f32::max);
        assert!(close(f64::from(high - low), 1.0));
        for triangle in part.indices.chunks(3) {
            let corners: Vec<&ModelVertex> = triangle
                .iter()
                .map(|&i| &part.vertices[i as usize])
                .collect();
            let middle: Vec<f32> = (0..3)
                .map(|a| corners.iter().map(|v| v.position[a]).sum::<f32>() / 3.0)
                .collect();
            let centre = [0.0, 0.0, 0.5];
            let outward: f32 = (0..3)
                .map(|a| (middle[a] - centre[a]) * corners[0].normal[a])
                .sum();
            assert!(
                outward > 0.0,
                "нормаль внутрь у зеркального узла: {corners:?}"
            );
        }
    }

    #[test]
    fn a_normal_map_without_tangents_gets_tangents_along_u() {
        let quad = TestMesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            normals: Some(vec![[0.0, 0.0, 1.0]; 4]),
            uvs: Some(vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]),
            material: Some(0),
            ..TestMesh::default()
        };
        let file = glb_file(
            &[quad],
            r#"[{"mesh":0}]"#,
            r#"[{"normalTexture":{"index":0,"scale":0.7},"pbrMetallicRoughness":{}}]"#,
            r#","textures":[{"source":0}],"images":[{"bufferView":0,"mimeType":"image/png"}]"#,
        );
        let model = parse_model("test", &file, |index| {
            assert_eq!(index, 0);
            Ok(Image {
                width: 1,
                height: 1,
                pixels: vec![128, 128, 255, 255],
            })
        })
        .expect("модель читается");
        let material = &model.materials[0];
        let (texture, scale) = material.normal_texture.expect("карта рельефа");
        assert_eq!(scale, 0.7);
        assert_eq!(texture.image, 0);
        assert_eq!(model.images.len(), 1);
        for vertex in &model.parts[0].vertices {
            // Файл: u вправо по +X файла → вправо по сцене; «вверх» карты — к +Y файла → к +Z сцены.
            assert!(close(f64::from(vertex.tangent[0]), 1.0), "{vertex:?}");
            assert!(close(
                f64::from(vertex.tangent[1]).abs() + f64::from(vertex.tangent[2]).abs(),
                0.0
            ));
            // cross(normal, tangent) × w должно давать направление вверх по карте — к +Y файла, то есть к
            // +Z сцены нормаль же направлена на +Z файла → +Y сцены.
            let n = vertex.normal;
            let t = [vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]];
            let b = [
                (n[1] * t[2] - n[2] * t[1]) * vertex.tangent[3],
                (n[2] * t[0] - n[0] * t[2]) * vertex.tangent[3],
                (n[0] * t[1] - n[1] * t[0]) * vertex.tangent[3],
            ];
            assert!(
                close(f64::from(b[2]), 1.0),
                "верх карты — вверх по сцене: {b:?}"
            );
        }
    }

    #[test]
    fn a_provided_tangent_keeps_its_direction_and_flips_handedness_with_the_axes() {
        let mut quad = cube([0.0; 3], [1.0; 3], None);
        quad.normals = Some(vec![[0.0, 0.0, 1.0]; 8]);
        quad.tangents = Some(vec![[1.0, 0.0, 0.0, 1.0]; 8]);
        let file = glb_file(&[quad], r#"[{"mesh":0}]"#, "", "");
        let model = parse(&file).expect("модель читается");
        let vertex = &model.parts[0].vertices[0];
        assert_eq!(&vertex.tangent[..3], &[1.0, 0.0, 0.0]);
        assert_eq!(vertex.tangent[3], -1.0);
    }

    #[test]
    fn compressed_meshes_are_an_error_naming_the_extension() {
        let file = glb_file(
            &[cube([0.0; 3], [1.0; 3], None)],
            r#"[{"mesh":0}]"#,
            "",
            r#","extensionsUsed":["KHR_draco_mesh_compression"],"extensionsRequired":["KHR_draco_mesh_compression"]"#,
        );
        let error = parse(&file).expect_err("Draco не читается");
        assert!(error.contains("KHR_draco_mesh_compression"), "{error}");
        let file = glb_file(
            &[cube([0.0; 3], [1.0; 3], None)],
            r#"[{"mesh":0}]"#,
            "",
            r#","extensionsUsed":["EXT_meshopt_compression"]"#,
        );
        assert!(
            parse(&file)
                .expect_err("meshopt не читается")
                .contains("EXT_meshopt_compression")
        );
    }

    #[test]
    fn unknown_extensions_and_skeletons_do_not_get_in_the_way() {
        let file = glb_file(
            &[cube([0.0; 3], [1.0; 3], Some(0))],
            r#"[{"mesh":0}]"#,
            r#"[{"pbrMetallicRoughness":{},"extensions":{"KHR_materials_clearcoat":{"clearcoatFactor":1}}}]"#,
            r#","extensionsUsed":["KHR_materials_clearcoat","KHR_texture_transform"]"#,
        );
        assert!(parse(&file).is_ok());
    }

    #[test]
    fn material_and_texture_transform_extensions_may_be_required() {
        let file = glb_file(
            &[cube([0.0; 3], [1.0; 3], Some(0))],
            r#"[{"mesh":0}]"#,
            r#"[{"pbrMetallicRoughness":{}}]"#,
            r#","extensionsUsed":["KHR_materials_clearcoat","KHR_texture_transform"],"extensionsRequired":["KHR_materials_clearcoat","KHR_texture_transform"]"#,
        );
        assert!(parse(&file).is_ok());
        let file = glb_file(
            &[cube([0.0; 3], [1.0; 3], None)],
            r#"[{"mesh":0}]"#,
            "",
            r#","extensionsUsed":["EXT_unknown"],"extensionsRequired":["EXT_unknown"]"#,
        );
        assert!(
            parse(&file).is_err(),
            "чужое обязательное расширение — ошибка"
        );
    }

    #[test]
    fn a_file_that_is_not_a_single_glb_is_an_error() {
        assert!(
            parse(b"{\"asset\":{\"version\":\"2.0\"}}")
                .expect_err("не glb")
                .contains("glb")
        );
        assert!(parse(b"glTF").is_err(), "обрыв в заголовке");
        let mut file = glb_file(&[cube([0.0; 3], [1.0; 3], None)], r#"[{"mesh":0}]"#, "", "");
        file.truncate(file.len() - 8);
        assert!(parse(&file).is_err(), "короче, чем сказано в заголовке");
        let mut tiny = vec![];
        tiny.extend(b"glTF");
        tiny.extend(2u32.to_le_bytes());
        tiny.extend(3u32.to_le_bytes());
        assert!(parse(&tiny).is_err(), "длина меньше заголовка");
    }

    /// Тот же файл, у которого в тексте JSON `from` заменено на `to`: куски пересобираются заново.
    fn rewrite_json(file: &[u8], from: &str, to: &str) -> Vec<u8> {
        let json_length = u32::from_le_bytes([file[12], file[13], file[14], file[15]]) as usize;
        let json = String::from_utf8(file[20..20 + json_length].to_vec()).expect("JSON");
        let mut json = json.trim_end().replacen(from, to, 1).into_bytes();
        while !json.len().is_multiple_of(4) {
            json.push(b' ');
        }
        let rest = &file[20 + json_length..];
        let total = 12 + 8 + json.len() + rest.len();
        let mut out = Vec::new();
        out.extend(&file[..8]);
        out.extend((total as u32).to_le_bytes());
        out.extend((json.len() as u32).to_le_bytes());
        out.extend(&file[16..20]);
        out.extend(json);
        out.extend(rest);
        out
    }

    #[test]
    fn a_buffer_outside_the_file_is_an_error() {
        let file = glb_file(&[cube([0.0; 3], [1.0; 3], None)], r#"[{"mesh":0}]"#, "", "");
        let outside = rewrite_json(
            &file,
            r#""buffers":[{"#,
            r#""buffers":[{"uri":"other.bin","#,
        );
        let error = parse(&outside).expect_err("буфер вне файла");
        assert!(error.contains("вне файла"), "{error}");
        assert!(parse(&file).is_ok(), "сам перепис без правки читается");
    }

    #[test]
    fn a_file_without_any_mesh_is_an_error() {
        let file = glb_file(&[], r#"[{}]"#, "", "");
        let error = parse(&file).expect_err("сеток нет");
        assert!(error.contains("нет ни одной сетки"), "{error}");
    }

    #[test]
    fn an_image_that_is_not_png_or_jpeg_or_does_not_decompress_is_an_error() {
        let quad = |material| TestMesh {
            material: Some(material),
            ..cube([0.0; 3], [1.0; 3], None)
        };
        let materials = r#"[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#;
        let webp = glb_file(
            &[quad(0)],
            r#"[{"mesh":0}]"#,
            materials,
            r#","textures":[{"source":0}],"images":[{"bufferView":0,"mimeType":"image/webp"}]"#,
        );
        let error = parse(&webp).expect_err("webp не берётся");
        assert!(error.contains("PNG и JPEG"), "{error}");
        let png = glb_file(
            &[quad(0)],
            r#"[{"mesh":0}]"#,
            materials,
            r#","textures":[{"source":0}],"images":[{"bufferView":0,"mimeType":"image/png"}]"#,
        );
        let error = parse_model("test", &png, |_| Err("не разжалась".to_string()))
            .expect_err("страница не разжала");
        assert_eq!(error, "не разжалась");
        let outside = glb_file(
            &[quad(0)],
            r#"[{"mesh":0}]"#,
            materials,
            r#","textures":[{"source":0}],"images":[{"uri":"base.png"}]"#,
        );
        assert!(
            parse(&outside)
                .expect_err("вне файла")
                .contains("вне файла")
        );
    }

    #[test]
    fn the_sampler_sets_the_wrap_and_the_filters_of_a_texture() {
        let file = glb_file(
            &[TestMesh {
                material: Some(0),
                ..cube([0.0; 3], [1.0; 3], None)
            }],
            r#"[{"mesh":0}]"#,
            r#"[{"pbrMetallicRoughness":{"baseColorTexture":{"index":0}}}]"#,
            r#","samplers":[{"magFilter":9728,"minFilter":9728,"wrapS":33071,"wrapT":33648}],"textures":[{"source":0,"sampler":0}],"images":[{"bufferView":0,"mimeType":"image/jpeg"}]"#,
        );
        let model = parse_model("test", &file, |_| {
            Ok(Image {
                width: 1,
                height: 1,
                pixels: vec![255; 4],
            })
        })
        .expect("модель читается");
        let sampler = model.materials[0].base_texture.expect("цвет").sampler;
        assert_eq!(sampler.wrap, [Wrap::Clamp, Wrap::Mirror]);
        assert!(!sampler.mag_linear && !sampler.min_linear);
    }
}

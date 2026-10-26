//! «Трёхмерная сцена» → «Модели и движения», «Отрисовка»: где в кадре стоит каждый объект с `model`. Ни
//! видеокарты, ни браузера: `wasm::mod` переводит это в данные для `render::gpu_models`.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::rc::Rc;
use std::sync::Arc;

use block_compression::decode::decompress_blocks_as_rgba8;
use block_compression::{BC7Settings, CompressionVariant};

use crate::core::game::Game;
use crate::core::model::{Image, ModelImage, PackedFormat, PackedImage};
use crate::core::property;
use crate::core::road_kinds::Junction;
use crate::core::value::Rotation;

use super::materials::{linear_to_srgb, srgb_to_linear};

/// Один объект модели в кадре: середина основания на земле с косинусом и синусом поворота, высота
/// основания, увеличение модели и растяжение вдоль её локальной оси +Y — у кусков дороги. Вершины модели уже
/// лежат серединой основания в начале.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ModelInstance {
    /// `[x, y, cos, sin]`.
    pub placement: [f32; 4],
    /// `[z, scale, stretch, tilt]`; `stretch` — единица у всего, кроме кусков дороги, `tilt` — наклон вдоль длины
    /// модели в радианах, нос выше при положительном, ноль у всего, кроме машины на подъёме.
    pub base: [f32; 4],
}

/// Объект с частью `BLEND`: номер модели и номер объекта в её списке кадра.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlendedDraw {
    pub model: u32,
    pub index: u32,
}

/// Объекты каждой модели кадра по номеру модели, самая высокая точка среди них — по ней охватывается карта
/// теней, — и середины объектов, чьи модели имеют часть `BLEND`.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ModelDraws {
    pub instances: Vec<Vec<ModelInstance>>,
    pub top: f64,
    blended: Vec<(BlendedDraw, [f64; 3])>,
}

impl ModelDraws {
    /// «Свет и материалы», прозрачность: объекты с частью `BLEND` от дальнего к ближнему по середине объекта,
    /// как их рисовать; равноудалённые — в порядке моделей и объектов.
    pub fn blended_far_to_near(&self, eye: [f64; 3]) -> Vec<BlendedDraw> {
        let reach = |middle: &[f64; 3]| {
            (0..3)
                .map(|axis| (middle[axis] - eye[axis]).powi(2))
                .sum::<f64>()
        };
        let mut order: Vec<&(BlendedDraw, [f64; 3])> = self.blended.iter().collect();
        order.sort_by(|a, b| reach(&b.1).total_cmp(&reach(&a.1)));
        order.into_iter().map(|(draw, _)| *draw).collect()
    }
}

/// Куски видов дорог кадра по номеру модели и перекрёстки раскладки: сначала рисуются все куски, потом все
/// перекрёстки. Список перекрёстков общий с раскладкой и новый при каждой её пересборке.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct RoadDraws {
    pub pieces: Vec<Vec<ModelInstance>>,
    pub junctions: Rc<Vec<Junction>>,
    /// Изогнутые копии кусков дорог по дугам и склонам; рисуются теми же буферами, что перекрёстки.
    pub bent: Rc<Vec<Junction>>,
}

/// «Вид дороги»: куски раскладки `game` по моделям и её перекрёстки.
pub fn road_draws(game: &Game) -> RoadDraws {
    let mut pieces = vec![Vec::new(); game.world.models().len()];
    let layout = game.road_kind_layout();
    for piece in layout.pieces() {
        pieces[piece.model].push(ModelInstance {
            placement: [
                piece.center[0] as f32,
                piece.center[1] as f32,
                piece.cos as f32,
                piece.sin as f32,
            ],
            base: [
                piece.z as f32,
                piece.scale as f32,
                piece.stretch as f32,
                0.0,
            ],
        });
    }
    RoadDraws {
        pieces,
        junctions: Rc::clone(layout.junctions()),
        bent: Rc::clone(layout.bent_pieces()),
    }
}

/// Части перекрёстков одной модели в общих буферах: номер модели, номер её части, первый индекс и число индексов.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JunctionRun {
    pub model: usize,
    pub part: usize,
    pub first: u32,
    pub count: u32,
}

/// Изогнутые копии частей всех перекрёстков в формате вершины модели (место, нормаль, точка картинки,
/// касательная, цвет) уже в осях мира и индексы по общему списку вершин; части одной модели лежат подряд.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct JunctionMesh {
    pub floats: Vec<f32>,
    pub indices: Vec<u32>,
    pub runs: Vec<JunctionRun>,
}

/// Числа на вершину в `JunctionMesh::floats`.
const JUNCTION_VERTEX_FLOATS: usize = 16;

/// «Вид дороги»: части `junctions` для видеокарты.
pub fn junction_mesh(junctions: &[Junction]) -> JunctionMesh {
    junction_mesh_of(&[junctions])
}

/// Части всех списков `lists` для видеокарты: перекрёстки и изогнутые копии кусков дорог одним набором буферов.
pub fn junction_mesh_of(lists: &[&[Junction]]) -> JunctionMesh {
    let mut order: Vec<&Junction> = lists.iter().flat_map(|list| list.iter()).collect();
    order.sort_by_key(|junction| junction.model);
    let mut mesh = JunctionMesh::default();
    let mut bases: Vec<Vec<u32>> = Vec::with_capacity(order.len());
    for junction in &order {
        let place = |local: f32, middle: f64| (f64::from(local) + middle) as f32;
        let mut part_bases = Vec::with_capacity(junction.parts.len());
        for part in &junction.parts {
            part_bases.push((mesh.floats.len() / JUNCTION_VERTEX_FLOATS) as u32);
            for vertex in &part.vertices {
                mesh.floats.extend([
                    place(vertex.position[0], junction.center[0]),
                    place(vertex.position[1], junction.center[1]),
                    place(vertex.position[2], junction.z),
                ]);
                mesh.floats.extend(vertex.normal);
                mesh.floats.extend(vertex.uv);
                mesh.floats.extend(vertex.tangent);
                mesh.floats.extend(vertex.color);
            }
        }
        bases.push(part_bases);
    }
    let mut start = 0;
    while start < order.len() {
        let model = order[start].model;
        let end = start
            + order[start..]
                .iter()
                .take_while(|junction| junction.model == model)
                .count();
        for part in 0..order[start].parts.len() {
            let first = mesh.indices.len() as u32;
            for (junction, part_bases) in order[start..end].iter().zip(&bases[start..end]) {
                if let Some(source) = junction.parts.get(part) {
                    mesh.indices
                        .extend(source.indices.iter().map(|index| part_bases[part] + index));
                }
            }
            let count = mesh.indices.len() as u32 - first;
            if count > 0 {
                mesh.runs.push(JunctionRun {
                    model,
                    part,
                    first,
                    count,
                });
            }
        }
        start = end;
    }
    mesh
}

/// Объекты мира с `model`, `position` и `size`, разложенные по моделям. Модель без частей не рисуется.
pub fn model_draws(game: &Game) -> ModelDraws {
    let world = &game.world;
    let models = world.models();
    let mut draws = ModelDraws {
        instances: vec![Vec::new(); models.len()],
        top: f64::NEG_INFINITY,
        blended: Vec::new(),
    };
    if models.is_empty() {
        return draws;
    }
    for id in world.ids() {
        let Some(name) = world.text(id, property::MODEL) else {
            continue;
        };
        let Some(index) = models.index_of(name) else {
            continue;
        };
        let (Some(model), Some(position), Some(size)) = (
            models.get(index),
            world.vec2(id, property::POSITION),
            world.vec2(id, property::SIZE),
        ) else {
            continue;
        };
        if model.parts.is_empty() {
            continue;
        }
        let scale = model.scale(world.number_like(id, property::HEIGHT));
        let (sin, cos) = world
            .rotation(id, property::ROTATION)
            .map_or((0.0, 1.0), Rotation::sin_cos);
        let base = world.base_z(id);
        let middle = [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0];
        draws.top = draws.top.max(base + model.size[2] * scale);
        if model.has_blend() {
            draws.blended.push((
                BlendedDraw {
                    model: index as u32,
                    index: draws.instances[index].len() as u32,
                },
                [middle[0], middle[1], base + model.size[2] * scale / 2.0],
            ));
        }
        draws.instances[index].push(ModelInstance {
            placement: [middle[0] as f32, middle[1] as f32, cos as f32, sin as f32],
            base: [base as f32, scale as f32, 1.0, world.tilt(id) as f32],
        });
    }
    draws
}

/// Одна ступень уменьшения картинки модели: четыре байта на точку.
#[derive(Debug, Clone, PartialEq)]
pub struct MipLevel {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

/// Ступени уменьшения картинки до одной точки, с первой, которая влезает в `max_side`. Цвет `srgb`
/// усредняется в линейной яркости, остальные карты — как есть.
pub fn mip_chain(image: &Image, srgb: bool, max_side: u32) -> Vec<MipLevel> {
    let mut level = MipLevel {
        width: image.width,
        height: image.height,
        pixels: image.pixels.clone(),
    };
    let mut chain = Vec::new();
    loop {
        let last = level.width <= 1 && level.height <= 1;
        let next = (!last).then(|| shrink(&level, srgb));
        if level.width <= max_side && level.height <= max_side {
            chain.push(level);
        }
        match next {
            Some(next) => level = next,
            None => return chain,
        }
    }
}

/// Вдвое меньше по каждой стороне (не меньше точки): каждая новая точка — среднее до четырёх старых.
fn shrink(level: &MipLevel, srgb: bool) -> MipLevel {
    let (width, height) = ((level.width / 2).max(1), (level.height / 2).max(1));
    let mut pixels = Vec::with_capacity(width as usize * height as usize * 4);
    let at = |x: u32, y: u32| (y as usize * level.width as usize + x as usize) * 4;
    for y in 0..height {
        for x in 0..width {
            let sources = [
                (2 * x, 2 * y),
                ((2 * x + 1).min(level.width - 1), 2 * y),
                (2 * x, (2 * y + 1).min(level.height - 1)),
                (
                    (2 * x + 1).min(level.width - 1),
                    (2 * y + 1).min(level.height - 1),
                ),
            ];
            for channel in 0..4 {
                let sum: f32 = sources
                    .iter()
                    .map(|&(sx, sy)| {
                        let byte = level.pixels[at(sx, sy) + channel];
                        if srgb && channel < 3 {
                            srgb_to_linear(byte)
                        } else {
                            f32::from(byte) / 255.0
                        }
                    })
                    .sum();
                let mean = sum / 4.0;
                pixels.push(if srgb && channel < 3 {
                    linear_to_srgb(mean)
                } else {
                    (mean * 255.0).round() as u8
                });
            }
        }
    }
    MipLevel {
        width,
        height,
        pixels,
    }
}

/// Картинка модели, как она ляжет в текстуру видеокарты: все ступени уменьшения подряд от самой крупной.
#[derive(Debug, Clone, PartialEq)]
pub struct TextureUpload {
    pub format: PackedFormat,
    pub width: u32,
    pub height: u32,
    pub levels: u32,
    pub data: Vec<u8>,
}

/// Точки, что разжала страница, со ступенями уменьшения до одной точки, начиная с первой, что влезает в `max_side`.
pub fn pixels_upload(image: &Image, srgb: bool, max_side: u32) -> TextureUpload {
    let chain = mip_chain(image, srgb, max_side);
    let (width, height) = chain.first().map_or((1, 1), |top| (top.width, top.height));
    TextureUpload {
        format: if srgb {
            PackedFormat::Rgba8Srgb
        } else {
            PackedFormat::Rgba8
        },
        width,
        height,
        levels: chain.len().max(1) as u32,
        data: chain.into_iter().flat_map(|level| level.pixels).collect(),
    }
}

/// Сжатый файл со своими ступенями из файла, начиная с первой, что влезает в `max_side`. Видеокарта читает BC
/// (`bc`) — блоки ложатся как есть; нет — каждая ступень разжимается на процессоре в четыре байта на точку.
pub fn packed_upload(image: &PackedImage, bc: bool, max_side: u32) -> TextureUpload {
    let kept: Vec<(u32, u32, &Vec<u8>)> = image
        .levels
        .iter()
        .enumerate()
        .map(|(level, data)| {
            let side = |full: u32| (full >> level).max(1);
            (side(image.width), side(image.height), data)
        })
        .filter(|&(width, height, _)| width <= max_side && height <= max_side)
        .collect();
    let (width, height) = kept
        .first()
        .map_or((1, 1), |&(width, height, _)| (width, height));
    let unpack = image.format.is_block() && !bc;
    let format = match (unpack, image.format.is_srgb()) {
        (false, _) => image.format,
        (true, true) => PackedFormat::Rgba8Srgb,
        (true, false) => PackedFormat::Rgba8,
    };
    let data = kept
        .iter()
        .flat_map(|&(width, height, data)| {
            if unpack {
                unpack_level(image.format, width, height, data)
            } else {
                data.clone()
            }
        })
        .collect();
    TextureUpload {
        format,
        width,
        height,
        levels: kept.len().max(1) as u32,
        data,
    }
}

/// Блоки одной ступени BC в четыре байта на точку; у BC5 синий и прозрачность нули. Ступень, чьи стороны не кратны
/// четырём, разжимается до целых блоков и обрезается.
fn unpack_level(format: PackedFormat, width: u32, height: u32, blocks: &[u8]) -> Vec<u8> {
    let (padded_width, padded_height) = (width.next_multiple_of(4), height.next_multiple_of(4));
    let variant = match format {
        PackedFormat::Bc5 => CompressionVariant::BC5,
        _ => CompressionVariant::BC7(BC7Settings::alpha_basic()),
    };
    let mut padded = vec![0u8; padded_width as usize * padded_height as usize * 4];
    decompress_blocks_as_rgba8(variant, padded_width, padded_height, blocks, &mut padded);
    if padded_width == width && padded_height == height {
        return padded;
    }
    let row = width as usize * 4;
    padded
        .chunks_exact(padded_width as usize * 4)
        .take(height as usize)
        .flat_map(|line| &line[..row])
        .copied()
        .collect()
}

/// Что ляжет в текстуру видеокарты для `image` в роли `srgb`: сжатый файл — со своим форматом, остальное — точки.
pub fn image_upload(image: &ModelImage, srgb: bool, bc: bool, max_side: u32) -> TextureUpload {
    match image {
        ModelImage::Pixels(image) => pixels_upload(image, srgb, max_side),
        ModelImage::Packed(image) => packed_upload(image, bc, max_side),
    }
}

/// Ширина, высота, роль цвета и хеш точек: по ним ищутся одинаковые картинки страницы.
type PixelsKey = (u32, u32, bool, u64);

/// Номера текстур картинок моделей игры: сжатый файл — одна текстура, сколько бы моделей и материалов на него ни
/// ссылалось, а одинаковые точки в одной роли цвета (sRGB или нет) — тоже одна.
#[derive(Default)]
pub struct ImageSlots<'a> {
    /// Картинка, уже названная по адресу в таблице моделей и роли: её точки второй раз не хешируются.
    visited: HashMap<(usize, bool), usize>,
    packed: HashMap<usize, usize>,
    pixels: HashMap<PixelsKey, Vec<(&'a Image, usize)>>,
    count: usize,
}

impl<'a> ImageSlots<'a> {
    /// Номер текстуры картинки `image` в роли `srgb` и признак, что номер новый — текстуру под него ещё не создавали.
    pub fn slot(&mut self, image: &'a ModelImage, srgb: bool) -> (usize, bool) {
        let address = (std::ptr::from_ref(image) as usize, srgb);
        if let Some(&slot) = self.visited.get(&address) {
            return (slot, false);
        }
        let known = match image {
            ModelImage::Packed(file) => self.packed.get(&(Arc::as_ptr(file) as usize)).copied(),
            ModelImage::Pixels(pixels) => self
                .pixels
                .get(&pixels_key(pixels, srgb))
                .and_then(|same| same.iter().find(|(other, _)| *other == pixels))
                .map(|&(_, slot)| slot),
        };
        let slot = known.unwrap_or(self.count);
        if known.is_none() {
            self.count += 1;
            match image {
                ModelImage::Packed(file) => {
                    self.packed.insert(Arc::as_ptr(file) as usize, slot);
                }
                ModelImage::Pixels(pixels) => self
                    .pixels
                    .entry(pixels_key(pixels, srgb))
                    .or_default()
                    .push((pixels, slot)),
            }
        }
        self.visited.insert(address, slot);
        (slot, known.is_none())
    }
}

fn pixels_key(image: &Image, srgb: bool) -> PixelsKey {
    let mut hasher = DefaultHasher::new();
    image.pixels.hash(&mut hasher);
    (image.width, image.height, srgb, hasher.finish())
}

/// Сколько текстур картинок моделей на видеокарте и сколько байт они занимают со всеми ступенями уменьшения.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImageStats {
    pub textures: usize,
    pub bytes: u64,
}

impl ImageStats {
    /// Строка консоли браузера после загрузки игры: «картинки моделей: 47 на видеокарте, 212 МБ».
    pub fn line(&self) -> String {
        let megabytes = self.bytes as f64 / (1024.0 * 1024.0);
        let size = if megabytes < 10.0 {
            format!("{megabytes:.1}").replace('.', ",")
        } else {
            format!("{}", megabytes.round() as u64)
        };
        format!(
            "картинки моделей: {} на видеокарте, {size} МБ",
            self.textures
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::model::{ModelVertex, Part};
    use block_compression::encode::compress_rgba8;

    fn image(width: u32, height: u32, pixel: [u8; 4]) -> Image {
        Image {
            width,
            height,
            pixels: pixel.repeat((width * height) as usize),
        }
    }

    /// Перекрёсток модели `model` из двух частей: квадрат со стороной 1 и один треугольник; вершины лежат
    /// относительно середины в `[at, 0]`.
    fn junction(model: usize, z: f64, at: f64) -> Junction {
        let vertex = |x: f32, y: f32| ModelVertex {
            position: [x, y, 0.0],
            normal: [0.0, 0.0, 1.0],
            uv: [x / 2.0, y / 2.0],
            tangent: [1.0, 0.0, 0.0, -1.0],
            color: [1.0, 1.0, 1.0, 0.5],
        };
        let square = Part {
            vertices: vec![
                vertex(0.0, 0.0),
                vertex(1.0, 0.0),
                vertex(1.0, 1.0),
                vertex(0.0, 1.0),
            ],
            indices: vec![0, 1, 2, 0, 2, 3],
            material: 0,
        };
        let triangle = Part {
            vertices: vec![vertex(0.0, 0.0), vertex(2.0, 0.0), vertex(0.0, 2.0)],
            indices: vec![0, 1, 2],
            material: 1,
        };
        Junction {
            model,
            center: [at, 3.0],
            width: 1.0,
            z,
            scale: 1.0,
            mirrored: false,
            arms: Vec::new(),
            parts: vec![square, triangle],
        }
    }

    #[test]
    fn junction_meshes_put_the_curved_vertices_in_the_world_and_keep_every_vertex_attribute() {
        let mesh = junction_mesh(&[junction(3, 0.75, 4.0)]);
        assert_eq!(mesh.floats.len(), 7 * JUNCTION_VERTEX_FLOATS);
        let third = &mesh.floats[2 * JUNCTION_VERTEX_FLOATS..3 * JUNCTION_VERTEX_FLOATS];
        assert_eq!(
            third,
            [
                5.0, 4.0,
                0.75, // место: середина перекрёстка и высота основания
                0.0, 0.0, 1.0, // нормаль
                0.5, 0.5, // точка картинки
                1.0, 0.0, 0.0, -1.0, // касательная
                1.0, 1.0, 1.0, 0.5, // цвет
            ]
        );
        assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 3, 4, 5, 6]);
        assert_eq!(
            mesh.runs,
            [
                JunctionRun {
                    model: 3,
                    part: 0,
                    first: 0,
                    count: 6
                },
                JunctionRun {
                    model: 3,
                    part: 1,
                    first: 6,
                    count: 3
                }
            ]
        );
    }

    #[test]
    fn junctions_of_one_model_lie_together_part_by_part_with_indices_into_the_shared_vertices() {
        let mesh = junction_mesh(&[
            junction(5, 0.0, 0.0),
            junction(2, 0.0, 10.0),
            junction(5, 0.0, 20.0),
        ]);
        let runs: Vec<(usize, usize, u32)> = mesh
            .runs
            .iter()
            .map(|run| (run.model, run.part, run.count))
            .collect();
        assert_eq!(runs, [(2, 0, 6), (2, 1, 3), (5, 0, 12), (5, 1, 6)]);
        assert_eq!(
            &mesh.indices[9..12],
            [7, 8, 9],
            "вершины первого перекрёстка модели 5 идут за семью вершинами модели 2"
        );
        let second_part = &mesh.runs[3];
        assert_eq!(
            &mesh.indices[second_part.first as usize..],
            [11, 12, 13, 18, 19, 20]
        );
        assert!(junction_mesh(&[]).indices.is_empty());
    }

    #[test]
    fn the_chain_goes_down_to_one_point_and_halves_every_side() {
        let chain = mip_chain(&image(8, 4, [10, 20, 30, 255]), false, 64);
        let sides: Vec<(u32, u32)> = chain.iter().map(|l| (l.width, l.height)).collect();
        assert_eq!(sides, [(8, 4), (4, 2), (2, 1), (1, 1)]);
        assert!(
            chain
                .iter()
                .all(|l| l.pixels == [10, 20, 30, 255].repeat((l.width * l.height) as usize))
        );
    }

    #[test]
    fn levels_that_do_not_fit_the_limit_are_dropped_from_the_top() {
        let chain = mip_chain(&image(16, 16, [1; 4]), false, 4);
        assert_eq!((chain[0].width, chain[0].height), (4, 4));
        assert_eq!(chain.len(), 3);
    }

    #[test]
    fn a_colour_is_averaged_in_linear_light_and_a_data_map_as_it_is() {
        let mut pixels = [0, 0, 0, 255].repeat(2);
        pixels.extend([255, 255, 255, 255].repeat(2));
        let source = Image {
            width: 2,
            height: 2,
            pixels,
        };
        let colour = &mip_chain(&source, true, 8)[1];
        // Среднее чёрного и белого в линейной яркости — 0,5, а это около 188 в sRGB.
        assert!(
            (i32::from(colour.pixels[0]) - 188).abs() <= 1,
            "{:?}",
            colour.pixels
        );
        let data = &mip_chain(&source, false, 8)[1];
        assert_eq!(data.pixels[0], 128);
    }

    #[test]
    fn odd_sides_do_not_read_past_the_edge() {
        let chain = mip_chain(&image(3, 5, [200; 4]), false, 64);
        assert_eq!(chain.last().map(|l| (l.width, l.height)), Some((1, 1)));
    }
    /// Рисунок с плавным переходом по обеим осям: красный и зелёный идут в разные стороны.
    fn gradient(width: u32, height: u32) -> Image {
        let mut pixels = Vec::new();
        for y in 0..height {
            for x in 0..width {
                pixels.extend([
                    (x * 255 / (width - 1).max(1)) as u8,
                    (y * 255 / (height - 1).max(1)) as u8,
                    128,
                    255,
                ]);
            }
        }
        Image {
            width,
            height,
            pixels,
        }
    }

    /// Сжатый файл из `image`: каждая ступень `mip_chain` сжимается отдельно, меньшие блока — целым блоком с краем.
    fn pack(image: &Image, format: PackedFormat) -> PackedImage {
        let variant = if format == PackedFormat::Bc5 {
            CompressionVariant::BC5
        } else {
            CompressionVariant::BC7(BC7Settings::opaque_basic())
        };
        let levels = mip_chain(image, format.is_srgb(), 1 << 16)
            .into_iter()
            .map(|level| {
                let width = level.width.next_multiple_of(4);
                let height = level.height.next_multiple_of(4);
                let mut padded = Vec::new();
                for y in 0..height {
                    for x in 0..width {
                        let (sx, sy) = (x.min(level.width - 1), y.min(level.height - 1));
                        let at = (sy * level.width + sx) as usize * 4;
                        padded.extend(&level.pixels[at..at + 4]);
                    }
                }
                let mut blocks = vec![0u8; variant.blocks_byte_size(width, height)];
                compress_rgba8(variant, &padded, &mut blocks, width, height, width * 4);
                blocks
            })
            .collect();
        PackedImage {
            format,
            width: image.width,
            height: image.height,
            levels,
        }
    }

    #[test]
    fn a_card_that_reads_bc_gets_the_blocks_of_the_file_as_they_are() {
        let packed = pack(&gradient(16, 8), PackedFormat::Bc7);
        let upload = packed_upload(&packed, true, 64);
        assert_eq!(upload.format, PackedFormat::Bc7);
        assert_eq!((upload.width, upload.height, upload.levels), (16, 8, 5));
        assert_eq!(upload.data, packed.levels.concat());
        assert_eq!(upload.data.len(), (4 * 2 + 2 + 1 + 1 + 1) * 16);
    }

    #[test]
    fn levels_that_do_not_fit_the_limit_are_dropped_from_the_top_of_a_packed_file() {
        let packed = pack(&gradient(16, 8), PackedFormat::Bc7Srgb);
        let upload = packed_upload(&packed, true, 8);
        assert_eq!((upload.width, upload.height, upload.levels), (8, 4, 4));
        assert_eq!(upload.data, packed.levels[1..].concat());
    }

    #[test]
    fn without_bc_every_level_is_unpacked_to_four_bytes_a_point_close_to_the_source() {
        let source = gradient(16, 8);
        let packed = pack(&source, PackedFormat::Bc7);
        let upload = packed_upload(&packed, false, 64);
        assert_eq!(upload.format, PackedFormat::Rgba8);
        assert_eq!((upload.width, upload.height, upload.levels), (16, 8, 5));
        let expected = pixels_upload(&source, false, 64);
        assert_eq!(
            upload.data.len(),
            expected.data.len(),
            "16×8, 8×4, 4×2, 2×1, 1×1"
        );
        let worst = upload
            .data
            .iter()
            .zip(&expected.data)
            .map(|(a, b)| a.abs_diff(*b))
            .max()
            .unwrap_or(0);
        assert!(worst <= 8, "BC7 отличается от исходной на {worst}");
    }

    #[test]
    fn an_unpacked_colour_keeps_its_srgb_role_and_a_bc5_map_its_two_channels() {
        let colour = packed_upload(&pack(&gradient(8, 8), PackedFormat::Bc7Srgb), false, 64);
        assert_eq!(colour.format, PackedFormat::Rgba8Srgb);
        let source = gradient(8, 8);
        let normal = packed_upload(&pack(&source, PackedFormat::Bc5), false, 64);
        assert_eq!(normal.format, PackedFormat::Rgba8);
        let top = &normal.data[..8 * 8 * 4];
        for (point, original) in top.chunks(4).zip(source.pixels.chunks(4)) {
            assert!(
                point[0].abs_diff(original[0]) <= 8 && point[1].abs_diff(original[1]) <= 8,
                "{point:?} {original:?}"
            );
        }
    }

    #[test]
    fn a_file_with_uncompressed_levels_is_not_touched_with_or_without_bc() {
        let source = gradient(5, 3);
        let levels: Vec<Vec<u8>> = mip_chain(&source, false, 64)
            .into_iter()
            .map(|level| level.pixels)
            .collect();
        let packed = PackedImage {
            format: PackedFormat::Rgba8,
            width: 5,
            height: 3,
            levels,
        };
        for bc in [true, false] {
            let upload = packed_upload(&packed, bc, 64);
            assert_eq!(upload.format, PackedFormat::Rgba8);
            assert_eq!(upload.data, packed.levels.concat());
        }
    }

    #[test]
    fn page_pixels_get_the_chain_of_the_engine_and_their_role() {
        let source = gradient(4, 4);
        let upload = image_upload(&ModelImage::Pixels(source.clone()), true, true, 64);
        assert_eq!(upload, pixels_upload(&source, true, 64));
        assert_eq!(upload.format, PackedFormat::Rgba8Srgb);
        assert_eq!(upload.levels, 3);
    }

    fn packed_image() -> ModelImage {
        ModelImage::Packed(Arc::new(pack(&gradient(4, 4), PackedFormat::Bc7)))
    }

    #[test]
    fn one_packed_file_is_one_texture_however_many_materials_name_it() {
        let file = packed_image();
        let other = packed_image();
        let mut slots = ImageSlots::default();
        assert_eq!(slots.slot(&file, true), (0, true));
        assert_eq!(slots.slot(&file, true), (0, false));
        assert_eq!(
            slots.slot(&file, false),
            (0, false),
            "роль у сжатого файла своя, в нём"
        );
        assert_eq!(
            slots.slot(&other, true),
            (1, true),
            "другой файл — другая текстура"
        );
    }

    #[test]
    fn equal_page_pixels_in_one_role_are_one_texture_and_a_new_role_or_point_is_another() {
        let one = ModelImage::Pixels(gradient(4, 4));
        let same = ModelImage::Pixels(gradient(4, 4));
        let mut changed = gradient(4, 4);
        changed.pixels[0] ^= 1;
        let changed = ModelImage::Pixels(changed);
        let mut slots = ImageSlots::default();
        assert_eq!(slots.slot(&one, true), (0, true));
        assert_eq!(slots.slot(&same, true), (0, false));
        assert_eq!(
            slots.slot(&one, false),
            (1, true),
            "цвет и данные — две текстуры"
        );
        assert_eq!(slots.slot(&changed, true), (2, true));
        assert_eq!(slots.slot(&same, false), (1, false));
    }

    #[test]
    fn the_console_line_counts_textures_and_megabytes_of_the_card() {
        let line = |textures, bytes| ImageStats { textures, bytes }.line();
        assert_eq!(
            line(47, 212 * 1024 * 1024 + 300_000),
            "картинки моделей: 47 на видеокарте, 212 МБ"
        );
        assert_eq!(
            line(1, 5_872_025),
            "картинки моделей: 1 на видеокарте, 5,6 МБ"
        );
        assert_eq!(line(0, 0), "картинки моделей: 0 на видеокарте, 0,0 МБ");
    }
}

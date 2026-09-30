//! «Свет и материалы» → «Материал», «Покрытия рельефа»: карты материалов и маски покрытий, собранные
//! для видеокарты без неё самой — упаковка в каналы двух текстур, ступени уменьшения, маски по
//! четыре в слой. Байты карт остаются у вызывающего: сюда они приходят взаймы и после отправки на
//! видеокарту не держатся.

use crate::core::terrain::{Cover, MAX_COVERS};
use crate::data::load::{ImageVerdict, MaterialDecl};

/// Самая большая сторона слоя масок: предел `downlevel_webgl2_defaults`.
const MAX_MASK_SIDE: u32 = 2048;

/// Строк таблицы слоёв для шейдера: заголовок и по строке на слой.
pub const COVER_TABLE_LEN: usize = 1 + MAX_COVERS;

/// Разжатая страницей картинка, четыре байта на точку.
#[derive(Debug, Clone, Copy)]
pub struct MapView<'a> {
    pub width: u32,
    pub height: u32,
    pub pixels: &'a [u8],
}

impl MapView<'_> {
    fn red(&self, index: usize) -> u8 {
        self.pixels[index * 4]
    }
}

/// Карты одного материала.
#[derive(Debug, Clone, Copy)]
pub struct MaterialMaps<'a> {
    pub color: MapView<'a>,
    pub normal: MapView<'a>,
    pub roughness: MapView<'a>,
    pub height: MapView<'a>,
    pub ao: Option<MapView<'a>>,
}

/// Всё, что отправляется на видеокарту при загрузке игры: материалы по номерам, маски покрытий в
/// порядке слоёв и таблица слоёв для шейдера.
#[derive(Debug)]
pub struct Relief<'a> {
    /// Сторона карт материалов в точках; 1, пока материалов нет.
    pub side: u32,
    pub materials: Vec<MaterialMaps<'a>>,
    pub masks: Vec<MapView<'a>>,
    /// Строка 0: число слоёв и размер сцены в клетках; дальше по строке на слой: номер материала и
    /// число карт на клетку сцены.
    pub table: [[f32; 4]; COVER_TABLE_LEN],
}

fn find_view<'a>(data: &'a [(String, ImageVerdict)], path: &str) -> Option<MapView<'a>> {
    let (_, verdict) = data.iter().find(|(name, _)| name == path)?;
    match verdict {
        ImageVerdict::Ok {
            width,
            height,
            pixels,
        } if !pixels.is_empty() && pixels.len() == *width as usize * *height as usize * 4 => {
            Some(MapView {
                width: *width,
                height: *height,
                pixels,
            })
        }
        _ => None,
    }
}

impl<'a> Relief<'a> {
    /// Материалы `decls`, слои `covers` и маски по их путям `mask_paths` из ответов страницы,
    /// которые уже прошли проверку загрузки. `None` — какой-то карты или маски в ответах нет.
    pub fn new(
        decls: &[MaterialDecl],
        material_data: &'a [(String, ImageVerdict)],
        covers: &[Cover],
        mask_paths: &[String],
        mask_data: &'a [(String, ImageVerdict)],
        scene: [u32; 2],
    ) -> Option<Relief<'a>> {
        let materials = decls
            .iter()
            .map(|decl| {
                let view = |path: &str| find_view(material_data, path);
                Some(MaterialMaps {
                    color: view(&decl.color)?,
                    normal: view(&decl.normal)?,
                    roughness: view(&decl.roughness)?,
                    height: view(&decl.height)?,
                    ao: match &decl.ao {
                        Some(path) => Some(view(path)?),
                        None => None,
                    },
                })
            })
            .collect::<Option<Vec<_>>>()?;
        let masks = mask_paths
            .iter()
            .map(|path| find_view(mask_data, path))
            .collect::<Option<Vec<_>>>()?;
        let side = materials.first().map_or(1, |maps| maps.color.width);
        Some(Relief {
            side,
            materials,
            masks,
            table: cover_table(covers, decls, scene),
        })
    }
}

/// Таблица слоёв для шейдера: строка 0 — число слоёв и размер сцены, дальше по слою снизу вверх.
fn cover_table(
    covers: &[Cover],
    decls: &[MaterialDecl],
    scene: [u32; 2],
) -> [[f32; 4]; COVER_TABLE_LEN] {
    let mut table = [[0.0; 4]; COVER_TABLE_LEN];
    let layers = covers.len().min(MAX_COVERS);
    table[0] = [layers as f32, scene[0] as f32, scene[1] as f32, 0.0];
    for (row, cover) in table[1..].iter_mut().zip(covers) {
        let size = decls.get(cover.material).map_or(1.0, |decl| decl.size);
        *row = [cover.material as f32, (1.0 / size) as f32, 0.0, 0.0];
    }
    table
}

/// Карты одного материала в двух текстурах, со всеми ступенями уменьшения от полной до 1 × 1.
/// Цвет — sRGB, в прозрачности высота; нормаль `xy`, шероховатость и затенение — линейные.
#[derive(Debug, PartialEq, Eq)]
pub struct PackedMaterial {
    pub side: u32,
    pub color: Vec<Vec<u8>>,
    pub data: Vec<Vec<u8>>,
}

/// Сторона ступени `level` квадрата со стороной `side`.
pub fn level_side(side: u32, level: usize) -> u32 {
    (side >> level).max(1)
}

/// Число ступеней квадрата со стороной `side`: от полной до 1 × 1.
pub fn level_count(side: u32) -> usize {
    (32 - side.max(1).leading_zeros()) as usize
}

fn srgb_to_linear(value: u8) -> f32 {
    let c = f32::from(value) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> u8 {
    let c = value.clamp(0.0, 1.0);
    let encoded = if c <= 0.003_130_8 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    };
    (encoded * 255.0).round() as u8
}

fn unit_byte(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// Единичная нормаль из записанных в байтах `x` и `y`: `z` восстановлена.
fn decode_normal(x: u8, y: u8) -> [f32; 3] {
    let x = f32::from(x) / 255.0 * 2.0 - 1.0;
    let y = f32::from(y) / 255.0 * 2.0 - 1.0;
    [x, y, (1.0 - x * x - y * y).max(0.0).sqrt()]
}

fn base_levels(maps: &MaterialMaps<'_>) -> (Vec<u8>, Vec<u8>) {
    let points = (maps.color.width * maps.color.height) as usize;
    let mut color = Vec::with_capacity(points * 4);
    let mut data = Vec::with_capacity(points * 4);
    for point in 0..points {
        let at = point * 4;
        color.extend_from_slice(&maps.color.pixels[at..at + 3]);
        color.push(maps.height.red(point));
        data.extend_from_slice(&[
            maps.normal.pixels[at],
            maps.normal.pixels[at + 1],
            maps.roughness.red(point),
            maps.ao.map_or(255, |ao| ao.red(point)),
        ]);
    }
    (color, data)
}

/// Ступень вдвое меньше: цвет усреднён в линейной яркости, высота — как есть, нормали усреднены
/// векторами и снова единичной длины, шероховатость и затенение — как есть.
fn shrink(color: &[u8], data: &[u8], side: u32, lut: &[f32; 256]) -> (Vec<u8>, Vec<u8>) {
    let out = level_side(side, 1) as usize;
    let side = side as usize;
    let mut color_out = Vec::with_capacity(out * out * 4);
    let mut data_out = Vec::with_capacity(out * out * 4);
    for y in 0..out {
        for x in 0..out {
            let block = [(0, 0), (1, 0), (0, 1), (1, 1)].map(|(dx, dy)| {
                let (sx, sy) = ((2 * x + dx).min(side - 1), (2 * y + dy).min(side - 1));
                (sy * side + sx) * 4
            });
            for channel in 0..3 {
                let sum: f32 = block
                    .iter()
                    .map(|&at| lut[color[at + channel] as usize])
                    .sum();
                color_out.push(linear_to_srgb(sum / 4.0));
            }
            let height: f32 = block.iter().map(|&at| f32::from(color[at + 3])).sum();
            color_out.push((height / 4.0).round() as u8);

            let mut normal = [0.0f32; 3];
            for &at in &block {
                let n = decode_normal(data[at], data[at + 1]);
                (0..3).for_each(|axis| normal[axis] += n[axis]);
            }
            let length = normal.iter().map(|c| c * c).sum::<f32>().sqrt();
            let normal = if length > 1e-6 {
                normal.map(|c| c / length)
            } else {
                [0.0, 0.0, 1.0]
            };
            data_out.push(unit_byte(normal[0] * 0.5 + 0.5));
            data_out.push(unit_byte(normal[1] * 0.5 + 0.5));
            for channel in 2..4 {
                let sum: f32 = block.iter().map(|&at| f32::from(data[at + channel])).sum();
                data_out.push((sum / 4.0).round() as u8);
            }
        }
    }
    (color_out, data_out)
}

/// Собирает текстуры материала из его карт. Карты — квадраты одной стороны: это уже проверила
/// загрузка.
pub fn pack_material(maps: &MaterialMaps<'_>) -> PackedMaterial {
    let side = maps.color.width;
    let lut: [f32; 256] = std::array::from_fn(|value| srgb_to_linear(value as u8));
    let (mut color, mut data) = base_levels(maps);
    let mut packed = PackedMaterial {
        side,
        color: Vec::new(),
        data: Vec::new(),
    };
    for level in 1..level_count(side) {
        let (next_color, next_data) = shrink(&color, &data, level_side(side, level - 1), &lut);
        packed.color.push(std::mem::replace(&mut color, next_color));
        packed.data.push(std::mem::replace(&mut data, next_data));
    }
    packed.color.push(color);
    packed.data.push(data);
    packed
}

/// Маски покрытий в слоях по четыре: маска `i` — канал `i % 4` слоя `i / 4`.
#[derive(Debug, PartialEq, Eq)]
pub struct PackedMasks {
    pub width: u32,
    pub height: u32,
    pub layers: Vec<Vec<u8>>,
}

/// Красный канал `mask`, приведённый билинейно к `width × height`.
fn resample_red(mask: &MapView<'_>, width: u32, height: u32) -> Vec<u8> {
    if (mask.width, mask.height) == (width, height) {
        return (0..(width * height) as usize)
            .map(|index| mask.red(index))
            .collect();
    }
    let source = |dst: u32, dst_side: u32, src_side: u32| {
        let at = (dst as f32 + 0.5) * src_side as f32 / dst_side as f32 - 0.5;
        let at = at.clamp(0.0, (src_side - 1) as f32);
        let low = at.floor() as u32;
        (low, (low + 1).min(src_side - 1), at - low as f32)
    };
    let mut out = Vec::with_capacity((width * height) as usize);
    for y in 0..height {
        let (y0, y1, fy) = source(y, height, mask.height);
        for x in 0..width {
            let (x0, x1, fx) = source(x, width, mask.width);
            let at = |px: u32, py: u32| f32::from(mask.red((py * mask.width + px) as usize));
            let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
            out.push((top * (1.0 - fy) + bottom * fy).round() as u8);
        }
    }
    out
}

/// Приводит маски к размеру самой крупной (но не больше слоя, который держит видеокарта) и кладёт
/// по четыре в слой RGBA; лишние каналы пусты. Слой есть всегда, даже без масок.
pub fn pack_masks(masks: &[MapView<'_>]) -> PackedMasks {
    let width = masks
        .iter()
        .map(|mask| mask.width)
        .max()
        .unwrap_or(1)
        .min(MAX_MASK_SIDE);
    let height = masks
        .iter()
        .map(|mask| mask.height)
        .max()
        .unwrap_or(1)
        .min(MAX_MASK_SIDE);
    let points = (width * height) as usize;
    let mut layers = vec![vec![0u8; points * 4]; masks.len().div_ceil(4).max(1)];
    for (index, mask) in masks.iter().enumerate() {
        let layer = &mut layers[index / 4];
        for (point, value) in resample_red(mask, width, height).into_iter().enumerate() {
            layer[point * 4 + index % 4] = value;
        }
    }
    PackedMasks {
        width,
        height,
        layers,
    }
}

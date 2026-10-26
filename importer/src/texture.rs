//! Одна картинка модели → файл KTX2: разжатие PNG и JPEG, уменьшенные копии движка, сжатие каждой.

use anyhow::{Context, Result, bail};
use block_compression::{BC7Settings, CompressionVariant, encode::compress_rgba8};
use engine::core::model::Image;
use engine::render::models::{MipLevel, mip_chain};
use rayon::prelude::*;
use sha2::{Digest, Sha256};

use crate::ktx2_writer::{self, Format};

/// Версия формата службы: входит в хеш файла картинки и в `index.json`.
pub const FORMAT_VERSION: u32 = 1;

const STRIP_ROWS: usize = 16;

/// Роль картинки в материале, от которой зависит формат.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Color,
    Data,
    Normal,
}

impl Role {
    fn tag(self) -> &'static str {
        match self {
            Role::Color => "color",
            Role::Data => "data",
            Role::Normal => "normal",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Png,
    Jpeg,
}

impl Encoding {
    pub fn from_mime(mime: &str) -> Option<Encoding> {
        match mime {
            "image/png" => Some(Encoding::Png),
            "image/jpeg" => Some(Encoding::Jpeg),
            _ => None,
        }
    }

    fn format(self) -> image::ImageFormat {
        match self {
            Encoding::Png => image::ImageFormat::Png,
            Encoding::Jpeg => image::ImageFormat::Jpeg,
        }
    }
}

/// Имя файла картинки без расширения: хеш байтов исходной картинки, роли и версии формата.
pub fn file_stem(source: &[u8], role: Role) -> String {
    let mut hasher = Sha256::new();
    hasher.update(FORMAT_VERSION.to_le_bytes());
    hasher.update(role.tag().as_bytes());
    hasher.update([0]);
    hasher.update(source);
    hex(&hasher.finalize()[..16])
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// Содержимое файла `.ktx2` картинки `source`, лежащей в файле модели в кодировке `encoding`.
pub fn encode(source: &[u8], encoding: Encoding, role: Role) -> Result<Vec<u8>> {
    let decoded = image::load_from_memory_with_format(source, encoding.format())
        .context("картинка не разжимается")?
        .to_rgba8();
    let (width, height) = decoded.dimensions();
    if width == 0 || height == 0 {
        bail!("у картинки нет точек");
    }
    let image = Image {
        width,
        height,
        pixels: decoded.into_raw(),
    };
    let format = choose_format(&image, role);
    let chain = mip_chain(&image, role == Role::Color, u32::MAX);
    let alpha = has_alpha(&image);
    let levels: Vec<Vec<u8>> = chain
        .par_iter()
        .map(|level| encode_level(level, format, alpha))
        .collect();
    Ok(ktx2_writer::write(format, width, height, &levels))
}

fn has_alpha(image: &Image) -> bool {
    image
        .pixels
        .iter()
        .skip(3)
        .step_by(4)
        .any(|&alpha| alpha < 255)
}

fn choose_format(image: &Image, role: Role) -> Format {
    let srgb = role == Role::Color;
    if !image.width.is_multiple_of(4) || !image.height.is_multiple_of(4) {
        return if srgb {
            Format::Rgba8Srgb
        } else {
            Format::Rgba8
        };
    }
    match role {
        Role::Color => Format::Bc7Srgb,
        Role::Data => Format::Bc7,
        Role::Normal => Format::Bc5,
    }
}

fn encode_level(level: &MipLevel, format: Format, alpha: bool) -> Vec<u8> {
    let variant = match format {
        Format::Bc7Srgb | Format::Bc7 if alpha => {
            CompressionVariant::BC7(BC7Settings::alpha_basic())
        }
        Format::Bc7Srgb | Format::Bc7 => CompressionVariant::BC7(BC7Settings::opaque_basic()),
        Format::Bc5 => CompressionVariant::BC5,
        Format::Rgba8Srgb | Format::Rgba8 => return level.pixels.clone(),
    };
    compress(level, variant)
}

/// Ступень дополняется до целых блоков 4 × 4 крайними точками и сжимается полосами по `STRIP_ROWS` рядов
/// параллельно.
fn compress(level: &MipLevel, variant: CompressionVariant) -> Vec<u8> {
    let (width, height) = (level.width as usize, level.height as usize);
    let (padded_width, padded_height) = (width.next_multiple_of(4), height.next_multiple_of(4));
    let mut padded = Vec::with_capacity(padded_width * padded_height * 4);
    for y in 0..padded_height {
        let row = y.min(height - 1) * width * 4;
        padded.extend_from_slice(&level.pixels[row..row + width * 4]);
        let edge = &level.pixels[row + (width - 1) * 4..row + width * 4];
        for _ in width..padded_width {
            padded.extend_from_slice(edge);
        }
    }
    let row_bytes = padded_width * 4;
    let block_row_bytes = variant.bytes_per_row(padded_width as u32) as usize;
    let mut blocks = vec![0u8; block_row_bytes * (padded_height / 4)];
    padded
        .par_chunks(row_bytes * STRIP_ROWS)
        .zip(blocks.par_chunks_mut(block_row_bytes * STRIP_ROWS / 4))
        .for_each(|(pixels, out)| {
            compress_rgba8(
                variant,
                pixels,
                out,
                padded_width as u32,
                (pixels.len() / row_bytes) as u32,
                row_bytes as u32,
            );
        });
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;
    use block_compression::decode::decompress_blocks_as_rgba8;
    use image::codecs::png::PngEncoder;
    use image::{ExtendedColorType, ImageEncoder};

    fn png(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [u8; 4]) -> Vec<u8> {
        let mut raw = Vec::new();
        for y in 0..height {
            for x in 0..width {
                raw.extend_from_slice(&pixel(x, y));
            }
        }
        let mut out = Vec::new();
        PngEncoder::new(&mut out)
            .write_image(&raw, width, height, ExtendedColorType::Rgba8)
            .expect("png encodes");
        out
    }

    fn read(file: &[u8]) -> (Format, Vec<(Vec<u8>, u32, u32)>) {
        let reader = ktx2::Reader::new(file).expect("ktx2 reads the file");
        let header = reader.header();
        let format = match header.format {
            Some(ktx2::Format::BC7_SRGB_BLOCK) => Format::Bc7Srgb,
            Some(ktx2::Format::BC7_UNORM_BLOCK) => Format::Bc7,
            Some(ktx2::Format::BC5_UNORM_BLOCK) => Format::Bc5,
            Some(ktx2::Format::R8G8B8A8_SRGB) => Format::Rgba8Srgb,
            Some(ktx2::Format::R8G8B8A8_UNORM) => Format::Rgba8,
            other => panic!("unexpected format {other:?}"),
        };
        let levels = reader
            .levels()
            .enumerate()
            .map(|(index, level)| {
                let side = |full: u32| (full >> index).max(1);
                (
                    level.data.to_vec(),
                    side(header.pixel_width),
                    side(header.pixel_height),
                )
            })
            .collect();
        (format, levels)
    }

    #[test]
    fn roles_pick_their_formats() {
        let source = png(16, 8, |x, y| [x as u8 * 10, y as u8 * 20, 90, 255]);
        let formats = [
            (Role::Color, Format::Bc7Srgb),
            (Role::Data, Format::Bc7),
            (Role::Normal, Format::Bc5),
        ];
        for (role, expected) in formats {
            let (format, _) = read(&encode(&source, Encoding::Png, role).unwrap());
            assert_eq!(format, expected);
        }
    }

    #[test]
    fn a_side_not_divisible_by_four_stays_uncompressed() {
        let source = png(10, 8, |x, _| [x as u8, 0, 0, 255]);
        let (colour, _) = read(&encode(&source, Encoding::Png, Role::Color).unwrap());
        let (data, levels) = read(&encode(&source, Encoding::Png, Role::Normal).unwrap());
        assert_eq!(colour, Format::Rgba8Srgb);
        assert_eq!(data, Format::Rgba8);
        assert_eq!(levels[0].0.len(), 10 * 8 * 4);
    }

    #[test]
    fn every_level_down_to_one_point_is_present_with_its_size() {
        let source = png(20, 12, |x, y| [x as u8, y as u8, 0, 255]);
        let (_, levels) = read(&encode(&source, Encoding::Png, Role::Color).unwrap());
        let sizes: Vec<(u32, u32)> = levels.iter().map(|level| (level.1, level.2)).collect();
        assert_eq!(sizes, [(20, 12), (10, 6), (5, 3), (2, 1), (1, 1)]);
        for (data, width, height) in &levels {
            assert_eq!(
                data.len(),
                width.div_ceil(4) as usize * height.div_ceil(4) as usize * 16
            );
        }
    }

    #[test]
    fn compressed_level_decodes_close_to_the_source() {
        let source = png(32, 32, |x, y| [(x * 8) as u8, (y * 8) as u8, 128, 255]);
        let (_, levels) = read(&encode(&source, Encoding::Png, Role::Data).unwrap());
        let mut decoded = vec![0u8; 32 * 32 * 4];
        decompress_blocks_as_rgba8(
            CompressionVariant::BC7(BC7Settings::opaque_basic()),
            32,
            32,
            &levels[0].0,
            &mut decoded,
        );
        for y in 0..32usize {
            for x in 0..32usize {
                let at = (y * 32 + x) * 4;
                assert!(decoded[at].abs_diff((x * 8) as u8) <= 4, "red at {x},{y}");
                assert!(
                    decoded[at + 1].abs_diff((y * 8) as u8) <= 4,
                    "green at {x},{y}"
                );
            }
        }
    }

    #[test]
    fn normal_map_keeps_red_and_green() {
        let source = png(8, 8, |x, y| [(x * 30) as u8, (y * 30) as u8, 255, 255]);
        let (_, levels) = read(&encode(&source, Encoding::Png, Role::Normal).unwrap());
        let mut decoded = vec![0u8; 8 * 8 * 4];
        decompress_blocks_as_rgba8(CompressionVariant::BC5, 8, 8, &levels[0].0, &mut decoded);
        for y in 0..8usize {
            for x in 0..8usize {
                let at = (y * 8 + x) * 4;
                assert!(decoded[at].abs_diff((x * 30) as u8) <= 8);
                assert!(decoded[at + 1].abs_diff((y * 30) as u8) <= 8);
            }
        }
    }

    #[test]
    fn tiny_levels_are_whole_blocks() {
        let source = png(4, 4, |_, _| [200, 100, 50, 255]);
        let (_, levels) = read(&encode(&source, Encoding::Png, Role::Color).unwrap());
        assert_eq!(levels.len(), 3);
        assert!(levels.iter().all(|level| level.0.len() == 16));
    }

    #[test]
    fn transparent_picture_uses_the_alpha_variant() {
        let source = png(8, 8, |x, _| [255, 255, 255, if x < 4 { 0 } else { 255 }]);
        let (_, levels) = read(&encode(&source, Encoding::Png, Role::Color).unwrap());
        let mut decoded = vec![0u8; 8 * 8 * 4];
        decompress_blocks_as_rgba8(
            CompressionVariant::BC7(BC7Settings::alpha_basic()),
            8,
            8,
            &levels[0].0,
            &mut decoded,
        );
        assert!(decoded[3] < 16);
        assert!(decoded[7 * 4 + 3] > 240);
    }

    #[test]
    fn broken_picture_is_an_error() {
        assert!(encode(b"not a picture", Encoding::Png, Role::Color).is_err());
    }

    #[test]
    fn name_depends_on_bytes_and_role() {
        let a = png(4, 4, |_, _| [1, 2, 3, 255]);
        let b = png(4, 4, |_, _| [3, 2, 1, 255]);
        assert_eq!(file_stem(&a, Role::Color), file_stem(&a, Role::Color));
        assert_ne!(file_stem(&a, Role::Color), file_stem(&b, Role::Color));
        assert_ne!(file_stem(&a, Role::Color), file_stem(&a, Role::Normal));
    }
}

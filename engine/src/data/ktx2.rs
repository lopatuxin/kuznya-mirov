//! «Трёхмерная сцена» → «Модели и движения»: сжатая картинка модели из контейнера KTX2 службы сжатия — без
//! дополнительного сжатия, один слой, одна грань, ступени уменьшения от полного размера до одной точки.

use crate::core::model::{PackedFormat, PackedImage};

const IDENTIFIER: [u8; 12] = [
    0xAB, b'K', b'T', b'X', b' ', b'2', b'0', 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
const HEADER_BYTES: usize = 80;
const LEVEL_INDEX_ENTRY_BYTES: usize = 24;
/// Число ступеней не больше, чем у картинки со стороной в 2^32.
const MAX_LEVELS: u32 = 32;

const VK_RGBA8_UNORM: u32 = 37;
const VK_RGBA8_SRGB: u32 = 43;
const VK_BC5_UNORM: u32 = 141;
const VK_BC7_UNORM: u32 = 145;
const VK_BC7_SRGB: u32 = 146;

fn format_of(vk_format: u32) -> Option<PackedFormat> {
    match vk_format {
        VK_RGBA8_UNORM => Some(PackedFormat::Rgba8),
        VK_RGBA8_SRGB => Some(PackedFormat::Rgba8Srgb),
        VK_BC5_UNORM => Some(PackedFormat::Bc5),
        VK_BC7_UNORM => Some(PackedFormat::Bc7),
        VK_BC7_SRGB => Some(PackedFormat::Bc7Srgb),
        _ => None,
    }
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32, String> {
    bytes
        .get(at..at + 4)
        .and_then(|slice| slice.try_into().ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| "файл обрывается в заголовке".to_string())
}

fn read_u64(bytes: &[u8], at: usize) -> Result<u64, String> {
    bytes
        .get(at..at + 8)
        .and_then(|slice| slice.try_into().ok())
        .map(u64::from_le_bytes)
        .ok_or_else(|| "файл обрывается в списке ступеней".to_string())
}

/// Читает файл KTX2 в картинку модели. Любое отступление от того, что пишет служба сжатия, — ошибка с причиной.
pub fn parse_ktx2(bytes: &[u8]) -> Result<PackedImage, String> {
    if bytes.len() < HEADER_BYTES || bytes[..12] != IDENTIFIER {
        return Err("это не файл KTX2".to_string());
    }
    let vk_format = read_u32(bytes, 12)?;
    let format = format_of(vk_format)
        .ok_or_else(|| format!("формат {vk_format} не из тех, что пишет служба сжатия"))?;
    let width = read_u32(bytes, 20)?;
    let height = read_u32(bytes, 24)?;
    let depth = read_u32(bytes, 28)?;
    let layers = read_u32(bytes, 32)?;
    let faces = read_u32(bytes, 36)?;
    let level_count = read_u32(bytes, 40)?;
    let supercompression = read_u32(bytes, 44)?;
    if width == 0 || height == 0 || depth > 1 || layers > 1 || faces != 1 {
        return Err("ожидалась плоская картинка в один слой".to_string());
    }
    if supercompression != 0 {
        return Err("файл сжат поверх блоков".to_string());
    }
    if format.is_block() && !(width.is_multiple_of(4) && height.is_multiple_of(4)) {
        return Err(format!(
            "картинка {width} × {height} в блоках BC: стороны должны делиться на 4"
        ));
    }
    let expected_levels = u32::BITS - width.max(height).leading_zeros();
    if level_count != expected_levels || level_count > MAX_LEVELS {
        return Err(format!(
            "ступеней уменьшения {level_count}, а картинке {width} × {height} нужно {expected_levels}"
        ));
    }
    let mut levels = Vec::with_capacity(level_count as usize);
    for level in 0..level_count as usize {
        let entry = HEADER_BYTES + level * LEVEL_INDEX_ENTRY_BYTES;
        let offset = read_u64(bytes, entry)?;
        let length = read_u64(bytes, entry + 8)?;
        let uncompressed = read_u64(bytes, entry + 16)?;
        let side = |full: u32| (full >> level).max(1);
        let expected = format.level_bytes(side(width), side(height));
        if length != expected as u64 || uncompressed != length {
            return Err(format!(
                "ступень {level} занимает {length} байт, а нужно {expected}"
            ));
        }
        let data = usize::try_from(offset)
            .ok()
            .and_then(|start| Some(start..start.checked_add(expected)?))
            .and_then(|range| bytes.get(range))
            .ok_or_else(|| format!("ступень {level} лежит за концом файла"))?;
        levels.push(data.to_vec());
    }
    Ok(PackedImage {
        format,
        width,
        height,
        levels,
    })
}

/// Файл KTX2 в том виде, как его пишет служба сжатия: для проверок движка и его тестов.
#[cfg(test)]
pub(crate) fn write_ktx2(vk_format: u32, width: u32, height: u32, levels: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend(IDENTIFIER);
    for value in [vk_format, 1, width, height, 0, 0, 1, levels.len() as u32, 0] {
        out.extend(value.to_le_bytes());
    }
    out.extend([0u8; 32]);
    let mut offset = HEADER_BYTES + levels.len() * LEVEL_INDEX_ENTRY_BYTES;
    let mut index = Vec::new();
    let mut data = Vec::new();
    for level in levels {
        offset = offset.next_multiple_of(16);
        data.resize(
            offset - HEADER_BYTES - levels.len() * LEVEL_INDEX_ENTRY_BYTES,
            0,
        );
        for value in [offset, level.len(), level.len()] {
            index.extend((value as u64).to_le_bytes());
        }
        data.extend(level);
        offset += level.len();
    }
    out.extend(index);
    out.extend(data);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chain(format: PackedFormat, width: u32, height: u32) -> Vec<Vec<u8>> {
        let count = u32::BITS - width.max(height).leading_zeros();
        (0..count)
            .map(|level| {
                let side = |full: u32| (full >> level).max(1);
                vec![level as u8 + 1; format.level_bytes(side(width), side(height))]
            })
            .collect()
    }

    #[test]
    fn every_format_of_the_importer_reads_with_all_its_levels() {
        let formats = [
            (VK_BC7_SRGB, PackedFormat::Bc7Srgb),
            (VK_BC7_UNORM, PackedFormat::Bc7),
            (VK_BC5_UNORM, PackedFormat::Bc5),
            (VK_RGBA8_SRGB, PackedFormat::Rgba8Srgb),
            (VK_RGBA8_UNORM, PackedFormat::Rgba8),
        ];
        for (vk, format) in formats {
            let levels = chain(format, 16, 8);
            let image = parse_ktx2(&write_ktx2(vk, 16, 8, &levels))
                .unwrap_or_else(|e| panic!("{format:?}: {e}"));
            assert_eq!(image.format, format);
            assert_eq!((image.width, image.height), (16, 8));
            assert_eq!(image.levels, levels, "{format:?}");
            assert_eq!(image.levels.len(), 5, "16 → 8 → 4 → 2 → 1");
        }
    }

    #[test]
    fn levels_smaller_than_a_block_are_one_whole_block() {
        let image = parse_ktx2(&write_ktx2(
            VK_BC7_UNORM,
            4,
            4,
            &chain(PackedFormat::Bc7, 4, 4),
        ))
        .expect("файл читается");
        assert_eq!(image.levels.len(), 3);
        assert!(image.levels.iter().all(|level| level.len() == 16));
    }

    #[test]
    fn a_block_format_with_a_side_not_divisible_by_four_is_an_error() {
        for (vk, format) in [
            (VK_BC7_SRGB, PackedFormat::Bc7Srgb),
            (VK_BC7_UNORM, PackedFormat::Bc7),
            (VK_BC5_UNORM, PackedFormat::Bc5),
        ] {
            for (width, height) in [(6, 8), (8, 6), (5, 5)] {
                let file = write_ktx2(vk, width, height, &chain(format, width, height));
                let message = parse_ktx2(&file).expect_err("сторона не кратна 4");
                assert!(
                    message.contains("делиться на 4")
                        && message.contains(&format!("{width} × {height}")),
                    "{format:?} {width} × {height}: {message}"
                );
            }
        }
        for vk in [VK_RGBA8_SRGB, VK_RGBA8_UNORM] {
            let format = format_of(vk).expect("формат известен");
            assert!(parse_ktx2(&write_ktx2(vk, 6, 8, &chain(format, 6, 8))).is_ok());
        }
    }

    #[test]
    fn an_odd_side_halves_down_and_stops_at_one_point() {
        let levels = chain(PackedFormat::Rgba8, 5, 3);
        let image = parse_ktx2(&write_ktx2(VK_RGBA8_UNORM, 5, 3, &levels)).expect("файл читается");
        let sizes: Vec<usize> = image.levels.iter().map(Vec::len).collect();
        assert_eq!(sizes, [60, 8, 4], "5×3, 2×1, 1×1");
    }

    fn broken(change: impl FnOnce(&mut Vec<u8>)) -> String {
        let mut file = write_ktx2(VK_BC7_UNORM, 8, 8, &chain(PackedFormat::Bc7, 8, 8));
        change(&mut file);
        parse_ktx2(&file).expect_err("файл испорчен")
    }

    fn set_u32(file: &mut [u8], at: usize, value: u32) {
        file[at..at + 4].copy_from_slice(&value.to_le_bytes());
    }

    #[test]
    fn a_file_that_departs_from_the_importer_format_is_an_error() {
        assert!(broken(|file| file[0] = 0).contains("не файл KTX2"));
        assert!(broken(|file| file.truncate(40)).contains("не файл KTX2"));
        assert!(broken(|file| set_u32(file, 12, 98)).contains("98"));
        assert!(broken(|file| set_u32(file, 44, 1)).contains("сжат поверх"));
        assert!(broken(|file| set_u32(file, 36, 6)).contains("плоская"));
        assert!(broken(|file| set_u32(file, 32, 2)).contains("плоская"));
        assert!(broken(|file| set_u32(file, 20, 0)).contains("плоская"));
        assert!(broken(|file| set_u32(file, 40, 2)).contains("ступеней уменьшения 2"));
        assert!(broken(|file| set_u32(file, 40, 0)).contains("ступеней уменьшения 0"));
    }

    #[test]
    fn a_wrong_size_of_a_level_or_a_level_past_the_end_is_an_error() {
        assert!(broken(|file| file[HEADER_BYTES + 8] = 17).contains("ступень 0"));
        assert!(broken(|file| file[HEADER_BYTES + 16] = 17).contains("ступень 0"));
        assert!(broken(|file| file.truncate(file.len() - 1)).contains("за концом файла"));
        assert!(broken(|file| file[HEADER_BYTES + 7] = 0xFF).contains("за концом файла"));
    }
}

//! Запись KTX2 без дополнительного сжатия и без пар «ключ — значение»: одна плоская картинка со всеми
//! уменьшенными копиями до точки.

const IDENTIFIER: [u8; 12] = [
    0xAB, 0x4B, 0x54, 0x58, 0x20, 0x32, 0x30, 0xBB, 0x0D, 0x0A, 0x1A, 0x0A,
];
const HEADER_SIZE: usize = 80;
const LEVEL_INDEX_ENTRY: usize = 24;
const MAX_LEVELS: usize = 32;
/// Сколько байт с начала файла хватает, чтобы прочитать заголовок и список ступеней любого файла службы.
pub const HEAD_LIMIT: usize = HEADER_SIZE + LEVEL_INDEX_ENTRY * MAX_LEVELS;

const MODEL_RGBSDA: u32 = 1;
const MODEL_BC5: u32 = 132;
const MODEL_BC7: u32 = 134;
const PRIMARIES_BT709: u32 = 1;
const TRANSFER_LINEAR: u32 = 1;
const TRANSFER_SRGB: u32 = 2;
const CHANNEL_RED: u32 = 0;
const CHANNEL_GREEN: u32 = 1;
const CHANNEL_BLUE: u32 = 2;
const CHANNEL_ALPHA: u32 = 15;
const QUALIFIER_LINEAR: u32 = 0x10;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Bc7Srgb,
    Bc7,
    Bc5,
    Rgba8Srgb,
    Rgba8,
}

impl Format {
    pub fn vk_format(self) -> u32 {
        match self {
            Format::Bc7Srgb => 146,
            Format::Bc7 => 145,
            Format::Bc5 => 141,
            Format::Rgba8Srgb => 43,
            Format::Rgba8 => 37,
        }
    }

    pub fn is_block_compressed(self) -> bool {
        matches!(self, Format::Bc7Srgb | Format::Bc7 | Format::Bc5)
    }

    pub fn bytes_per_block(self) -> usize {
        if self.is_block_compressed() { 16 } else { 4 }
    }

    pub fn is_srgb(self) -> bool {
        matches!(self, Format::Bc7Srgb | Format::Rgba8Srgb)
    }

    fn data_format_descriptor(self) -> Vec<u8> {
        let transfer = if self.is_srgb() {
            TRANSFER_SRGB
        } else {
            TRANSFER_LINEAR
        };
        let (model, block, samples) = match self {
            Format::Bc7Srgb | Format::Bc7 => (MODEL_BC7, 3, vec![sample(0, 128, 0, u32::MAX)]),
            Format::Bc5 => (
                MODEL_BC5,
                3,
                vec![
                    sample(0, 64, CHANNEL_RED, u32::MAX),
                    sample(64, 64, CHANNEL_GREEN, u32::MAX),
                ],
            ),
            Format::Rgba8Srgb | Format::Rgba8 => {
                let alpha = CHANNEL_ALPHA | if self.is_srgb() { QUALIFIER_LINEAR } else { 0 };
                (
                    MODEL_RGBSDA,
                    0,
                    vec![
                        sample(0, 8, CHANNEL_RED, 255),
                        sample(8, 8, CHANNEL_GREEN, 255),
                        sample(16, 8, CHANNEL_BLUE, 255),
                        sample(24, 8, alpha, 255),
                    ],
                )
            }
        };
        let block_size = 24 + 16 * samples.len() as u32;
        let mut words = vec![
            4 + block_size,
            0,
            2 | (block_size << 16),
            model | (PRIMARIES_BT709 << 8) | (transfer << 16),
            block | (block << 8),
            self.bytes_per_block() as u32,
            0,
        ];
        for sample in samples {
            words.extend(sample);
        }
        words.iter().flat_map(|word| word.to_le_bytes()).collect()
    }
}

fn sample(bit_offset: u32, bit_length: u32, channel: u32, upper: u32) -> [u32; 4] {
    [
        bit_offset | ((bit_length - 1) << 16) | (channel << 24),
        0,
        0,
        upper,
    ]
}

fn align(value: usize, to: usize) -> usize {
    value.div_ceil(to) * to
}

/// Файл KTX2 из ступеней `levels` (от полного размера к точке); размер ступени `i` — `width >> i`, не меньше
/// единицы. Ступени лежат в файле от мелкой к крупной, каждая с началом на границе 16 байт.
pub fn write(format: Format, width: u32, height: u32, levels: &[Vec<u8>]) -> Vec<u8> {
    let descriptor = format.data_format_descriptor();
    let descriptor_offset = HEADER_SIZE + LEVEL_INDEX_ENTRY * levels.len();
    let mut cursor = align(descriptor_offset + descriptor.len(), 16);

    let mut offsets = vec![0usize; levels.len()];
    for (offset, level) in offsets.iter_mut().zip(levels).rev() {
        *offset = cursor;
        cursor = align(cursor + level.len(), 16);
    }

    let mut out = Vec::with_capacity(cursor);
    out.extend_from_slice(&IDENTIFIER);
    for word in [
        format.vk_format(),
        1,
        width,
        height,
        0,
        0,
        1,
        levels.len() as u32,
        0,
        descriptor_offset as u32,
        descriptor.len() as u32,
        0,
        0,
    ] {
        out.extend_from_slice(&word.to_le_bytes());
    }
    out.extend_from_slice(&[0; 16]);
    for (offset, level) in offsets.iter().zip(levels) {
        for number in [*offset as u64, level.len() as u64, level.len() as u64] {
            out.extend_from_slice(&number.to_le_bytes());
        }
    }
    out.extend_from_slice(&descriptor);
    for (offset, level) in offsets.iter().zip(levels).rev() {
        out.resize(*offset, 0);
        out.extend_from_slice(level);
    }
    out
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes
        .get(at..at + 4)?
        .try_into()
        .ok()
        .map(u32::from_le_bytes)
}

fn u64_at(bytes: &[u8], at: usize) -> Option<u64> {
    bytes
        .get(at..at + 8)?
        .try_into()
        .ok()
        .map(u64::from_le_bytes)
}

/// Файл цел, если по `head` (начало файла) читается заголовок KTX2 и длина файла `file_len` сходится с концом
/// самой дальней ступени из списка ступеней.
pub fn is_intact(head: &[u8], file_len: u64) -> bool {
    if head.get(..IDENTIFIER.len()) != Some(&IDENTIFIER[..]) {
        return false;
    }
    let Some(count) = u32_at(head, 40).map(|count| count as usize) else {
        return false;
    };
    if !(1..=MAX_LEVELS).contains(&count) {
        return false;
    }
    let mut end = 0u64;
    for level in 0..count {
        let entry = HEADER_SIZE + level * LEVEL_INDEX_ENTRY;
        let (Some(offset), Some(length)) = (u64_at(head, entry), u64_at(head, entry + 8)) else {
            return false;
        };
        let Some(level_end) = offset.checked_add(length) else {
            return false;
        };
        end = end.max(level_end);
    }
    end == file_len
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_and_levels_are_laid_out_as_the_specification_says() {
        let levels = vec![vec![1u8; 64], vec![2u8; 16]];
        let file = write(Format::Bc7, 8, 8, &levels);
        let reader = ktx2::Reader::new(&file).expect("ktx2 reads the file");
        let header = reader.header();
        assert_eq!(header.format, Some(ktx2::Format::BC7_UNORM_BLOCK));
        assert_eq!((header.pixel_width, header.pixel_height), (8, 8));
        assert_eq!(header.level_count, 2);
        let read: Vec<&[u8]> = reader.levels().map(|level| level.data).collect();
        assert_eq!(read, [&levels[0][..], &levels[1][..]]);
        for level in reader.levels() {
            assert_eq!(
                (level.data.as_ptr() as usize - file.as_ptr() as usize) & 15,
                0
            );
        }
    }

    #[test]
    fn a_file_is_intact_only_when_its_length_meets_the_end_of_the_last_level() {
        let file = write(Format::Bc7, 8, 8, &[vec![1u8; 64], vec![2u8; 16]]);
        let length = file.len() as u64;
        assert!(is_intact(&file, length));
        assert!(is_intact(&file[..HEAD_LIMIT.min(file.len())], length));
        assert!(!is_intact(&file, length - 1));
        assert!(!is_intact(&file, length + 1));
        assert!(!is_intact(&file[..60], length));
        assert!(!is_intact(&[], 0));

        let mut foreign = file.clone();
        foreign[0] = 0;
        assert!(!is_intact(&foreign, length));
        let mut no_levels = file.clone();
        no_levels[40..44].copy_from_slice(&0u32.to_le_bytes());
        assert!(!is_intact(&no_levels, length));
        let mut too_many = file;
        too_many[40..44].copy_from_slice(&33u32.to_le_bytes());
        assert!(!is_intact(&too_many, length));
    }

    #[test]
    fn every_format_has_its_vulkan_code() {
        for (format, code) in [
            (Format::Bc7Srgb, ktx2::Format::BC7_SRGB_BLOCK),
            (Format::Bc5, ktx2::Format::BC5_UNORM_BLOCK),
            (Format::Rgba8Srgb, ktx2::Format::R8G8B8A8_SRGB),
            (Format::Rgba8, ktx2::Format::R8G8B8A8_UNORM),
        ] {
            let level = vec![
                0u8;
                format.bytes_per_block()
                    * if format.is_block_compressed() { 1 } else { 16 }
            ];
            let file = write(format, 4, 4, &[level]);
            let reader = ktx2::Reader::new(&file).expect("ktx2 reads the file");
            assert_eq!(reader.header().format, Some(code));
        }
    }
}

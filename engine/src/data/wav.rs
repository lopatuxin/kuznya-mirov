//! «Звук» → «Загрузка и проверка»: разбирает ровно столько заголовка WAV,
//! сколько нужно для длительности — деления `размер данных / (частота × каналы × байт на отсчёт)`.
//! Ни кодек, ни сами отсчёты здесь не нужны. Берутся форматы, которые распаковывает любой браузер:
//! PCM 8/16/24/32 бита и float 32 бита, в том числе в обёртке WAVE_FORMAT_EXTENSIBLE — у неё те же
//! поля `fmt ` по тем же смещениям, плюс подформат, которым эта обёртка называет настоящий кодек.
//! Посторонние блоки пропускаются.

const WAVE_FORMAT_PCM: u16 = 0x0001;
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const WAVE_FORMAT_EXTENSIBLE: u16 = 0xFFFE;

/// Смещение подформата от начала расширенного `fmt `: 16 байт основных полей + 2 (cbSize) + 2
/// (validBitsPerSample) + 4 (channelMask).
const EXTENSIBLE_SUBFORMAT_OFFSET: usize = 24;
/// Расширенный `fmt ` целиком: смещение подформата плюс сами 16 байт GUID.
const EXTENSIBLE_FMT_LEN: usize = EXTENSIBLE_SUBFORMAT_OFFSET + 16;

/// Общий для всех стандартных `KSDATAFORMAT_SUBTYPE_*` суффикс GUID — отличаются только первые
/// два байта, тот же код, что несёт обычный (не `EXTENSIBLE`) `wFormatTag`.
const SUBFORMAT_GUID_SUFFIX: [u8; 14] = [
    0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B, 0x71,
];

pub struct WavInfo {
    pub duration_seconds: f64,
}

fn read_u16(bytes: &[u8], pos: usize) -> Option<u16> {
    Some(u16::from_le_bytes(
        bytes.get(pos..pos + 2)?.try_into().ok()?,
    ))
}

fn read_u32(bytes: &[u8], pos: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(pos..pos + 4)?.try_into().ok()?,
    ))
}

fn accepted_bits_per_sample(format_tag: u16, bits_per_sample: u16) -> bool {
    match format_tag {
        WAVE_FORMAT_PCM => matches!(bits_per_sample, 8 | 16 | 24 | 32),
        WAVE_FORMAT_IEEE_FLOAT => bits_per_sample == 32,
        _ => false,
    }
}

/// `WAVE_FORMAT_EXTENSIBLE`'s own `wFormatTag` says nothing about the codec — that's the first two
/// bytes of the `SubFormat` GUID at the end of the extended `fmt ` body, the rest of which is the
/// fixed suffix every standard subtype GUID shares. `None` if the body's too short to carry that
/// extension, or the GUID isn't the PCM/float subtype (e.g. ADPCM wrapped in `EXTENSIBLE`).
fn extensible_subformat(bytes: &[u8], fmt_body_start: usize, fmt_chunk_size: u32) -> Option<u16> {
    if (fmt_chunk_size as usize) < EXTENSIBLE_FMT_LEN {
        return None;
    }
    let guid_start = fmt_body_start + EXTENSIBLE_SUBFORMAT_OFFSET;
    let sub_format_tag = read_u16(bytes, guid_start)?;
    let suffix = bytes.get(guid_start + 2..guid_start + 16)?;
    (suffix == SUBFORMAT_GUID_SUFFIX).then_some(sub_format_tag)
}

/// `None` for anything that isn't a playable PCM/float WAV: a bad `RIFF`/`WAVE` signature, a
/// truncated or malformed chunk, a missing `fmt `/`data` chunk, a bit depth this engine doesn't
/// read for the codec at hand, or a compressed format tag no browser is guaranteed to decode.
pub fn parse_wav(bytes: &[u8]) -> Option<WavInfo> {
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return None;
    }

    let mut pos = 12;
    // (format_tag, sample_rate, channels, bits_per_sample, fmt_body_start, fmt_chunk_size)
    let mut fmt: Option<(u16, u32, u16, u16, usize, u32)> = None;
    let mut data_size: Option<u32> = None;
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = read_u32(bytes, pos + 4)?;
        let body_start = pos + 8;
        let body_end = body_start.checked_add(chunk_size as usize)?;
        if body_end > bytes.len() {
            return None; // файл обрезан посреди блока
        }
        match chunk_id {
            b"fmt " => {
                if chunk_size < 16 {
                    return None;
                }
                fmt = Some((
                    read_u16(bytes, body_start)?,
                    read_u32(bytes, body_start + 4)?,
                    read_u16(bytes, body_start + 2)?,
                    read_u16(bytes, body_start + 14)?,
                    body_start,
                    chunk_size,
                ));
            }
            b"data" => data_size = Some(chunk_size),
            _ => {}
        }
        // Каждый блок RIFF выровнен по чётной границе — нечётный размер несёт один байт паддинга.
        pos = body_end + (chunk_size as usize % 2);
    }

    let (format_tag, sample_rate, channels, bits_per_sample, fmt_body_start, fmt_chunk_size) = fmt?;
    let data_size = data_size?;
    if sample_rate == 0 || channels == 0 || bits_per_sample == 0 {
        return None;
    }
    let codec = match format_tag {
        WAVE_FORMAT_PCM | WAVE_FORMAT_IEEE_FLOAT => format_tag,
        WAVE_FORMAT_EXTENSIBLE => extensible_subformat(bytes, fmt_body_start, fmt_chunk_size)?,
        _ => return None,
    };
    if !accepted_bits_per_sample(codec, bits_per_sample) {
        return None;
    }

    let bytes_per_sample = u64::from(bits_per_sample) / 8;
    let block_align = bytes_per_sample * u64::from(channels);
    let frames = u64::from(data_size) / block_align;
    Some(WavInfo {
        duration_seconds: frames as f64 / f64::from(sample_rate),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds a minimal valid WAV with a 16-byte `fmt ` under the given format tag, then `data`
    /// filled with zero samples for `duration_seconds` at the given rate/channels/bits — enough to
    /// drive `parse_wav`, not a real waveform.
    fn build_wav_tagged(
        format_tag: u16,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
        frames: u32,
    ) -> Vec<u8> {
        let block_align = channels * (bits_per_sample / 8);
        let byte_rate = sample_rate * u32::from(block_align);
        let data_size = frames * u32::from(block_align);
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(36 + data_size).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&16u32.to_le_bytes());
        out.extend_from_slice(&format_tag.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits_per_sample.to_le_bytes());
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_size.to_le_bytes());
        out.resize(out.len() + data_size as usize, 0);
        out
    }

    fn build_wav(sample_rate: u32, channels: u16, bits_per_sample: u16, frames: u32) -> Vec<u8> {
        build_wav_tagged(
            WAVE_FORMAT_PCM,
            sample_rate,
            channels,
            bits_per_sample,
            frames,
        )
    }

    /// Builds a minimal valid `WAVE_FORMAT_EXTENSIBLE` WAV: 40-byte `fmt ` (16 base fields, cbSize,
    /// validBitsPerSample, channelMask, and the 16-byte `SubFormat` GUID naming `subformat`), then
    /// `data` the same way `build_wav_tagged` does.
    fn build_extensible_wav(
        subformat: u16,
        sample_rate: u32,
        channels: u16,
        bits_per_sample: u16,
        frames: u32,
    ) -> Vec<u8> {
        let block_align = channels * (bits_per_sample / 8);
        let byte_rate = sample_rate * u32::from(block_align);
        let data_size = frames * u32::from(block_align);
        let fmt_size = 40u32;
        let mut out = Vec::new();
        out.extend_from_slice(b"RIFF");
        out.extend_from_slice(&(20 + fmt_size + data_size).to_le_bytes());
        out.extend_from_slice(b"WAVE");
        out.extend_from_slice(b"fmt ");
        out.extend_from_slice(&fmt_size.to_le_bytes());
        out.extend_from_slice(&WAVE_FORMAT_EXTENSIBLE.to_le_bytes());
        out.extend_from_slice(&channels.to_le_bytes());
        out.extend_from_slice(&sample_rate.to_le_bytes());
        out.extend_from_slice(&byte_rate.to_le_bytes());
        out.extend_from_slice(&block_align.to_le_bytes());
        out.extend_from_slice(&bits_per_sample.to_le_bytes());
        out.extend_from_slice(&22u16.to_le_bytes()); // cbSize
        out.extend_from_slice(&bits_per_sample.to_le_bytes()); // validBitsPerSample
        out.extend_from_slice(&0u32.to_le_bytes()); // channelMask
        out.extend_from_slice(&subformat.to_le_bytes());
        out.extend_from_slice(&SUBFORMAT_GUID_SUFFIX);
        out.extend_from_slice(b"data");
        out.extend_from_slice(&data_size.to_le_bytes());
        out.resize(out.len() + data_size as usize, 0);
        out
    }

    #[test]
    fn valid_16_bit_mono_wav_gives_its_duration() {
        let bytes = build_wav(44_100, 1, 16, 22_050); // half a second
        let info = parse_wav(&bytes).expect("годный WAV должен разбираться");
        assert!((info.duration_seconds - 0.5).abs() < 1e-9);
    }

    #[test]
    fn extra_chunks_around_fmt_and_data_are_skipped() {
        let mut bytes = build_wav(44_100, 1, 16, 4_410); // 0.1s
        // Splice a harmless "LIST" chunk right after the WAVE tag, before `fmt `.
        let mut extra = Vec::new();
        extra.extend_from_slice(b"LIST");
        extra.extend_from_slice(&4u32.to_le_bytes());
        extra.extend_from_slice(b"INFO");
        bytes.splice(12..12, extra);
        // Fix up the RIFF size to account for the inserted bytes.
        let new_riff_size = (bytes.len() - 8) as u32;
        bytes[4..8].copy_from_slice(&new_riff_size.to_le_bytes());

        let info = parse_wav(&bytes).expect("посторонний блок не должен ломать разбор");
        assert!((info.duration_seconds - 0.1).abs() < 1e-9);
    }

    #[test]
    fn six_second_file_is_recognized_as_longer_than_five() {
        let bytes = build_wav(8_000, 1, 16, 8_000 * 6);
        let info = parse_wav(&bytes).expect("годный WAV должен разбираться");
        assert!(info.duration_seconds > 5.0);
    }

    #[test]
    fn garbage_bytes_do_not_parse() {
        assert!(parse_wav(b"not a wav file at all").is_none());
    }

    #[test]
    fn truncated_file_does_not_parse() {
        let bytes = build_wav(44_100, 1, 16, 1_000);
        assert!(parse_wav(&bytes[..bytes.len() - 10]).is_none());
    }

    #[test]
    fn missing_data_chunk_does_not_parse() {
        let bytes = build_wav(44_100, 1, 16, 0);
        // `build_wav` with zero frames still emits an empty `data` chunk; strip it entirely.
        let without_data = &bytes[..bytes.len() - 8];
        assert!(parse_wav(without_data).is_none());
    }

    #[test]
    fn unsupported_compressed_format_does_not_parse() {
        let mut bytes = build_wav(44_100, 1, 16, 100);
        // Overwrite the format tag (right after "fmt " + its 4-byte size) with ADPCM (0x0002).
        bytes[20..22].copy_from_slice(&2u16.to_le_bytes());
        assert!(parse_wav(&bytes).is_none());
    }

    #[test]
    fn pcm_8_16_24_and_32_bit_are_all_accepted() {
        for bits in [8u16, 16, 24, 32] {
            let bytes = build_wav(44_100, 1, bits, 100);
            assert!(
                parse_wav(&bytes).is_some(),
                "{bits}-битный PCM должен разбираться"
            );
        }
    }

    #[test]
    fn unsupported_pcm_bit_depths_do_not_parse() {
        for bits in [1u16, 4, 12, 20, 64] {
            let bytes = build_wav(44_100, 1, bits, 100);
            assert!(
                parse_wav(&bytes).is_none(),
                "{bits}-битный PCM не входит в 8/16/24/32 и не должен разбираться"
            );
        }
    }

    #[test]
    fn float_32_bit_is_accepted() {
        let bytes = build_wav_tagged(WAVE_FORMAT_IEEE_FLOAT, 44_100, 1, 32, 100);
        assert!(
            parse_wav(&bytes).is_some(),
            "32-битный float должен разбираться"
        );
    }

    #[test]
    fn float_16_bit_does_not_parse() {
        let bytes = build_wav_tagged(WAVE_FORMAT_IEEE_FLOAT, 44_100, 1, 16, 100);
        assert!(
            parse_wav(&bytes).is_none(),
            "float берётся только 32-битный, 16 бит не должен разбираться"
        );
    }

    #[test]
    fn extensible_pcm_wrapper_is_accepted() {
        let bytes = build_extensible_wav(WAVE_FORMAT_PCM, 44_100, 2, 24, 100);
        assert!(
            parse_wav(&bytes).is_some(),
            "EXTENSIBLE с подформатом PCM должен разбираться"
        );
    }

    #[test]
    fn extensible_float_wrapper_is_accepted() {
        let bytes = build_extensible_wav(WAVE_FORMAT_IEEE_FLOAT, 44_100, 1, 32, 100);
        assert!(
            parse_wav(&bytes).is_some(),
            "EXTENSIBLE с подформатом float должен разбираться"
        );
    }

    #[test]
    fn extensible_adpcm_wrapper_does_not_parse() {
        let bytes = build_extensible_wav(0x0002, 44_100, 1, 16, 100);
        assert!(
            parse_wav(&bytes).is_none(),
            "ADPCM в обёртке EXTENSIBLE не разжимается браузером и не должен разбираться"
        );
    }

    #[test]
    fn extensible_fmt_chunk_too_short_for_subformat_does_not_parse() {
        // A 16-byte `fmt ` claiming `WAVE_FORMAT_EXTENSIBLE` but with none of the extension bytes
        // that would name the real subformat.
        let bytes = build_wav_tagged(WAVE_FORMAT_EXTENSIBLE, 44_100, 1, 16, 100);
        assert!(parse_wav(&bytes).is_none());
    }
}

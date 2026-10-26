//! Общее для проверок игры `games/platformer`: файлы игры и то, что страница ответила бы про её картинки
//! и видео.

#![allow(dead_code)]

use std::fs;
use std::path::PathBuf;

use engine::data::load::{ImageDecl, ImageVerdict, ReverseVerdict, read_entry, read_texts};

pub fn platformer_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("platformer")
}

pub fn read(name: &str) -> String {
    fs::read_to_string(platformer_dir().join(name))
        .unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"))
}

fn read_bytes(path: &str) -> Vec<u8> {
    fs::read(platformer_dir().join(path))
        .unwrap_or_else(|e| panic!("не смог прочитать {path}: {e}"))
}

fn be32(bytes: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap())
}

/// Размер и длина ролика MP4 из его заголовков `mvhd` и `tkhd`: ширина и высота файла, оба слоя вместе, и
/// секунды. Файлы игры пишет ffmpeg с заголовками нулевой версии.
fn mp4_file(path: &str) -> (u32, u32, f64) {
    let bytes = read_bytes(path);
    let after = |tag: &[u8; 4]| {
        let at = bytes
            .windows(4)
            .position(|window| window == tag)
            .unwrap_or_else(|| panic!("в {path} нет заголовка {tag:?}"))
            + 4;
        assert_eq!(bytes[at], 0, "{path}: заголовок не нулевой версии");
        at
    };
    let movie = after(b"mvhd");
    let seconds = f64::from(be32(&bytes, movie + 16)) / f64::from(be32(&bytes, movie + 12));
    let track = after(b"tkhd");
    (
        be32(&bytes, track + 76) >> 16,
        be32(&bytes, track + 80) >> 16,
        seconds,
    )
}

/// Размер PNG из заголовка `IHDR` (байты 16..24) и пустые точки такого размера: проверке и атласу нужен
/// только размер, точки движок получает от страницы.
fn blank_image(path: &str) -> ImageVerdict {
    let bytes = read_bytes(path);
    let width = be32(&bytes, 16);
    let height = be32(&bytes, 20);
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

/// Что страница ответила бы про файл картинки: у PNG — размер и пустые точки, у видео — размер и длину файла
/// и такие же сведения о втором файле `second`, который она прочитала для видео (`None` — второго файла не
/// просили).
pub fn file_verdict(decl: &ImageDecl, second: Option<&str>) -> ImageVerdict {
    if !decl.video {
        return blank_image(&decl.path);
    }
    let (width, height, seconds) = mp4_file(&decl.path);
    let reverse = second.map(|path| {
        let (width, height, seconds) = mp4_file(path);
        ReverseVerdict::Ok {
            width,
            height,
            seconds,
        }
    });
    ImageVerdict::Video {
        width,
        height,
        seconds,
        reverse,
    }
}

/// Ответы страницы по всем картинкам игры. Второй файл она читает ровно там, где его просит движок:
/// `reverse` видео по наведению и тот же ролик ещё раз у видео по кругу.
pub fn image_verdicts(images: &[ImageDecl]) -> Vec<(String, ImageVerdict)> {
    let (config, _) = read_entry(&read("game.json")).expect("game.json разобран");
    let asked = read_texts(
        &config,
        Some(&read("properties.json")),
        Some(&read("screens.json")),
        None,
    )
    .reverses;
    images
        .iter()
        .map(|decl| {
            let second = asked
                .iter()
                .find(|(name, _)| *name == decl.name)
                .map(|(_, path)| path.as_str());
            (decl.name.clone(), file_verdict(decl, second))
        })
        .collect()
}

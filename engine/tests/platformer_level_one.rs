//! Фаза 05 платформера — первый уровень `games/platformer`: игра грузится целиком, с картинками и
//! шрифтами, без ошибок и предупреждений.

use std::fs;
use std::path::PathBuf;

use engine::data::load::{ImageVerdict, load_rest_with_stamps, read_entry};

fn platformer_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("platformer")
}

fn read(name: &str) -> String {
    fs::read_to_string(platformer_dir().join(name))
        .unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"))
}

/// Размер картинки из заголовка `IHDR` PNG (байты 16..24) и пустые точки такого размера: проверке и
/// атласу нужен только размер, точки движок получает от страницы.
fn blank_image(path: &str) -> ImageVerdict {
    let path = platformer_dir().join(path);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

#[test]
fn the_first_level_loads_without_errors_or_warnings() {
    let game_json = read("game.json");
    let (config, mut warnings) =
        read_entry(&game_json).unwrap_or_else(|e| panic!("game.json не разобран: {e:?}"));
    let images: Vec<(String, ImageVerdict)> = config
        .files
        .images
        .iter()
        .map(|decl| (decl.name.clone(), blank_image(&decl.path)))
        .collect();
    let fonts: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(platformer_dir().join(path)).ok()))
        .collect();
    let (_game, _screens, rest_warnings, _images) = load_rest_with_stamps(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &fonts,
        &[],
        &[],
        &images,
        Some(&read("code.lua")),
        false,
        &[],
        None,
        &[],
        &[],
        &[],
    )
    .unwrap_or_else(|e| panic!("первый уровень не загрузился: {e:?}"));
    warnings.extend(rest_warnings);
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
}

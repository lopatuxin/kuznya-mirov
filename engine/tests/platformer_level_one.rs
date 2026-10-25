//! Фаза 05 платформера — первый уровень `games/platformer`: игра грузится целиком, с картинками и
//! шрифтами, без ошибок и предупреждений.

use std::fs;

use engine::data::load::{load_rest_with_stamps, read_entry};

#[path = "platformer_support/mod.rs"]
mod support;
use support::{platformer_dir, read};

#[test]
fn the_first_level_loads_without_errors_or_warnings() {
    let game_json = read("game.json");
    let (config, mut warnings) =
        read_entry(&game_json).unwrap_or_else(|e| panic!("game.json не разобран: {e:?}"));
    let images = support::image_verdicts(&config.files.images);
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

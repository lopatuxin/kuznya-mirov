use std::fs;
use std::path::PathBuf;

use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::{ImageVerdict, MusicVerdict, load_rest, read_entry};

fn game_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("snake");
    path.push(name);
    path
}

fn read(name: &str) -> String {
    let path = game_path(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

/// `width`/`height` straight out of the PNG's own `IHDR` chunk (bytes 16..24, big-endian) — the
/// engine never decodes PNG itself, but a test fixture reading its own fixed-position header is
/// not that: it needs no pixel data at all, only the two numbers `validate_image_files` checks.
fn png_dimensions(path: &std::path::Path) -> (u32, u32) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (width, height)
}

/// Loads the real demo folder end to end, fonts and sound bytes included, music verdicts all
/// `Ok` — unlike `load_game_from_texts`, which carries no binary bytes and is only good for data
/// that declares none. «Звук»: every declared sound is read whether or not
/// `play_sound` uses it, so `files.sounds` is read in full here just like `files.fonts` is.
fn load_demo() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let game_json = read("game.json");
    let (config, _entry_warnings) =
        read_entry(&game_json).expect("game.json демо-змейки должен разбираться");
    let font_bytes: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(game_path(path)).ok()))
        .collect();
    let sound_bytes: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .sounds
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(game_path(path)).ok()))
        .collect();
    let music_verdicts: Vec<(String, MusicVerdict)> = config
        .files
        .music
        .iter()
        .map(|(name, _)| (name.clone(), MusicVerdict::Ok))
        .collect();
    let image_verdicts: Vec<(String, ImageVerdict)> = config
        .files
        .images
        .iter()
        .map(|decl| {
            let (width, height) = png_dimensions(&game_path(&decl.path));
            (
                decl.name.clone(),
                ImageVerdict::Ok {
                    width,
                    height,
                    pixels: vec![0u8; (width * height * 4) as usize],
                },
            )
        })
        .collect();
    let (game, screens, warnings) = load_rest(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &font_bytes,
        &sound_bytes,
        &music_verdicts,
        &image_verdicts,
        None,
    )
    .expect("демо-змейка должна проходить предстартовую проверку");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    (game, screens)
}

/// The demo data plays: the head walks in a straight line (no input queued) and, after a known
/// number of steps, has hopped a known number of times and dropped exactly that many tail
/// segments, none of which live long enough to have expired yet.
#[test]
fn snake_demo_runs_known_steps_with_expected_trail() {
    let (mut game, _screens) = load_demo();
    assert_eq!(
        game.world.alive_count(),
        0,
        "стартовый экран — меню без world_runs, мира ещё нет"
    );

    // Same effect the "Играть" button's `["new_game", "game"]` has.
    game.new_game();
    assert_eq!(game.world.alive_count(), 5, "голова + 4 стены");

    for _ in 0..20 {
        game.step(StepInput::empty());
    }

    assert!(
        game.is_running(),
        "змейка не должна проиграть за 20 шагов по прямой"
    );

    // scene.json lists the head first, so it is object 0 for the whole run.
    let head_pos = game.world.vec2(0, property::POSITION).expect("голова жива");
    assert_eq!(
        head_pos,
        [12.0, 10.0],
        "два хода по клеткам за 20 шагов при интервале 7 шагов"
    );

    let deadly = game.properties.resolve("deadly").unwrap();
    let tail_segments = game
        .world
        .ids()
        .filter(|&id| game.world.flag(id, deadly))
        .filter(|&id| game.world.vec2(id, property::SIZE) == Some([1.0, 1.0]))
        .count();
    assert_eq!(tail_segments, 2, "один сегмент на каждый ход головы");
}

//! Prestart validation for `files.sounds`/`files.music`, `play_sound`, the screen `music` field
//! and `toggle_sound` — «Звук» → «Загрузка и проверка» — plus the screen
//! key/object key collision warning from «Экраны и состояние» → «Клавиши экрана». Companion to
//! `prestart_validation.rs` and `screens_validation.rs`, which cover everything else.

use engine::data::error::LoadFailure;
use engine::data::load::{MusicVerdict, load_rest, read_entry};

/// Builds `game.json` with `files_extra` spliced right after `"fonts":{}` in the `files` block —
/// e.g. `,"sounds":{"eat":"sounds/eat.wav"}` — so each test only has to name the one table shape
/// it's checking.
fn game_json(files_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":4,"height":4,"background":"#000000"}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{files_extra}}}}}"##
    )
}

const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const SCENE_EMPTY: &str = r#"{"objects":[]}"#;
const RULES_EMPTY: &str = r#"{"rules":[]}"#;
const SCREENS_MAIN: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

#[allow(clippy::too_many_arguments)]
fn load(
    game_json: &str,
    props: &str,
    scene: &str,
    rules: &str,
    screens: &str,
    sounds: &[(&str, &[u8])],
    music: &[(&str, MusicVerdict)],
) -> Result<
    (
        engine::core::game::Game,
        engine::core::screens::ScreensConfig,
        Vec<engine::data::error::GameError>,
    ),
    LoadFailure,
> {
    let (config, _entry_warnings) = read_entry(game_json).expect("game.json должен разбираться");
    let sound_bytes: Vec<(String, Option<Vec<u8>>)> = sounds
        .iter()
        .map(|(name, bytes)| (name.to_string(), Some(bytes.to_vec())))
        .collect();
    let music_verdicts: Vec<(String, MusicVerdict)> = music
        .iter()
        .map(|(name, verdict)| (name.to_string(), *verdict))
        .collect();
    load_rest(
        game_json,
        config,
        Some(props),
        Some(scene),
        Some(rules),
        Some(screens),
        &[],
        &sound_bytes,
        &music_verdicts,
        &[],
        None,
        false,
    )
    .map(|(game, screens, warnings, _images)| (game, screens, warnings))
}

/// Minimal valid 16-bit mono PCM WAV, `frames` samples at 44.1kHz.
fn wav_bytes(frames: u32) -> Vec<u8> {
    let sample_rate = 44_100u32;
    let channels = 1u16;
    let bits_per_sample = 16u16;
    let block_align = channels * (bits_per_sample / 8);
    let byte_rate = sample_rate * u32::from(block_align);
    let data_size = frames * u32::from(block_align);
    let mut out = Vec::new();
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_size).to_le_bytes());
    out.extend_from_slice(b"WAVE");
    out.extend_from_slice(b"fmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // WAVE_FORMAT_PCM
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

// ---------------------------------------------------------------------------------------------
// `files.sounds` / `files.music` — errors caught straight out of `game.json`, so `read_entry`
// alone already fails.
// ---------------------------------------------------------------------------------------------

#[test]
fn sounds_table_written_as_a_string_is_reported() {
    let game = game_json(r#","sounds":"sounds/""#);
    let LoadFailure { errors, .. } = read_entry(&game).expect_err("files.sounds строкой — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("files.sounds") && e.message.contains("files.fonts")),
        "{errors:?}"
    );
}

#[test]
fn music_table_written_as_a_list_is_reported() {
    let game = game_json(r#","music":["theme","battle"]"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("files.music списком имён — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("files.music")),
        "{errors:?}"
    );
}

#[test]
fn empty_name_in_sounds_table_is_reported() {
    let game = game_json(r#","sounds":{"":"sounds/eat.wav"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("пустое имя в files.sounds — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("пустое имя") && e.message.contains("правило")),
        "{errors:?}"
    );
}

#[test]
fn empty_name_in_music_table_is_reported() {
    let game = game_json(r#","music":{"":"music/theme.mp3"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("пустое имя в files.music — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("пустое имя") && e.message.contains("экран")),
        "{errors:?}"
    );
}

#[test]
fn non_string_path_in_sounds_table_is_reported() {
    let game = game_json(r#","sounds":{"eat":3}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("путь числом в files.sounds — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("eat") && e.message.contains("путь к файлу строкой")),
        "{errors:?}"
    );
}

#[test]
fn non_string_path_in_music_table_is_reported() {
    let game = game_json(r#","music":{"theme":3}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("путь числом в files.music — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("theme") && e.message.contains("путь к файлу строкой")),
        "{errors:?}"
    );
}

#[test]
fn wrong_extension_in_sounds_table_is_reported() {
    let game = game_json(r#","sounds":{"hit":"sounds/hit.ogg"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("hit.ogg в files.sounds — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("hit.ogg") && e.message.contains("files.music")),
        "{errors:?}"
    );
}

#[test]
fn mp3_path_in_sounds_table_gets_the_specific_hint() {
    let game = game_json(r#","sounds":{"hit":"sounds/hit.mp3"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("hit.mp3 в files.sounds — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("это MP3")),
        "{errors:?}"
    );
}

#[test]
fn wrong_extension_in_music_table_is_reported() {
    let game = game_json(r#","music":{"theme":"music/theme.ogg"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("theme.ogg в files.music — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("theme.ogg") && e.message.contains("files.sounds")),
        "{errors:?}"
    );
}

#[test]
fn wav_path_in_music_table_gets_the_specific_hint() {
    let game = game_json(r#","music":{"theme":"music/theme.wav"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("theme.wav в files.music — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("это WAV")),
        "{errors:?}"
    );
}

#[test]
fn unknown_files_key_is_reported() {
    let game = game_json(r#","soundz":{"eat":"sounds/eat.wav"}"#);
    let LoadFailure { errors, .. } =
        read_entry(&game).expect_err("опечатка в имени таблицы files — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("soundz")),
        "{errors:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// `play_sound` — needs `rules.json`, so these go through `load_rest`.
// ---------------------------------------------------------------------------------------------

#[test]
fn play_sound_of_an_undeclared_name_lists_the_declared_ones() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav","hit":"sounds/hit.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene","do":[["play_sound","eet"]]}
    ]}"#;
    let sounds: &[(&str, &[u8])] = &[("eat", &wav_bytes(100)), ("hit", &wav_bytes(100))];
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        sounds,
        &[],
    )
    .expect_err("незнакомое имя звука — ошибка");
    assert!(
        errors.iter().any(|e| {
            e.message.contains("eet") && e.message.contains("eat") && e.message.contains("hit")
        }),
        "{errors:?}"
    );
}

#[test]
fn play_sound_naming_a_music_track_is_reported() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene","do":[["play_sound","theme"]]}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        &[],
        &[("theme", MusicVerdict::Ok)],
    )
    .expect_err("play_sound на имя из files.music — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("files.music") && e.message.contains("files.sounds")),
        "{errors:?}"
    );
}

#[test]
fn play_sound_without_a_name_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene","do":[["play_sound"]]}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        &[("eat", &wav_bytes(100))],
        &[],
    )
    .expect_err("play_sound без имени — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("ровно одну настройку")),
        "{errors:?}"
    );
}

#[test]
fn play_sound_with_two_names_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav","hit":"sounds/hit.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene",
         "do":[["play_sound","eat","hit"]]}
    ]}"#;
    let sounds: &[(&str, &[u8])] = &[("eat", &wav_bytes(100)), ("hit", &wav_bytes(100))];
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        sounds,
        &[],
    )
    .expect_err("play_sound с двумя именами — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("ровно одну настройку")),
        "{errors:?}"
    );
}

#[test]
fn play_sound_with_a_number_instead_of_a_string_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene","do":[["play_sound",3]]}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        &[("eat", &wav_bytes(100))],
        &[],
    )
    .expect_err("play_sound с числом вместо строки — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("ровно одну настройку")),
        "{errors:?}"
    );
}

#[test]
fn play_sound_inside_effects_is_reported() {
    let game = game_json(r#","sounds":{"hit":"sounds/hit.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},
         "effects":{"a":[["play_sound","hit"]]}}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        &[("hit", &wav_bytes(100))],
        &[],
    )
    .expect_err("play_sound в effects — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("это общее действие, его место в do")),
        "{errors:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The screen `music` field and `toggle_sound` — need `screens.json`, so `load_rest` again.
// ---------------------------------------------------------------------------------------------

#[test]
fn screen_music_not_a_string_is_reported() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3"}"#);
    let screens = r#"{"screens":[{"name":"main","world_runs":true,"music":3,"elements":[]}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[("theme", MusicVerdict::Ok)],
    )
    .expect_err("music числом — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("music")),
        "{errors:?}"
    );
}

#[test]
fn screen_music_as_a_list_gets_the_playlist_hint() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3","battle":"music/battle.mp3"}"#);
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"music":["theme","battle"],"elements":[]}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[("theme", MusicVerdict::Ok), ("battle", MusicVerdict::Ok)],
    )
    .expect_err("music списком — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("плейлиста нет, у экрана один трек")),
        "{errors:?}"
    );
}

#[test]
fn screen_music_of_an_undeclared_name_lists_the_declared_ones() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3"}"#);
    let screens =
        r#"{"screens":[{"name":"main","world_runs":true,"music":"battle","elements":[]}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[("theme", MusicVerdict::Ok)],
    )
    .expect_err("незнакомое имя трека — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("battle") && e.message.contains("theme")),
        "{errors:?}"
    );
}

#[test]
fn screen_music_naming_a_sound_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let screens = r#"{"screens":[{"name":"main","world_runs":true,"music":"eat","elements":[]}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[("eat", &wav_bytes(100))],
        &[],
    )
    .expect_err("music на имя из files.sounds — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("files.sounds") && e.message.contains("files.music")),
        "{errors:?}"
    );
}

#[test]
fn unknown_screen_field_is_reported() {
    let game = game_json("");
    let screens =
        r#"{"screens":[{"name":"main","world_runs":true,"music_file":"x","elements":[]}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[],
    )
    .expect_err("опечатка вместо music — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("music_file")),
        "{errors:?}"
    );
}

#[test]
fn toggle_sound_with_settings_is_reported() {
    let game = game_json("");
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"keys":{"KeyM":["toggle_sound","off"]},"elements":[]}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[],
    )
    .expect_err("toggle_sound с настройкой — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("у toggle_sound настроек нет")),
        "{errors:?}"
    );
}

#[test]
fn toggle_sound_written_as_a_bare_string_is_reported() {
    let game = game_json("");
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"keys":{"KeyM":"toggle_sound"},"elements":[]}
    ]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[],
    )
    .expect_err("\"toggle_sound\" вместо [\"toggle_sound\"] — ошибка");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].file, "screens.json", "{errors:?}");
    assert_eq!(errors[0].path, "screens[0] → keys → KeyM", "{errors:?}");
    assert_eq!(
        errors[0].message, "ожидался массив, получено строка",
        "{errors:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Sound/music byte-level checks — WAV header, duration, and the executor's MP3 verdict.
// ---------------------------------------------------------------------------------------------

#[test]
fn missing_sound_file_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[],
        &[],
    )
    .expect_err("файл звука не найден — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("eat.wav")
            && e.message.contains("не найден")
            && e.message.contains("ожидался WAV-звук")),
        "{errors:?}"
    );
}

#[test]
fn sound_file_that_does_not_parse_as_wav_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("eat", b"garbage, not a wav")],
        &[],
    )
    .expect_err("мусор вместо WAV — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("не разбирается как WAV")),
        "{errors:?}"
    );
}

#[test]
fn sound_longer_than_five_seconds_is_reported() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let six_seconds = wav_bytes(44_100 * 6);
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("eat", &six_seconds)],
        &[],
    )
    .expect_err("звук длиннее пяти секунд — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("длиннее пяти секунд")),
        "{errors:?}"
    );
}

#[test]
fn missing_music_verdict_is_reported_as_not_found() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3"}"#);
    let screens =
        r#"{"screens":[{"name":"main","world_runs":true,"music":"theme","elements":[]}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[("theme", MusicVerdict::Missing)],
    )
    .expect_err("трек не найден — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("theme.mp3")
            && e.message.contains("не найден")
            && e.message.contains("ожидался MP3-трек")),
        "{errors:?}"
    );
}

#[test]
fn music_rejected_by_the_executor_names_the_executor() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3"}"#);
    let screens =
        r#"{"screens":[{"name":"main","world_runs":true,"music":"theme","elements":[]}]}"#;
    let LoadFailure { errors, .. } = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[("theme", MusicVerdict::Rejected)],
    )
    .expect_err("исполнитель отверг файл — ошибка");
    assert!(
        errors.iter().any(|e| {
            e.message
                .contains("исполнитель (браузер) не берётся разжимать")
        }),
        "{errors:?}"
    );
}

#[test]
fn an_unreferenced_track_is_never_read_so_a_bad_verdict_for_it_is_not_an_error() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3","unused":"music/unused.mp3"}"#);
    let screens =
        r#"{"screens":[{"name":"main","world_runs":true,"music":"theme","elements":[]}]}"#;
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        screens,
        &[],
        &[
            ("theme", MusicVerdict::Ok),
            ("unused", MusicVerdict::Rejected),
        ],
    )
    .expect("непрослушиваемый трек не должен проверяться вовсе");
    assert!(
        warnings.iter().any(
            |w| w.message.contains("unused") && w.message.contains("не называет ни один экран")
        ),
        "{warnings:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Warnings — the game still starts.
// ---------------------------------------------------------------------------------------------

/// Воспроизведённый баг: `common_actions`/`walk_common_actions` не заходили в `if_blocked`
/// `shift`/`turn`, так что `play_sound` внутри него не считался использованием звука.
#[test]
fn play_sound_inside_if_blocked_counts_as_used() {
    let game = game_json(r#","sounds":{"thud":"sounds/thud.wav"}"#);
    let props = r#"{"properties":{"g":"flag","wall":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true},
        {"position":[0.5,0],"size":[1,1],"wall":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["g"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                          "if_blocked":[["play_sound","thud"]]}]]}
    ]}"#;
    let (_game, _screens, warnings) = load(
        &game,
        props,
        scene,
        rules,
        SCREENS_MAIN,
        &[("thud", &wav_bytes(100))],
        &[],
    )
    .expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("thud")),
        "play_sound в if_blocked должен считаться использованием звука: {warnings:?}"
    );
}

/// Тот же баг, для второй проверки, которая тоже читает `common_actions` — «в игре есть звук,
/// но toggle_sound не назначен» должна сработать и когда единственный `play_sound` спрятан в
/// `if_blocked`, а не только у нижнего уровня `do`.
#[test]
fn play_sound_inside_if_blocked_satisfies_toggle_sound_requirement() {
    let game = game_json(r#","sounds":{"thud":"sounds/thud.wav"}"#);
    let props = r#"{"properties":{"g":"flag","wall":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true},
        {"position":[0.5,0],"size":[1,1],"wall":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["g"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                          "if_blocked":[["play_sound","thud"]]}]]}
    ]}"#;
    let (_game, _screens, warnings) = load(
        &game,
        props,
        scene,
        rules,
        SCREENS_MAIN,
        &[("thud", &wav_bytes(100))],
        &[],
    )
    .expect("должно загрузиться");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("toggle_sound не назначен")),
        "{warnings:?}"
    );
}

#[test]
fn unused_sound_is_a_warning() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[("eat", &wav_bytes(100))],
        &[],
    )
    .expect("объявленный неиспользуемый звук — не ошибка");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("eat") && w.message.contains("play_sound")),
        "{warnings:?}"
    );
}

#[test]
fn track_unreferenced_by_any_screen_is_a_warning() {
    let game = game_json(r#","music":{"theme":"music/theme.mp3"}"#);
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[],
        &[],
    )
    .expect("объявленный неиспользуемый трек — не ошибка");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("theme") && w.message.contains("не прочитан")),
        "{warnings:?}"
    );
}

#[test]
fn sound_without_a_toggle_sound_binding_is_a_warning() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene","do":[["play_sound","eat"]]}
    ]}"#;
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        SCREENS_MAIN,
        &[("eat", &wav_bytes(100))],
        &[],
    )
    .expect("звук без toggle_sound — не ошибка");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("toggle_sound не назначен")),
        "{warnings:?}"
    );
}

#[test]
fn sound_with_a_toggle_sound_binding_gets_no_such_warning() {
    let game = game_json(r#","sounds":{"eat":"sounds/eat.wav"}"#);
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"when":"outside_scene","do":[["play_sound","eat"]]}
    ]}"#;
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"keys":{"KeyM":["toggle_sound"]},"elements":[]}
    ]}"#;
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        rules,
        screens,
        &[("eat", &wav_bytes(100))],
        &[],
    )
    .expect("звук с toggle_sound — не ошибка");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("toggle_sound не назначен")),
        "{warnings:?}"
    );
}

#[test]
fn screen_key_matching_an_objects_own_key_binding_is_a_warning() {
    let game = game_json("");
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],
         "keys":{"Space":{"press":[["velocity",[1,0]]]}}}
    ]}"#;
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"keys":{"Space":["show_screen","main"]},"elements":[]}
    ]}"#;
    let (_game, _screens, warnings) =
        load(&game, PROPS_EMPTY, scene, RULES_EMPTY, screens, &[], &[])
            .expect("совпадение клавиш — не ошибка");
    assert!(
        warnings.iter().any(|w| {
            w.message.contains("Space")
                && w.message.contains("main")
                && w.message.contains("работать не будет")
        }),
        "{warnings:?}"
    );
}

#[test]
fn screen_key_matching_an_object_binding_on_a_non_live_screen_is_not_flagged() {
    let game = game_json("");
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],
         "keys":{"Space":{"press":[["velocity",[1,0]]]}}}
    ]}"#;
    let screens = r#"{"screens":[
        {"name":"main","world_runs":true,"elements":[]},
        {"name":"pause","world_runs":false,
         "keys":{"Space":["show_screen","main"]},"elements":[]}
    ]}"#;
    let (_game, _screens, warnings) =
        load(&game, PROPS_EMPTY, scene, RULES_EMPTY, screens, &[], &[])
            .expect("должно загрузиться");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("работать не будет")),
        "клавиша неигрового экрана мир и так не трогает: {warnings:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// A game with no sound at all keeps loading exactly as before.
// ---------------------------------------------------------------------------------------------

#[test]
fn a_game_without_any_sound_tables_loads_with_no_errors_and_no_sound_warnings() {
    let game = game_json("");
    let (_game, _screens, warnings) = load(
        &game,
        PROPS_EMPTY,
        SCENE_EMPTY,
        RULES_EMPTY,
        SCREENS_MAIN,
        &[],
        &[],
    )
    .expect("игра без звука должна грузиться, как и раньше");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
}

use std::fs;
use std::path::PathBuf;

use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::{ImageVerdict, MusicVerdict, load_rest, read_entry};

fn game_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("arkanoid");
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

/// The demo's `start_screen` is the menu, so the freshly loaded game has no world at all —
/// `new_game()` is the same call `["new_game", "game"]` on the "Играть" button would make.
/// Loads the real demo folder end to end, fonts and sound bytes included, music verdicts all
/// `Ok` — unlike `load_game_from_texts`, which carries no binary bytes and is only good for data
/// that declares none. «Звук»: every declared sound is read whether or not
/// `play_sound` uses it, so `files.sounds` is read in full here just like `files.fonts` is.
fn load() -> engine::core::game::Game {
    let game_json = read("game.json");
    let (config, _entry_warnings) =
        read_entry(&game_json).expect("game.json демо-арканоида должен разбираться");
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
    let (mut game, _screens, warnings, _images) = load_rest(
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
        Some(&read("code.lua")),
        false,
    )
    .expect("демо-арканоид должен проходить предстартовую проверку");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.new_game();
    game
}

#[test]
fn arkanoid_demo_loads_and_plays_without_input() {
    let mut game = load();
    let ball_flag = game.properties.resolve("ball").unwrap();
    let ball = game
        .world
        .ids()
        .find(|&id| game.world.flag(id, ball_flag))
        .expect("мяч есть на сцене");
    let initial_position = game
        .world
        .vec2(ball, property::POSITION)
        .expect("у мяча есть position");

    for _ in 0..30 {
        game.step(StepInput::empty());
    }

    // 30 steps is not enough to run out the ball or clear every brick either way.
    assert!(game.is_running());
    assert!(game.world.has(ball, property::POSITION), "мяч всё ещё жив");
    let position = game
        .world
        .vec2(ball, property::POSITION)
        .expect("у живого мяча есть position");
    assert_ne!(
        position, initial_position,
        "мяч должен реально сдвинуться за 30 шагов, а не просто остаться на сцене"
    );
}

/// Corner case, pushed in by hand rather than waited for: the ball overlaps two solid walls at
/// once. It bounces on the axis with the smaller overlap and only once, even though it sits in
/// two colliding pairs this step.
#[test]
fn ball_bounces_on_the_smaller_overlap_axis_once_per_step() {
    let mut game = load();
    let ball_flag = game.properties.resolve("ball").unwrap();
    let ball = game
        .world
        .ids()
        .find(|&id| game.world.flag(id, ball_flag))
        .expect("мяч есть на сцене");

    // Overlaps wall_top (y in [0,1)) by 0.5 on y, and wall_left (x in [0,1)) by 0.5 on x.
    game.world.set_vec2(ball, property::POSITION, [0.5, 0.5]);
    let original_velocity = [7.0, -11.0];
    game.world
        .set_vec2(ball, property::VELOCITY, original_velocity);

    game.step(StepInput::empty());

    let velocity = game.world.vec2(ball, property::VELOCITY).unwrap();
    let position = game.world.vec2(ball, property::POSITION).unwrap();

    assert_eq!(
        velocity[1], -original_velocity[1],
        "по оси меньшего перекрытия (y) скорость разворачивается"
    );
    assert_eq!(
        velocity[0], original_velocity[0],
        "второй отскок в этом же шаге пропускается — x не трогали"
    );
    assert!(
        (position[1] - 1.0).abs() < 1e-9,
        "мяч выталкивается наружу на величину перекрытия: {position:?}"
    );
}

/// «Исполнение игры» → «Столкновения»: объект, накрывший стену по ширине целиком, выталкивается к
/// ближнему краю стены по той оси, где выталкивать меньше, — а не по оси меньшего пересечения,
/// которая здесь указала бы вниз и увела ракетку за сцену.
#[test]
fn paddle_covering_a_wall_is_pushed_out_to_the_nearest_side() {
    let mut game = load();
    let named = |game: &engine::core::game::Game, name: &str| {
        game.world
            .ids()
            .find(|&id| game.world.text(id, property::NAME) == Some(name))
            .unwrap_or_else(|| panic!("{name} есть на сцене"))
    };
    let paddle = named(&game, "paddle");
    let wall = named(&game, "wall_left");
    game.world.set_vec2(wall, property::SIZE, [1.5, 24.0]);
    game.world.set_vec2(paddle, property::POSITION, [0.0, 22.0]);

    game.step(StepInput::empty());

    let position = game.world.vec2(paddle, property::POSITION).unwrap();
    assert_eq!(position, [1.5, 22.0], "ракетка вплотную справа от стены");
}

/// «Код игры» → пункт 24: `paddle_bounce` — угол зависит только от места удара. Середина — строго
/// вверх; на полпути к краю — 30°; у края и за краем (прижато) — 60°; скорость по величине не
/// меняется, мяч ставится вплотную над ракеткой.
#[test]
fn paddle_bounce_angle_depends_only_on_where_the_ball_hit() {
    let angle_for_offset = |offset_cells: f64| -> (f64, f64, f64) {
        let mut game = load();
        let ball_flag = game.properties.resolve("ball").unwrap();
        let ball = game
            .world
            .ids()
            .find(|&id| game.world.flag(id, ball_flag))
            .expect("мяч есть на сцене");
        let paddle = game
            .world
            .ids()
            .find(|&id| game.world.has(id, property::KEYS))
            .expect("у ракетки есть keys");
        let paddle_pos = game.world.vec2(paddle, property::POSITION).unwrap();
        let paddle_size = game.world.vec2(paddle, property::SIZE).unwrap();
        let ball_size = game.world.vec2(ball, property::SIZE).unwrap();

        let paddle_mid = paddle_pos[0] + paddle_size[0] / 2.0;
        let ball_mid_x = paddle_mid + offset_cells;
        game.world.set_vec2(
            ball,
            property::POSITION,
            [ball_mid_x - ball_size[0] / 2.0, paddle_pos[1] - 0.5],
        );
        let speed = 5.0;
        game.world.set_vec2(ball, property::VELOCITY, [0.0, speed]);

        game.step(StepInput::empty());
        assert!(game.is_running(), "{:?}", game.messages());

        let velocity = game.world.vec2(ball, property::VELOCITY).unwrap();
        let position = game.world.vec2(ball, property::POSITION).unwrap();
        let got_speed = (velocity[0] * velocity[0] + velocity[1] * velocity[1]).sqrt();
        assert!(
            (got_speed - speed).abs() < 1e-9,
            "скорость по величине не меняется: {velocity:?}"
        );
        assert!(velocity[1] < 0.0, "мяч должен полететь вверх: {velocity:?}");
        assert!(
            (position[1] - (paddle_pos[1] - ball_size[1])).abs() < 1e-9,
            "мяч ставится вплотную над ракеткой: {position:?}"
        );
        let angle_deg = velocity[0].atan2(-velocity[1]).to_degrees();
        (angle_deg, velocity[0], velocity[1])
    };

    let (angle_mid, vx_mid, _) = angle_for_offset(0.0);
    assert!(
        angle_mid.abs() < 1e-6,
        "середина — строго вверх: {angle_mid}"
    );
    assert!(vx_mid.abs() < 1e-9);

    let (angle_half, _, _) = angle_for_offset(1.0);
    assert!(
        (angle_half - 30.0).abs() < 1e-6,
        "на полпути к краю — 30°: {angle_half}"
    );

    let (angle_edge, _, _) = angle_for_offset(2.0);
    assert!(
        (angle_edge - 60.0).abs() < 1e-6,
        "у края — 60°: {angle_edge}"
    );

    // Still overlapping the paddle's rectangle (half-width 2, ball half-width 0.5) but past the
    // raw offset that would clamp to exactly the edge.
    let (angle_beyond, _, _) = angle_for_offset(2.3);
    assert!(
        (angle_beyond - 60.0).abs() < 1e-6,
        "за краем — прижато к 60°: {angle_beyond}"
    );
}

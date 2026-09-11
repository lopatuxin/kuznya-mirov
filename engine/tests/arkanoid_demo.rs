use std::fs;
use std::path::PathBuf;

use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::{load_rest, read_entry};

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

/// The demo's `start_screen` is the menu, so the freshly loaded game has no world at all —
/// `new_game()` is the same call `["new_game", "game"]` on the "Играть" button would make.
/// Loads the real demo folder end to end, fonts included — unlike `load_game_from_texts`, which
/// carries no font bytes and is only good for data that declares none.
fn load() -> engine::core::game::Game {
    let (config, _entry_warnings) =
        read_entry(&read("game.json")).expect("game.json демо-арканоида должен разбираться");
    let font_bytes: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(game_path(path)).ok()))
        .collect();
    let (mut game, _screens, warnings) = load_rest(
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &font_bytes,
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

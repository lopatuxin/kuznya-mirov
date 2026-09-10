use std::fs;
use std::path::PathBuf;

use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::load_game_from_texts;

fn read(name: &str) -> String {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("snake");
    path.push(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

/// The demo data plays: the head walks in a straight line (no input queued) and, after a known
/// number of steps, has hopped a known number of times and dropped exactly that many tail
/// segments, none of which live long enough to have expired yet.
#[test]
fn snake_demo_runs_known_steps_with_expected_trail() {
    let (mut game, warnings) = load_game_from_texts(
        &read("game.json"),
        &read("properties.json"),
        &read("scene.json"),
        &read("rules.json"),
    )
    .expect("демо-змейка должна проходить предстартовую проверку");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

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

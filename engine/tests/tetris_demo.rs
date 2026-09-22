//! «Загрузка»: демо-тетрис грузится полностью, без ошибок и предупреждений, и партия идёт —
//! первая фигура появляется и падает — без ошибки кода. Медиа-байты не нужны здесь (шрифты,
//! звуки и картинки этой проверки не касаются, а полную загрузку с настоящими файлами уже
//! проверяют `arkanoid_demo`/`snake_demo`): демо-тетрис использует ту же экономную загрузку,
//! что и `tests/replays.rs` — «Тесты по записанному вводу», требование 33.

use std::fs;
use std::path::PathBuf;

use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::{load_rest, read_entry};

fn game_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("tetris");
    path.push(name);
    path
}

fn read(name: &str) -> String {
    let path = game_path(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

fn load() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let game_json = read("game.json");
    let (config, entry_warnings) =
        read_entry(&game_json).expect("game.json демо-тетриса должен разбираться");
    let (game, screens, warnings, _images) = load_rest(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &[],
        &[],
        &[],
        &[],
        Some(&read("code.lua")),
        true,
    )
    .expect("демо-тетрис должен проходить предстартовую проверку");
    let mut all_warnings = entry_warnings;
    all_warnings.extend(warnings);
    assert_eq!(all_warnings, Vec::new(), "{all_warnings:?}");
    (game, screens)
}

#[test]
fn tetris_demo_loads_with_no_warnings_and_the_first_piece_spawns_and_falls() {
    let (mut game, screens) = load();
    assert_eq!(
        game.world.alive_count(),
        0,
        "стартовый экран — меню без world_runs, мира ещё нет"
    );
    assert!(
        screens
            .screens
            .iter()
            .any(|s| s.name == "game" && s.world_runs)
    );
    assert!(screens.loss_screen.is_some());

    game.new_game();
    // game, 2 hidden wall segments above the well, 40 side wall cubes, 12 floor cubes,
    // hidden_rows, next_panel, flash = 58 scene objects.
    assert_eq!(game.world.alive_count(), 58);

    let falling = game.properties.resolve("falling").unwrap();
    let pivot = game.properties.resolve("pivot").unwrap();
    for _ in 0..2 {
        game.step(StepInput::empty());
    }
    assert!(game.is_running(), "{:?}", game.code_error());
    // + 4 в стакане (упавшая фигура) + 4 в окошке (следующая) — требование 47.
    assert_eq!(game.world.alive_count(), 66, "первая фигура появилась");
    let falling_count = game
        .world
        .ids()
        .filter(|&id| game.world.has(id, falling))
        .count();
    assert_eq!(falling_count, 4);
    let pivot_count = game
        .world
        .ids()
        .filter(|&id| game.world.has(id, pivot))
        .count();
    assert_eq!(pivot_count, 1);

    for _ in 0..50 {
        game.step(StepInput::empty());
    }
    assert!(game.is_running(), "{:?}", game.code_error());
    let head = game
        .world
        .ids()
        .find(|&id| game.world.has(id, pivot))
        .unwrap();
    let pos = game
        .world
        .vec2(head, property::POSITION)
        .expect("у фигуры есть position");
    assert!(pos[1] > 6.0, "фигура должна была сдвинуться вниз: {pos:?}");
}

/// «Правила игры», требование 38: форма и опорный кубик каждой из семи фигур при появлении —
/// NES ставит T/J/L плоской стороной вверх, пивот I на третьем кубике слева, пивот S на
/// верхнем среднем. Проверяется на самих данных `rules.json`, а не через прогон партии — так
/// не зависит от того, в каком порядке фигуры выпадут.
#[test]
fn tetris_piece_shapes_match_nes_orientation_and_pivot() {
    use engine::core::rules::{Rule, TemplateValue};
    use engine::core::value::Value;

    let (game, _screens) = load();
    let window_pivot = game.properties.resolve("window_pivot").unwrap();

    let mut expected: Vec<[[i32; 2]; 4]> = vec![
        [[-2, 0], [-1, 0], [0, 0], [1, 0]], // I
        [[-1, 0], [0, 0], [-1, 1], [0, 1]], // O
        [[-1, 0], [0, 0], [1, 0], [0, 1]],  // T
        [[0, 0], [1, 0], [-1, 1], [0, 1]],  // S
        [[-1, 0], [0, 0], [0, 1], [1, 1]],  // Z
        [[-1, 0], [0, 0], [1, 0], [1, 1]],  // J
        [[-1, 0], [0, 0], [1, 0], [-1, 1]], // L
    ]
    .into_iter()
    .map(|mut shape| {
        shape.sort();
        shape
    })
    .collect();

    let mut found = 0;
    for rule in &game.rules.rules {
        let Rule::Spawn {
            pick_one: Some(variants),
            ..
        } = rule
        else {
            continue;
        };
        for variant in variants {
            assert_eq!(variant.cells.len(), 4, "у фигуры тетриса четыре кубика");
            let pivot_at = variant
                .cells
                .iter()
                .find(|c| {
                    c.fields.iter().any(|(prop, v)| {
                        *prop == window_pivot
                            && matches!(v, TemplateValue::Const(Value::Flag(true)))
                    })
                })
                .map(|c| c.at)
                .expect("у каждой фигуры один опорный кубик");
            let mut shape: Vec<[i32; 2]> = variant
                .cells
                .iter()
                .map(|c| {
                    [
                        (c.at[0] - pivot_at[0]).round() as i32,
                        (c.at[1] - pivot_at[1]).round() as i32,
                    ]
                })
                .collect();
            shape.sort();
            let shape: [[i32; 2]; 4] = shape.try_into().unwrap();
            let idx = expected
                .iter()
                .position(|e| *e == shape)
                .unwrap_or_else(|| {
                    panic!("форма фигуры не совпала ни с одной из ожидаемых: {shape:?}")
                });
            expected.remove(idx);
            found += 1;
        }
    }
    assert_eq!(found, 7, "должны были встретиться все семь форм");
    assert!(expected.is_empty(), "не встретились формы: {expected:?}");
}

use std::fs;
use std::path::{Path, PathBuf};

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::{ImageVerdict, load_rest, read_entry};

fn game_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("rpg");
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
fn png_dimensions(path: &Path) -> (u32, u32) {
    let bytes = fs::read(path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (width, height)
}

/// Loads the real `games/rpg` folder end to end, image bytes included (no fonts, sounds or
/// music — the game declares none). Unlike `arkanoid_demo`'s `load`, this never calls
/// `new_game()`: "location" already runs the world from the moment `load_rest` returns it
/// (`Game::new` builds a live world and calls `update_camera()` itself when `start_screen` runs
/// straight away), exactly how the real page hands this same `Game` to the renderer without ever
/// going through `new_game`/`PlaySession` — calling `new_game()` here would only forget the
/// camera's followed point until the next step.
fn load() -> Game {
    let game_json = read("game.json");
    let (config, entry_warnings) =
        read_entry(&game_json).expect("game.json ролевой игры должен разбираться");
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
    let (game, _screens, warnings, _images) = load_rest(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &[],
        &[],
        &[],
        &image_verdicts,
        Some(&read("code.lua")),
        false,
    )
    .expect("ролевая игра должна проходить предстартовую проверку");
    let mut all_warnings = entry_warnings;
    all_warnings.extend(warnings);
    assert_eq!(all_warnings, Vec::new(), "{all_warnings:?}");
    game
}

fn find_named(game: &Game, name: &str) -> u32 {
    game.world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some(name))
        .unwrap_or_else(|| panic!("объекта \"{name}\" нет на сцене"))
}

#[derive(Clone, Copy, Debug)]
struct Rect {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}

fn rect_of(game: &Game, id: u32) -> Rect {
    let pos = game
        .world
        .vec2(id, property::POSITION)
        .unwrap_or_else(|| panic!("у объекта {id} нет position"));
    let size = game
        .world
        .vec2(id, property::SIZE)
        .unwrap_or_else(|| panic!("у объекта {id} нет size"));
    Rect {
        x0: pos[0],
        y0: pos[1],
        x1: pos[0] + size[0],
        y1: pos[1] + size[1],
    }
}

/// True overlap only — rectangles that merely touch at an edge (as a walker stopped flush
/// against an obstacle does) don't count, hence the small epsilon.
fn overlaps(a: Rect, b: Rect) -> bool {
    const EPS: f64 = 1e-6;
    a.x0 < b.x1 - EPS && a.x1 > b.x0 + EPS && a.y0 < b.y1 - EPS && a.y1 > b.y0 + EPS
}

fn obstacle_rects(game: &Game) -> Vec<Rect> {
    let obstacle = game
        .properties
        .resolve("obstacle")
        .expect("свойство \"obstacle\" объявлено");
    game.world
        .ids()
        .filter(|&id| game.world.flag(id, obstacle))
        .map(|id| rect_of(game, id))
        .collect()
}

#[test]
fn rpg_loads_without_errors_or_warnings() {
    load();
}

#[test]
fn images_are_declared_at_their_real_generated_size() {
    let (hero_w, hero_h) = png_dimensions(&game_path("images/hero.png"));
    assert_eq!((hero_w, hero_h), (384, 48), "лента героя — 12 кадров 32×48");
    let (marker_w, marker_h) = png_dimensions(&game_path("images/marker.png"));
    assert_eq!((marker_w, marker_h), (32, 32), "кольцо — 32×32");
}

/// «Локация», требование 9: 40–60 препятствий, все целиком в сцене, ни одно не накрывает вход.
#[test]
fn obstacles_are_within_range_inside_the_scene_and_clear_of_the_entrance() {
    let game = load();
    let rects = obstacle_rects(&game);
    assert!(
        (40..=60).contains(&rects.len()),
        "препятствий {}, ожидалось 40–60",
        rects.len()
    );
    for r in &rects {
        assert!(
            r.x0 >= 0.0 && r.y0 >= 0.0 && r.x1 <= 64.0 && r.y1 <= 40.0,
            "препятствие {r:?} выходит за пределы сцены 64×40"
        );
    }

    let hero = find_named(&game, "hero");
    let hero_rect = rect_of(&game, hero);
    let entrance_clearance = Rect {
        x0: hero_rect.x0 - 3.0,
        y0: hero_rect.y0 - 3.0,
        x1: hero_rect.x1 + 3.0,
        y1: hero_rect.y1 + 3.0,
    };
    for r in &rects {
        assert!(
            !overlaps(*r, entrance_clearance),
            "препятствие {r:?} стоит ближе 3 клеток ко входу {hero_rect:?}"
        );
    }
}

/// «Камера и окно», требование 19: окно 1600×900 в начале партии — центр видимой части
/// (10⅔; 18.75), камера прижата к левому краю у входа.
#[test]
fn camera_is_pinned_to_the_left_edge_at_the_entrance() {
    let game = load();
    let (scale, offset) = game.camera_frame([1600.0, 900.0]);
    let center = game
        .scene
        .window_to_scene_frame([800.0, 450.0], scale, offset);
    assert!((center[0] - 32.0 / 3.0).abs() < 1e-3, "{center:?}");
    assert!((center[1] - 18.75).abs() < 1e-3, "{center:?}");
}

/// «Крайние случаи»: щелчок за брод и через проём развалин — герой доходит до цели, и на ни одном
/// шаге его прямоугольник не заходит внутрь ни одного препятствия (стены, воды, камня или ствола).
#[test]
fn hero_walks_through_a_ford_and_a_ruin_gap_without_ever_entering_an_obstacle() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let obstacles = obstacle_rects(&game);

    // Inside the ruins' interior, clear of both rocks placed there (48,17)-(50,18) and
    // (51,20)-(52,21) — reached only through the left gap (45,18)-(46,20) or the bottom one
    // (49,24)-(51,25), themselves reached only by crossing the river through one of its two
    // fords.
    game.set_cursor_cell([53.0, 17.0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);

    let max_steps = 3000;
    let mut arrived = false;
    for step in 0..max_steps {
        let rect = rect_of(&game, hero);
        for obstacle in &obstacles {
            assert!(
                !overlaps(rect, *obstacle),
                "шаг {step}: герой {rect:?} зашёл внутрь препятствия {obstacle:?}"
            );
        }
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            arrived = true;
            break;
        }
        game.step(StepInput::empty());
    }
    assert!(arrived, "герой не дошёл до цели за {max_steps} шагов");

    let rect = rect_of(&game, hero);
    let center = [(rect.x0 + rect.x1) / 2.0, (rect.y0 + rect.y1) / 2.0];
    assert!((center[0] - 53.0).abs() < 1e-3, "{center:?}");
    assert!((center[1] - 17.0).abs() < 1e-3, "{center:?}");
}

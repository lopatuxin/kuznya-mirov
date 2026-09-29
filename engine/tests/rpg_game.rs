use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

use engine::core::game::Game;
use engine::core::grid::Rect as PathRect;
use engine::core::input::StepInput;
use engine::core::pathfind::{self, WalkCaches};
use engine::core::property;
use engine::data::load::{ImageVerdict, load_rest_with_tables, read_entry};

/// Картинки героя и врагов LPC — 32 точки на клетку; остальные картинки игры нарисованы для
/// деревни, 96 точек на клетку.
const LPC_IMAGES: [&str; 6] = [
    "hero",
    "goblin",
    "orc",
    "hero_attack",
    "goblin_attack",
    "orc_attack",
];

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

/// Loads the real `games/rpg` folder end to end, image bytes and the `enemies` table included
/// (no fonts or sounds — the game declares no sounds, and the font is only needed by rendering,
/// not by prestart validation, so its bytes are never read here either). Like `arkanoid_demo`'s
/// `load`, never calls `new_game()`: "location" already runs the world from the moment
/// `load_rest_with_tables` returns it.
/// Loads with `scene_json` in place of the real `games/rpg/scene.json` text — used as-is by
/// `load()`, and with a deliberately corrupted copy by
/// `enemy_catalog_typo_names_the_enemy_and_its_bad_row`.
fn load_with_scene(scene_json: &str) -> Game {
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
    let table_texts: Vec<(String, Option<String>)> = config
        .files
        .tables
        .iter()
        .map(|(name, path)| (name.clone(), Some(read(path))))
        .collect();
    let font_bytes: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| {
            (
                name.clone(),
                Some(fs::read(game_path(path)).unwrap_or_else(|e| panic!("{path}: {e}"))),
            )
        })
        .collect();
    let (game, _screens, warnings, _images) = load_rest_with_tables(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(scene_json),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &font_bytes,
        &[],
        &[],
        &image_verdicts,
        Some(&read("code.lua")),
        false,
        &table_texts,
    )
    .expect("ролевая игра должна проходить предстартовую проверку");
    let mut all_warnings = entry_warnings;
    all_warnings.extend(warnings);
    assert_eq!(all_warnings, Vec::new(), "{all_warnings:?}");
    game
}

fn load() -> Game {
    load_with_scene(&read("scene.json"))
}

fn find_named(game: &Game, name: &str) -> u32 {
    game.world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some(name))
        .unwrap_or_else(|| panic!("объекта \"{name}\" нет на сцене"))
}

/// `game.json → files → images`, in declared order — the same order `data::load::load_rest_with_tables`
/// gives each image its `ImageId`, so `image_names()[id]` names what `world.image` returns.
fn image_names() -> Vec<String> {
    let game_json = read("game.json");
    let (config, _) = read_entry(&game_json).expect("game.json должен разбираться");
    config.files.images.iter().map(|d| d.name.clone()).collect()
}

fn current_image<'a>(game: &Game, names: &'a [String], id: u32) -> Option<&'a str> {
    let prop = game
        .properties
        .resolve("image")
        .expect("свойство \"image\" объявлено");
    game.world.image(id, prop).map(|idx| names[idx].as_str())
}

/// Mirrors `code.lua`'s own `side_from_delta` — used to derive the side a test *expects* from the
/// actual geometry after the hero arrives, rather than hardcoding an assumed approach direction.
fn side_from_delta(dx: f64, dy: f64) -> &'static str {
    if dx.abs() >= dy.abs() {
        if dx >= 0.0 { "right" } else { "left" }
    } else if dy >= 0.0 {
        "down"
    } else {
        "up"
    }
}

fn row_of(side: &str) -> i64 {
    match side {
        "up" => 0,
        "left" => 1,
        "down" => 2,
        "right" => 3,
        other => panic!("неизвестная сторона {other}"),
    }
}

fn idle_frame_value(side: &str) -> f64 {
    (9 * row_of(side)) as f64
}

fn attack_frame_value(side: &str, col: i64) -> f64 {
    (6 * row_of(side) + col) as f64
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

fn rect_center(r: Rect) -> (f64, f64) {
    ((r.x0 + r.x1) / 2.0, (r.y0 + r.y1) / 2.0)
}

/// Every `obstacle` object's rectangle, split into the ones that are also an `enemy_unit` and
/// the ones that aren't — «Локация», требования 12 и 11: connectivity without any enemy versus
/// with each enemy standing.
fn obstacle_rects_by_kind(game: &Game) -> (Vec<Rect>, Vec<(String, Rect)>) {
    let obstacle = game
        .properties
        .resolve("obstacle")
        .expect("свойство \"obstacle\" объявлено");
    let enemy_unit = game
        .properties
        .resolve("enemy_unit")
        .expect("свойство \"enemy_unit\" объявлено");
    let mut static_rects = Vec::new();
    let mut enemy_rects = Vec::new();
    for id in game.world.ids() {
        if !game.world.flag(id, obstacle) {
            continue;
        }
        if game.world.flag(id, enemy_unit) {
            let name = game
                .world
                .text(id, property::NAME)
                .unwrap_or_else(|| panic!("у врага {id} нет имени"))
                .to_string();
            enemy_rects.push((name, rect_of(game, id)));
        } else {
            static_rects.push(rect_of(game, id));
        }
    }
    (static_rects, enemy_rects)
}

// «Ходьба», требование 27 и `engine/src/core/pathfind.rs`: препятствие раздувается на половину
// ширины и высоты идущего, сам идущий считается точкой. Тело героя из scene.json — 0.75 × 0.5.
const HERO_HALF_W: f64 = 0.75 / 2.0;
const HERO_HALF_H: f64 = 0.5 / 2.0;
// Достаточно мельче половины запаса вокруг любого препятствия (0.05 клетки хватает 2-клеточному
// коридору с запасом), чтобы не потерять узкий проход между стеной и одиночным препятствием.
const SAMPLE: f64 = 0.05;
/// Half a sample step, off every integer/quarter-cell coordinate. Obstacle edges in this scene
/// always land on such "nice" numbers, so a sampling grid aligned exactly to them can graze a
/// boundary two unrelated obstacles' inflated rectangles happen to share (their edges meeting at
/// the exact same coordinate) and read a mathematically zero-width seam as "free" — a single
/// point no actual hero-sized body could ever occupy. Offsetting every probed point by half a
/// step keeps it strictly off such a seam.
const SAMPLE_OFFSET: f64 = SAMPLE / 2.0;

fn inflate(rects: &[Rect]) -> Vec<Rect> {
    rects
        .iter()
        .map(|r| Rect {
            x0: r.x0 - HERO_HALF_W,
            y0: r.y0 - HERO_HALF_H,
            x1: r.x1 + HERO_HALF_W,
            y1: r.y1 + HERO_HALF_H,
        })
        .collect()
}

fn point_blocked(p: [f64; 2], inflated: &[Rect]) -> bool {
    inflated
        .iter()
        .any(|r| p[0] > r.x0 && p[0] < r.x1 && p[1] > r.y0 && p[1] < r.y1)
}

fn sample_index(v: f64) -> i64 {
    ((v - SAMPLE_OFFSET) / SAMPLE).round() as i64
}

fn sample_coord(i: i64) -> f64 {
    i as f64 * SAMPLE + SAMPLE_OFFSET
}

/// The search region a flood fill explores, in scene cells — `(x_min, y_min, x_max, y_max)`.
type Bounds = (f64, f64, f64, f64);

fn scene_bounds(game: &Game) -> Bounds {
    (0.0, 0.0, game.scene.width as f64, game.scene.height as f64)
}

/// Где может стоять середина героя: сцена, сжатая на его полуразмер — как в `pathfind::advance`.
fn hero_center_bounds(game: &Game) -> Bounds {
    (
        HERO_HALF_W,
        HERO_HALF_H,
        game.scene.width as f64 - HERO_HALF_W,
        game.scene.height as f64 - HERO_HALF_H,
    )
}

/// Flood fill of the sample grid from `start`, avoiding every inflated rect and never leaving
/// `bounds` — a discretized version of the same "point walker, obstacles inflated by its own
/// half-size" model `pathfind.rs` uses for the real `walk` rule.
fn flood_fill(inflated: &[Rect], start: [f64; 2], bounds: Bounds) -> HashSet<(i64, i64)> {
    let (x_min, y_min, x_max, y_max) = bounds;
    let col_min = sample_index(x_min);
    let col_max = sample_index(x_max);
    let row_min = sample_index(y_min);
    let row_max = sample_index(y_max);
    let mut visited = HashSet::new();
    let mut queue = VecDeque::new();
    let start_idx = (sample_index(start[0]), sample_index(start[1]));
    if point_blocked(
        [sample_coord(start_idx.0), sample_coord(start_idx.1)],
        inflated,
    ) {
        panic!("стартовая точка {start:?} сама внутри препятствия");
    }
    visited.insert(start_idx);
    queue.push_back(start_idx);
    while let Some((cx, cy)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let (nx, ny) = (cx + dx, cy + dy);
            if nx < col_min
                || ny < row_min
                || nx > col_max
                || ny > row_max
                || visited.contains(&(nx, ny))
            {
                continue;
            }
            let p = [sample_coord(nx), sample_coord(ny)];
            if point_blocked(p, inflated) {
                continue;
            }
            visited.insert((nx, ny));
            queue.push_back((nx, ny));
        }
    }
    visited
}

fn count_free(inflated: &[Rect], bounds: Bounds) -> usize {
    let (x_min, y_min, x_max, y_max) = bounds;
    let mut free = 0;
    let mut cx = sample_index(x_min);
    while cx <= sample_index(x_max) {
        let mut cy = sample_index(y_min);
        while cy <= sample_index(y_max) {
            let p = [sample_coord(cx), sample_coord(cy)];
            if !point_blocked(p, inflated) {
                free += 1;
            }
            cy += 1;
        }
        cx += 1;
    }
    free
}

/// «Фаза 2.5», требование 16 (`web/scripts/rpg-village/layout.mjs`, `ENEMIES`): три прохода
/// деревни идут цепочкой от входа героя к кузнице — гоблин в проходе внешней стены, гоблин в
/// проходе перегородки за ней, орк во входе во двор кузницы. Другого пути нет: мимо живого врага
/// дальше не пройти.
const ENEMY_CHAIN: [&str; 3] = ["goblin_1", "goblin_2", "orc_1"];

/// Свободная точка сразу за каждым врагом цепочки: за проходом внешней стены, за проходом
/// перегородки и во дворе кузницы, перед её фасадом (сама кузница занимает (27; 1)–(30,5; 3,5)).
const FORGE_TARGET: [f64; 2] = [28.5, 4.2];
const BEHIND_ENEMY: [[f64; 2]; 3] = [[21.5, 8.0], [27.5, 11.0], FORGE_TARGET];

/// Точки по всему двору кузницы — от левого края до правого, включая закуток за орком.
const FORGE_YARD_SAMPLES: [[f64; 2]; 5] = [
    FORGE_TARGET,
    [26.0, 3.0],
    [24.0, 2.5],
    [31.5, 3.0],
    [31.5, 4.5],
];

#[test]
fn rpg_loads_without_errors_or_warnings() {
    load();
}

#[test]
fn images_are_declared_at_their_real_generated_size() {
    for name in ["hero", "goblin", "orc"] {
        let (w, h) = png_dimensions(&game_path(&format!("images/{name}.png")));
        assert_eq!((w, h), (576, 320), "{name}: лист 9×5 кадров 64×64");
    }
    let (gw, gh) = png_dimensions(&game_path("images/grass.png"));
    assert_eq!((gw, gh), (384, 384), "grass: 4×4 плитки по 96×96");

    // «Фаза 2.5», требования 1 и 6: нарисованные куски деревни — 96 точек на клетку, у картинки
    // без `size` (трава) кадр — одна клетка; картинки героя и врагов LPC — по-прежнему 32 точки на
    // клетку, лист удара — сетка 6×4 кадров того же расчёта.
    let game_json = read("game.json");
    let (config, _) = read_entry(&game_json).expect("game.json должен разбираться");
    let grass = config
        .files
        .images
        .iter()
        .find(|decl| decl.name == "grass")
        .expect("картинка \"grass\" объявлена");
    assert_eq!(
        (grass.frames, grass.columns),
        (16, Some(4)),
        "grass: набор из 16 плиток 4×4"
    );
    for decl in &config.files.images {
        let Some(size) = decl.size else { continue };
        let pixels_per_cell = if LPC_IMAGES.contains(&decl.name.as_str()) {
            32.0
        } else {
            96.0
        };
        let (w, h) = png_dimensions(&game_path(&decl.path));
        if decl.frames > 1 {
            let columns = decl.columns.unwrap_or(1).max(1);
            let rows = decl.frames.div_ceil(columns);
            let expected = (
                (size[0] * pixels_per_cell * columns as f64).round() as u32,
                (size[1] * pixels_per_cell * rows as f64).round() as u32,
            );
            assert_eq!(
                (w, h),
                expected,
                "{}: размер листа кадров не сходится",
                decl.name
            );
            continue;
        }
        let expected = (
            (size[0] * pixels_per_cell).round() as u32,
            (size[1] * pixels_per_cell).round() as u32,
        );
        assert_eq!(
            (w, h),
            expected,
            "{}: размер картинки не {pixels_per_cell} точек на клетку",
            decl.name
        );
    }
}

#[test]
fn every_walkable_cell_is_reachable_from_the_entrance_without_any_enemy() {
    let game = load();
    let (static_rects, _enemies) = obstacle_rects_by_kind(&game);
    let inflated = inflate(&static_rects);
    let hero = find_named(&game, "hero");
    let hero_rect = rect_of(&game, hero);
    let start = [
        (hero_rect.x0 + hero_rect.x1) / 2.0,
        (hero_rect.y0 + hero_rect.y1) / 2.0,
    ];
    let bounds = scene_bounds(&game);
    let visited = flood_fill(&inflated, start, bounds);
    let free = count_free(&inflated, bounds);
    assert_eq!(
        visited.len(),
        free,
        "без врагов из входа должна быть достижима каждая проходимая клетка: дошёл до {} точек из {}",
        visited.len(),
        free
    );
}

#[test]
fn the_forge_yard_is_unreachable_while_all_enemies_stand() {
    let game = load();
    let (static_rects, enemy_rects) = obstacle_rects_by_kind(&game);
    let hero = find_named(&game, "hero");
    let (hx, hy) = rect_center(rect_of(&game, hero));
    let start = [hx, hy];

    let bounds = hero_center_bounds(&game);
    let free_without_enemies = flood_fill(&inflate(&static_rects), start, bounds);
    let mut all_rects = static_rects;
    all_rects.extend(enemy_rects.into_iter().map(|(_, r)| r));
    let visited = flood_fill(&inflate(&all_rects), start, bounds);
    for p in FORGE_YARD_SAMPLES {
        let idx = (sample_index(p[0]), sample_index(p[1]));
        assert!(
            free_without_enemies.contains(&idx),
            "точка двора {p:?} должна быть достижима без врагов, иначе проверка пуста"
        );
        assert!(
            !visited.contains(&idx),
            "двор кузницы {p:?} достижим, хотя все враги живы"
        );
    }
}

/// «Фаза 2.5», требование 16: цель каждого прохода достижима из входа тогда и только тогда, когда
/// убиты все враги цепочки до неё включительно (за первым проходом — гоблин 1; за вторым — оба
/// гоблина; во дворе кузницы — все трое) — настоящим поиском пути движка (`pathfind::advance`,
/// тот же, что использует правило `walk`), не сеткой проб. Проверяет все 8 наборов убитых врагов.
#[test]
fn every_passage_opens_only_when_all_enemies_up_to_it_are_gone() {
    let game = load();
    let (static_rects, enemy_rects) = obstacle_rects_by_kind(&game);
    let hero = find_named(&game, "hero");
    let hero_rect = rect_of(&game, hero);
    let hero_size = [hero_rect.x1 - hero_rect.x0, hero_rect.y1 - hero_rect.y0];
    let start = {
        let (x, y) = rect_center(hero_rect);
        [x, y]
    };
    let scene_size = (game.scene.width as f64, game.scene.height as f64);

    let path_rect = |r: &Rect| PathRect {
        x: r.x0,
        y: r.y0,
        w: r.x1 - r.x0,
        h: r.y1 - r.y0,
    };
    let base_obstacles: Vec<(u32, PathRect)> = static_rects
        .iter()
        .enumerate()
        .map(|(i, r)| (i as u32, path_rect(r)))
        .collect();

    for mask in 0u8..(1 << ENEMY_CHAIN.len()) {
        let removed: [bool; 3] = std::array::from_fn(|i| mask & (1 << i) != 0);
        let mut obstacles = base_obstacles.clone();
        let mut next_id = obstacles.len() as u32;
        for (i, name) in ENEMY_CHAIN.iter().enumerate() {
            if removed[i] {
                continue;
            }
            let (_, rect) = enemy_rects
                .iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("врага \"{name}\" нет на сцене"));
            obstacles.push((next_id, path_rect(rect)));
            next_id += 1;
        }

        for (passage, target) in BEHIND_ENEMY.iter().enumerate() {
            let mut caches = WalkCaches::new();
            let (pos, _) = pathfind::advance(
                hero,
                start,
                hero_size,
                *target,
                obstacles.clone(),
                scene_size,
                10_000.0,
                &mut caches,
            );
            let reached = (pos[0] - target[0]).abs() < 1e-6 && (pos[1] - target[1]).abs() < 1e-6;
            let expected = removed[..=passage].iter().all(|&gone| gone);
            assert_eq!(
                reached, expected,
                "маска {mask:03b} (goblin_1={}, goblin_2={}, orc_1={}): цель за врагом {} ({target:?}) — достижимость {reached}, ожидалось {expected}",
                removed[0], removed[1], removed[2], ENEMY_CHAIN[passage]
            );
        }
    }
}

/// «Фаза 2.5», требование 16: каждый враг цепочки один (остальные убраны) перекрывает свой проход
/// целиком — с одной стороны от него до другой (от входа героя, от цели прохода до него) по
/// сетке проб не дойти, а без него дойти, так что перекрывает именно он.
#[test]
fn each_enemy_alone_blocks_its_own_corridor() {
    let game = load();
    let (static_rects, enemy_rects) = obstacle_rects_by_kind(&game);
    assert_eq!(
        enemy_rects.len(),
        ENEMY_CHAIN.len(),
        "враги сцены: {ENEMY_CHAIN:?}"
    );
    let hero = find_named(&game, "hero");
    let (hx, hy) = rect_center(rect_of(&game, hero));
    let entrance = [hx, hy];
    let inflated_static = inflate(&static_rects);
    let bounds = hero_center_bounds(&game);

    for (i, name) in ENEMY_CHAIN.iter().enumerate() {
        let before = if i == 0 {
            entrance
        } else {
            BEHIND_ENEMY[i - 1]
        };
        let after = BEHIND_ENEMY[i];
        let after_idx = (sample_index(after[0]), sample_index(after[1]));

        let open = flood_fill(&inflated_static, before, bounds);
        assert!(
            open.contains(&after_idx),
            "без врагов из {before:?} должно быть можно дойти до {after:?}, иначе проверка пуста"
        );

        let (_, rect) = enemy_rects
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("врага \"{name}\" нет на сцене"));
        let mut rects = static_rects.clone();
        rects.push(*rect);
        let visited = flood_fill(&inflate(&rects), before, bounds);
        assert!(
            !visited.contains(&after_idx),
            "{name} один должен перекрывать свой проход: {before:?} не должно достигать {after:?}"
        );
    }
}

/// «Крайние случаи»: щелчок по гоблину — герой подходит вплотную и бой начинается сразу, враг
/// бьёт первым; повторный щелчок по тому же врагу не сбивает ход ударов; после третьего удара
/// героя гоблин падает и через 30 шагов удаляется, у героя 70 из 100.
#[test]
fn clicking_an_enemy_walks_the_hero_adjacent_and_combat_starts_with_the_enemys_hit() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let goblin_rect = rect_of(&game, goblin);
    let click = [
        (goblin_rect.x0 + goblin_rect.x1) / 2.0,
        (goblin_rect.y0 + goblin_rect.y1) / 2.0,
    ];
    game.set_cursor_cell(click);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);

    let max_steps = 2000;
    let mut arrived = false;
    for _ in 0..max_steps {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            arrived = true;
            break;
        }
        game.step(StepInput::empty());
    }
    assert!(arrived, "герой не дошёл до гоблина");

    let health = game.properties.resolve("health").unwrap();
    let max_health = game.properties.resolve("max_health").unwrap();
    assert_eq!(game.world.number_like(goblin, max_health), Some(30.0));
    assert_eq!(game.world.number_like(hero, max_health), Some(100.0));
    assert_eq!(
        game.world.number_like(hero, health),
        Some(100.0),
        "враг ещё не бил"
    );

    // Требование 24: первый удар — вражеский, на 15-м шаге боя, не раньше.
    for _ in 0..13 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(hero, health),
        Some(100.0),
        "на шаге раньше урон ещё не засчитан"
    );
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(hero, health),
        Some(90.0),
        "удар гоблина засчитан на 15-м шаге боя"
    );

    // Требование 24: следующий удар — геройский, через 60 шагов после первого (на 75-м шаге боя).
    for _ in 0..60 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(20.0),
        "удар героя засчитан на 75-м шаге боя"
    );

    // «Крайние случаи»: повторный щелчок по тому же врагу, герой не сдвинулся — бой не прерван,
    // ход ударов не сбит.
    game.key_up("MouseLeft");
    let release_snap = game.take_input_snapshot();
    game.step(release_snap);
    game.set_cursor_cell(click);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);

    // Off-by-one in both directions: one step before the second hero hit (step 195 of combat),
    // and exactly on it — a re-click that reset the turn cycle would shift this step, not just
    // change the final number a wide range of steps happens to also land on.
    for _ in 0..117 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(20.0),
        "на шаге раньше второй удар героя ещё не засчитан — повторный щелчок хода не сбил"
    );
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(10.0),
        "второй удар героя засчитан ровно на 195-м шаге боя — повторный щелчок не сбил ход"
    );

    for _ in 0..120 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(0.0),
        "третий удар героя добивает гоблина"
    );
    assert_eq!(
        game.world.number_like(hero, health),
        Some(70.0),
        "три удара гоблина по 10 — здоровье героя 70"
    );

    // Требование 26 (ошибка из ревью): герой остаётся лицом туда, где стоял погибший враг — не
    // «вниз» и не прежней стороной ходьбы, которую иначе переписал бы `animate_hero_walk` этим
    // же шагом.
    let frame = game.properties.resolve("frame").unwrap();
    let (hx, hy) = rect_center(rect_of(&game, hero));
    let (gx, gy) = rect_center(goblin_rect);
    let hero_side = side_from_delta(gx - hx, gy - hy);
    assert_eq!(
        game.world.number_like(hero, frame),
        Some(idle_frame_value(hero_side)),
        "герой должен остаться лицом туда, где стоял гоблин"
    );

    // Требование 26: падение — 30 шагов кадрами строки падения, затем гоблин удалён.
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(hero, frame),
        Some(idle_frame_value(hero_side)),
        "и на следующем шаге герой не должен вернуться к прежней стороне ходьбы"
    );
    assert_eq!(
        game.world.number_like(goblin, frame),
        Some(36.0),
        "первый шаг падения — кадр 0 строки 4 (9×4+0)"
    );
    for _ in 0..25 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, frame),
        Some(41.0),
        "на 26-м шаге падения — последний кадр строки 4 (9×4+5)"
    );
    for _ in 0..4 {
        game.step(StepInput::empty());
    }
    assert!(
        !game.world.is_alive(goblin),
        "гоблин должен быть удалён через 30 шагов после падения"
    );

    // «Локация», требование 11: проход свободен теперь, что гоблин погиб — герой доходит туда,
    // где тот стоял, а не просто снимает `walk_to` (снятый и потому, что цель недостижима, тоже
    // выглядел бы как «дошёл»): сверяем, где герой в итоге оказался.
    let beyond = [goblin_rect.x0 + 0.05, goblin_rect.y0 - 0.3];
    // The re-click earlier in this test left "MouseLeft" held — release it first, or this press
    // is suppressed (`InputQueue::press`) and the hero never gets a new `walk_to`.
    game.key_up("MouseLeft");
    let release_snap = game.take_input_snapshot();
    game.step(release_snap);
    game.set_cursor_cell(beyond);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    for _ in 0..2000 {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            break;
        }
        game.step(StepInput::empty());
    }
    let (hx, hy) = rect_center(rect_of(&game, hero));
    assert!(
        (hx - beyond[0]).abs() < 1e-6 && (hy - beyond[1]).abs() < 1e-6,
        "герой должен был реально дойти туда, где стоял гоблин: середина {:?} vs цель {beyond:?}",
        (hx, hy)
    );
}

/// «Крайние случаи»: щелчок по земле посреди боя прерывает его без урона.
#[test]
fn clicking_open_ground_mid_combat_interrupts_it_without_further_damage() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let goblin_rect = rect_of(&game, goblin);
    let click = [
        (goblin_rect.x0 + goblin_rect.x1) / 2.0,
        (goblin_rect.y0 + goblin_rect.y1) / 2.0,
    ];
    game.set_cursor_cell(click);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    for _ in 0..2000 {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            break;
        }
        game.step(StepInput::empty());
    }

    // Гоблин уже нанёс первый удар (15-й шаг боя) — здоровье героя ниже полного.
    for _ in 0..20 {
        game.step(StepInput::empty());
    }
    let health = game.properties.resolve("health").unwrap();
    let goblin_health_before = game.world.number_like(goblin, health).unwrap();
    let hero_health_before = game.world.number_like(hero, health).unwrap();
    assert!(hero_health_before < 100.0, "гоблин должен был уже ударить");

    // Отпустить первый щелчок, прежде чем слать следующий — «зажатая клавиша», без release,
    // подавила бы повторный press (`InputQueue::press`).
    game.key_up("MouseLeft");
    let release_snap = game.take_input_snapshot();
    game.step(release_snap);

    game.set_cursor_cell([1.0, 11.0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);

    // Достаточно шагов, чтобы геройский удар состоялся, не будь бой прерван.
    for _ in 0..200 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(goblin_health_before),
        "здоровье гоблина не должно было измениться — бой прерван щелчком по земле"
    );
    assert_eq!(
        game.world.number_like(hero, health),
        Some(hero_health_before),
        "здоровье героя не должно было измениться после прерывания"
    );

    // Требование 25: прерванный враг поворачивается лицом вниз; герой возвращается на основной
    // лист (мог быть на листе удара посреди своего замаха).
    let frame = game.properties.resolve("frame").unwrap();
    assert_eq!(
        game.world.number_like(goblin, frame),
        Some(idle_frame_value("down")),
        "враг должен встать лицом вниз после прерывания"
    );
    let names = image_names();
    assert_eq!(
        current_image(&game, &names, hero),
        Some("hero"),
        "герой должен вернуться на основной лист после прерывания"
    );
}

/// «Здоровье и исход»: герой, начавший бой с малым здоровьем, погибает от орка, встаёт у входа
/// с полным здоровьем, орк остаётся раненым.
#[test]
fn hero_dying_to_the_orc_respawns_at_the_entrance_while_the_orc_stays_wounded() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let orc = find_named(&game, "orc_1");
    let health = game.properties.resolve("health").unwrap();
    let entrance = rect_of(&game, hero);

    game.world.set_number(hero, health, 40.0);
    // Both goblins stand earlier in the same chain of passages and would otherwise block the way
    // there — removing them outright isolates this test to the orc fight itself, same as the real
    // "враг упал — удаляется" outcome, rather than leaving a dead `obstacle` flag lying around them.
    for goblin in ["goblin_1", "goblin_2"] {
        let goblin = find_named(&game, goblin);
        game.world.delete(goblin);
    }

    let orc_rect = rect_of(&game, orc);
    // Щелчок по нижней части орка, не по середине: цель внутри раздутого орка `walk` выносит к
    // ближайшему краю, а от середины (вход шире орка на 0,1 клетки по обе стороны) края север и
    // юг равноудалены — герой остановился бы в клетке от орка, не дойдя до боя.
    let click = [(orc_rect.x0 + orc_rect.x1) / 2.0, orc_rect.y1 - 0.2];
    game.set_cursor_cell(click);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    for _ in 0..2000 {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            break;
        }
        game.step(StepInput::empty());
    }

    // «Фаза 2.5», требование 16: орк стоит в единственном входе во двор кузницы — герой подходит к
    // нему снаружи двора, из коридора у края сцены (с юга), а не из-за спины из самого двора.
    let hero_rect = rect_of(&game, hero);
    assert!(
        hero_rect.y0 >= orc_rect.y1 - 1e-6,
        "герой должен подойти к орку снаружи двора (с юга), а не из него: {hero_rect:?} vs {orc_rect:?}"
    );

    // Критерий готовности: орк бьёт на 15/135/255-м шаге боя (15 урона за удар); 40 → 25 → 10 →
    // -5: герой погибает на третьем ударе орка, затем 30 шагов падения кадрами строки 4.
    for _ in 0..254 {
        game.step(StepInput::empty());
    }
    let frame = game.properties.resolve("frame").unwrap();
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(hero, frame),
        Some(36.0),
        "первый шаг падения героя — кадр 0 строки 4 (9×4+0)"
    );

    // Требование 26: пока герой падает, щелчки его не двигают.
    let mid_fall_rect = rect_of(&game, hero);
    game.set_cursor_cell([mid_fall_rect.x0 + 3.0, mid_fall_rect.y0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    let after_click_rect = rect_of(&game, hero);
    assert_eq!(
        (after_click_rect.x0, after_click_rect.y0),
        (mid_fall_rect.x0, mid_fall_rect.y0),
        "щелчок посреди падения не должен сдвинуть героя"
    );
    game.key_up("MouseLeft");
    let release_snap = game.take_input_snapshot();
    game.step(release_snap);

    for _ in 0..40 {
        game.step(StepInput::empty());
    }

    let max_health = game.properties.resolve("max_health").unwrap();
    let hero_rect = rect_of(&game, hero);
    assert_eq!(
        game.world.number_like(hero, health),
        game.world.number_like(hero, max_health),
        "герой должен воскреснуть с полным здоровьем"
    );
    assert!(
        (hero_rect.x0 - entrance.x0).abs() < 1e-6 && (hero_rect.y0 - entrance.y0).abs() < 1e-6,
        "герой должен встать у входа: {hero_rect:?} vs {entrance:?}"
    );
    assert_eq!(
        game.world.number_like(orc, health),
        Some(40.0),
        "орк должен остаться раненым на 40 из 60"
    );

    // «Здоровье и исход», требование 26: герой встаёт кадром 18 (стойка лицом вниз); враг, с
    // которым он бился, остаётся без выбора и лицом вниз.
    let frame = game.properties.resolve("frame").unwrap();
    assert_eq!(
        game.world.number_like(hero, frame),
        Some(18.0),
        "герой должен встать лицом вниз, кадр 18"
    );
    let target = game.properties.resolve("target").unwrap();
    assert!(
        !game.world.flag(orc, target),
        "с орка должен быть снят target"
    );
    assert_eq!(
        game.world.number_like(orc, frame),
        Some(idle_frame_value("down")),
        "орк должен остаться лицом вниз"
    );
}

/// «Фаза 2.5», требование 16: гоблин_1 перекрывает свой проход целиком — щелчок по клетке за ним
/// (та же самая цель, что доказывает `each_enemy_alone_blocks_its_own_corridor` по сетке проб),
/// пока он жив, не должен пропустить героя на ту сторону настоящими шагами движка, а не гипотезой.
#[test]
fn clicking_past_a_live_enemy_does_not_let_the_hero_bypass_it() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let entrance = rect_of(&game, hero);
    // Hero starts south of goblin_1's gap — the target is the point on the *north* side, across
    // the gap it plugs.
    let target = BEHIND_ENEMY[0];
    let goblin_rect = rect_of(&game, goblin);

    game.set_cursor_cell(target);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    for _ in 0..3000 {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            break;
        }
        game.step(StepInput::empty());
    }

    let hero_rect = rect_of(&game, hero);
    let (ex, ey) = rect_center(entrance);
    let (hx, hy) = rect_center(hero_rect);
    assert!(
        ((hx - ex).powi(2) + (hy - ey).powi(2)).sqrt() > 1.0,
        "герой должен был реально отойти от входа, а не остаться на месте: {hero_rect:?}"
    );
    assert!(
        hy >= goblin_rect.y1,
        "герой не должен пройти мимо живого гоблина_1 на северную сторону его прохода: середина {:?}",
        (hx, hy)
    );

    let health = game.properties.resolve("health").unwrap();
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(30.0),
        "гоблин ещё цел — герой лишь подошёл вплотную, а не прошёл сквозь"
    );
}

/// Clicks `target` and steps until the hero arrives (`walk_to` clears) or `max_steps` runs out.
fn click_and_wait_for_arrival(game: &mut Game, hero: u32, target: [f64; 2], max_steps: u32) {
    game.set_cursor_cell(target);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    for _ in 0..max_steps {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            return;
        }
        game.step(StepInput::empty());
    }
    panic!("герой не дошёл до цели {target:?} за {max_steps} шагов");
}

/// Требование 24: во время боя герой и враг стоят лицом друг к другу — на удар бьющий переходит
/// на свой лист удара и снова на основной по завершении замаха, кадры взмаха идут в сторону
/// противника.
#[test]
fn attacking_unit_wears_its_attack_sheet_during_the_swing_and_faces_the_foe() {
    let mut game = load();
    let names = image_names();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let goblin_rect = rect_of(&game, goblin);
    let click = [
        (goblin_rect.x0 + goblin_rect.x1) / 2.0,
        (goblin_rect.y0 + goblin_rect.y1) / 2.0,
    ];
    click_and_wait_for_arrival(&mut game, hero, click, 2000);

    let (hx, hy) = rect_center(rect_of(&game, hero));
    let (gx, gy) = rect_center(rect_of(&game, goblin));
    let hero_side = side_from_delta(gx - hx, gy - hy);
    let goblin_side = side_from_delta(hx - gx, hy - gy);
    let frame = game.properties.resolve("frame").unwrap();

    // Combat step 0 (arrival tick): the enemy already swings, on its own attack sheet, toward
    // the hero; the hero (defending) stands idle, facing the enemy.
    assert_eq!(current_image(&game, &names, goblin), Some("goblin_attack"));
    assert_eq!(
        game.world.number_like(goblin, frame),
        Some(attack_frame_value(goblin_side, 0))
    );
    assert_eq!(
        game.world.number_like(hero, frame),
        Some(idle_frame_value(hero_side))
    );

    // Five steps into the swing: the column advances every 5 steps.
    for _ in 0..5 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, frame),
        Some(attack_frame_value(goblin_side, 1))
    );

    // Past the 30-step swing, still inside the enemy's own 60-step turn: back on the base
    // sheet, idle, facing the hero.
    for _ in 0..25 {
        game.step(StepInput::empty());
    }
    assert_eq!(current_image(&game, &names, goblin), Some("goblin"));
    assert_eq!(
        game.world.number_like(goblin, frame),
        Some(idle_frame_value(goblin_side))
    );

    // The hero's own turn starts (30 more steps): the hero switches to its attack sheet,
    // swinging toward the enemy.
    for _ in 0..30 {
        game.step(StepInput::empty());
    }
    assert_eq!(current_image(&game, &names, hero), Some("hero_attack"));
    assert_eq!(
        game.world.number_like(hero, frame),
        Some(attack_frame_value(hero_side, 0))
    );

    // Past the hero's own swing: back on its base sheet.
    for _ in 0..30 {
        game.step(StepInput::empty());
    }
    assert_eq!(current_image(&game, &names, hero), Some("hero"));
}

/// «Крайние случаи»: бой прерван в тот же шаг, где засчитывается урон, — урон не засчитан.
#[test]
fn interrupting_combat_on_the_exact_damage_step_counts_no_damage() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let health = game.properties.resolve("health").unwrap();
    let goblin_rect = rect_of(&game, goblin);
    let click = [
        (goblin_rect.x0 + goblin_rect.x1) / 2.0,
        (goblin_rect.y0 + goblin_rect.y1) / 2.0,
    ];
    click_and_wait_for_arrival(&mut game, hero, click, 2000);

    // Release the click that walked the hero here — a still-held key would suppress the next
    // press (`InputQueue::press`) below. This release is itself one more combat step.
    game.key_up("MouseLeft");
    let release_snap = game.take_input_snapshot();
    game.step(release_snap);

    // Combat step 0 (arrival) and step 1 (the release above) already ran — 12 more steps reach
    // combat step 13, one short of the 15th step (index 14) that would count the enemy's hit.
    for _ in 0..12 {
        game.step(StepInput::empty());
    }
    assert_eq!(game.world.number_like(hero, health), Some(100.0));

    // Click open ground on this exact step: the hero starts moving this same tick, so `update_game`
    // sees it as already moved before the combat tick that would otherwise land the hit runs.
    game.set_cursor_cell([1.0, 11.0]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);

    assert_eq!(
        game.world.number_like(hero, health),
        Some(100.0),
        "удар на том самом шаге, где бой прервался, не должен был засчитаться"
    );
    assert_eq!(game.world.number_like(goblin, health), Some(30.0));

    for _ in 0..30 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(hero, health),
        Some(100.0),
        "здоровье героя должно было остаться прежним и позже"
    );
}

/// «Крайние случаи», требования 21, 23: щелчок по самому герою не двигает его (тот уже стоит
/// там), но снимает выбор со всех врагов через их же `keys` — бой заканчивается без урона.
#[test]
fn clicking_the_hero_itself_deselects_the_enemy_without_moving_it_and_ends_combat() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let health = game.properties.resolve("health").unwrap();
    let goblin_rect = rect_of(&game, goblin);
    let click = [
        (goblin_rect.x0 + goblin_rect.x1) / 2.0,
        (goblin_rect.y0 + goblin_rect.y1) / 2.0,
    ];
    click_and_wait_for_arrival(&mut game, hero, click, 2000);
    for _ in 0..5 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(hero, health),
        Some(100.0),
        "ещё до первого удара"
    );

    game.key_up("MouseLeft");
    let release_snap = game.take_input_snapshot();
    game.step(release_snap);

    let (hx, hy) = rect_center(rect_of(&game, hero));
    let before_click = rect_of(&game, hero);
    game.set_cursor_cell([hx, hy]);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    let after_click = rect_of(&game, hero);
    assert_eq!(
        (before_click.x0, before_click.y0),
        (after_click.x0, after_click.y0),
        "щелчок по самому себе не должен сдвинуть героя"
    );

    // Well past where the interrupted hit would have landed — combat stayed off.
    for _ in 0..120 {
        game.step(StepInput::empty());
    }
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(30.0),
        "бой должен был закончиться без единого удара"
    );
    assert_eq!(game.world.number_like(hero, health), Some(100.0));
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// «Локация», требование 13: поиск пути от входа в самый дальний угол лабиринта (по прямой) не
/// дольше 5 мс в `cargo test --release` — реальные препятствия сцены (без врагов: цель здесь —
/// самая дальняя достижимая точка без них), тот же `pathfind::advance` использует и правило
/// `walk`.
#[test]
fn pathfinding_to_the_farthest_free_corner_is_fast_in_release() {
    let game = load();
    let (static_rects, _enemies) = obstacle_rects_by_kind(&game);
    let hero = find_named(&game, "hero");
    let hero_rect = rect_of(&game, hero);
    let start = [
        (hero_rect.x0 + hero_rect.x1) / 2.0,
        (hero_rect.y0 + hero_rect.y1) / 2.0,
    ];
    let bounds = scene_bounds(&game);
    let inflated = inflate(&static_rects);
    let visited = flood_fill(&inflated, start, bounds);
    let farthest = visited
        .iter()
        .map(|&(cx, cy)| [sample_coord(cx), sample_coord(cy)])
        .max_by(|a, b| dist(start, *a).partial_cmp(&dist(start, *b)).unwrap())
        .expect("хотя бы одна достижимая точка");

    let obstacles: Vec<(u32, PathRect)> = static_rects
        .iter()
        .enumerate()
        .map(|(i, r)| {
            (
                i as u32,
                PathRect {
                    x: r.x0,
                    y: r.y0,
                    w: r.x1 - r.x0,
                    h: r.y1 - r.y0,
                },
            )
        })
        .collect();
    let hero_size = [hero_rect.x1 - hero_rect.x0, hero_rect.y1 - hero_rect.y0];
    let scene_size = (game.scene.width as f64, game.scene.height as f64);
    let mut caches = WalkCaches::new();

    let start_time = Instant::now();
    pathfind::advance(
        hero,
        start,
        hero_size,
        farthest,
        obstacles,
        scene_size,
        0.0,
        &mut caches,
    );
    let elapsed = start_time.elapsed();

    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_micros() <= 5000,
            "поиск пути до {farthest:?} занял {elapsed:?}, дольше 5 мс"
        );
    }
}

/// Требование 23: расстояние между прямоугольниками — обычное (по диагонали), не наибольший
/// зазор по осям. Ставит героя по диагонали от гоблина (юго-запад, в открытой земле — не внутри
/// стены) с зазором 0,3 клетки на каждой оси — наибольший зазор по осям был бы ровно 0,3 (бой
/// начался бы по старой, неверной формуле), а настоящее расстояние по диагонали — 0,3√2 ≈ 0,424,
/// дальше порога, так что бой не должен начаться.
#[test]
fn combat_distance_is_the_diagonal_not_the_larger_axis_gap() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let goblin_rect = rect_of(&game, goblin);

    game.world.set_vec2(
        hero,
        property::POSITION,
        [goblin_rect.x0 - 0.75 - 0.3, goblin_rect.y1 + 0.3],
    );
    let target = game.properties.resolve("target").unwrap();
    game.world.set_flag(goblin, target, true);

    for _ in 0..30 {
        game.step(StepInput::empty());
    }

    let health = game.properties.resolve("health").unwrap();
    assert_eq!(
        game.world.number_like(goblin, health),
        Some(30.0),
        "по диагонали 0,3×0,3 (наибольший зазор по осям 0,3, но по диагонали дальше) бой не должен был начаться"
    );
    assert_eq!(game.world.number_like(hero, health), Some(100.0));
}

/// Обратная проверка: зазор по одной оси (другая ось перекрывается, дистанция там 0) — что по
/// старой, что по новой формуле ровно 0,3, бой должен начаться. Ставит героя к югу от орка, в
/// коридоре перед входом во двор. Орк, а не гоблин: у обоих гоблинов зазор 0,3 по вычитанию выходит
/// на 7·10⁻¹⁶ больше порога (0,05 в их координатах двоично неточны), у орка — на 2·10⁻¹⁶ меньше.
#[test]
fn combat_starts_at_exactly_0_3_gap_on_a_single_axis() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let orc = find_named(&game, "orc_1");
    let orc_rect = rect_of(&game, orc);

    game.world.set_vec2(
        hero,
        property::POSITION,
        [(orc_rect.x0 + orc_rect.x1) / 2.0 - 0.375, orc_rect.y1 + 0.3],
    );
    let gap = rect_of(&game, hero).y0 - orc_rect.y1;
    assert!(
        (0.3 - 1e-9..=0.3).contains(&gap),
        "зазор должен быть 0,3 и не больше порога боя: {gap}"
    );
    let target = game.properties.resolve("target").unwrap();
    game.world.set_flag(orc, target, true);

    for _ in 0..30 {
        game.step(StepInput::empty());
    }

    let health = game.properties.resolve("health").unwrap();
    assert_eq!(
        game.world.number_like(hero, health),
        Some(85.0),
        "зазор ровно 0,3 по одной оси (по другой оси прямоугольники перекрываются) — бой должен был начаться, враг уже ударил первым"
    );
    assert_eq!(game.world.number_like(orc, health), Some(60.0));
}

/// `scene.json`'s real text with one enemy's `"enemy"` catalog reference corrupted — a typo, not
/// a missing field, so it still parses and loads; only `init_enemies`'s catalog lookup at the
/// first step fails. Built from the real file with an in-memory substitution (scene.json itself
/// is never touched), targeting `object_name`'s own block so the other two goblins keep their
/// real, valid reference.
fn scene_with_broken_enemy_reference(object_name: &str) -> String {
    let scene = read("scene.json");
    let name_marker = format!("\"name\": \"{object_name}\"");
    let name_pos = scene
        .find(&name_marker)
        .unwrap_or_else(|| panic!("объекта \"{object_name}\" нет в scene.json"));
    let enemy_marker = "\"enemy\": \"";
    let enemy_rel = scene[name_pos..]
        .find(enemy_marker)
        .unwrap_or_else(|| panic!("у \"{object_name}\" нет свойства \"enemy\""));
    let value_start = name_pos + enemy_rel + enemy_marker.len();
    let value_end = value_start
        + scene[value_start..]
            .find('"')
            .expect("строковое значение \"enemy\" закрывается кавычкой");
    let mut corrupted = scene;
    corrupted.replace_range(value_start..value_end, "gobelin");
    corrupted
}

/// Требование 20 (ошибка из ревью): опечатка в `enemy` даёт понятную ошибку кода с именем врага
/// и его строкой каталога — не «attempt to index a nil value».
#[test]
fn enemy_catalog_typo_names_the_enemy_and_its_bad_row() {
    let mut game = load_with_scene(&scene_with_broken_enemy_reference("goblin_1"));
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("опечатка в enemy должна остановить партию понятной ошибкой");
    assert!(
        err.message.contains("goblin_1"),
        "сообщение должно называть врага: {}",
        err.message
    );
    assert!(
        err.message.contains("gobelin"),
        "сообщение должно называть его строку каталога: {}",
        err.message
    );
    assert!(
        !err.message.contains("nil value"),
        "не должно быть сырой ошибкой Lua: {}",
        err.message
    );
}

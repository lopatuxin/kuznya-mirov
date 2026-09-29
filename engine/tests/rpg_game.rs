//! Ролевая игра `games/rpg` — деревня из фигур («Фаза-15», «Трёхмерная сцена»). Игра грузится из
//! настоящих файлов, ходьба, поиск пути, щелчок лучом и бой идут настоящими шагами движка.

use std::collections::{HashSet, VecDeque};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use engine::core::footprint::Footprint;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::pathfind::{self, WalkCaches};
use engine::core::property;
use engine::core::shapes::Body;
use engine::core::value::Shape;
use engine::data::load::{ImageVerdict, load_rest_with_tables, read_entry};

const WINDOW: [f32; 2] = [1920.0, 1080.0];

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

/// `width`/`height` straight out of the PNG's own `IHDR` chunk (bytes 16..24, big-endian).
fn png_dimensions(name: &str) -> (u32, u32) {
    let path = game_path(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    (width, height)
}

/// Loads the real `games/rpg` folder end to end, image sizes and the `enemies` table included.
/// `scene_json` stands in for the real `scene.json` text — a deliberately corrupted copy by
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
            let (width, height) = png_dimensions(&decl.path);
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

fn place_of(game: &Game, id: u32) -> Footprint {
    let position = game
        .world
        .vec2(id, property::POSITION)
        .unwrap_or_else(|| panic!("у объекта {id} нет position"));
    let size = game
        .world
        .vec2(id, property::SIZE)
        .unwrap_or_else(|| panic!("у объекта {id} нет size"));
    Footprint::rotated(position, size, game.world.rotation(id, property::ROTATION))
}

fn center_of(game: &Game, id: u32) -> [f64; 2] {
    place_of(game, id).center()
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

/// Every `obstacle` object's place, split into the ones that are also an `enemy_unit` and the
/// ones that aren't — «Локация»: connectivity without any enemy versus with each enemy standing.
fn obstacles_by_kind(game: &Game) -> (Vec<Footprint>, Vec<(String, Footprint)>) {
    let obstacle = game
        .properties
        .resolve("obstacle")
        .expect("obstacle объявлен");
    let enemy_unit = game
        .properties
        .resolve("enemy_unit")
        .expect("enemy_unit объявлен");
    let mut statics = Vec::new();
    let mut enemies = Vec::new();
    for id in game.world.ids() {
        if !game.world.flag(id, obstacle) {
            continue;
        }
        if game.world.flag(id, enemy_unit) {
            let name = game
                .world
                .text(id, property::NAME)
                .expect("у врага есть имя");
            enemies.push((name.to_string(), place_of(game, id)));
        } else {
            statics.push(place_of(game, id));
        }
    }
    (statics, enemies)
}

const HERO_SIZE: f64 = 0.6;
const HERO_HALF: f64 = HERO_SIZE / 2.0;
/// Шаг сетки проб; каждая проба сдвинута на полшага, чтобы не лечь на общий край двух препятствий.
const SAMPLE: f64 = 0.1;
const SAMPLE_OFFSET: f64 = SAMPLE / 2.0;

fn sample_index(v: f64) -> i64 {
    ((v - SAMPLE_OFFSET) / SAMPLE).round() as i64
}

fn sample_coord(i: i64) -> f64 {
    i as f64 * SAMPLE + SAMPLE_OFFSET
}

/// Помещается ли герой серединой в `p`: его квадрат не пересекает ни одно препятствие.
fn hero_fits(p: [f64; 2], obstacles: &[Footprint]) -> bool {
    let hero = Footprint::flat([p[0] - HERO_HALF, p[1] - HERO_HALF], [HERO_SIZE, HERO_SIZE]);
    let hero_box = hero.aabb();
    obstacles.iter().all(|o| {
        let b = o.aabb();
        let apart = b.x >= hero_box.x + hero_box.w
            || hero_box.x >= b.x + b.w
            || b.y >= hero_box.y + hero_box.h
            || hero_box.y >= b.y + b.h;
        apart || !o.overlaps(&hero)
    })
}

/// Flood fill of the sample grid from `start`, never leaving the scene shrunk by the hero's half
/// size — a discretized version of the same walk `pathfind.rs` performs for the real `walk` rule.
fn flood_fill(obstacles: &[Footprint], start: [f64; 2], scene: (f64, f64)) -> HashSet<(i64, i64)> {
    let (col_min, col_max) = (sample_index(HERO_HALF), sample_index(scene.0 - HERO_HALF));
    let (row_min, row_max) = (sample_index(HERO_HALF), sample_index(scene.1 - HERO_HALF));
    let start_idx = (sample_index(start[0]), sample_index(start[1]));
    assert!(
        hero_fits(
            [sample_coord(start_idx.0), sample_coord(start_idx.1)],
            obstacles
        ),
        "стартовая точка {start:?} сама внутри препятствия"
    );
    let mut visited = HashSet::from([start_idx]);
    let mut queue = VecDeque::from([start_idx]);
    while let Some((cx, cy)) = queue.pop_front() {
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            let next = (cx + dx, cy + dy);
            if next.0 < col_min
                || next.0 > col_max
                || next.1 < row_min
                || next.1 > row_max
                || visited.contains(&next)
                || !hero_fits([sample_coord(next.0), sample_coord(next.1)], obstacles)
            {
                continue;
            }
            visited.insert(next);
            queue.push_back(next);
        }
    }
    visited
}

fn count_free(obstacles: &[Footprint], scene: (f64, f64)) -> usize {
    let (col_min, col_max) = (sample_index(HERO_HALF), sample_index(scene.0 - HERO_HALF));
    let (row_min, row_max) = (sample_index(HERO_HALF), sample_index(scene.1 - HERO_HALF));
    let mut free = 0;
    for cx in col_min..=col_max {
        for cy in row_min..=row_max {
            if hero_fits([sample_coord(cx), sample_coord(cy)], obstacles) {
                free += 1;
            }
        }
    }
    free
}

fn scene_size(game: &Game) -> (f64, f64) {
    (game.scene.width as f64, game.scene.height as f64)
}

/// «Фаза 2.5», требование 16: три прохода деревни идут цепочкой от входа героя к кузнице — гоблин в
/// проходе внешней стены, гоблин в проходе перегородки за ней, орк во входе во двор кузницы.
/// Другого пути нет: мимо живого врага дальше не пройти.
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
    let game = load();
    assert!(game.scene.is_3d());
    assert!(!game.scene.y_sort);
}

/// Требование 32: из `images` уходят все картинки, кроме травы и кольца отметки, и файлов в папке
/// тоже остаётся только два.
#[test]
fn only_the_grass_and_the_marker_ring_remain_as_images() {
    let (config, _) = read_entry(&read("game.json")).expect("game.json должен разбираться");
    let names: Vec<&str> = config
        .files
        .images
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, ["grass", "marker"]);
    let (gw, gh) = png_dimensions("images/grass.png");
    assert_eq!((gw, gh), (384, 384), "grass: 4×4 плитки по 96×96");
    let mut files: Vec<String> = fs::read_dir(game_path("images"))
        .expect("папка images")
        .map(|e| e.expect("запись").file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    assert_eq!(files, ["grass.png", "marker.png"]);
}

/// Требование 33: каждый кусок стены — одна повёрнутая коробка толщиной 0,5 нужной высоты; никаких
/// лесенок и полосок картинок.
#[test]
fn every_wall_is_one_turned_box_half_a_cell_thick() {
    let game = load();
    let mut walls = 0;
    for id in game.world.ids() {
        let name = game.world.text(id, property::NAME).unwrap_or_default();
        assert!(
            !name.contains("_strip_"),
            "{name}: полоска картинки осталась"
        );
        let Some(material) = ["wall_stone", "wall_log", "wall_palisade"]
            .into_iter()
            .find(|m| name.starts_with(m))
        else {
            continue;
        };
        walls += 1;
        assert_eq!(
            game.world.shape(id, property::SHAPE),
            Some(Shape::Box),
            "{name}"
        );
        let size = game.world.vec2(id, property::SIZE).expect("size");
        assert!((size[1] - 0.5).abs() < 1e-9, "{name}: толщина {}", size[1]);
        let rotation = game
            .world
            .rotation(id, property::ROTATION)
            .expect("rotation");
        assert!(
            rotation.angle().abs() > 20.0 && rotation.angle().abs() < 30.0,
            "{name}: угол {}",
            rotation.angle()
        );
        let height = game
            .world
            .number_like(id, property::HEIGHT)
            .expect("height");
        let expected = match material {
            "wall_stone" => 1.2,
            "wall_log" => 2.0,
            _ => 2.4,
        };
        assert_eq!(height, expected, "{name}");
        assert!(
            game.world
                .flag(id, game.properties.resolve("obstacle").unwrap()),
            "{name}"
        );
    }
    assert_eq!(walls, 8, "восемь кусков стен деревни");
}

/// Требования 33: фигуры деревни — что и на каких прямоугольниках.
#[test]
fn buildings_trees_rocks_and_the_cast_are_the_prescribed_shapes() {
    let game = load();
    let check = |name: &str, shape: Shape, height: f64, size: [f64; 2]| {
        let id = find_named(&game, name);
        assert_eq!(game.world.shape(id, property::SHAPE), Some(shape), "{name}");
        assert_eq!(
            game.world.number_like(id, property::HEIGHT),
            Some(height),
            "{name}"
        );
        assert_eq!(game.world.vec2(id, property::SIZE), Some(size), "{name}");
    };
    check("izba_17", Shape::Box, 2.5, [3.0, 2.0]);
    check("smithy_19", Shape::Box, 3.0, [3.5, 2.5]);
    check("chertog_18", Shape::Box, 3.5, [4.0, 2.0]);
    check("post_8", Shape::Box, 2.6, [0.5, 0.5]);
    check("spruce_23", Shape::Cylinder, 4.2, [0.8, 0.5]);
    check("birch_33", Shape::Cylinder, 3.8, [0.7, 0.5]);
    check("boulder_39", Shape::Sphere, 0.8, [1.1, 0.7]);
    check("bush_45", Shape::Sphere, 1.0, [0.7, 0.5]);
    check("hero", Shape::Capsule, 1.8, [0.6, 0.6]);
    check("goblin_1", Shape::Capsule, 1.3, [0.9, 0.9]);
    check("orc_1", Shape::Capsule, 2.0, [0.9, 0.9]);
    // Вход героя и места врагов — как в расстановке деревни на начало фазы 15.
    assert_eq!(center_of(&game, find_named(&game, "hero")), [2.5, 19.5]);
    assert_eq!(center_of(&game, find_named(&game, "goblin_1")), [21.5, 9.5]);
    assert_eq!(center_of(&game, find_named(&game, "goblin_2")), [27.5, 9.5]);
    assert_eq!(center_of(&game, find_named(&game, "orc_1")), [31.5, 6.5]);
}

/// Требование 33: ручей, мост и тропа — плоские полосы на земле, без фигур; под водой — невидимые
/// повёрнутые препятствия.
#[test]
fn the_stream_the_bridge_and_the_trail_lie_flat_and_the_water_underneath_is_invisible() {
    let game = load();
    for name in [
        "trail",
        "trail_far",
        "bridge",
        "stream_0_water",
        "stream_bridge_1_water",
        "stream_2_water",
    ] {
        let id = find_named(&game, name);
        assert_eq!(game.world.shape(id, property::SHAPE), None, "{name}");
        assert!(game.world.color(id, property::COLOR).is_some(), "{name}");
        assert!(
            game.world.rotation(id, property::ROTATION).is_some(),
            "{name}"
        );
    }
    let water: Vec<u32> = game
        .world
        .ids()
        .filter(|&id| {
            game.world
                .text(id, property::NAME)
                .is_some_and(|n| n.starts_with("water_"))
        })
        .collect();
    assert!(water.len() >= 4);
    for id in water {
        assert!(game.world.color(id, property::COLOR).is_none());
        assert!(game.world.image(id, property::IMAGE).is_none());
        assert!(game.world.rotation(id, property::ROTATION).is_some());
        assert!(
            game.world
                .flag(id, game.properties.resolve("obstacle").unwrap())
        );
    }
}

/// Ось вдоль полосы: единичный вектор её длины и координата середины вдоль него.
fn along(game: &Game, id: u32) -> ([f64; 2], f64) {
    let (sin, cos) = game
        .world
        .rotation(id, property::ROTATION)
        .expect("rotation")
        .sin_cos();
    let axis = [cos, sin];
    let center = center_of(game, id);
    (axis, center[0] * axis[0] + center[1] * axis[1])
}

/// Конец полосы — середина её короткой стороны: `sign` −1 — начало, +1 — конец вдоль длины.
fn strip_end(game: &Game, id: u32, sign: f64) -> [f64; 2] {
    let (axis, _) = along(game, id);
    let half = game.world.vec2(id, property::SIZE).expect("size")[0] / 2.0;
    let center = center_of(game, id);
    [
        center[0] + sign * half * axis[0],
        center[1] + sign * half * axis[1],
    ]
}

/// Требование 33: мост поперёк ручья — под прямым углом к его направлению, на его середине; щель
/// в препятствиях воды — ровно по ширине моста вдоль ручья; тропа доходит до концов моста.
#[test]
fn the_bridge_lies_across_the_stream_and_the_trail_reaches_both_of_its_ends() {
    let game = load();
    let stream = find_named(&game, "stream_bridge_1_water");
    let bridge = find_named(&game, "bridge");
    let angle = |id: u32| {
        game.world
            .rotation(id, property::ROTATION)
            .expect("rotation")
            .angle()
    };
    let turn = (angle(bridge) - angle(stream)).rem_euclid(180.0);
    assert!((turn - 90.0).abs() < 1e-3, "мост к ручью под {turn}°");

    let (stream_axis, _) = along(&game, stream);
    let normal = [-stream_axis[1], stream_axis[0]];
    let across = |p: [f64; 2]| p[0] * normal[0] + p[1] * normal[1];
    let deck_center = center_of(&game, bridge);
    let deck_half_width = game.world.vec2(bridge, property::SIZE).expect("size")[1] / 2.0;
    let deck_along = deck_center[0] * stream_axis[0] + deck_center[1] * stream_axis[1];
    assert!(
        (across(deck_center) - across(center_of(&game, find_named(&game, "water_2")))).abs() < 0.1,
        "мост стоит не на середине ручья"
    );

    let (before, before_along) = along(&game, find_named(&game, "water_2"));
    let (after, after_along) = along(&game, find_named(&game, "water_1"));
    assert_eq!((before, after), (stream_axis, stream_axis));
    let half_length = |name: &str| {
        game.world
            .vec2(find_named(&game, name), property::SIZE)
            .expect("size")[0]
            / 2.0
    };
    assert!(
        (before_along + half_length("water_2") - (deck_along - deck_half_width)).abs() < 1e-3,
        "вода до моста кончается не у его края"
    );
    assert!(
        (after_along - half_length("water_1") - (deck_along + deck_half_width)).abs() < 1e-3,
        "вода за мостом начинается не у его края"
    );

    let deck = place_of(&game, bridge);
    let trail = find_named(&game, "trail");
    let trail_far = find_named(&game, "trail_far");
    let entrance = center_of(&game, find_named(&game, "hero"));
    assert!(dist(strip_end(&game, trail, -1.0), entrance) < 0.1);
    assert!(
        deck.contains(strip_end(&game, trail, 1.0)),
        "тропа не дошла до моста"
    );
    assert!(
        deck.contains(strip_end(&game, trail_far, -1.0)),
        "дальняя тропа не начинается на мосту"
    );
    let passage = center_of(&game, find_named(&game, "goblin_1"));
    assert!(dist(strip_end(&game, trail_far, 1.0), passage) < 0.5);
}

/// Требование 33: под водой препятствия, под мостом их нет — квадрат героя, целиком стоящий на воде
/// у моста, пересекает препятствие воды или стоит на настиле, а сквозь воду по обе стороны от моста
/// не пройти. Пересечения — повёрнутых прямоугольников (`footprint`).
#[test]
fn the_water_beside_the_bridge_is_impassable() {
    let game = load();
    let (statics, _enemies) = obstacles_by_kind(&game);
    let scene = scene_size(&game);
    let water = place_of(&game, find_named(&game, "stream_bridge_1_water"));
    let deck = place_of(&game, find_named(&game, "bridge"));
    let obstacles: Vec<Footprint> = game
        .world
        .ids()
        .filter(|&id| {
            game.world
                .text(id, property::NAME)
                .is_some_and(|name| name.starts_with("water_"))
        })
        .map(|id| place_of(&game, id))
        .collect();
    let stands_in = |outer: &Footprint, hero: &Footprint| {
        hero.corners().iter().all(|corner| outer.contains(*corner))
    };
    assert!(
        hero_fits(deck.center(), &statics),
        "на середине моста герой не помещается"
    );
    let (col_min, col_max) = (sample_index(HERO_HALF), sample_index(scene.0 - HERO_HALF));
    let (row_min, row_max) = (sample_index(HERO_HALF), sample_index(scene.1 - HERO_HALF));
    let (mut on_the_deck, mut in_the_water) = (0, 0);
    for cx in col_min..=col_max {
        for cy in row_min..=row_max {
            let p = [sample_coord(cx), sample_coord(cy)];
            let hero =
                Footprint::flat([p[0] - HERO_HALF, p[1] - HERO_HALF], [HERO_SIZE, HERO_SIZE]);
            if !stands_in(&water, &hero) {
                continue;
            }
            if obstacles.iter().any(|obstacle| obstacle.overlaps(&hero)) {
                in_the_water += 1;
            } else {
                assert!(
                    stands_in(&deck, &hero),
                    "герой целиком на воде в {p:?}, не на мосту и не в препятствии"
                );
                on_the_deck += 1;
            }
        }
    }
    assert!(on_the_deck > 0, "на мосту герою негде встать");
    assert!(
        in_the_water > 0,
        "препятствия воды не мешают ни одной пробе"
    );
}

#[test]
fn every_walkable_place_is_reachable_from_the_entrance_without_any_enemy() {
    let game = load();
    let (statics, _enemies) = obstacles_by_kind(&game);
    let hero = find_named(&game, "hero");
    let scene = scene_size(&game);
    let visited = flood_fill(&statics, center_of(&game, hero), scene);
    let free = count_free(&statics, scene);
    assert_eq!(
        visited.len(),
        free,
        "без врагов из входа должно быть достижимо каждое место, где помещается герой: дошёл до {} проб из {}",
        visited.len(),
        free
    );
}

#[test]
fn the_forge_yard_is_unreachable_while_all_enemies_stand() {
    let game = load();
    let (statics, enemies) = obstacles_by_kind(&game);
    let hero = find_named(&game, "hero");
    let start = center_of(&game, hero);
    let scene = scene_size(&game);

    let without_enemies = flood_fill(&statics, start, scene);
    let mut everything = statics;
    everything.extend(enemies.into_iter().map(|(_, place)| place));
    let with_enemies = flood_fill(&everything, start, scene);
    for p in FORGE_YARD_SAMPLES {
        let idx = (sample_index(p[0]), sample_index(p[1]));
        assert!(
            without_enemies.contains(&idx),
            "точка двора {p:?} должна быть достижима без врагов, иначе проверка пуста"
        );
        assert!(
            !with_enemies.contains(&idx),
            "двор кузницы {p:?} достижим, хотя все враги живы"
        );
    }
}

fn obstacles_with(
    statics: &[Footprint],
    enemies: &[(String, Footprint)],
    alive: &[bool; 3],
) -> Vec<(u32, Footprint)> {
    let mut obstacles: Vec<(u32, Footprint)> = statics
        .iter()
        .enumerate()
        .map(|(i, place)| (i as u32, *place))
        .collect();
    for (i, name) in ENEMY_CHAIN.iter().enumerate() {
        if !alive[i] {
            continue;
        }
        let (_, place) = enemies
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("врага \"{name}\" нет на сцене"));
        obstacles.push((obstacles.len() as u32, *place));
    }
    obstacles
}

/// «Фаза 2.5», требование 16, требование 34: цель каждого прохода достижима из входа тогда и только
/// тогда, когда убиты все враги цепочки до неё включительно — настоящим поиском пути движка
/// (`pathfind::advance`, тот же, что использует правило `walk`). Все 8 наборов убитых врагов.
#[test]
fn every_passage_opens_only_when_all_enemies_up_to_it_are_gone() {
    let game = load();
    let (statics, enemies) = obstacles_by_kind(&game);
    let hero = find_named(&game, "hero");
    let start = center_of(&game, hero);
    let scene = scene_size(&game);

    for mask in 0u8..(1 << ENEMY_CHAIN.len()) {
        let killed: [bool; 3] = std::array::from_fn(|i| mask & (1 << i) != 0);
        let alive = killed.map(|gone| !gone);
        let obstacles = obstacles_with(&statics, &enemies, &alive);
        for (passage, target) in BEHIND_ENEMY.iter().enumerate() {
            let mut caches = WalkCaches::new();
            let (pos, _) = pathfind::advance(
                hero,
                start,
                [HERO_SIZE, HERO_SIZE],
                None,
                *target,
                obstacles.clone(),
                scene,
                10_000.0,
                &mut caches,
            );
            let reached = dist(pos, *target) < 1e-6;
            let expected = killed[..=passage].iter().all(|&gone| gone);
            assert_eq!(
                reached, expected,
                "маска {mask:03b} (goblin_1={}, goblin_2={}, orc_1={}): цель за врагом {} ({target:?}) — достижимость {reached}, ожидалось {expected}",
                killed[0], killed[1], killed[2], ENEMY_CHAIN[passage]
            );
        }
    }
}

/// Требование 34: каждый враг цепочки один (остальные убраны) перекрывает свой проход целиком — по
/// сетке проб с одной стороны от него до другой не дойти, а без него дойти.
#[test]
fn each_enemy_alone_blocks_its_own_corridor() {
    let game = load();
    let (statics, enemies) = obstacles_by_kind(&game);
    assert_eq!(
        enemies.len(),
        ENEMY_CHAIN.len(),
        "враги сцены: {ENEMY_CHAIN:?}"
    );
    let hero = find_named(&game, "hero");
    let entrance = center_of(&game, hero);
    let scene = scene_size(&game);

    for (i, name) in ENEMY_CHAIN.iter().enumerate() {
        let before = if i == 0 {
            entrance
        } else {
            BEHIND_ENEMY[i - 1]
        };
        let after = BEHIND_ENEMY[i];
        let after_idx = (sample_index(after[0]), sample_index(after[1]));

        let open = flood_fill(&statics, before, scene);
        assert!(
            open.contains(&after_idx),
            "без врагов из {before:?} должно быть можно дойти до {after:?}, иначе проверка пуста"
        );

        let (_, place) = enemies
            .iter()
            .find(|(n, _)| n == name)
            .unwrap_or_else(|| panic!("врага \"{name}\" нет на сцене"));
        let mut blocked = statics.clone();
        blocked.push(*place);
        let visited = flood_fill(&blocked, before, scene);
        assert!(
            !visited.contains(&after_idx),
            "{name} один должен перекрывать свой проход: {before:?} не должно достигать {after:?}"
        );
    }
}

/// Щелчок по точке тела фигуры: луч от глаза камеры через точку на теле; в записи — точка земли
/// под курсором, не прижатая к краю сцены, и место камеры.
fn click_body(game: &mut Game, id: u32, height_share: f64) {
    let camera = game.camera_3d(WINDOW).expect("сцена трёхмерная");
    let body = Body::of_object(&game.world, id).expect("у объекта есть фигура");
    let point = [body.center[0], body.center[1], body.height * height_share];
    let window = camera.project(point).expect("тело перед камерой");
    press(game, camera.ground_point(window));
}

fn click_ground(game: &mut Game, cell: [f64; 2]) {
    press(game, cell);
}

fn press(game: &mut Game, cell: [f64; 2]) {
    let camera = game.camera_3d(WINDOW).expect("сцена трёхмерная");
    game.set_cursor_ray(cell, camera.eye);
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
}

fn release(game: &mut Game) {
    game.key_up("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
}

fn walk_until_arrival(game: &mut Game, hero: u32, max_steps: u32) {
    for _ in 0..max_steps {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            return;
        }
        game.step(StepInput::empty());
    }
    panic!("герой не дошёл за {max_steps} шагов");
}

fn number(game: &Game, id: u32, name: &str) -> Option<f64> {
    let prop = game
        .properties
        .resolve(name)
        .unwrap_or_else(|| panic!("свойства {name} нет"));
    game.world.number_like(id, prop)
}

fn steps(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

/// Требования 36–37: щелчок по капсуле гоблина попадает лучом во врага, герой встаёт вплотную,
/// бой идёт по очереди — первым бьёт враг, урон на 15-м шаге удара; повторный щелчок по тому же
/// врагу бой не прерывает; гоблин исчезает на шаге третьего удара героя.
#[test]
fn clicking_a_goblins_body_walks_the_hero_adjacent_and_the_fight_runs_to_the_goblins_end() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    click_body(&mut game, goblin, 0.7);
    walk_until_arrival(&mut game, hero, 2000);

    assert_eq!(number(&game, goblin, "max_health"), Some(30.0));
    assert_eq!(number(&game, hero, "max_health"), Some(100.0));
    assert_eq!(
        number(&game, hero, "health"),
        Some(100.0),
        "враг ещё не бил"
    );
    let flush = place_of(&game, hero).aabb();
    let enemy = place_of(&game, goblin).aabb();
    let gap_x = (enemy.x - (flush.x + flush.w))
        .max(flush.x - (enemy.x + enemy.w))
        .max(0.0);
    let gap_y = (enemy.y - (flush.y + flush.h))
        .max(flush.y - (enemy.y + enemy.h))
        .max(0.0);
    assert!(
        gap_x.hypot(gap_y) < 1e-6,
        "герой встал вплотную: зазоры {gap_x} {gap_y}"
    );

    // Первый удар — вражеский, на 15-м шаге боя, не раньше.
    steps(&mut game, 13);
    assert_eq!(
        number(&game, hero, "health"),
        Some(100.0),
        "на шаге раньше урон ещё не засчитан"
    );
    steps(&mut game, 1);
    assert_eq!(
        number(&game, hero, "health"),
        Some(90.0),
        "удар гоблина засчитан на 15-м шаге боя"
    );

    // Следующий удар — геройский, через 60 шагов (на 75-м шаге боя).
    steps(&mut game, 60);
    assert_eq!(
        number(&game, goblin, "health"),
        Some(20.0),
        "удар героя засчитан на 75-м шаге боя"
    );

    // Повторный щелчок по тому же врагу, герой не сдвинулся — бой не прерван, ход ударов не сбит.
    release(&mut game);
    click_body(&mut game, goblin, 0.7);
    steps(&mut game, 117);
    assert_eq!(
        number(&game, goblin, "health"),
        Some(20.0),
        "на шаге раньше второй удар героя ещё не засчитан"
    );
    steps(&mut game, 1);
    assert_eq!(
        number(&game, goblin, "health"),
        Some(10.0),
        "второй удар героя засчитан ровно на 195-м шаге боя"
    );

    // Третий удар героя — на 315-м шаге боя, гоблин исчезает на том же шаге.
    steps(&mut game, 119);
    assert!(
        game.world.is_alive(goblin),
        "за шаг до третьего удара гоблин ещё стоит"
    );
    steps(&mut game, 1);
    assert!(
        !game.world.is_alive(goblin),
        "гоблин исчез на шаге третьего удара героя"
    );
    assert_eq!(
        number(&game, hero, "health"),
        Some(70.0),
        "три удара гоблина по 10 — здоровье героя 70"
    );

    // Проход свободен: герой доходит туда, где гоблин стоял.
    let beyond = [21.5, 8.0];
    release(&mut game);
    click_ground(&mut game, beyond);
    walk_until_arrival(&mut game, hero, 2000);
    assert!(
        dist(center_of(&game, hero), beyond) < 1e-6,
        "{:?}",
        center_of(&game, hero)
    );
}

/// Требование 37: точка земли под курсором у капсулы лежит за ней, а герой всё равно идёт к самому
/// врагу — откуда бы ни пришёлся щелчок по телу.
#[test]
fn a_click_on_any_part_of_the_body_leads_the_hero_to_the_enemy_itself() {
    // Враг цепочки, кого убрали раньше него (проход к нему иначе закрыт), и доля высоты щелчка.
    let cases: [(&str, &[&str], f64); 7] = [
        ("goblin_1", &[], 0.15),
        ("goblin_1", &[], 0.5),
        ("goblin_1", &[], 0.95),
        ("goblin_2", &["goblin_1"], 0.15),
        ("goblin_2", &["goblin_1"], 0.5),
        ("orc_1", &["goblin_1", "goblin_2"], 0.15),
        ("orc_1", &["goblin_1", "goblin_2"], 0.9),
    ];
    for (name, killed, share) in cases {
        let mut game = load();
        for gone in killed {
            let gone = find_named(&game, gone);
            game.world.delete(gone);
        }
        let hero = find_named(&game, "hero");
        let goblin = find_named(&game, name);
        click_body(&mut game, goblin, share);
        let target = game.properties.resolve("target").unwrap();
        assert!(
            game.world.flag(goblin, target),
            "{name}: враг выбран щелчком на доле {share}"
        );
        walk_until_arrival(&mut game, hero, 3000);
        let (hero_box, goblin_box) = (place_of(&game, hero).aabb(), place_of(&game, goblin).aabb());
        let touching = hero_box.x + hero_box.w >= goblin_box.x - 1e-6
            && hero_box.x <= goblin_box.x + goblin_box.w + 1e-6
            && hero_box.y + hero_box.h >= goblin_box.y - 1e-6
            && hero_box.y <= goblin_box.y + goblin_box.h + 1e-6;
        assert!(
            touching,
            "{name}, доля {share}: герой встал не вплотную: {hero_box:?} против {goblin_box:?}"
        );
    }
}

/// «Крайние случаи»: щелчок по земле посреди боя прерывает его без дальнейшего урона.
#[test]
fn clicking_open_ground_mid_combat_interrupts_it_without_further_damage() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    click_body(&mut game, goblin, 0.7);
    walk_until_arrival(&mut game, hero, 2000);

    // Гоблин уже нанёс первый удар — здоровье героя ниже полного.
    steps(&mut game, 20);
    let goblin_health = number(&game, goblin, "health").unwrap();
    let hero_health = number(&game, hero, "health").unwrap();
    assert!(hero_health < 100.0, "гоблин должен был уже ударить");

    release(&mut game);
    click_ground(&mut game, [1.0, 11.0]);
    steps(&mut game, 200);
    assert_eq!(
        number(&game, goblin, "health"),
        Some(goblin_health),
        "бой прерван щелчком по земле"
    );
    assert_eq!(
        number(&game, hero, "health"),
        Some(hero_health),
        "и после прерывания урона нет"
    );
}

/// «Здоровье и исход», требование 36: герой, начавший бой с малым здоровьем, погибает от орка,
/// на том же шаге встаёт у входа с полным здоровьем; у орка остаётся то, что успел отнять герой.
#[test]
fn hero_dying_to_the_orc_stands_at_the_entrance_the_same_step_while_the_orc_stays_wounded() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let orc = find_named(&game, "orc_1");
    let entrance = center_of(&game, hero);

    let health = game.properties.resolve("health").unwrap();
    game.world.set_number(hero, health, 40.0);
    // Оба гоблина стоят раньше в той же цепочке проходов и закрыли бы путь: убираем их, как убрал
    // бы бой, чтобы проверка касалась только орка.
    for goblin in ["goblin_1", "goblin_2"] {
        let goblin = find_named(&game, goblin);
        game.world.delete(goblin);
    }
    click_body(&mut game, orc, 0.9);
    walk_until_arrival(&mut game, hero, 3000);
    assert!(
        dist(center_of(&game, hero), entrance) > 5.0,
        "герой дошёл до орка"
    );

    // Орк бьёт на 15/135/255-м шаге боя (15 урона за удар): 40 → 25 → 10 → −5; герой бьёт на 75-м
    // и 195-м — у орка 60 → 50 → 40.
    steps(&mut game, 253);
    assert_eq!(number(&game, hero, "health"), Some(10.0));
    steps(&mut game, 1);
    assert_eq!(
        number(&game, hero, "health"),
        number(&game, hero, "max_health"),
        "встал с полным здоровьем на том же шаге"
    );
    assert_eq!(center_of(&game, hero), entrance, "и стоит у входа");
    assert_eq!(
        number(&game, orc, "health"),
        Some(40.0),
        "орк остался раненым на 40 из 60"
    );
    assert!(
        game.world.vec2(hero, property::WALK_TO).is_none(),
        "walk_to снят"
    );
    let target = game.properties.resolve("target").unwrap();
    assert!(!game.world.flag(orc, target), "выбор орка снят");
}

/// Требование 34: гоблин_1 перекрывает свой проход целиком — щелчок по земле за ним, пока он жив,
/// не пропускает героя на ту сторону настоящими шагами движка.
#[test]
fn clicking_past_a_live_enemy_does_not_let_the_hero_bypass_it() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let entrance = center_of(&game, hero);
    let enemy_box = place_of(&game, goblin).aabb();

    click_ground(&mut game, BEHIND_ENEMY[0]);
    walk_until_arrival(&mut game, hero, 3000);

    let at = center_of(&game, hero);
    assert!(
        dist(at, entrance) > 1.0,
        "герой отошёл от входа, а не остался на месте: {at:?}"
    );
    assert!(
        at[1] >= enemy_box.y + enemy_box.h,
        "герой не должен пройти мимо живого гоблина на северную сторону его прохода: {at:?}"
    );
    assert_eq!(number(&game, goblin, "health"), Some(30.0), "гоблин цел");
}

/// Щелчок по земле под самим героем не двигает его (тот уже стоит там), но снимает выбор со всех
/// врагов через их `keys` — бой заканчивается без урона.
#[test]
fn clicking_the_ground_under_the_hero_deselects_the_enemy_and_ends_combat() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    click_body(&mut game, goblin, 0.7);
    walk_until_arrival(&mut game, hero, 2000);
    steps(&mut game, 5);
    release(&mut game);

    let before = center_of(&game, hero);
    click_ground(&mut game, before);
    assert_eq!(
        center_of(&game, hero),
        before,
        "щелчок под собой не сдвинул героя"
    );
    steps(&mut game, 120);
    assert_eq!(
        number(&game, goblin, "health"),
        Some(30.0),
        "бой закончился без единого удара"
    );
    assert_eq!(number(&game, hero, "health"), Some(100.0));
}

/// Требование 36: расстояние между прямоугольниками — по диагонали, а не наибольший зазор по осям.
#[test]
fn combat_distance_is_the_diagonal_not_the_larger_axis_gap() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let enemy = place_of(&game, goblin).aabb();
    game.world.set_vec2(
        hero,
        property::POSITION,
        [enemy.x - HERO_SIZE - 0.25, enemy.y + enemy.h + 0.25],
    );
    let target = game.properties.resolve("target").unwrap();
    game.world.set_flag(goblin, target, true);
    steps(&mut game, 30);
    assert_eq!(
        number(&game, goblin, "health"),
        Some(30.0),
        "по диагонали 0,25×0,25 — дальше порога 0,3"
    );
    assert_eq!(number(&game, hero, "health"), Some(100.0));
}

#[test]
fn combat_starts_within_the_threshold_on_a_single_axis() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let orc = find_named(&game, "orc_1");
    let enemy = place_of(&game, orc).aabb();
    game.world.set_vec2(
        hero,
        property::POSITION,
        [
            enemy.x + (enemy.w - HERO_SIZE) / 2.0,
            enemy.y + enemy.h + 0.25,
        ],
    );
    let target = game.properties.resolve("target").unwrap();
    game.world.set_flag(orc, target, true);
    steps(&mut game, 30);
    assert_eq!(
        number(&game, hero, "health"),
        Some(85.0),
        "зазор 0,25 по одной оси — враг уже ударил первым"
    );
}

/// «Локация»: поиск пути от входа в самый дальний угол лабиринта не дольше 5 мс в
/// `cargo test --release` — реальные препятствия сцены, включая повёрнутые стены.
#[test]
fn pathfinding_to_the_farthest_free_corner_is_fast_in_release() {
    let game = load();
    let (statics, _enemies) = obstacles_by_kind(&game);
    let hero = find_named(&game, "hero");
    let start = center_of(&game, hero);
    let scene = scene_size(&game);
    let visited = flood_fill(&statics, start, scene);
    let farthest = visited
        .iter()
        .map(|&(cx, cy)| [sample_coord(cx), sample_coord(cy)])
        .max_by(|a, b| dist(start, *a).total_cmp(&dist(start, *b)))
        .expect("хотя бы одна достижимая точка");

    let obstacles: Vec<(u32, Footprint)> = statics
        .iter()
        .enumerate()
        .map(|(i, place)| (i as u32, *place))
        .collect();
    let mut caches = WalkCaches::new();
    let started = Instant::now();
    pathfind::advance(
        hero,
        start,
        [HERO_SIZE, HERO_SIZE],
        None,
        farthest,
        obstacles,
        scene,
        0.0,
        &mut caches,
    );
    let elapsed = started.elapsed();
    if !cfg!(debug_assertions) {
        assert!(
            elapsed.as_micros() <= 5000,
            "поиск пути до {farthest:?} занял {elapsed:?}, дольше 5 мс"
        );
    }
}

/// `scene.json`'s real text with one enemy's `"enemy"` catalog reference corrupted — a typo, not
/// a missing field, so it still parses and loads; only `init_enemies`'s catalog lookup at the first
/// step fails. Built with an in-memory substitution (scene.json itself is never touched).
fn scene_with_broken_enemy_reference(object_name: &str) -> String {
    let scene = read("scene.json");
    let name_marker = format!("\"name\":\"{object_name}\"");
    let name_pos = scene
        .find(&name_marker)
        .unwrap_or_else(|| panic!("объекта \"{object_name}\" нет в scene.json"));
    let enemy_marker = "\"enemy\":\"";
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

/// Опечатка в `enemy` даёт понятную ошибку кода с именем врага и его строкой каталога.
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
        "и его строку каталога: {}",
        err.message
    );
    assert!(
        !err.message.contains("nil value"),
        "не сырая ошибка Lua: {}",
        err.message
    );
}

//! Ролевая игра `games/rpg` — деревня из фигур на рельефе («Фаза-15», «Фаза-17», «Трёхмерная сцена»,
//! «Рельеф»). Игра грузится из настоящих файлов, ходьба, поиск пути, щелчок лучом и бой идут
//! настоящими шагами движка.

use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use engine::core::footprint::Footprint;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::pathfind::WalkCaches;
use engine::core::property;
use engine::core::scene::{ground_footprint, pointer_hit, top_surface_height};
use engine::core::shapes::Body;
use engine::core::surface;
use engine::core::value::Shape;
use engine::core::walk3d::{self, Blocker, Deck, Goal, Surfaces, Walker};
use engine::data::load::{ImageVerdict, load_rest_with_terrain, read_entry};

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

/// Loads the real `games/rpg` folder end to end, image sizes, the `enemies` table and the terrain
/// file included. `scene_json` stands in for the real `scene.json` text — a deliberately corrupted
/// copy by `enemy_catalog_typo_names_the_enemy_and_its_bad_row`.
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
    let (game, _screens, warnings, _images) = load_rest_with_terrain(
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
        Some(&read("terrain.json")),
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
    ground_footprint(&game.world, id)
        .unwrap_or_else(|| panic!("у объекта {id} нет position и size"))
}

fn center_of(game: &Game, id: u32) -> [f64; 2] {
    place_of(game, id).center()
}

fn z_of(game: &Game, id: u32) -> f64 {
    game.world.base_z(id)
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2)).sqrt()
}

fn scene_size(game: &Game) -> (f64, f64) {
    (game.scene.width as f64, game.scene.height as f64)
}

fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{what}: {actual} вместо {expected}"
    );
}

const HERO_SIZE: f64 = 0.6;

/// Свободна ли земля под героем с серединой в `p` и на какой высоте он на ней стоит: прямоугольник
/// героя (с запасом на сотую) не задевает ни препятствие, ни крутое или подводное место рельефа, и
/// ни один настил не нависает над ним ниже его роста. `None` — герой здесь не помещается.
fn ground_fit(game: &Game, hero: u32, p: [f64; 2]) -> Option<f64> {
    const HALF: f64 = HERO_SIZE / 2.0 + 0.01;
    let scene = scene_size(game);
    if p[0] < HALF || p[1] < HALF || p[0] > scene.0 - HALF || p[1] > scene.1 - HALF {
        return None;
    }
    let world = &game.world;
    let rect = Footprint::flat([p[0] - HALF, p[1] - HALF], [2.0 * HALF, 2.0 * HALF]);
    let obstacle = game
        .properties
        .resolve("obstacle")
        .expect("obstacle объявлен");
    if world
        .ids()
        .any(|id| id != hero && world.flag(id, obstacle) && place_of(game, id).overlaps(&rect))
    {
        return None;
    }
    let corners = rect.corners();
    if world
        .terrain()
        .walk_blocked()
        .iter()
        .any(|piece| polygons_overlap(piece, &corners))
    {
        return None;
    }
    let z = world.terrain().min_under(&rect);
    let height = surface::body_height(world, hero);
    let covered = surface::decks(world)
        .any(|(id, place, _)| place.overlaps(&rect) && world.base_z(id) - z < height);
    (!covered).then_some(z)
}

/// Пересекаются ли внутренности двух выпуклых многоугольников (по теореме о разделяющей оси).
fn polygons_overlap(a: &[[f64; 2]], b: &[[f64; 2]]) -> bool {
    [a, b].into_iter().all(|polygon| {
        (0..polygon.len()).all(|i| {
            let (p, q) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            let axis = [-(q[1] - p[1]), q[0] - p[0]];
            let span = |points: &[[f64; 2]]| {
                points
                    .iter()
                    .map(|v| v[0] * axis[0] + v[1] * axis[1])
                    .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), d| {
                        (lo.min(d), hi.max(d))
                    })
            };
            let (a_span, b_span) = (span(a), span(b));
            a_span.1 > b_span.0 + 1e-9 && b_span.1 > a_span.0 + 1e-9
        })
    })
}

/// Кто идёт и откуда: середина, основание и сторона квадратного тела.
struct Stance {
    center: [f64; 2],
    z: f64,
    size: f64,
}

/// Ломаная героя от его середины до `target` по поверхностям — тем поиском, что ведёт правило `walk`;
/// `named_z` — высота цели, как у `walk_to` с третьим числом.
fn plan_to(
    game: &Game,
    hero: u32,
    caches: &mut WalkCaches,
    target: [f64; 2],
    named_z: Option<f64>,
) -> Vec<[f64; 2]> {
    let stance = Stance {
        center: center_of(game, hero),
        z: z_of(game, hero),
        size: HERO_SIZE,
    };
    plan_from(game, hero, caches, &stance, target, named_z)
}

/// Тот же поиск для любого тела и места старта — например, для тела шире героя, чтобы измерить проход.
fn plan_from(
    game: &Game,
    hero: u32,
    caches: &mut WalkCaches,
    stance: &Stance,
    target: [f64; 2],
    named_z: Option<f64>,
) -> Vec<[f64; 2]> {
    let world = &game.world;
    let obstacle = game
        .properties
        .resolve("obstacle")
        .expect("obstacle объявлен");
    let decks = surface::decks(world)
        .map(|(id, place, top)| Deck {
            id,
            place,
            bottom: world.base_z(id),
            top,
        })
        .collect();
    let blockers = world
        .ids()
        .filter(|&id| id != hero && world.flag(id, obstacle))
        .filter_map(|id| {
            Some(Blocker {
                id,
                place: ground_footprint(world, id)?,
                pillar: surface::pillar(world, id),
            })
        })
        .collect();
    let surfaces = Surfaces {
        terrain: world.terrain(),
        decks,
        blockers,
    };
    let walker = Walker {
        id: hero,
        center: stance.center,
        size: [stance.size, stance.size],
        rotation: None,
        height: surface::body_height(world, hero),
        z: stance.z,
    };
    let wanted_z = named_z.unwrap_or_else(|| top_surface_height(world, target));
    let goal = Goal {
        point: target,
        named_z,
        wanted_z,
    };
    walk3d::plan(&surfaces, &walker, &goal, scene_size(game), caches)
}

fn reaches(path: &[[f64; 2]], target: [f64; 2]) -> bool {
    path.last().is_some_and(|end| dist(*end, target) < 1e-6)
}

/// Убирает объекты с этими именами, как убрал бы их бой.
fn remove_named(game: &mut Game, names: &[&str]) {
    for name in names {
        let id = find_named(game, name);
        game.world.delete(id);
    }
}

/// Ставит героя серединой в `center`, как ставит объект вызов редактора `move_object`.
fn put_hero(game: &mut Game, hero: u32, center: [f64; 2]) {
    game.move_object(
        hero,
        [center[0] - HERO_SIZE / 2.0, center[1] - HERO_SIZE / 2.0],
        None,
    );
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

/// Требование 32 фазы 15: из `images` уходят все картинки, кроме травы и кольца отметки, и файлов в
/// папке тоже остаётся только два.
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

/// Требование 52: полос ручья и невидимых препятствий воды нет — воду и берега держит рельеф.
#[test]
fn the_stream_strips_and_the_invisible_water_are_gone() {
    let game = load();
    for id in game.world.ids() {
        let name = game.world.text(id, property::NAME).unwrap_or_default();
        assert!(
            !name.starts_with("stream_") && !name.starts_with("water_"),
            "{name} остался"
        );
    }
}

/// Требование 33 фазы 15: каждый кусок стены — одна повёрнутая коробка нужной высоты; никаких лесенок
/// и полосок картинок. Палисад `wall_palisade_20` хозяин правил сам — у него только коробка и высота.
#[test]
fn every_wall_is_one_box() {
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
        assert!(
            game.world
                .flag(id, game.properties.resolve("obstacle").unwrap()),
            "{name}"
        );
        if name == "wall_palisade_20" {
            continue;
        }
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
    }
    assert_eq!(walls, 8, "восемь кусков стен деревни");
}

/// Требования 33 фазы 15: фигуры деревни — что и на каких прямоугольниках.
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
    check("izba_17", Shape::Box, 2.5, [3.03, 2.0]);
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

/// Требование 53: двор кузницы закрыт, пока стоит хоть один враг, и открыт, когда все убиты — холм и
/// овраг обходом не стали.
#[test]
fn the_forge_yard_is_unreachable_while_any_enemy_stands() {
    let hero_start = |game: &Game| center_of(game, find_named(game, "hero"));
    for killed in 0..ENEMY_CHAIN.len() {
        let mut game = load();
        let hero = find_named(&game, "hero");
        remove_named(&mut game, &ENEMY_CHAIN[..killed]);
        assert_eq!(hero_start(&game), [2.5, 19.5]);
        let mut caches = WalkCaches::new();
        for point in FORGE_YARD_SAMPLES {
            let path = plan_to(&game, hero, &mut caches, point, None);
            assert!(
                !reaches(&path, point),
                "двор кузницы {point:?} достижим, хотя стоят враги {:?}",
                &ENEMY_CHAIN[killed..]
            );
        }
    }
    let mut game = load();
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    let mut caches = WalkCaches::new();
    for point in FORGE_YARD_SAMPLES {
        let path = plan_to(&game, hero, &mut caches, point, None);
        assert!(
            reaches(&path, point),
            "двор {point:?} достижим, когда врагов нет"
        );
    }
}

/// «Фаза 2.5», требование 16, требование 34: цель каждого прохода достижима из входа тогда и только
/// тогда, когда убиты все враги цепочки до неё включительно — настоящим поиском пути движка. Все 8
/// наборов убитых врагов.
#[test]
fn every_passage_opens_only_when_all_enemies_up_to_it_are_gone() {
    for mask in 0u8..(1 << ENEMY_CHAIN.len()) {
        let killed: [bool; 3] = std::array::from_fn(|i| mask & (1 << i) != 0);
        let mut game = load();
        let hero = find_named(&game, "hero");
        let gone: Vec<&str> = (0..3)
            .filter(|&i| killed[i])
            .map(|i| ENEMY_CHAIN[i])
            .collect();
        remove_named(&mut game, &gone);
        let mut caches = WalkCaches::new();
        for (passage, target) in BEHIND_ENEMY.iter().enumerate() {
            let path = plan_to(&game, hero, &mut caches, *target, None);
            let reached = reaches(&path, *target);
            let expected = killed[..=passage].iter().all(|&gone| gone);
            assert_eq!(
                reached, expected,
                "маска {mask:03b} (goblin_1={}, goblin_2={}, orc_1={}): цель за врагом {} ({target:?}) — достижимость {reached}, ожидалось {expected}",
                killed[0], killed[1], killed[2], ENEMY_CHAIN[passage]
            );
        }
    }
}

/// Требование 34 фазы 15: каждый враг цепочки один (остальные убраны) перекрывает свой проход целиком —
/// с одной стороны от него до другой не дойти, а без него дойти.
#[test]
fn each_enemy_alone_blocks_its_own_corridor() {
    for (i, name) in ENEMY_CHAIN.iter().enumerate() {
        let before = if i == 0 {
            [2.5, 19.5]
        } else {
            BEHIND_ENEMY[i - 1]
        };
        let after = BEHIND_ENEMY[i];
        for enemy_stands in [true, false] {
            let mut game = load();
            let hero = find_named(&game, "hero");
            let others: Vec<&str> = ENEMY_CHAIN
                .iter()
                .copied()
                .filter(|other| other != name || !enemy_stands)
                .collect();
            remove_named(&mut game, &others);
            put_hero(&mut game, hero, before);
            let mut caches = WalkCaches::new();
            let path = plan_to(&game, hero, &mut caches, after, None);
            assert_eq!(
                reaches(&path, after),
                !enemy_stands,
                "{name} {}: из {before:?} до {after:?}",
                if enemy_stands {
                    "стоит"
                } else {
                    "убран"
                }
            );
        }
    }
}

/// Щелчок по точке тела фигуры: луч от глаза камеры через точку на теле; в записи — точка поверхности
/// под курсором с высотой (рельеф, вода или верх настила) и место камеры.
fn click_body(game: &mut Game, id: u32, height_share: f64) {
    let camera = game.camera_3d(WINDOW).expect("сцена трёхмерная");
    let body = Body::of_object(&game.world, id).expect("у объекта есть фигура");
    let point = [
        body.center[0],
        body.center[1],
        body.base + body.height * height_share,
    ];
    let window = camera.project(point).expect("тело перед камерой");
    let hit = pointer_hit(&game.world, &game.scene, &camera, window).expect("луч ложится на землю");
    press(game, hit);
}

/// Щелчок по верхней поверхности в этом месте: верх настила, иначе рельеф.
fn click_ground(game: &mut Game, cell: [f64; 2]) {
    let z = top_surface_height(&game.world, cell);
    press(game, [cell[0], cell[1], z]);
}

fn press(game: &mut Game, point: [f64; 3]) {
    let camera = game.camera_3d(WINDOW).expect("сцена трёхмерная");
    game.set_cursor_point(point, Some(camera.eye));
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

/// Требования 36–37 фазы 15: щелчок по капсуле гоблина попадает лучом во врага, герой встаёт вплотную,
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

/// Требование 37 фазы 15: точка земли под курсором у капсулы лежит за ней, а герой всё равно идёт к
/// самому врагу — откуда бы ни пришёлся щелчок по телу.
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
    click_ground(&mut game, [16.0, 12.0]);
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
    remove_named(&mut game, &["goblin_1", "goblin_2"]);
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
    near(z_of(&game, hero), 0.0, "у входа на плато");
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

/// Требование 36 фазы 15: расстояние между прямоугольниками — по диагонали, а не наибольший зазор по
/// осям.
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

/// Самое дальнее от входа место, куда можно дойти, если врагов нет: по длине пути на сетке через
/// клетку.
fn farthest_place(game: &Game, hero: u32, caches: &mut WalkCaches) -> [f64; 2] {
    let mut best = ([0.0, 0.0], 0.0);
    for row in 0..24 {
        for column in 0..32 {
            let point = [0.5 + column as f64, 0.5 + row as f64];
            let Some(z) = ground_fit(game, hero, point) else {
                continue;
            };
            let path = plan_to(game, hero, caches, point, Some(z));
            if !reaches(&path, point) {
                continue;
            }
            let start = center_of(game, hero);
            let length: f64 = std::iter::once(start)
                .chain(path.iter().copied())
                .collect::<Vec<_>>()
                .windows(2)
                .map(|pair| dist(pair[0], pair[1]))
                .sum();
            if length > best.1 {
                best = (point, length);
            }
        }
    }
    best.0
}

/// Путь после гибели врага, найденный по полю, что подгоняли под новый набор препятствий, — тот же, что
/// находит поиск на свежем поле: история гибелей не влияет на путь.
#[test]
fn a_path_after_an_enemy_dies_is_the_path_a_search_from_scratch_finds() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let targets = [
        [19.3, 0.3],
        [16.0, 21.0],
        [26.0, 12.0],
        [10.0, 6.0],
        [30.0, 2.0],
    ];
    let mut warm = WalkCaches::new();
    for target in targets {
        plan_to(&game, hero, &mut warm, target, None);
    }
    for name in ENEMY_CHAIN {
        let enemy = find_named(&game, name);
        game.world.delete(enemy);
        for target in targets {
            let incremental = plan_to(&game, hero, &mut warm, target, None);
            let fresh = plan_to(&game, hero, &mut WalkCaches::new(), target, None);
            assert_eq!(incremental, fresh, "после гибели {name}, к {target:?}");
        }
    }
}

/// Один шаг с приказом идти в `target`: сколько он длился. Путь ищется в этом шаге — первый шаг после
/// приказа и после смены набора препятствий.
fn timed_order(game: &mut Game, hero: u32, target: [f64; 2]) -> Duration {
    game.world.set_walk_to(hero, target, None);
    let started = Instant::now();
    game.step(StepInput::empty());
    started.elapsed()
}

/// «Нефункциональные требования» фазы 17, требование 32 фазы 18: рельеф, настилы и видимость готовятся при
/// сборке мира, и любой отдельный поиск пути от входа до самого дальнего места, куда можно дойти, — первый
/// после сборки и первый после гибели каждого врага — не дольше 5 мс в `cargo test --release`. Сборка мира
/// меряется отдельно, вместе с загрузкой игры: не дольше 50 мс.
#[test]
fn pathfinding_is_ready_with_the_world_and_every_search_takes_at_most_five_milliseconds_in_release()
{
    let started = Instant::now();
    let mut game = load();
    let build = started.elapsed();
    let hero = find_named(&game, "hero");

    let mut free = load();
    let free_hero = find_named(&free, "hero");
    remove_named(&mut free, &ENEMY_CHAIN);
    let farthest = farthest_place(&free, free_hero, &mut WalkCaches::new());
    assert!(
        reaches(
            &plan_to(&free, free_hero, &mut WalkCaches::new(), farthest, None),
            farthest
        ),
        "самое дальнее место {farthest:?} достижимо"
    );

    let mut spans = vec![(
        "первый путь после сборки мира, до самого дальнего места".to_string(),
        timed_order(&mut game, hero, farthest),
    )];
    for (n, name) in ENEMY_CHAIN.iter().enumerate() {
        let enemy = find_named(&game, name);
        game.world.delete(enemy);
        let shift = 0.05 * (n + 1) as f64;
        spans.push((
            format!("первый путь после гибели {name}"),
            timed_order(&mut game, hero, [farthest[0] - shift, farthest[1]]),
        ));
    }

    println!("сборка мира и загрузка {build:?}");
    for (what, elapsed) in &spans {
        println!("{what}: {elapsed:?}");
    }
    if !cfg!(debug_assertions) {
        assert!(
            build.as_millis() <= 50,
            "сборка мира с загрузкой: {build:?}, дольше 50 мс"
        );
        for (what, elapsed) in &spans {
            assert!(
                elapsed.as_micros() <= 5000,
                "{what}: {elapsed:?}, дольше 5 мс"
            );
        }
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

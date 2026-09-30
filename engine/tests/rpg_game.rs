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
use engine::core::terrain::{MAX_SLOPE, Terrain};
use engine::core::value::Shape;
use engine::core::walk3d::{self, Blocker, Deck, Goal, Surfaces, Walker};
use engine::data::load::{ImageVerdict, load_rest_with_terrain, read_entry};
use serde_json::Value as Json;

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

/// Ручей течёт по прямой `y = x / 2 + 13`: `u` — вдоль него (к нижнему краю сцены), `d` — поперёк,
/// в сторону входа героя (юго-запад); обе оси в клетках.
fn from_stream(u: f64, d: f64) -> [f64; 2] {
    let root = 5.0_f64.sqrt();
    [(2.0 * u - d) / root, 13.0 + (u + 2.0 * d) / root]
}

fn to_stream(point: [f64; 2]) -> (f64, f64) {
    let root = 5.0_f64.sqrt();
    (
        (2.0 * point[0] + (point[1] - 13.0)) / root,
        (2.0 * (point[1] - 13.0) - point[0]) / root,
    )
}

/// Где мост поперёк ручья: середина его ширины вдоль ручья.
const BRIDGE_U: f64 = 11.0;
/// Берег под мостом: поперёк ручья `d` и высота земли.
const BANK_D: f64 = 1.9;
const BANK_HEIGHT: f64 = -2.1;

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

fn remove_stairs(game: &mut Game) {
    let stairs: Vec<u32> = game
        .world
        .ids()
        .filter(|&id| {
            game.world
                .text(id, property::NAME)
                .is_some_and(|name| name.starts_with("stair_"))
        })
        .collect();
    assert_eq!(stairs.len(), STAIR_STEPS, "ступеней лестницы");
    for id in stairs {
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

/// Верх холма: середина самых высоких точек рельефа и их высота.
fn summit(game: &Game) -> ([f64; 2], f64) {
    let terrain = game.world.terrain();
    let (_, top) = terrain.height_range();
    let (mut sum, mut count) = ([0.0, 0.0], 0.0);
    for row in 0..terrain.rows() {
        for column in 0..terrain.columns() {
            if terrain.point_height(column, row) >= top - 0.005 {
                sum = [sum[0] + column as f64 / 2.0, sum[1] + row as f64 / 2.0];
                count += 1.0;
            }
        }
    }
    ([sum[0] / count, sum[1] / count], top)
}

const STAIR_STEPS: usize = 10;
/// Подножие лестницы: на столько клеток по её оси назад от середины нижней ступени.
const FOOT_BACK: f64 = 1.3;

/// Ось холма вдоль стен деревни — те же 26,57°, что у ручья и стен: единичный вектор вдоль неё.
fn along_walls() -> [f64; 2] {
    let root = 5.0_f64.sqrt();
    [2.0 / root, 1.0 / root]
}

/// Подножие лестницы — место на земле перед нижней ступенью; считается, пока ступени стоят.
fn stair_foot(game: &Game) -> [f64; 2] {
    let first = place_of(game, find_named(game, "stair_1")).center();
    let axis = along_walls();
    [
        first[0] - FOOT_BACK * axis[0],
        first[1] - FOOT_BACK * axis[1],
    ]
}

/// Свободная земля сразу за проходом гоблина `goblin_1`, со стороны холма: отсюда начинается холмовая
/// часть сцены.
const HILL_GATE: [f64; 2] = [22.8, 9.4];
/// Поляна к северу от холма: через неё идёт по земле путь от `HILL_GATE` к подножию лестницы, не задевая
/// ни холм, ни ступени.
const GLADE: [f64; 2] = [17.0, 2.4];
/// Место у верхней кромки сцены, севернее нижних ступеней: отсюда к подножию лестницы ведёт ровная земля,
/// а ступени остаются в стороне.
const STAIR_SIDE: [f64; 2] = [9.6, 0.6];
/// Сторона тела шире героя вдвое, которым меряется проход к подножию.
const WIDE_BODY: f64 = 1.2;

/// Наклон, круче которого пологий склон не бывает — 35° как тангенс, каким движок считает наклон.
fn gentle_limit() -> f64 {
    35.0_f64.to_radians().tan()
}

/// Все треугольники рельефа, как их делит движок: вершины `[x, y, z]`.
fn triangles(terrain: &Terrain) -> Vec<[[f64; 3]; 3]> {
    let [columns, rows] = terrain.squares();
    (0..rows)
        .flat_map(|row| {
            (0..columns).flat_map(move |column| (0..2).map(move |which| (column, row, which)))
        })
        .map(|(column, row, which)| terrain.triangle(column, row, which))
        .collect()
}

/// Самая нижняя и самая высокая вершина треугольника.
fn z_range(triangle: &[[f64; 3]; 3]) -> (f64, f64) {
    triangle
        .iter()
        .map(|vertex| vertex[2])
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(low, high), z| {
            (low.min(z), high.max(z))
        })
}

/// Треугольник рельефа, в котором лежит точка сцены: квадрат сетки режется диагональю от левой верхней
/// вершины к правой нижней.
fn triangle_at(terrain: &Terrain, point: [f64; 2]) -> [[f64; 3]; 3] {
    let [columns, rows] = terrain.squares();
    let (x, y) = (point[0] * 2.0, point[1] * 2.0);
    let column = (x.floor() as usize).min(columns - 1);
    let row = (y.floor() as usize).min(rows - 1);
    let above_diagonal = x - column as f64 >= y - row as f64;
    terrain.triangle(column, row, usize::from(!above_diagonal))
}

fn centroid(triangle: &[[f64; 3]; 3]) -> [f64; 2] {
    [
        triangle.iter().map(|v| v[0]).sum::<f64>() / 3.0,
        triangle.iter().map(|v| v[1]).sum::<f64>() / 3.0,
    ]
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
    assert!(!game.world.terrain().is_trivial());
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

/// Требование 48: файл рельефа — 49 строк по 65 чисел (две высоты сцены плюс одна и две ширины плюс
/// одна), числа до сотых, вода на своём месте, лишних ключей нет.
#[test]
fn the_terrain_file_is_49_rows_of_65_numbers_in_hundredths_with_calm_water() {
    let terrain: Json = serde_json::from_str(&read("terrain.json")).expect("terrain.json — JSON");
    let keys: Vec<&String> = terrain.as_object().expect("объект").keys().collect();
    assert_eq!(keys.len(), 2, "только heights и water: {keys:?}");
    let rows = terrain["heights"].as_array().expect("heights");
    assert_eq!(rows.len(), 49);
    for (index, row) in rows.iter().enumerate() {
        let row = row.as_array().expect("строка heights");
        assert_eq!(row.len(), 65, "строка {index}");
        for value in row {
            let number = value.as_f64().expect("высота — число");
            assert!(
                ((number * 100.0).round() / 100.0 - number).abs() < 1e-9,
                "строка {index}: {number} — не сотые"
            );
        }
    }
    let water = &terrain["water"];
    let level = water["level"].as_f64().expect("water.level");
    assert!((-2.4..=-2.2).contains(&level), "уровень воды {level}");
    assert!(water["color"].as_str().is_some_and(|c| c.starts_with('#')));
    let game = load();
    assert_eq!(game.world.terrain().columns(), 65);
    assert_eq!(game.world.terrain().rows(), 49);
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

/// Требования 49–50: овраг с ручьём по прямой `y = x / 2 + 13`. Вода на −2,3; со стороны входа берег
/// шириной не меньше клетки на −2,1; от входа к берегу ведёт спуск не круче 35°, остальные стенки
/// круче 45°.
#[test]
fn the_ravine_holds_calm_water_a_bank_and_one_gentle_way_down() {
    let game = load();
    let terrain = game.world.terrain();
    let water = terrain.water().expect("в овраге вода");
    assert!((-2.4..=-2.2).contains(&water.level));

    let height_at = |u: f64, d: f64| {
        let p = from_stream(u, d);
        terrain.height_at(p[0], p[1])
    };
    let sweep = |from: f64, to: f64| {
        (0..=((to - from) / 0.05).round() as usize).map(move |i| from + 0.05 * i as f64)
    };

    // Берег: ровный кусок на −2,1 не уже клетки поперёк ручья.
    let bank: Vec<f64> = sweep(0.0, 4.0)
        .filter(|&d| (height_at(BRIDGE_U, d) - BANK_HEIGHT).abs() < 0.03)
        .collect();
    let width = bank.last().expect("берег есть") - bank.first().expect("берег есть");
    assert!(width >= 1.0, "берег шириной {width}");
    assert!(
        bank.iter().any(|&d| (d - BANK_D).abs() < 1e-6),
        "берег занимает d = {BANK_D}"
    );

    // Плато с обеих сторон, дно под водой.
    assert_eq!(height_at(BRIDGE_U, -4.0), 0.0);
    assert_eq!(height_at(BRIDGE_U, 5.0), 0.0);
    assert!(height_at(BRIDGE_U, 0.0) < water.level, "дно под водой");

    // Ниже по течению, за мостом и спуском, в овраге ходимо только ровное: ни одного склона между 0 и 45°
    // в куске, что не под водой, — стенки круче 45°.
    let [columns, rows] = terrain.squares();
    let mut checked = 0;
    for row in 0..rows {
        for column in 0..columns {
            for which in 0..2 {
                let triangle = terrain.triangle(column, row, which);
                let middle = [
                    triangle.iter().map(|v| v[0]).sum::<f64>() / 3.0,
                    triangle.iter().map(|v| v[1]).sum::<f64>() / 3.0,
                ];
                let (u, d) = to_stream(middle);
                let top = triangle
                    .iter()
                    .map(|v| v[2])
                    .fold(f64::NEG_INFINITY, f64::max);
                let bottom = triangle.iter().map(|v| v[2]).fold(f64::INFINITY, f64::min);
                if !(12.0..=16.0).contains(&u) || d.abs() > 4.0 {
                    continue;
                }
                if top <= water.level + 0.01 || bottom >= -0.01 {
                    continue;
                }
                checked += 1;
                let slope = Terrain::slope(&triangle);
                assert!(
                    slope < 1e-9 || slope > 1.0 + 1e-6,
                    "кусок земли в овраге положе 45° и не ровный: {slope} у {middle:?}"
                );
            }
        }
    }
    assert!(checked > 20, "стенок оврага проверено {checked}");

    // Спуск к берегу вдоль ручья: от плато на −2,1, круче 35° нигде.
    let ramp: Vec<f64> = sweep(2.0, 9.5).map(|u| height_at(u, BANK_D)).collect();
    assert!(ramp[0] > -0.05, "спуск начинается на плато: {}", ramp[0]);
    assert!((ramp[ramp.len() - 1] - BANK_HEIGHT).abs() < 0.03);
    let steepest = ramp
        .windows(2)
        .map(|pair| (pair[1] - pair[0]).abs() / 0.05)
        .fold(0.0, f64::max);
    assert!(steepest <= gentle_limit(), "спуск круче 35°: {steepest}");
    assert!(
        ramp.windows(2).all(|pair| pair[1] <= pair[0] + 1e-9),
        "спуск только вниз"
    );
}

/// Требование 49: спуск к берегу не круче 35° — на каждом треугольнике, по его нормали, как считает
/// наклон движок, а не по срезам вдоль осей. Спуск — вся проходимая земля ниже плато и выше воды.
#[test]
fn the_way_down_to_the_water_is_at_most_35_degrees_on_every_triangle() {
    let game = load();
    let terrain = game.world.terrain();
    let level = terrain.water().expect("в овраге вода").level;
    let (mut sloped, mut steepest) = (0, 0.0_f64);
    for triangle in triangles(terrain) {
        let (bottom, top) = z_range(&triangle);
        if bottom > -0.02 || top < level + 0.05 {
            continue;
        }
        let slope = Terrain::slope(&triangle);
        if slope > MAX_SLOPE + 1e-9 {
            continue;
        }
        if slope > 1e-9 {
            sloped += 1;
        }
        steepest = steepest.max(slope);
    }
    assert!(sloped > 20, "треугольников спуска: {sloped}");
    assert!(
        steepest <= gentle_limit(),
        "треугольник спуска круче 35°: тангенс {steepest}"
    );
}

/// Всюду, где можно пройти, земля не круче 35°: каждый треугольник рельефа либо не круче 35°, либо
/// стена круче 45° (с запасом в полградуса над проходимым пределом движка).
#[test]
fn every_triangle_of_the_ground_is_either_gentle_or_a_wall() {
    let game = load();
    let wall = 45.5_f64.to_radians().tan();
    for triangle in triangles(game.world.terrain()) {
        let slope = Terrain::slope(&triangle);
        let middle = centroid(&triangle);
        assert!(
            slope <= gentle_limit() || slope >= wall,
            "проходимый треугольник круче 35°: тангенс {slope} у {middle:?}"
        );
    }
}

/// Требование 50: мост — настил-коробка поперёк оврага, без `z` в данных, стоит на плато, верх выше
/// плато с обеих сторон не больше чем на 0,4, а над берегом до низа моста не меньше 1,9.
#[test]
fn the_bridge_is_a_wooden_deck_across_the_ravine_with_room_to_walk_beneath() {
    let game = load();
    let world = &game.world;
    let bridge = find_named(&game, "bridge");
    assert!(world.flag(bridge, property::DECK));
    assert_eq!(world.shape(bridge, property::SHAPE), Some(Shape::Box));
    let height = world.number_like(bridge, property::HEIGHT).expect("height");
    assert!((0.2..=0.4).contains(&height), "высота моста {height}");
    let scene: Json = serde_json::from_str(&read("scene.json")).expect("scene.json — JSON");
    let entry = scene["objects"]
        .as_array()
        .expect("objects")
        .iter()
        .find(|o| o["name"] == "bridge")
        .expect("мост в данных");
    assert_eq!(
        entry["position"].as_array().map(Vec::len),
        Some(2),
        "мост без z"
    );

    let place = place_of(&game, bridge);
    let terrain = world.terrain();
    near(
        z_of(&game, bridge),
        terrain.max_under(&place),
        "низ моста — на самой высокой точке",
    );
    let top = surface::deck_top(world, bridge);
    for d in [-2.4, 4.3] {
        let p = from_stream(BRIDGE_U, d);
        assert!(place.contains(p), "концы моста лежат на плато: d = {d}");
        let rise = top - terrain.height_at(p[0], p[1]);
        assert!(
            rise <= 0.4 + 1e-9,
            "верх моста над плато на {rise}, d = {d}"
        );
    }
    let angle = world
        .rotation(bridge, property::ROTATION)
        .expect("rotation")
        .angle();
    let turn = (angle - 26.5651).rem_euclid(180.0);
    assert!((turn - 90.0).abs() < 1e-3, "мост к ручью под {turn}°");
    let mut under = 0;
    for step in 0..=20 {
        let (u, d) = (
            BRIDGE_U - 0.9 + 0.09 * step as f64,
            1.2 + 0.06 * step as f64,
        );
        let p = from_stream(u, d);
        if (terrain.height_at(p[0], p[1]) - BANK_HEIGHT).abs() < 0.03 {
            under += 1;
            let clearance = z_of(&game, bridge) - BANK_HEIGHT;
            assert!(clearance >= 1.9, "просвет над берегом {clearance}");
        }
    }
    assert!(under > 5, "под мостом есть берег");
}

/// Требование 52: тропа и дальняя тропа лежат по рельефу — без склонов и воды под собой; тропа идёт
/// от входа героя к мосту, дальняя — с моста к проходу гоблина.
#[test]
fn the_trails_lie_on_level_ground_and_join_the_entrance_the_bridge_and_the_first_passage() {
    let game = load();
    let deck = place_of(&game, find_named(&game, "bridge"));
    let entrance = center_of(&game, find_named(&game, "hero"));
    let passage = center_of(&game, find_named(&game, "goblin_1"));
    let end = |id: u32, sign: f64| {
        let half = game.world.vec2(id, property::SIZE).expect("size")[0] / 2.0;
        let (sin, cos) = game
            .world
            .rotation(id, property::ROTATION)
            .expect("rotation")
            .sin_cos();
        let center = center_of(&game, id);
        [center[0] + sign * half * cos, center[1] + sign * half * sin]
    };
    let trail = find_named(&game, "trail");
    let trail_far = find_named(&game, "trail_far");
    for id in [trail, trail_far] {
        assert!(game.world.color(id, property::COLOR).is_some());
        assert_eq!(
            game.world.shape(id, property::SHAPE),
            None,
            "плоская полоса"
        );
        let (low, high) = game.world.terrain().range_under(&place_of(&game, id));
        assert!(
            low > -0.01 && high - low < 0.05,
            "тропа на склоне: {low}..{high}"
        );
        near(
            z_of(&game, id),
            low,
            "тропа стоит на рельефе, а не на мосту",
        );
        assert!(
            matches!(
                surface::lies_on(&game.world, id),
                Some(surface::Lies::Terrain { .. })
            ),
            "тропа лежит не по рельефу"
        );
    }
    assert!(dist(end(trail, -1.0), entrance) < 0.1);
    assert!(deck.contains(end(trail, 1.0)), "тропа не дошла до моста");
    assert!(
        deck.contains(end(trail_far, -1.0)),
        "дальняя тропа не начинается на мосту"
    );
    assert!(dist(end(trail_far, 1.0), passage) < 0.5);
    let marker = find_named(&game, "marker");
    assert!(game.world.has(marker, property::IMAGE));
}

/// Требование 52: деревья, камни и кусты стоят на ровном месте выше воды — не на обрыве, не в воде и
/// не на стенке оврага; постройки, стены и враги остались там, где были.
#[test]
fn trees_rocks_and_bushes_stand_on_level_ground_and_the_buildings_stay_put() {
    let game = load();
    let terrain = game.world.terrain();
    let mut planted = 0;
    for id in game.world.ids() {
        let name = game.world.text(id, property::NAME).unwrap_or_default();
        if !["spruce_", "birch_", "boulder_", "bush_"]
            .iter()
            .any(|kind| name.starts_with(kind))
        {
            continue;
        }
        planted += 1;
        let (low, high) = terrain.range_under(&place_of(&game, id));
        assert!(
            low > -0.01 && high - low < 0.1,
            "{name} на склоне: {low}..{high}"
        );
    }
    assert_eq!(planted, 39, "деревья, камни и кусты деревни");
    let obstacle = game
        .properties
        .resolve("obstacle")
        .expect("obstacle объявлен");
    for id in game.world.ids().filter(|&id| game.world.flag(id, obstacle)) {
        let name = game.world.text(id, property::NAME).unwrap_or_default();
        let (low, high) = terrain.range_under(&place_of(&game, id));
        assert!(
            low > -0.01 && high - low < 0.05,
            "{name} стоит не на ровном месте: {low}..{high}"
        );
    }
    for (name, position) in [
        ("chertog_18", [20.0, 16.0]),
        ("smithy_19", [27.0, 1.0]),
        ("wall_stone_5", [21.3782, 12.3842]),
        ("wall_log_6", [10.4098, 7.25]),
        ("wall_palisade_7", [3.5869, 2.9643]),
        ("wall_stone_14", [20.4098, 4.25]),
        ("post_9", [3.0, 0.5]),
    ] {
        let id = find_named(&game, name);
        assert_eq!(
            game.world.vec2(id, property::POSITION),
            Some(position),
            "{name}"
        );
    }
}

/// Требование 52: правки хозяина в `scene.json` сохранены как есть.
#[test]
fn the_owners_edits_of_the_izba_and_the_palisade_stay_as_written() {
    let game = load();
    let izba = find_named(&game, "izba_17");
    assert_eq!(
        game.world.vec2(izba, property::POSITION),
        Some([5.985, 12.0])
    );
    assert_eq!(game.world.vec2(izba, property::SIZE), Some([3.03, 2.0]));
    let palisade = find_named(&game, "wall_palisade_20");
    assert_eq!(
        game.world.vec2(palisade, property::POSITION),
        Some([5.6151, 7.725])
    );
    assert_eq!(
        game.world.vec2(palisade, property::SIZE),
        Some([4.64, 0.41])
    );
    near(
        game.world
            .rotation(palisade, property::ROTATION)
            .expect("rotation")
            .angle(),
        171.0,
        "поворот палисада",
    );
    assert_eq!(
        game.world.number_like(palisade, property::HEIGHT),
        Some(1.85)
    );
    let text = read("scene.json");
    assert!(text.contains(
        r##"{"name":"izba_17","position":[5.985, 12],"size":[3.03, 2],"shape":"box","height":2.5,"color":"#a8713e","obstacle":true}"##
    ));
    assert!(text.contains(
        r##"{"name":"wall_palisade_20","position":[5.6151, 7.725],"size":[4.64, 0.41],"rotation":171,"shape":"box","height":1.85,"color":"#a06d3b","obstacle":true}"##
    ));
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

/// Все шаги хода героя до прихода или до `max_steps`: середина и основание после каждого шага.
fn walk_track(game: &mut Game, hero: u32, max_steps: u32) -> Vec<([f64; 2], f64)> {
    let mut track = Vec::new();
    for _ in 0..max_steps {
        if game.world.vec2(hero, property::WALK_TO).is_none() {
            break;
        }
        game.step(StepInput::empty());
        track.push((center_of(game, hero), z_of(game, hero)));
    }
    track
}

/// Требования 50–52: от входа за мост — герой идёт по мосту, стоит на его верху, за мостом сходит на
/// плато; в овраг и воду не спускается.
#[test]
fn the_hero_crosses_the_ravine_by_the_bridge_and_never_leaves_its_deck_sideways() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let bridge = find_named(&game, "bridge");
    let deck = place_of(&game, bridge);
    let top = surface::deck_top(&game.world, bridge);
    let target = from_stream(12.0, -4.5);
    game.world.set_walk_to(hero, target, None);
    let track = walk_track(&mut game, hero, 4000);
    let (end, end_z) = *track.last().expect("герой шёл");
    assert!(game.world.vec2(hero, property::WALK_TO).is_none(), "дошёл");
    assert!(dist(end, target) < 1e-6, "{end:?}");
    near(end_z, 0.0, "за мостом герой на плато");
    let mut on_the_bridge = 0;
    for (center, z) in &track {
        assert!(*z > -0.01, "герой не спускается в овраг: z = {z}");
        let (u, d) = to_stream(*center);
        if (-1.2..=1.2).contains(&d) {
            on_the_bridge += 1;
            near(*z, top, "над ручьём герой на верху моста");
            assert!(
                deck.contains(*center),
                "над ручьём герой в пределах моста: {center:?}"
            );
            assert!((BRIDGE_U - 0.95..=BRIDGE_U + 0.95).contains(&u), "u = {u}");
        }
    }
    assert!(on_the_bridge > 10, "герой прошёл по мосту: {on_the_bridge}");
}

/// Требования 50, 49: от входа к берегу по другую сторону моста герой спускается по пологому спуску,
/// проходит под мостом по берегу на его высоте и в воду не заходит.
#[test]
fn the_hero_walks_down_the_ramp_and_along_the_bank_under_the_bridge() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let target = from_stream(BRIDGE_U + 3.0, BANK_D);
    game.world.set_walk_to(hero, target, Some(BANK_HEIGHT));
    let track = walk_track(&mut game, hero, 4000);
    assert!(game.world.vec2(hero, property::WALK_TO).is_none(), "дошёл");
    let (end, end_z) = *track.last().expect("герой шёл");
    assert!(dist(end, target) < 1e-6, "{end:?}");
    near(end_z, BANK_HEIGHT, "на берегу");
    let mut lowest = 0.0_f64;
    let mut passed_under = false;
    let mut went_down = false;
    for (center, z) in &track {
        lowest = lowest.min(*z);
        let (u, _) = to_stream(*center);
        if *z < -1.0 {
            went_down = true;
        }
        if (BRIDGE_U - 0.9..=BRIDGE_U + 0.9).contains(&u) {
            assert!(
                !went_down || *z < -2.0,
                "под мостом герой на берегу: z = {z}, u = {u}"
            );
            passed_under |= *z < -2.0;
        }
    }
    assert!(passed_under, "герой прошёл под мостом");
    assert!(
        lowest >= BANK_HEIGHT - 0.05,
        "герой не зашёл в воду: {lowest}"
    );
}

/// Требование 20: щелчок по мосту без высоты ведёт на мост, а по воде — к кромке воды.
#[test]
fn a_click_on_the_bridge_leads_onto_it_and_a_click_on_the_water_ends_at_its_edge() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let bridge = find_named(&game, "bridge");
    let on_the_deck = from_stream(BRIDGE_U, BANK_D);
    game.world.set_walk_to(hero, on_the_deck, None);
    walk_track(&mut game, hero, 4000);
    near(
        z_of(&game, hero),
        surface::deck_top(&game.world, bridge),
        "на мосту",
    );

    let mut game = load();
    let water = from_stream(5.0, -0.2);
    game.world.set_walk_to(hero, water, None);
    let track = walk_track(&mut game, hero, 4000);
    let (end, end_z) = *track.last().expect("герой шёл");
    assert!(end_z > -2.15, "герой не в воде: {end_z}");
    let (_, d) = to_stream(end);
    assert!(
        d > 0.3,
        "герой встал у кромки воды со своей стороны: d = {d}"
    );
    assert!(track.iter().all(|(_, z)| *z > -2.15));
}

/// Требование 49: вброд не пройти — без моста от входа на другую сторону ручья не попасть.
#[test]
fn without_the_bridge_there_is_no_way_across_the_water() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    let bridge = find_named(&game, "bridge");
    game.world.delete(bridge);
    let target = from_stream(12.0, -4.5);
    game.world.set_walk_to(hero, target, None);
    let track = walk_track(&mut game, hero, 3000);
    let (end, end_z) = *track.last().expect("герой шёл");
    let (_, d) = to_stream(end);
    assert!(d > 0.3, "герой остался по свою сторону ручья: d = {d}");
    assert!(end_z >= BANK_HEIGHT - 0.05, "и не в воде: {end_z}");
    assert!(
        dist(end, target) > 3.0,
        "цель по ту сторону недостижима, герой у берега: {end:?}"
    );
}

/// Середина героя у моста или на нём: там основание меняется ступенькой, а не склоном.
fn bridge_edge(bridge: &Footprint, center: [f64; 2]) -> bool {
    let half = HERO_SIZE / 2.0 + 0.2;
    bridge.overlaps(&Footprint::flat(
        [center[0] - half, center[1] - half],
        [2.0 * half, 2.0 * half],
    ))
}

/// Требование 51: на вершину холма герой поднимается по склону — без лестницы, пешком по пологой
/// стороне; лестница не нужна, чтобы подняться.
#[test]
fn the_hero_climbs_to_the_top_of_the_hill_by_the_gentle_slope() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    remove_stairs(&mut game);
    let (top, height) = summit(&game);
    game.world.set_walk_to(hero, top, None);
    let track = walk_track(&mut game, hero, 6000);
    assert!(game.world.vec2(hero, property::WALK_TO).is_none(), "дошёл");
    let (end, end_z) = *track.last().expect("герой шёл");
    assert!(dist(end, top) < 1e-6, "{end:?}");
    near(end_z, height, "на вершине холма");
    let bridge = place_of(&game, find_named(&game, "bridge"));
    let steepest_step = track
        .windows(2)
        .filter(|pair| !pair.iter().any(|(center, _)| bridge_edge(&bridge, *center)))
        .map(|pair| (pair[1].1 - pair[0].1).abs() / dist(pair[0].0, pair[1].0).max(1e-9))
        .fold(0.0, f64::max);
    assert!(
        steepest_step <= 1.0 + 1e-9,
        "герой шёл по склону положе 45°: {steepest_step}"
    );
}

/// Требование 51: обрыв круче 60°, склон с другой стороны — не круче 35°; вдоль оси холма от середины
/// вершины в обе стороны. По обрыву не пройти: на середине его высоты герою негде встать.
#[test]
fn the_hill_has_a_gentle_slope_on_one_side_and_a_cliff_on_the_other() {
    let game = load();
    let hero = find_named(&game, "hero");
    let terrain = game.world.terrain();
    let (top, height) = summit(&game);
    assert!((3.0..=4.0).contains(&height), "высота холма {height}");
    let axis = along_walls();
    // Вдоль оси: от вершины до её края (высота ниже вершины больше, чем на 0,02) и до подножия
    // (ниже 0,05); горизонталь между краем и подножием.
    let profile = |sign: f64| -> Vec<f64> {
        (0..400)
            .map(|i| {
                let t = 0.05 * i as f64;
                terrain.height_at(top[0] + sign * axis[0] * t, top[1] + sign * axis[1] * t)
            })
            .collect()
    };
    for (sign, name) in [(1.0, "склон"), (-1.0, "обрыв")] {
        let heights = profile(sign);
        let edge = heights
            .iter()
            .position(|&h| h < height - 0.02)
            .expect("край вершины");
        let foot = heights.iter().position(|&h| h < 0.05).expect("подножие");
        let run = (foot - edge) as f64 * 0.05;
        let angle = (height / run).atan().to_degrees();
        if sign > 0.0 {
            assert!(angle <= 35.0, "{name} круче 35°: {angle}° на {run} клетках");
        } else {
            assert!(angle > 60.0, "{name} положе 60°: {angle}° на {run} клетках");
            for level in [1.0, 2.0] {
                let at = heights
                    .iter()
                    .position(|&h| h < level)
                    .expect("обрыв доходит до земли");
                let t = 0.05 * at as f64;
                let point = [top[0] - axis[0] * t, top[1] - axis[1] * t];
                assert_eq!(
                    ground_fit(&game, hero, point),
                    None,
                    "на обрыве, на высоте {level}, герой не встаёт: {point:?}"
                );
            }
        }
    }
}

/// Требование 51: пологий склон холма не круче 35° — на каждом треугольнике, по его нормали, а не по
/// срезу вдоль оси. Лестницы нет, единственный подъём — склон: по каким треугольникам, задетым телом
/// героя, он поднимается с земли на вершину, ни один не круче 35°.
#[test]
fn every_triangle_the_hero_climbs_the_hill_by_is_at_most_35_degrees() {
    let mut game = load();
    let (top, height) = summit(&game);
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    remove_stairs(&mut game);
    put_hero(&mut game, hero, HILL_GATE);
    let track = walk_to_and_track(&mut game, hero, top);
    near(track.last().expect("герой шёл").1, height, "на вершине");
    let terrain = game.world.terrain();
    let half = HERO_SIZE / 2.0;
    let mut climbed = Vec::new();
    for (center, _) in &track {
        for (dx, dy) in [
            (0.0, 0.0),
            (-half, -half),
            (half, -half),
            (-half, half),
            (half, half),
        ] {
            let triangle = triangle_at(terrain, [center[0] + dx, center[1] + dy]);
            let slope = Terrain::slope(&triangle);
            assert!(
                slope <= gentle_limit(),
                "герой встал на треугольник круче 35° у {center:?}: тангенс {slope}"
            );
            if slope > 1e-9 && !climbed.contains(&triangle) {
                climbed.push(triangle);
            }
        }
    }
    assert!(
        climbed.len() > 20,
        "треугольников склона: {}",
        climbed.len()
    );
}

fn stair_ids(game: &Game) -> Vec<u32> {
    (1..=STAIR_STEPS)
        .map(|n| find_named(game, &format!("stair_{n}")))
        .collect()
}

/// Требование 51: лестница — ряд настилов-коробок с `z` в данных, каждая ступень выше предыдущей на
/// 0,3 и не уже 1,2; верх нижней — не выше рельефа у её края больше чем на 0,3, верх верхней —
/// вровень с вершиной обрыва.
#[test]
fn the_stair_is_a_row_of_decks_each_three_tenths_above_the_last() {
    let game = load();
    let world = &game.world;
    let steps = stair_ids(&game);
    let (_, height) = summit(&game);
    let scene: Json = serde_json::from_str(&read("scene.json")).expect("scene.json — JSON");
    for (index, &id) in steps.iter().enumerate() {
        let name = format!("stair_{}", index + 1);
        assert!(world.flag(id, property::DECK), "{name}");
        assert_eq!(world.shape(id, property::SHAPE), Some(Shape::Box), "{name}");
        let size = world.vec2(id, property::SIZE).expect("size");
        assert!(size[1] >= 1.2, "{name}: ширина {}", size[1]);
        let entry = scene["objects"]
            .as_array()
            .expect("objects")
            .iter()
            .find(|o| o["name"] == name.as_str())
            .expect("ступень в данных");
        assert_eq!(
            entry["position"].as_array().map(Vec::len),
            Some(3),
            "{name}: z в данных"
        );
        near(
            surface::deck_top(world, id),
            0.3 * (index + 1) as f64,
            &format!("верх ступени {name}"),
        );
    }
    let lowest = place_of(&game, steps[0]);
    let ground = world.terrain().max_under(&lowest);
    assert!(surface::deck_top(world, steps[0]) - ground <= 0.3 + 1e-9);
    assert!((surface::deck_top(world, steps[STAIR_STEPS - 1]) - height).abs() < 0.05);
}

/// Основания героя по порядку, без повторов подряд: так видно, на каких высотах он стоял.
fn distinct_levels(track: &[([f64; 2], f64)]) -> Vec<f64> {
    let mut levels: Vec<f64> = Vec::new();
    for (_, z) in track {
        if levels.last().is_none_or(|last| (z - last).abs() > 1e-9) {
            levels.push(*z);
        }
    }
    levels
}

/// Герой идёт к `target` настоящими шагами движка и приходит точно в него; след — середина и основание после
/// каждого шага.
fn walk_to_and_track(game: &mut Game, hero: u32, target: [f64; 2]) -> Vec<([f64; 2], f64)> {
    game.world.set_walk_to(hero, target, None);
    let track = walk_track(game, hero, 6000);
    assert!(game.world.vec2(hero, property::WALK_TO).is_none(), "дошёл");
    assert!(
        dist(center_of(game, hero), target) < 1e-6,
        "герой у цели {target:?}"
    );
    track
}

/// Требование 51: на обрыв ведёт лестница, и к её подножию подходят снизу, по земле. Врагов нет, герой
/// стоит на свободной земле сразу за проходом гоблина; через поляну севернее холма и место у верхней
/// кромки сцены он идёт к подножию — не поднимаясь ни на холм, ни на ступени, — а дальше по лестнице
/// поднимается на верх обрыва: основание растёт ступенями по 0,3.
#[test]
fn the_hero_comes_to_the_foot_of_the_stair_on_the_ground_and_climbs_it_by_steps_of_three_tenths() {
    let mut game = load();
    let (top, height) = summit(&game);
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    let foot = stair_foot(&game);
    for place in [HILL_GATE, GLADE, STAIR_SIDE, foot] {
        assert_eq!(
            ground_fit(&game, hero, place),
            Some(0.0),
            "на ровной земле {place:?} герой встаёт"
        );
    }
    put_hero(&mut game, hero, HILL_GATE);

    let mut approach = Vec::new();
    for waypoint in [GLADE, STAIR_SIDE, foot] {
        approach.extend(walk_to_and_track(&mut game, hero, waypoint));
    }
    let highest = approach.iter().map(|(_, z)| *z).fold(0.0, f64::max);
    assert!(
        highest < 0.1,
        "по пути к подножию герой не поднимался на холм и на ступени: {highest}"
    );
    near(
        approach.last().expect("герой шёл").1,
        0.0,
        "у подножия лестницы",
    );

    let climb = walk_to_and_track(&mut game, hero, top);
    near(climb.last().expect("герой шёл").1, height, "на вершине");
    let levels = distinct_levels(&climb);
    let expected: Vec<f64> = (0..=STAIR_STEPS).map(|n| 0.3 * n as f64).collect();
    assert_eq!(
        levels.len(),
        expected.len(),
        "основание по ступеням: {levels:?}"
    );
    for (level, expected) in levels.iter().zip(&expected) {
        near(*level, *expected, "ступень");
    }
}

/// Требование 51: проход к подножию лестницы не уже 1,2 клетки. Тело в 1,2 клетки идёт от места за
/// проходом гоблина к подножию, а от верхней кромки сцены до ближайшего угла любой ступени — не меньше
/// 1,2: между кромкой и лестницей есть где пройти.
#[test]
fn the_passage_to_the_foot_of_the_stair_is_at_least_1_2_wide() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    let foot = stair_foot(&game);
    let wide = Stance {
        center: HILL_GATE,
        z: 0.0,
        size: WIDE_BODY,
    };
    let path = plan_from(&game, hero, &mut WalkCaches::new(), &wide, foot, None);
    assert!(
        reaches(&path, foot),
        "тело в {WIDE_BODY} клетки не доходит до подножия {foot:?}"
    );
    for (index, id) in stair_ids(&game).into_iter().enumerate() {
        let nearest = place_of(&game, id)
            .corners()
            .iter()
            .map(|corner| corner[1])
            .fold(f64::INFINITY, f64::min);
        assert!(
            nearest >= WIDE_BODY,
            "ступень {} в {nearest} от верхней кромки сцены",
            index + 1
        );
    }
}

/// Герой стоит у подножия и идёт на вершину холма; `with_stairs = false` — лестница убрана.
fn climb_from_the_foot(with_stairs: bool) -> Vec<([f64; 2], f64)> {
    let mut game = load();
    let (top, height) = summit(&game);
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    let foot = stair_foot(&game);
    if !with_stairs {
        remove_stairs(&mut game);
    }
    put_hero(&mut game, hero, foot);
    let track = walk_to_and_track(&mut game, hero, top);
    near(track.last().expect("герой шёл").1, height, "на вершине");
    track
}

/// Требование 51: прямо по обрыву пути нет. Без лестницы из подножия к вершине герой идёт в обход, по
/// пологому склону, — заметно дольше, чем по лестнице, — и нигде не круче 45°.
#[test]
fn without_the_stair_there_is_no_way_up_the_cliff_only_the_long_way_round() {
    let by_stairs = climb_from_the_foot(true);
    let around = climb_from_the_foot(false);
    assert!(
        around.len() * 2 > 3 * by_stairs.len(),
        "без лестницы в обход: {} шагов против {}",
        around.len(),
        by_stairs.len()
    );
    let steepest = around
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1).abs() / dist(pair[0].0, pair[1].0).max(1e-9))
        .fold(0.0, f64::max);
    assert!(
        steepest <= 1.0 + 1e-9,
        "без лестницы герой шёл по круче 45°: {steepest}"
    );
}

/// `scene.json` ролевой игры с лестницей, сдвинутой целиком на `shift` клеток.
fn scene_with_the_stair_moved(shift: [f64; 2]) -> String {
    let mut scene: Json = serde_json::from_str(&read("scene.json")).expect("scene.json — JSON");
    let mut moved = 0;
    for object in scene["objects"].as_array_mut().expect("objects") {
        if !object["name"]
            .as_str()
            .is_some_and(|name| name.starts_with("stair_"))
        {
            continue;
        }
        let position = object["position"].as_array_mut().expect("position");
        for axis in 0..2 {
            position[axis] = Json::from(position[axis].as_f64().expect("число") + shift[axis]);
        }
        moved += 1;
    }
    assert_eq!(moved, STAIR_STEPS, "ступеней лестницы");
    scene.to_string()
}

fn length_of(from: [f64; 2], points: &[[f64; 2]]) -> f64 {
    let mut previous = from;
    points
        .iter()
        .map(|&point| {
            let length = dist(previous, point);
            previous = point;
            length
        })
        .sum()
}

/// Ошибка ходьбы по лестнице: путь по ступеням находился, только когда лестница стояла ровно там, где
/// её подобрали, а сдвинутая вбок на пару десятых уходил в обход вдвое длиннее. Лестница целиком
/// сдвинута вбок (поперёк её оси и по осям сцены), и путь из подножия на вершину холма всё равно идёт по
/// ней: не длиннее ломаной через середины ступеней больше чем на 2 %, а сам герой поднимается ступенями
/// по 0,3.
#[test]
fn the_stair_moved_aside_is_still_climbed_by_the_way_through_the_middles_of_its_steps() {
    let root = 5.0_f64.sqrt();
    let across = [-1.0 / root, 2.0 / root];
    let shifts = [
        ("на месте", [0.0, 0.0]),
        ("поперёк на 0,2", [0.2 * across[0], 0.2 * across[1]]),
        ("поперёк на -0,2", [-0.2 * across[0], -0.2 * across[1]]),
        ("поперёк на 0,5", [0.5 * across[0], 0.5 * across[1]]),
        ("по x на 0,2", [0.2, 0.0]),
        ("по x на -0,2", [-0.2, 0.0]),
        ("по y на 0,2", [0.0, 0.2]),
        ("по y на -0,2", [0.0, -0.2]),
    ];
    for (what, shift) in shifts {
        let mut game = load_with_scene(&scene_with_the_stair_moved(shift));
        let (top, height) = summit(&game);
        let hero = find_named(&game, "hero");
        remove_named(&mut game, &ENEMY_CHAIN);
        let foot = stair_foot(&game);
        put_hero(&mut game, hero, foot);
        let middles: Vec<[f64; 2]> = stair_ids(&game)
            .into_iter()
            .map(|id| center_of(&game, id))
            .chain([top])
            .collect();
        let path = plan_to(&game, hero, &mut WalkCaches::new(), top, None);
        assert!(reaches(&path, top), "{what}: путь на вершину не дошёл");
        let (found, through_middles) = (length_of(foot, &path), length_of(foot, &middles));
        assert!(
            found <= through_middles * 1.02,
            "{what}: путь {found:.3} длиннее ломаной через середины ступеней {through_middles:.3}"
        );

        let climb = walk_to_and_track(&mut game, hero, top);
        near(climb.last().expect("герой шёл").1, height, what);
        let levels = distinct_levels(&climb);
        assert_eq!(levels.len(), STAIR_STEPS + 1, "{what}: {levels:?}");
        for (n, level) in levels.iter().enumerate() {
            near(*level, 0.3 * n as f64, what);
        }
    }
}

/// Требование 53: без врагов из входа можно дойти в каждое место, где помещается герой, — по земле;
/// поиск пути движка сверяется с независимой прикидкой: прямоугольник героя не задевает ни
/// препятствия, ни крутые и подводные куски рельефа.
#[test]
fn every_walkable_place_is_reachable_from_the_entrance_without_any_enemy() {
    let mut game = load();
    let hero = find_named(&game, "hero");
    remove_named(&mut game, &ENEMY_CHAIN);
    let mut caches = WalkCaches::new();
    let (mut fits, mut unreachable) = (0, Vec::new());
    for row in 0..48 {
        for column in 0..64 {
            let point = [0.3 + column as f64 * 0.5, 0.3 + row as f64 * 0.5];
            let Some(z) = ground_fit(&game, hero, point) else {
                continue;
            };
            fits += 1;
            let path = plan_to(&game, hero, &mut caches, point, Some(z));
            if !reaches(&path, point) {
                unreachable.push(point);
            }
        }
    }
    assert!(fits > 1000, "проверка не пуста: {fits}");
    assert_eq!(
        unreachable,
        Vec::<[f64; 2]>::new(),
        "из {fits} мест, где помещается герой, до этих не дойти"
    );
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

/// Требование 28: враг за холмом щелчком не выбрать — рельеф заслоняет его. Гоблин переставлен за
/// вершину, если смотреть с камеры игры: луч от камеры через его тело сперва встречает склон.
#[test]
fn a_hill_hides_an_enemy_behind_it_from_a_click() {
    let mut game = load();
    let goblin = find_named(&game, "goblin_1");
    let (top, _) = summit(&game);
    let camera = game.camera_3d(WINDOW).expect("сцена трёхмерная");
    let away = [top[0] - camera.eye[0], top[1] - camera.eye[1]];
    let length = away[0].hypot(away[1]);
    let behind = [
        top[0] + 2.0 * away[0] / length,
        top[1] + 2.0 * away[1] / length,
    ];
    game.move_object(goblin, [behind[0] - 0.45, behind[1] - 0.45], None);
    let body = Body::of_object(&game.world, goblin).expect("у объекта есть фигура");
    let window = camera
        .project([body.center[0], body.center[1], body.base + 0.5])
        .expect("тело перед камерой");
    let hit = pointer_hit(&game.world, &game.scene, &camera, window).expect("луч ложится на землю");
    let target = game.properties.resolve("target").unwrap();
    game.set_cursor_point(hit, Some(camera.eye));
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    assert!(!game.world.flag(goblin, target), "гоблин за холмом выбран");
    release(&mut game);

    let visible = [
        top[0] - 6.0 * away[0] / length,
        top[1] - 6.0 * away[1] / length,
    ];
    game.move_object(goblin, [visible[0] - 0.45, visible[1] - 0.45], None);
    let body = Body::of_object(&game.world, goblin).expect("у объекта есть фигура");
    let window = camera
        .project([body.center[0], body.center[1], body.base + 0.5])
        .expect("тело перед камерой");
    let hit = pointer_hit(&game.world, &game.scene, &camera, window).expect("луч ложится на землю");
    game.set_cursor_point(hit, Some(camera.eye));
    game.key_down("MouseLeft");
    let snap = game.take_input_snapshot();
    game.step(snap);
    assert!(
        game.world.flag(goblin, target),
        "гоблин перед холмом не выбран"
    );
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

/// «Нефункциональные требования» фазы 17: рельеф, настилы и видимость готовятся при сборке мира, и любой
/// отдельный поиск пути — первый после сборки, первый после гибели каждого врага, до самого дальнего места и
/// на вершину холма, с лестницы и со входа — не дольше 5 мс в `cargo test --release`. Сборка мира
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
    let (top, _) = summit(&free);
    let farthest = if cfg!(debug_assertions) {
        [19.3, 0.3]
    } else {
        farthest_place(&free, free_hero, &mut WalkCaches::new())
    };
    let mut caches = WalkCaches::new();
    assert!(
        reaches(
            &plan_to(&free, free_hero, &mut caches, farthest, None),
            farthest
        ),
        "самое дальнее место {farthest:?} достижимо"
    );
    assert!(
        reaches(&plan_to(&free, free_hero, &mut caches, top, None), top),
        "вершина холма достижима"
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
    spans.push((
        "со входа на вершину холма".to_string(),
        timed_order(&mut game, hero, top),
    ));
    let foot = stair_foot(&game);
    put_hero(&mut game, hero, HILL_GATE);
    spans.push((
        "с места за проходом гоблина к подножию лестницы".to_string(),
        timed_order(&mut game, hero, foot),
    ));
    put_hero(&mut game, hero, foot);
    spans.push((
        "с подножия лестницы на вершину холма".to_string(),
        timed_order(&mut game, hero, [top[0] + 0.05, top[1]]),
    ));

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

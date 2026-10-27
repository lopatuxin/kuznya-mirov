//! Ролевая игра `games/rpg` — деревня на рельефе («Фаза-02-6»). Деревня грузится из настоящих файлов.
//! Бой, щелчок лучом, ходьба и поиск пути проверяются настоящими шагами движка на маленькой
//! трёхмерной сцене — коридоре из стен, — которую проверки собирают в коде вместе с остальными
//! файлами игры.

use std::fs;
use std::path::PathBuf;

use engine::core::footprint::Footprint;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::pathfind::WalkCaches;
use engine::core::property;
use engine::core::scene::{ground_footprint, pointer_hit, top_surface_height};
use engine::core::shapes::Body;
use engine::core::surface;
use engine::core::walk3d::{self, Blocker, Deck, Goal, Surfaces, Walker};
use engine::data::load::{
    GameConfig, ImageVerdict, ModelFile, load_rest_with_motions, read_entry, terrain_image_paths,
};
use serde_json::json;

const WINDOW: [f32; 2] = [1920.0, 1080.0];

/// Склон деревни, на котором земля под ступнями героя не на уровне его основания.
const SLOPE: [f64; 2] = [41.0, 16.0];

#[path = "skeleton_support/mod.rs"]
mod skeleton_support;
use skeleton_support::{joint_point, posed_point};

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

/// Размер карты материала по её файлу: у PNG — из `IHDR`, у JPEG — из начала кадра.
fn map_dimensions(name: &str) -> (u32, u32) {
    if name.ends_with(".png") {
        png_dimensions(name)
    } else {
        jpeg_dimensions(name)
    }
}

/// `width`/`height` of a JPEG straight out of its start-of-frame segment: the first `SOF` marker,
/// then precision (one byte), height and width (two bytes each, big-endian).
fn jpeg_dimensions(name: &str) -> (u32, u32) {
    let path = game_path(name);
    let bytes = fs::read(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"));
    assert_eq!(bytes[..2], [0xFF, 0xD8], "{name}: не JPEG");
    let mut at = 2;
    while at + 4 <= bytes.len() {
        assert_eq!(bytes[at], 0xFF, "{name}: сломан сегмент на {at}");
        let marker = bytes[at + 1];
        let length = usize::from(u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]));
        if matches!(marker, 0xC0..=0xC3 | 0xC5..=0xC7 | 0xC9..=0xCB | 0xCD..=0xCF) {
            let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]);
            let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]);
            return (u32::from(width), u32::from(height));
        }
        at += 2 + length;
    }
    panic!("{name}: в JPEG нет кадра");
}

fn ok_verdict((width, height): (u32, u32)) -> ImageVerdict {
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

type Verdicts = Vec<(String, ImageVerdict)>;

/// Ответы страницы по картам материалов и маскам покрытий: размеры настоящих файлов, точки нулевые.
fn map_data(config: &GameConfig, terrain_json: Option<&str>) -> (Verdicts, Verdicts) {
    let maps = config
        .files
        .materials
        .iter()
        .flat_map(|material| material.maps())
        .map(|(_, path)| (path.to_string(), ok_verdict(map_dimensions(path))))
        .collect();
    let masks = terrain_json
        .map(terrain_image_paths)
        .unwrap_or_default()
        .into_iter()
        .map(|path| {
            let verdict = ok_verdict(png_dimensions(&path));
            (path, verdict)
        })
        .collect();
    (maps, masks)
}

/// Файлы моделей игры, как их отдала бы страница: байты `.glb` и картинки внутри него готовыми точками.
fn model_files(config: &GameConfig) -> Vec<(String, ModelFile)> {
    let image = ImageVerdict::Ok {
        width: 4,
        height: 4,
        pixels: vec![200; 64],
    };
    config
        .files
        .models
        .iter()
        .map(|decl| {
            let bytes =
                fs::read(game_path(&decl.path)).unwrap_or_else(|e| panic!("{}: {e}", decl.path));
            let images = (0..8).map(|index| (index, image.clone())).collect();
            (
                decl.name.clone(),
                ModelFile {
                    bytes: Some(bytes),
                    images,
                },
            )
        })
        .collect()
}

/// Loads a `games/rpg` folder end to end, image sizes, the `enemies` table, the terrain file, the
/// material maps and the cover masks included. `game_json`, `scene_json` and `terrain_json` stand in for the real files: the village
/// passes them as they are, the combat scene passes its own.
fn load_files(game_json: &str, scene_json: &str, terrain_json: Option<&str>) -> Game {
    let (config, entry_warnings) =
        read_entry(game_json).expect("game.json ролевой игры должен разбираться");
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
    let stamp_texts: Vec<(String, Option<String>)> = config
        .files
        .stamps
        .iter()
        .map(|(name, path)| (name.clone(), Some(read(path))))
        .collect();
    let (map_verdicts, mask_verdicts) = map_data(&config, terrain_json);
    let model_files = model_files(&config);
    let motions_text = config.files.motions.as_deref().map(read);
    let (game, _screens, warnings, _images) = load_rest_with_motions(
        game_json,
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
        terrain_json,
        &map_verdicts,
        &mask_verdicts,
        &stamp_texts,
        &model_files,
        None,
        &[],
        motions_text.as_deref(),
    )
    .expect("ролевая игра должна проходить предстартовую проверку");
    let mut all_warnings = entry_warnings;
    all_warnings.extend(warnings);
    // Движения, заведённые на стенде анимации и ещё не отданные герою, игре не мешают.
    all_warnings.retain(|warning| {
        !warning.message.contains("не сжата") && !warning.message.contains("нигде не названо")
    });
    assert_eq!(all_warnings, Vec::new(), "{all_warnings:?}");
    game
}

/// The real village: `game.json`, `terrain.json` and `scene_json` in place of `scene.json`.
fn load_village(scene_json: &str) -> Game {
    load_files(&read("game.json"), scene_json, Some(&read("terrain.json")))
}

const COMBAT_SCENE: [u32; 2] = [12, 24];
const CORRIDOR_X: f64 = 6.0;
const CORRIDOR_WIDTH: f64 = 1.4;
const ENTRANCE: [f64; 2] = [CORRIDOR_X, 21.5];

/// Враги идут по коридору от входа с юга на север: гоблин, гоблин, орк. Каждый стоит посреди
/// коридора и перекрывает его целиком — зазор до стены у́же героя.
const ENEMY_CHAIN: [&str; 3] = ["goblin_1", "goblin_2", "orc_1"];

/// Свободная точка сразу за каждым врагом цепочки, дальше от входа.
const BEHIND_ENEMY: [[f64; 2]; 3] = [[CORRIDOR_X, 12.5], [CORRIDOR_X, 7.5], [CORRIDOR_X, 2.5]];

fn wall_json(name: &str, x: f64) -> String {
    format!(
        r##"{{"name":"{name}","position":[{x},0],"size":[0.5,{}],"shape":"box","height":2.4,"color":"#8a8f94","obstacle":true}}"##,
        COMBAT_SCENE[1]
    )
}

/// Герой с правилами ходьбы, боя и следования камеры; `center` — середина его основания.
fn hero_json(center: [f64; 2]) -> String {
    format!(
        r##"{{"name":"hero","position":[{},{}],"size":[{HERO_SIZE},{HERO_SIZE}],"layer":2,"hero":true,"camera_follows":true,"walk_speed":4,"shape":"capsule","height":1.8,"color":"#2f6fdb","health":100,"max_health":100,"damage":10,"keys":{{"MouseLeft":{{"press":[["walk_to","cursor"]]}}}}}}"##,
        center[0] - HERO_SIZE / 2.0,
        center[1] - HERO_SIZE / 2.0
    )
}

fn enemy_json(name: &str, kind: &str, y: f64, height: f64, color: &str) -> String {
    format!(
        r##"{{"name":"{name}","position":[{},{}],"size":[0.9,0.9],"layer":2,"enemy_unit":true,"obstacle":true,"enemy":"{kind}","shape":"capsule","height":{height},"color":"{color}","keys":{{"MouseLeft":{{"press":[["target",false]]}}}},"on_click":[["target",true]]}}"##,
        CORRIDOR_X - 0.45,
        y - 0.45
    )
}

/// Отметка щелчка из настоящего `scene.json`: правила игры ждут её на сцене.
fn marker_json() -> String {
    let scene: serde_json::Value =
        serde_json::from_str(&read("scene.json")).expect("scene.json должен разбираться");
    scene["objects"]
        .as_array()
        .and_then(|objects| objects.iter().find(|object| object["name"] == "marker"))
        .expect("в scene.json есть отметка marker")
        .to_string()
}

/// Коридор во всю высоту сцены между двумя стенами: с обоих концов он упирается в край сцены, обойти
/// врагов негде. `first_goblin_kind` — строка каталога первого гоблина, с опечаткой она ломает раздачу
/// каталога.
fn combat_scene(first_goblin_kind: &str) -> String {
    let objects = [
        wall_json("wall_west", CORRIDOR_X - CORRIDOR_WIDTH / 2.0 - 0.5),
        wall_json("wall_east", CORRIDOR_X + CORRIDOR_WIDTH / 2.0),
        enemy_json("goblin_1", first_goblin_kind, 15.0, 1.3, "#7dc243"),
        enemy_json("goblin_2", "goblin", 10.0, 1.3, "#7dc243"),
        enemy_json("orc_1", "orc", 5.0, 2.0, "#b03a2e"),
        hero_json(ENTRANCE),
        marker_json(),
    ];
    format!("{{\"objects\":[{}]}}", objects.join(",\n"))
}

/// `game.json` игры с боевой сценой вместо деревни: свой размер, ровная земля без файла рельефа и без
/// материалов, которым на ней нечего одевать; без моделей — и без движений к ним.
fn combat_game_json() -> String {
    let mut game_json: serde_json::Value =
        serde_json::from_str(&read("game.json")).expect("game.json должен разбираться");
    game_json["scene"]["width"] = json!(COMBAT_SCENE[0]);
    game_json["scene"]["height"] = json!(COMBAT_SCENE[1]);
    let files = game_json["files"].as_object_mut().expect("files — объект");
    files.remove("terrain");
    files.remove("materials");
    files.remove("stamps");
    files.remove("models");
    files.remove("motions");
    game_json.to_string()
}

fn combat_game_with_first_goblin(kind: &str) -> Game {
    load_files(&combat_game_json(), &combat_scene(kind), None)
}

fn combat_game() -> Game {
    combat_game_with_first_goblin("goblin")
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

/// Ломаная героя от его середины до `target` по поверхностям — тем поиском, что ведёт правило `walk`;
/// `named_z` — высота цели, как у `walk_to` с третьим числом.
fn plan_to(
    game: &Game,
    hero: u32,
    caches: &mut WalkCaches,
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
        center: center_of(game, hero),
        size: [HERO_SIZE, HERO_SIZE],
        rotation: None,
        height: surface::body_height(world, hero),
        z: z_of(game, hero),
    };
    let wanted_z = named_z.unwrap_or_else(|| top_surface_height(world, target));
    let goal = Goal {
        point: target,
        named_z,
        wanted_z,
    };
    walk3d::plan(&surfaces, &walker, &goal, scene_size(game), caches)
}

/// Убирает объекты с этими именами, как убрал бы их бой.
fn remove_named(game: &mut Game, names: &[&str]) {
    for name in names {
        let id = find_named(game, name);
        game.world.delete(id);
    }
}

#[test]
fn rpg_loads_without_errors_or_warnings() {
    let game = load_village(&read("scene.json"));
    assert!(game.scene.is_3d());
    assert!(!game.scene.y_sort);
}

/// Требования 28–29 и 31 фазы 19 с поправками фаз 20, 21 и 21.5: материалов у деревни нет — трава и
/// камень ушли до шага текстур, — покрытий, карты цвета и картинок рельефа нет, плиток травы на сцене нет.
#[test]
fn the_village_has_no_materials_until_the_texturing_step() {
    let (config, _) = read_entry(&read("game.json")).expect("game.json должен разбираться");
    let game = load_village(&read("scene.json"));
    assert!(game.ground.is_empty(), "плитки травы ушли");
    assert!(config.files.materials.is_empty(), "материалов нет");
    assert!(game.world.terrain().covers().is_empty(), "покрытий нет");
    assert!(
        !game.world.terrain().has_tint(),
        "карты цвета у деревни нет"
    );
    assert_eq!(
        terrain_image_paths(&read("terrain.json")),
        Vec::<String>::new(),
        "картинок рельефа нет"
    );
}

/// Фаза 20: каждый штамп деревни поставлен отпечатком, и отпечатки поднимают землю над высотами файла.
#[test]
fn every_stamp_of_the_village_stands_as_an_imprint_that_lifts_the_land() {
    let (config, _) = read_entry(&read("game.json")).expect("game.json должен разбираться");
    let game = load_village(&read("scene.json"));
    let terrain = game.world.terrain();
    for (index, (name, _)) in config.files.stamps.iter().enumerate() {
        assert!(
            terrain
                .imprints()
                .iter()
                .any(|imprint| imprint.stamp == index),
            "штамп \"{name}\" не поставлен"
        );
    }
    assert!(
        terrain
            .heights()
            .iter()
            .zip(terrain.base_heights())
            .any(|(effective, base)| effective > base),
        "отпечатки поднимают землю"
    );
}

/// Требование 32 фазы 15 с поправкой требования 31 фазы 19: плитки травы ушли — из `images` остаётся
/// только кольцо отметки, и файл в папке тоже один.
#[test]
fn only_the_marker_ring_remains_as_an_image() {
    let (config, _) = read_entry(&read("game.json")).expect("game.json должен разбираться");
    let names: Vec<&str> = config
        .files
        .images
        .iter()
        .map(|d| d.name.as_str())
        .collect();
    assert_eq!(names, ["marker"]);
    let mut files: Vec<String> = fs::read_dir(game_path("images"))
        .expect("папка images")
        .map(|e| e.expect("запись").file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    assert_eq!(files, ["marker.png"]);
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
    let mut game = combat_game();
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
    let beyond = BEHIND_ENEMY[0];
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
        let mut game = combat_game();
        remove_named(&mut game, killed);
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
    let mut game = combat_game();
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
    click_ground(&mut game, [CORRIDOR_X, 19.0]);
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
    let mut game = combat_game();
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
    near(z_of(&game, hero), 0.0, "вход на ровной земле");
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

/// Гоблин перекрывает коридор целиком — щелчок по земле за ним, пока он жив, не пропускает героя на
/// ту сторону настоящими шагами движка. Сначала герой идёт по коридору к гоблину, чтобы проверка не
/// прошла оттого, что ходьба сломана.
#[test]
fn clicking_past_a_live_enemy_does_not_let_the_hero_bypass_it() {
    let mut game = combat_game();
    let hero = find_named(&game, "hero");
    let goblin = find_named(&game, "goblin_1");
    let enemy_box = place_of(&game, goblin).aabb();

    let near_enemy = [CORRIDOR_X, 18.0];
    click_ground(&mut game, near_enemy);
    walk_until_arrival(&mut game, hero, 3000);
    assert!(
        dist(center_of(&game, hero), near_enemy) < 1e-6,
        "герой дошёл по коридору: {:?}",
        center_of(&game, hero)
    );

    release(&mut game);
    click_ground(&mut game, BEHIND_ENEMY[0]);
    walk_until_arrival(&mut game, hero, 3000);

    let at = center_of(&game, hero);
    assert!(
        at[1] >= enemy_box.y + enemy_box.h,
        "герой не должен пройти мимо живого гоблина на северную сторону коридора: {at:?}"
    );
    assert_eq!(number(&game, goblin, "health"), Some(30.0), "гоблин цел");
}

/// Щелчок по земле под самим героем не двигает его (тот уже стоит там), но снимает выбор со всех
/// врагов через их `keys` — бой заканчивается без урона.
#[test]
fn clicking_the_ground_under_the_hero_deselects_the_enemy_and_ends_combat() {
    let mut game = combat_game();
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
    let mut game = combat_game();
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
    let mut game = combat_game();
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

/// Путь после гибели врага, найденный по полю, что подгоняли под новый набор препятствий, — тот же, что
/// находит поиск на свежем поле: история гибелей не влияет на путь.
#[test]
fn a_path_after_an_enemy_dies_is_the_path_a_search_from_scratch_finds() {
    let mut game = combat_game();
    let hero = find_named(&game, "hero");
    // За каждым врагом коридора и у его северного конца: пока враг впереди жив, пути туда нет, после
    // его гибели путь есть.
    let targets = [
        BEHIND_ENEMY[0],
        BEHIND_ENEMY[1],
        BEHIND_ENEMY[2],
        [CORRIDOR_X, 0.5],
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

/// Опечатка в `enemy` даёт понятную ошибку кода с именем врага и его строкой каталога.
#[test]
fn enemy_catalog_typo_names_the_enemy_and_its_bad_row() {
    let mut game = combat_game_with_first_goblin("gobelin");
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

/// Фаза 27: герой деревни — модель героя Орды со скелетом, стойкой, лёгким бегом и ступнями вместо капсулы.
#[test]
fn the_hero_is_the_horde_model_with_its_motions_and_feet_instead_of_a_capsule() {
    let game = load_village(&read("scene.json"));
    let id = find_named(&game, "hero");
    assert_eq!(game.world.text(id, property::MODEL), Some("hero"));
    assert!(
        game.world.shape(id, property::SHAPE).is_none(),
        "капсулы нет"
    );
    assert_eq!(game.world.number_like(id, property::HEIGHT), Some(1.8));
    let model = game.world.model_of(id).expect("модель героя");
    let skeleton = model.skeleton.as_ref().expect("у модели героя есть скелет");
    for motion in [
        "Стойка",
        "Бег",
        "Спринт",
        "Попадание",
        "Смерть",
        "Перекат",
        "Ремонт",
    ] {
        assert!(
            skeleton.clip_named(motion).is_some(),
            "нет движения {motion}"
        );
    }
    assert_eq!(skeleton.idle, skeleton.clip_named("Стойка"));
    assert_eq!(skeleton.walk, skeleton.clip_named("бег_героя"));
    assert!(skeleton.legs.is_some(), "ступни на склонах");
    assert_eq!(skeleton.spine, skeleton.bone_named("mixamorig:Spine"));
    assert_eq!(
        skeleton.hands,
        Some(["mixamorig:LeftHand", "mixamorig:RightHand"].map(|name| {
            skeleton
                .bone_named(name)
                .expect("кисть героя есть в скелете")
        }))
    );
    let credits = read("CREDITS.txt");
    assert!(
        credits.contains("Quaternius Universal Animation Library") && credits.contains("CC0"),
        "движения героя названы в авторах"
    );
    assert!(credits.contains("Tripo"), "модель героя названа в авторах");
}

/// Поза бега героя с поправками `lean` и `arms_out` и без них: где в осях модели голова и две кисти.
fn jog_points(lean: f32, arms_out: f32) -> Vec<[f32; 3]> {
    use engine::core::skeleton::{Clip, PoseScratch, Retouch};
    let game = load_village(&read("scene.json"));
    let id = find_named(&game, "hero");
    let mut skeleton = game
        .world
        .model_of(id)
        .and_then(|model| model.skeleton.clone())
        .expect("скелет героя");
    let jog = skeleton.clip_named("Бег").expect("бег");
    let length = skeleton.clips[jog].length;
    skeleton.clips.push(Clip {
        name: "Corrected".to_string(),
        start: 0.0,
        length,
        tracks: Vec::new(),
        retouch: Some(Retouch {
            base: jog,
            looped: None,
            upper: None,
            bounce: 1.0,
            pelvis: None,
            lean,
            arms_out,
            bones: Vec::new(),
        }),
    });
    let corrected = skeleton.clips.len() - 1;
    let mut scratch = PoseScratch::default();
    skeleton.pose(
        &engine::core::skeleton::Blend::single(engine::core::skeleton::Playing {
            clip: Some(corrected),
            time: 0.25 * length,
            looped: true,
        }),
        &mut scratch,
    );
    [
        "mixamorig:Head",
        "mixamorig:LeftHand",
        "mixamorig:RightHand",
    ]
    .map(|name| {
        let bone = skeleton.bone_named(name).expect("кость есть");
        let m = scratch.globals()[bone];
        [m[3][0], m[3][1], m[3][2]]
    })
    .to_vec()
}

#[test]
fn lean_tips_the_real_hero_forward_and_arms_out_spreads_its_hands_apart() {
    let plain = jog_points(0.0, 0.0);
    let leaned = jog_points(15.0, 0.0);
    assert!(
        leaned[0][1] > plain[0][1] + 0.05,
        "голова ушла вперёд (+y модели): {:?} → {:?}",
        plain[0],
        leaned[0]
    );
    assert!(leaned[0][2] < plain[0][2], "и опустилась");
    let spread = jog_points(0.0, 25.0);
    let width = |points: &[[f32; 3]]| (points[2][0] - points[1][0]).abs();
    assert!(
        width(&spread) > width(&plain) + 0.1,
        "кисти разошлись: {} → {}",
        width(&plain),
        width(&spread)
    );
}

#[test]
fn the_hero_stands_in_the_first_pose_of_idle_and_runs_a_jog_toward_a_click_facing_its_way() {
    let mut game = load_village(&read("scene.json"));
    let id = find_named(&game, "hero");
    let model = game.world.model_of(id).expect("модель героя").clone();
    let skeleton = model.skeleton.expect("скелет");
    let stands = game.world.motion(id).blend(&skeleton);
    assert_eq!(stands.current().playing.clip, skeleton.idle);
    assert_eq!(
        stands.current().playing.time,
        0.0,
        "в редакторе — первая поза стойки"
    );
    let center = center_of(&game, id);
    game.world
        .set_vec2(id, property::WALK_TO, [center[0] + 6.0, center[1]]);
    for _ in 0..3 {
        game.step(StepInput::empty());
    }
    let runs = game.world.motion(id).blend(&skeleton);
    assert_eq!(runs.current().playing.clip, skeleton.walk, "идёт — Jog");
    assert_eq!(
        runs.layers().get(1).map(|layer| layer.playing.clip),
        Some(skeleton.idle),
        "плавно от стойки"
    );
    let facing = game
        .world
        .rotation(id, property::FACING)
        .map(|r| r.angle())
        .expect("повернулся по ходу");
    assert!((facing - 270.0).abs() < 1e-6, "вправо — 270: {facing}");
}

/// Точка связывания сустава в осях модели: обратный перенос и поворот матрицы `tail`, что переводит её в сустав.
fn bind_point(tail: &[[f32; 4]; 4]) -> [f32; 3] {
    let translation = [tail[3][0], tail[3][1], tail[3][2]];
    std::array::from_fn(|row| {
        -(tail[row][0] * translation[0]
            + tail[row][1] * translation[1]
            + tail[row][2] * translation[2])
    })
}

/// Лицо героя смотрит вдоль +Z файла, к игроку при `facing` 0: на бегу колено ушло вперёд от прямой бедро — ступня.
#[test]
fn the_knees_of_the_running_hero_bend_toward_the_front_of_the_model() {
    use engine::core::skeleton::{Blend, Playing, PoseScratch};
    let game = load_village(&read("scene.json"));
    let id = find_named(&game, "hero");
    let skeleton = game
        .world
        .model_of(id)
        .expect("модель героя")
        .skeleton
        .clone()
        .expect("скелет");
    let joint = |name: &str| {
        let at = skeleton
            .joint_named(name)
            .unwrap_or_else(|| panic!("нет кости {name}"));
        (at, bind_point(&skeleton.tails[at]))
    };
    let [hip, knee, ankle] = [
        "mixamorig:LeftUpLeg",
        "mixamorig:LeftLeg",
        "mixamorig:LeftFoot",
    ]
    .map(joint);
    let blend = Blend::single(Playing {
        clip: skeleton.walk,
        time: 0.6,
        looped: true,
    });
    let (mut scratch, mut matrices) = (PoseScratch::default(), Vec::new());
    skeleton.pose(&blend, &mut scratch);
    skeleton.skin_matrices(&scratch, &mut matrices);
    let place = |(at, bind): (usize, [f32; 3])| posed_point(&matrices[at], bind);
    let (hip, knee, ankle) = (place(hip), place(knee), place(ankle));
    let along = (hip[2] - knee[2]) / (hip[2] - ankle[2]);
    let line_y = hip[1] + along * (ankle[1] - hip[1]);
    assert!(
        knee[1] - line_y > 0.05,
        "колено позади прямой бедро — ступня: {hip:?} {knee:?} {ankle:?}"
    );
}

/// Фаза 27: герой доходит до склона ходом движка, и ступни стоят на земле под собой, как на ровном: над землёй ровно
/// настолько, насколько их держит движение, а таз и колени подстроились.
#[test]
fn on_a_slope_both_feet_of_the_hero_stand_the_same_height_above_the_ground_as_on_the_flat() {
    use engine::core::skeleton::PoseScratch;
    use engine::render::models::model_draws;
    let mut game = load_village(&read("scene.json"));
    let id = find_named(&game, "hero");
    game.world.set_vec2(id, property::WALK_TO, SLOPE);
    let mut guard = 0;
    while game.world.has(id, property::WALK_TO) && guard < 4000 {
        game.step(StepInput::empty());
        guard += 1;
    }
    assert!(guard < 4000, "герой дошёл до склона");
    for _ in 0..40 {
        game.step(StepInput::empty());
    }
    let model = game.world.models().index_of("hero").expect("модель героя");
    let skeleton = game
        .world
        .model_of(id)
        .expect("модель")
        .skeleton
        .clone()
        .expect("скелет");
    let draws = model_draws(&game);
    let instance = draws.instances[model][0];
    let [x, y, cos, sin] = instance.placement.map(f64::from);
    let [base, scale, ..] = instance.base.map(f64::from);
    assert!(
        (base - z_of(&game, id)).abs() < 1e-5,
        "основание то, что посадил движок"
    );
    let bones = &draws.bones[model];
    assert!(bones.iter().all(|c| c.is_finite()), "матрицы конечны");
    let (mut scratch, mut matrices) = (PoseScratch::default(), Vec::new());
    skeleton.pose(&game.world.motion(id).blend(&skeleton), &mut scratch);
    skeleton.skin_matrices(&scratch, &mut matrices);
    let flat: Vec<f32> = matrices.iter().flatten().flatten().copied().collect();
    let terrain = game.world.terrain();
    let mut lifts = Vec::new();
    for name in ["mixamorig:LeftFoot", "mixamorig:RightFoot"] {
        let at = skeleton.joint_named(name).expect("кость ступни");
        let bind = bind_point(&skeleton.tails[at]);
        let foot = joint_point(bones, at, bind);
        let (lx, ly) = (f64::from(foot[0]) * scale, f64::from(foot[1]) * scale);
        let ground = terrain.height_at(x + cos * lx - sin * ly, y + sin * lx + cos * ly);
        let above = base + f64::from(foot[2]) * scale - ground;
        let on_flat = f64::from(joint_point(&flat, at, bind)[2]) * scale;
        assert!(
            (above - on_flat).abs() < 2e-3,
            "{name}: над землёй {above}, на ровном {on_flat}"
        );
        lifts.push(ground - base);
    }
    assert!(
        lifts.iter().any(|lift| lift.abs() > 1e-3),
        "склон: земля под ступнями не на основании: {lifts:?}"
    );
}

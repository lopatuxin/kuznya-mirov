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
    GameConfig, ImageVerdict, load_rest_with_stamps, read_entry, terrain_image_paths,
};
use engine::render::materials::{Relief, pack_masks};
use serde_json::json;

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
    let (game, _screens, warnings, _images) = load_rest_with_stamps(
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
    )
    .expect("ролевая игра должна проходить предстартовую проверку");
    let mut all_warnings = entry_warnings;
    all_warnings.extend(warnings);
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
/// материалов, которым на ней нечего одевать.
fn combat_game_json() -> String {
    let mut game_json: serde_json::Value =
        serde_json::from_str(&read("game.json")).expect("game.json должен разбираться");
    game_json["scene"]["width"] = json!(COMBAT_SCENE[0]);
    game_json["scene"]["height"] = json!(COMBAT_SCENE[1]);
    let files = game_json["files"].as_object_mut().expect("files — объект");
    files.remove("terrain");
    files.remove("materials");
    files.remove("stamps");
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

/// Требования 28–29 и 31 фазы 19 с поправками требования 9 фазы 20 и требования 29 фазы 21: деревня
/// одета материалами — травой по всей земле и камнем по крутизне, без масок, — и плиток травы на сцене
/// нет; её карты собираются в текстуры видеокарты.
#[test]
fn the_village_cover_layers_assemble_into_textures_and_no_grass_tiles_remain() {
    let (config, _) = read_entry(&read("game.json")).expect("game.json должен разбираться");
    let game = load_village(&read("scene.json"));
    assert!(game.ground.is_empty(), "плитки травы ушли");
    let covers = game.world.terrain().covers();
    let layers: Vec<(&str, Option<f64>)> = covers
        .iter()
        .map(|cover| {
            let name = &config.files.materials[cover.material].name;
            assert_eq!(cover.mask, None, "у слоя {name} маски нет");
            (name.as_str(), cover.slope)
        })
        .collect();
    assert_eq!(
        layers,
        [
            ("grass", None),
            ("scree", Some(30.0)),
            ("rock_moss", Some(36.0)),
            ("rock", Some(44.0)),
        ],
        "трава на всём, камень по крутизне"
    );

    let terrain = read("terrain.json");
    let (map_verdicts, mask_verdicts) = map_data(&config, Some(&terrain));
    let image_paths = terrain_image_paths(&terrain);
    assert!(
        !game.world.terrain().has_tint(),
        "карты цвета у деревни нет"
    );
    assert_eq!(image_paths, Vec::<String>::new(), "картинок рельефа нет");
    let scene = [game.scene.width, game.scene.height];
    let relief = Relief::new(
        &config.files.materials,
        &map_verdicts,
        covers,
        false,
        &image_paths,
        &mask_verdicts,
        scene,
    )
    .expect("карты деревни собираются");
    assert_eq!(
        (relief.side, relief.materials.len(), relief.masks.len()),
        (1024, config.files.materials.len(), 0)
    );
    let packed = pack_masks(&relief.masks, relief.tint.as_ref());
    assert_eq!(packed.layers.len(), 1, "слой масок есть всегда");
}

/// Фаза 20: каждый штамп деревни поставлен горой, и горы поднимают землю над высотами файла.
#[test]
fn every_stamp_of_the_village_stands_as_a_mountain_that_lifts_the_land() {
    let (config, _) = read_entry(&read("game.json")).expect("game.json должен разбираться");
    let game = load_village(&read("scene.json"));
    let terrain = game.world.terrain();
    for (index, (name, _)) in config.files.stamps.iter().enumerate() {
        assert!(
            terrain
                .mountains()
                .iter()
                .any(|mountain| mountain.stamp == index),
            "штамп \"{name}\" не поставлен"
        );
    }
    assert!(
        terrain
            .heights()
            .iter()
            .zip(terrain.base_heights())
            .any(|(effective, base)| effective > base),
        "горы поднимают землю"
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

//! Транспортная компания (`games/transport`): игра грузится целиком, и по её улицам машина от конторы
//! доезжает до каждого места игры и каждого адреса жителей — карту пишет программа, и разрыв в сети
//! улиц иначе виден только посреди партии. Куски грунтовки ложатся на стыки улиц по одному узлу.

use std::fs;
use std::path::PathBuf;

use engine::core::game::Game;
use engine::core::roads::RoadNetwork;
use engine::data::load::{
    ImageVerdict, ModelFile, load_rest_with_models, read_entry, terrain_image_paths,
};
use serde_json::Value;

/// Скорость ГАЗели, клеток в секунду мира: 60 км/ч.
const SPEED: f64 = 16.67;
/// Насколько въезд в сеть и съезд с неё могут отстоять от точки остановки: точки остановок лежат на
/// осях улиц, так что съезд дальше этого — значит, до улицы места машина не доехала.
const REACH: f64 = 0.6;

fn game_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("transport")
        .join(name)
}

fn read(name: &str) -> String {
    fs::read_to_string(game_path(name)).unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"))
}

fn png_verdict(name: &str) -> ImageVerdict {
    let bytes =
        fs::read(game_path(name)).unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"));
    let width = u32::from_be_bytes(bytes[16..20].try_into().unwrap());
    let height = u32::from_be_bytes(bytes[20..24].try_into().unwrap());
    ImageVerdict::Ok {
        width,
        height,
        pixels: vec![0u8; (width * height * 4) as usize],
    }
}

/// Файлы кусков дорог, как их отдаёт страница: байты модели и её картинки — 4 × 4 точки, вид кусков тесту не
/// важен, а размеры кусков движок берёт из самой модели. Остальные модели тест не грузит.
fn road_models(game_json: &str) -> Vec<(String, ModelFile)> {
    let game: Value = serde_json::from_str(game_json).expect("game.json — JSON");
    game["files"]["models"]
        .as_object()
        .expect("в game.json нет моделей")
        .iter()
        .filter(|(name, _)| name.starts_with("road_"))
        .map(|(name, model)| {
            let path = model["path"].as_str().expect("у модели нет пути");
            let bytes = fs::read(game_path(path))
                .unwrap_or_else(|e| panic!("не смог прочитать {path}: {e}"));
            let images = (0..3)
                .map(|index| {
                    (
                        index,
                        ImageVerdict::Ok {
                            width: 4,
                            height: 4,
                            pixels: vec![200; 64],
                        },
                    )
                })
                .collect();
            (
                name.clone(),
                ModelFile {
                    bytes: Some(bytes),
                    images,
                },
            )
        })
        .collect()
}

fn load() -> Game {
    let game_json = read("game.json");
    let (config, _warnings) = read_entry(&game_json).expect("game.json должен разбираться");
    let tables: Vec<(String, Option<String>)> = config
        .files
        .tables
        .iter()
        .map(|(name, path)| (name.clone(), Some(read(path))))
        .collect();
    let fonts: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(game_path(path)).ok()))
        .collect();
    let terrain = read("terrain.json");
    let masks: Vec<(String, ImageVerdict)> = terrain_image_paths(&terrain)
        .into_iter()
        .map(|path| (path.clone(), png_verdict(&path)))
        .collect();
    let models = road_models(&game_json);
    let (game, _screens, _warnings, _images) = load_rest_with_models(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &fonts,
        &[],
        &[],
        &[],
        Some(&read("code.lua")),
        true,
        &tables,
        Some(&terrain),
        &[],
        &masks,
        &[],
        &models,
    )
    .expect("Транспортная компания должна проходить предстартовую проверку");
    game
}

fn network(game: &Game) -> RoadNetwork {
    let (mut shapes, mut speeds) = (Vec::new(), Vec::new());
    game.world.collect_roads(&mut shapes, &mut speeds);
    let mut network = RoadNetwork::default();
    network.sync(&shapes, &speeds);
    network
}

fn distance(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

#[test]
fn every_place_and_address_is_reached_by_road_from_the_office() {
    let mut game = load();
    game.new_game();
    let network = network(&game);
    let scene: Value = serde_json::from_str(&read("scene.json")).unwrap();
    let mut targets: Vec<(String, [f64; 2])> = Vec::new();
    for object in scene["objects"].as_array().unwrap() {
        if let (Some(place), Some(x), Some(y)) = (
            object["place"].as_str(),
            object["stop_x"].as_f64(),
            object["stop_y"].as_f64(),
        ) {
            targets.push((place.to_string(), [x, y]));
        }
    }
    let addresses: Value = serde_json::from_str(&read("tables/addresses.json")).unwrap();
    for address in addresses.as_array().unwrap() {
        let point = [
            address["x"].as_f64().unwrap(),
            address["y"].as_f64().unwrap(),
        ];
        targets.push((address["address"].as_str().unwrap().to_string(), point));
    }
    let office = targets
        .iter()
        .find(|(name, _)| name == "office")
        .expect("в сцене нет конторы")
        .1;
    assert!(
        targets.len() > 20,
        "мест и адресов слишком мало: {}",
        targets.len()
    );
    let mut unreached = Vec::new();
    for (name, goal) in &targets {
        for (from, to) in [(office, *goal), (*goal, office)] {
            match network.route(from, to, SPEED, None) {
                Some(route)
                    if distance(route.entry, from) <= REACH
                        && distance(route.exit, to) <= REACH => {}
                Some(route) => unreached.push(format!(
                    "{name}: въезд {:?}, съезд {:?}",
                    route.entry, route.exit
                )),
                None => unreached.push(format!("{name}: пути нет")),
            }
        }
    }
    assert!(unreached.is_empty(), "по улицам не доехать: {unreached:#?}");
}

#[test]
fn road_kind_joints_get_one_node_each() {
    let mut game = load();
    game.new_game();
    let layout = game.road_kind_layout();
    assert!(!layout.pieces().is_empty(), "на карте нет кусков вида");
    // Стык двух отрезков одной улицы — продолжение или один узел; три узла подряд встали бы ближе ширины улицы.
    let nodes = layout.nodes();
    for (i, a) in nodes.iter().enumerate() {
        for b in &nodes[i + 1..] {
            assert!(
                distance(a.center, b.center) > a.width.max(b.width),
                "узлы вплотную: {:?} и {:?}",
                a.center,
                b.center
            );
        }
    }
}

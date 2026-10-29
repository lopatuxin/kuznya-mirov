//! Орда `games/horde` — здания из объёма и срез при входе («Фаза-57», «Фаза-61»). Игра грузится из настоящих
//! файлов, герой бегает настоящими шагами движка по нажатиям клавиш; стены, опоры и пол вышки движок берёт
//! из её объёма.

use std::fs;
use std::path::PathBuf;

use engine::core::building::dissolve_share;
use engine::core::game::Game;
use engine::core::input::{KeyAction, KeyEvent, StepInput};
use engine::core::property;
use engine::core::scene::ground_footprint;
use engine::data::load::{ImageVerdict, ModelFile, load_rest_with_motions, read_entry};

/// Вышка в сцене и её части в осях объёма (`art/horde/3d/tower/build_tower.py`): объём повёрнут на 45°.
const TOWER_AT: [f64; 2] = [9.0, 16.0];
const POST_SQUARE: f64 = 2.95;
const HUT_HALF: f64 = 1.6;
const DECK_TOP: f64 = 4.5;
/// Передний край нар у задней стены комнаты.
const BUNK_FRONT: f64 = 0.675;
/// Ближний к камере край ящиков поперёк дальних полос балкона.
const CRATES_NEAR: f64 = -0.975;
/// Места у низа и у верха лестницы вышки: где герой стоит перед подъёмом и после схода (сняты со скрипта объёма).
const LADDER_FOOT: [f64; 2] = [13.81, 19.25];
const LADDER_TOP: [f64; 2] = [11.90, 17.34];
/// Подход, подъём на 4,5 клетки со скоростью 1,2 и сход наверху с запасом.
const CLIMB_SECONDS: f64 = 6.0;
const STEPS_PER_SECOND: usize = 60;
const WINDOW: [f32; 2] = [1024.0, 768.0];

fn game_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("horde");
    path.push(name);
    path
}

fn read(name: &str) -> String {
    let path = game_path(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

fn load() -> Game {
    let game_json = read("game.json");
    let (config, entry_warnings) =
        read_entry(&game_json).expect("game.json Орды должен разбираться");
    let image = ImageVerdict::Ok {
        width: 4,
        height: 4,
        pixels: vec![200; 64],
    };
    let model_files: Vec<(String, ModelFile)> = config
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
        .collect();
    let motions = config.files.motions.as_deref().map(read);
    let (game, _screens, warnings, _images) = load_rest_with_motions(
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
        false,
        &[],
        None,
        &[],
        &[],
        &[],
        &model_files,
        None,
        &[],
        motions.as_deref(),
    )
    .expect("Орда должна проходить предстартовую проверку");
    let mut all_warnings = entry_warnings;
    all_warnings.extend(warnings);
    all_warnings.retain(|warning| !warning.message.contains("не сжата"));
    assert_eq!(all_warnings, Vec::new(), "{all_warnings:?}");
    game
}

fn named(game: &Game, name: &str) -> u32 {
    game.world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some(name))
        .unwrap_or_else(|| panic!("объекта \"{name}\" нет на сцене"))
}

fn hero(game: &Game) -> u32 {
    named(game, "hero")
}

fn feet(game: &Game) -> [f64; 2] {
    ground_footprint(&game.world, hero(game))
        .expect("у героя есть место на земле")
        .center()
}

fn z_of_hero(game: &Game) -> f64 {
    game.world.base_z(hero(game))
}

/// Точка сцены в осях объёма вышки: стены будки и стойки там идут вдоль осей.
fn local(point: [f64; 2]) -> [f64; 2] {
    let (x, y) = (point[0] - TOWER_AT[0], -(point[1] - TOWER_AT[1]));
    let turn = -std::f64::consts::FRAC_PI_4;
    [
        x * turn.cos() - y * turn.sin(),
        x * turn.sin() + y * turn.cos(),
    ]
}

/// Точка в осях объёма вышки — в клетки сцены.
fn scene(local_point: [f64; 2]) -> [f64; 2] {
    let turn = std::f64::consts::FRAC_PI_4;
    let (x, y) = (local_point[0], local_point[1]);
    [
        TOWER_AT[0] + x * turn.cos() - y * turn.sin(),
        TOWER_AT[1] - (x * turn.sin() + y * turn.cos()),
    ]
}

fn put_hero(game: &mut Game, at: [f64; 2], z: f64) {
    let id = hero(game);
    let size = game
        .world
        .vec2(id, property::SIZE)
        .expect("у героя есть size");
    game.world
        .set_position_exact(id, [at[0] - size[0] / 2.0, at[1] - size[1] / 2.0], z);
}

fn keys(codes: &[&str], action: KeyAction) -> StepInput {
    StepInput {
        events: codes
            .iter()
            .map(|code| KeyEvent {
                code: code.to_string(),
                action,
                target: None,
            })
            .collect(),
        cursor: None,
    }
}

/// Держит клавиши `seconds` секунд и после каждого шага отдаёт игру проверке.
fn hold(game: &mut Game, codes: &[&str], seconds: f64, mut each: impl FnMut(&Game)) {
    game.step(keys(codes, KeyAction::Press));
    each(game);
    for _ in 1..(seconds * STEPS_PER_SECOND as f64) as usize {
        game.step(StepInput::empty());
        each(game);
    }
    game.step(keys(codes, KeyAction::Release));
}

fn tap(game: &mut Game, code: &str) {
    game.step(keys(&[code], KeyAction::Press));
    game.step(keys(&[code], KeyAction::Release));
}

#[test]
fn the_horde_loads_without_errors_or_warnings() {
    load();
}

/// Земля и вышка стоят на земле, заплатка земли — чуть выше плит, чтобы не спорить с ними глубиной.
#[test]
fn the_painted_ground_soil_and_tower_stand_where_the_data_puts_them() {
    let game = load();
    for id in game.world.ids() {
        let Some(model) = game.world.text(id, property::MODEL) else {
            continue;
        };
        let expected = match model {
            "hero" => continue,
            "soil" => 0.01,
            _ => 0.0,
        };
        assert!(
            (game.world.base_z(id) - expected).abs() < 1e-9,
            "{model} стоит на высоте {}",
            game.world.base_z(id)
        );
    }
}

#[test]
fn arrow_keys_run_the_hero_right_and_down() {
    let mut game = load();
    let start = feet(&game);
    hold(&mut game, &["ArrowRight"], 1.0, |_| {});
    let right = feet(&game);
    assert!(right[0] > start[0] + 3.0, "вправо: {start:?} → {right:?}");
    hold(&mut game, &["ArrowDown"], 0.5, |_| {});
    let down = feet(&game);
    assert!(down[1] > right[1] + 1.5, "вниз: {right:?} → {down:?}");
}

/// Связи между опорами — стенки: под вышку между опорами герой не забегает.
#[test]
fn the_hero_does_not_run_between_the_legs_of_the_tower() {
    for (start, codes) in [
        ([TOWER_AT[0], TOWER_AT[1] + 7.0], ["ArrowUp"].as_slice()),
        ([TOWER_AT[0] - 7.0, TOWER_AT[1]], ["ArrowRight"].as_slice()),
        (
            [TOWER_AT[0] - 5.0, TOWER_AT[1] + 5.0],
            ["ArrowRight", "ArrowUp"].as_slice(),
        ),
    ] {
        let mut game = load();
        put_hero(&mut game, start, 0.0);
        hold(&mut game, codes, 3.0, |game| {
            let at = local(feet(game));
            assert!(
                at[0].abs() > POST_SQUARE || at[1].abs() > POST_SQUARE,
                "{codes:?} от {start:?}: между опорами в {at:?}"
            );
        });
    }
}

fn run_for(game: &mut Game, seconds: f64) {
    for _ in 0..(seconds * STEPS_PER_SECOND as f64) as usize {
        game.step(StepInput::empty());
    }
}

fn climbing(game: &Game) -> bool {
    game.world.flag(hero(game), property::CLIMBING)
}

fn assert_stands_at(game: &Game, place: [f64; 2], z: f64) {
    let at = feet(game);
    assert!(
        (at[0] - place[0]).hypot(at[1] - place[1]) < 0.3,
        "герой на {at:?}, а не у {place:?}"
    );
    assert!(
        (z_of_hero(game) - z).abs() < 1e-6,
        "на высоте {}",
        z_of_hero(game)
    );
    assert!(!climbing(game), "герой всё ещё лезет");
}

#[test]
fn e_at_the_ladder_foot_climbs_the_hero_onto_the_deck_and_back_down() {
    let mut game = load();
    put_hero(&mut game, LADDER_FOOT, 0.0);
    tap(&mut game, "KeyE");
    run_for(&mut game, 1.5);
    assert!(climbing(&game), "герой не полез");
    let midway = z_of_hero(&game);
    assert!(
        midway > 0.5 && midway < DECK_TOP - 0.5,
        "через 1,5 с на высоте {midway}"
    );
    run_for(&mut game, CLIMB_SECONDS);
    assert_stands_at(&game, LADDER_TOP, DECK_TOP);

    tap(&mut game, "KeyE");
    run_for(&mut game, CLIMB_SECONDS);
    assert_stands_at(&game, LADDER_FOOT, 0.0);
}

#[test]
fn held_e_climbs_the_hero_once() {
    let mut game = load();
    put_hero(&mut game, LADDER_FOOT, 0.0);
    hold(&mut game, &["KeyE"], 2.0 * CLIMB_SECONDS, |_| {});
    assert_stands_at(&game, LADDER_TOP, DECK_TOP);
}

#[test]
fn e_away_from_the_ladder_does_nothing() {
    let mut game = load();
    let before = feet(&game);
    tap(&mut game, "KeyE");
    run_for(&mut game, 1.0);
    assert!(!climbing(&game));
    assert!(!game.world.flag(hero(&game), property::CLIMB));
    assert_eq!(feet(&game), before);
    assert!(z_of_hero(&game).abs() < 1e-6);
}

#[test]
fn run_keys_do_not_pull_the_climbing_hero_off_the_ladder() {
    let mut game = load();
    put_hero(&mut game, LADDER_FOOT, 0.0);
    hold(&mut game, &["KeyE", "KeyD"], 2.0, |_| {});
    run_for(&mut game, CLIMB_SECONDS);
    assert_stands_at(&game, LADDER_TOP, DECK_TOP);
}

/// На балконе герой бегает у будки, но за её задние стены не заходит: поперёк обеих дальних полос балкона стоят
/// ящики, иначе будка спрятала бы его от камеры целиком. Пока он снаружи, будка видна целиком — срез не включается.
#[test]
fn on_the_deck_crates_stop_the_hero_before_the_hut_hides_him() {
    // Вдоль осей будки: +y — влево-вверх, +x — вправо-вверх. Полоса +y лежит за левой задней стеной, +x — за правой.
    for (start, runs, blocked) in [
        (
            [-2.4, -2.4],
            &[["ArrowLeft", "ArrowUp"], ["ArrowRight", "ArrowUp"]][..],
            0,
        ),
        ([2.4, -2.4], &[["ArrowLeft", "ArrowUp"]][..], 1),
    ] {
        let mut game = load();
        let tower = named(&game, "tower");
        put_hero(&mut game, scene(start), DECK_TOP);
        for codes in runs {
            hold(&mut game, codes, 1.5, |game| {
                let at = local(feet(game));
                assert!(
                    (z_of_hero(game) - DECK_TOP).abs() < 1e-6,
                    "сошёл с настила в {at:?} на {}",
                    z_of_hero(game)
                );
                let in_hut = at[0].abs() < HUT_HALF && at[1].abs() < HUT_HALF;
                assert!(!in_hut, "в стене будки в {at:?}");
                assert!(at[blocked] < CRATES_NEAR, "прошёл за ящики в {at:?}");
                assert_eq!(
                    dissolve_share(&game.world, tower),
                    0.0,
                    "срез включился снаружи в {at:?}"
                );
            });
        }
        let stop = local(feet(&game));
        assert!(
            stop[blocked] > CRATES_NEAR - 0.6,
            "не дошёл до ящиков: {stop:?}"
        );
    }
}

/// В комнате герой упирается в нары и не проходит сквозь них.
#[test]
fn in_the_room_the_hero_does_not_walk_through_the_bunk() {
    let mut game = load();
    put_hero(&mut game, scene([-0.35, -0.3]), DECK_TOP);
    hold(&mut game, &["ArrowLeft", "ArrowUp"], 1.5, |game| {
        let at = local(feet(game));
        assert!(at[1] < BUNK_FRONT, "в нарах в {at:?}");
    });
}

/// В дверь герой входит в комнату, стоит на её полу, и вместо будки видна она же в разрезе.
#[test]
fn through_the_door_the_hero_enters_the_room_and_the_hut_is_cut_away() {
    let mut game = load();
    put_hero(&mut game, scene([0.3, -2.6]), DECK_TOP);
    let tower = named(&game, "tower");
    assert_eq!(
        dissolve_share(&game.world, tower),
        0.0,
        "снаружи видна целиком"
    );
    hold(&mut game, &["ArrowLeft", "ArrowUp"], 1.0, |_| {});
    let at = local(feet(&game));
    assert!(
        at[0].abs() < HUT_HALF && at[1].abs() < HUT_HALF,
        "не вошёл в комнату: {at:?}"
    );
    assert!(
        (z_of_hero(&game) - DECK_TOP).abs() < 1e-6,
        "не на полу комнаты"
    );
    assert!(
        dissolve_share(&game.world, tower) > 0.99,
        "будка не срезана"
    );
}

/// «Камера», требование 5: герой на балконе поднят над землёй, а окно всё равно не выходит за дальний край.
#[test]
fn with_the_hero_on_the_deck_the_window_does_not_open_past_the_far_edge() {
    let mut game = load();
    put_hero(&mut game, scene([-1.4, 2.6]), DECK_TOP);
    game.step(StepInput::empty());
    let camera = game.camera_3d(WINDOW).expect("камера трёхмерная");
    assert!(
        (camera.target_z - DECK_TOP).abs() < 1e-6,
        "камера не поднялась к балкону"
    );
    let top = camera
        .ground_hit([f64::from(WINDOW[0]) / 2.0, 0.0])
        .expect("верх окна на земле");
    assert!(top[1] > -1e-6, "верх окна за краем: {top:?}");
}

//! Потоп (`games/flood`), «Фаза-01-прототип» игры: игра грузится целиком без ошибок, а из предупреждений —
//! только то, что руль плота забрал у камеры игрока стрелки и W A S D: камеру в этой игре приближают
//! колесом, а водит её плот. Пять минут партии с рулём проходят без ошибок кода, в море появляются
//! налётчики, крюк подтягивает обломок к плоту и кладёт его в трюм, а стройка ставит к плоту новую клетку
//! за доски и «Снять» убирает её, возвращая половину цены.

use std::fs;
use std::path::PathBuf;

use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::data::load::{load_rest_with_stamps, read_entry};

fn game_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("games")
        .join("flood")
        .join(name)
}

fn read(name: &str) -> String {
    fs::read_to_string(game_path(name)).unwrap_or_else(|e| panic!("не смог прочитать {name}: {e}"))
}

fn load() -> Game {
    let game_json = read("game.json");
    let (config, mut warnings) =
        read_entry(&game_json).unwrap_or_else(|e| panic!("game.json не разобран: {e:?}"));
    let fonts: Vec<(String, Option<Vec<u8>>)> = config
        .files
        .fonts
        .iter()
        .map(|(name, path)| (name.clone(), fs::read(game_path(path)).ok()))
        .collect();
    let tables: Vec<(String, Option<String>)> = config
        .files
        .tables
        .iter()
        .map(|(name, path)| (name.clone(), Some(read(path))))
        .collect();
    let (game, _screens, rest_warnings, _images) = load_rest_with_stamps(
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
        false,
        &tables,
        Some(&read("terrain.json")),
        &[],
        &[],
        &[],
    )
    .unwrap_or_else(|e| panic!("Потоп не загрузился: {e:?}"));
    warnings.extend(rest_warnings);
    warnings.retain(|w| !w.message.ends_with("камера её не берёт"));
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game
}

fn steps(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
        if let Some(error) = game.code_error() {
            panic!("ошибка кода на шаге {}: {error:?}", game.step_count());
        }
    }
}

fn hold(game: &mut Game, key: &str, down: bool) {
    if down {
        game.key_down(key);
    } else {
        game.key_up(key);
    }
    let snap = game.take_input_snapshot();
    game.step(snap);
}

fn ids_with(game: &Game, flag: &str) -> Vec<u32> {
    let prop = game.properties.resolve(flag).expect("свойство объявлено");
    game.world
        .ids()
        .filter(|&id| game.world.flag(id, prop))
        .collect()
}

fn game_text(game: &Game, name: &str) -> String {
    let prop = game.properties.resolve(name).expect("свойство объявлено");
    let id = ids_with(game, "game")[0];
    game.world.text(id, prop).unwrap_or("").to_string()
}

#[test]
fn the_game_loads_without_errors_or_warnings() {
    load();
}

#[test]
fn five_minutes_at_sea_bring_raiders_and_no_code_errors() {
    let mut game = load();
    game.new_game();
    steps(&mut game, 2);
    hold(&mut game, "KeyW", true);
    let mut raiders_seen = false;
    for turn in 0..75 {
        let key = if turn % 2 == 0 { "KeyA" } else { "KeyD" };
        hold(&mut game, key, true);
        steps(&mut game, 90);
        hold(&mut game, key, false);
        steps(&mut game, 150);
        let name = game.properties.resolve("name").expect("name");
        raiders_seen |= ids_with(&game, "helm_label")
            .iter()
            .any(|&id| game.world.text(id, name) == Some("Налётчик"));
    }
    assert!(
        raiders_seen,
        "за пять минут в море не появилось ни одного налётчика"
    );
}

fn center(game: &Game, id: u32) -> [f64; 2] {
    let pos = game.world.vec2(id, property::POSITION).expect("место");
    let size = game.world.vec2(id, property::SIZE).expect("размер");
    [pos[0] + size[0] / 2.0, pos[1] + size[1] / 2.0]
}

#[test]
fn the_hook_pulls_a_nearby_plank_into_the_hold() {
    let mut game = load();
    game.new_game();
    steps(&mut game, 3);
    let part = game.properties.resolve("part").expect("part");
    let helm = ids_with(&game, "cell")
        .into_iter()
        .find(|&id| game.world.text(id, part) == Some("helm"))
        .expect("у плота есть кормило");
    let at = center(&game, helm);
    let plank = ids_with(&game, "debris")
        .into_iter()
        .filter(|&id| {
            let c = center(&game, id);
            (c[0] - at[0]).hypot(c[1] - at[1]) < 8.0
        })
        .min_by_key(|&id| {
            let c = center(&game, id);
            ((c[0] - at[0]).hypot(c[1] - at[1]) * 100.0) as i64
        })
        .expect("у плота в начале партии плавают обломки");
    let before = game_text(&game, "res_text");
    // Номер подобранного обломка движок отдаёт новому, поэтому обломок узнаём по его собственному номеру игры.
    let did = game.properties.resolve("did").expect("did");
    let hooked = game
        .world
        .number_like(plank, did)
        .expect("у обломка есть номер игры");
    let clicked = game.properties.resolve("clicked").expect("clicked");
    game.world.set_flag(plank, clicked, true);
    steps(&mut game, 2);
    assert_eq!(ids_with(&game, "rope").len(), 1, "крюк не полетел");
    steps(&mut game, 180);
    assert!(
        ids_with(&game, "debris")
            .iter()
            .all(|&id| game.world.number_like(id, did) != Some(hooked)),
        "подцепленный обломок всё ещё плавает"
    );
    assert_ne!(
        game_text(&game, "res_text"),
        before,
        "обломок не попал в трюм"
    );
    assert!(
        ids_with(&game, "rope").is_empty(),
        "верёвка осталась после подбора"
    );
}

#[test]
fn tuning_builds_a_deck_cell_for_three_boards_and_removing_it_returns_one() {
    let mut game = load();
    game.new_game();
    steps(&mut game, 3);
    let raft_cells = ids_with(&game, "cell").len();
    assert_eq!(
        game_text(&game, "res_text"),
        "Доски 4    Ткань 0    Железо 0"
    );
    game.run_screen_function("toggle_tuning", None, "тест");
    steps(&mut game, 1);
    let ghosts = ids_with(&game, "ghost");
    assert!(!ghosts.is_empty(), "у плота нет пустых клеток для стройки");
    let (gx, gy) = (
        game.properties.resolve("gx").expect("gx"),
        game.properties.resolve("gy").expect("gy"),
    );
    // Пустые клетки после стройки создаются заново, поэтому место запоминаем до щелчка.
    let ghost_at = (
        game.world.number_like(ghosts[0], gx),
        game.world.number_like(ghosts[0], gy),
    );
    let clicked = game.properties.resolve("clicked").expect("clicked");
    game.world.set_flag(ghosts[0], clicked, true);
    steps(&mut game, 2);
    assert_eq!(ids_with(&game, "cell").len(), raft_cells + 1);
    assert_eq!(
        game_text(&game, "res_text"),
        "Доски 1    Ткань 0    Железо 0"
    );

    let pk = game.properties.resolve("pk").expect("pk");
    let remove = game
        .world
        .ids()
        .find(|&id| game.world.text(id, pk) == Some("remove"))
        .expect("в списке стройки есть «Снять»");
    game.run_screen_function("pick_part", Some(remove), "тест");
    let built = ids_with(&game, "cell")
        .into_iter()
        .find(|&id| {
            (
                game.world.number_like(id, gx),
                game.world.number_like(id, gy),
            ) == ghost_at
        })
        .expect("новая клетка стоит там, где была пустая");
    game.world.set_flag(built, clicked, true);
    steps(&mut game, 2);
    assert_eq!(ids_with(&game, "cell").len(), raft_cells);
    assert_eq!(
        game_text(&game, "res_text"),
        "Доски 2    Ткань 0    Железо 0"
    );
}

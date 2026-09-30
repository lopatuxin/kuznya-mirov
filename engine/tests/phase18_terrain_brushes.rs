//! Фаза 18 — кисти рельефа: вызовы движка `set_terrain`, `terrain_heights`, `terrain_at`,
//! `terrain_height`, высота точки вращения камеры редактора. Игры собираются в коде теста, как в
//! `phase17_terrain.rs`; ролевая игра читается из `games/rpg`, чтобы померить скорость `set_terrain`.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use engine::core::camera::{Camera3d, EditorCamera};
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::scene::{pointer_hit, terrain_hit};
use engine::core::surface::{self, Lies};
use engine::core::terrain::Terrain;
use engine::core::value::parse_color;
use engine::data::edit::{set_terrain, terrain_heights};
use engine::data::load::{load_game_from_texts_with_terrain, load_rest_with_terrain, read_entry};
use engine::render::relief::TerrainMesh;

const WINDOW: [f64; 2] = [1920.0, 1080.0];
const WINDOW_F32: [f32; 2] = [1920.0, 1080.0];

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

const PROPS: &str = r#"{"properties":{"hero":"flag","wall":"flag"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const WALK_RULES: &str =
    r#"{"rules":[{"kind":"walk","for":{"has":["hero"]},"avoid":{"has":["wall"]}}]}"#;

fn game_json(camera: bool, terrain: bool) -> String {
    let camera = if camera {
        r#","view_height":12,"camera":{"pitch":55}"#
    } else {
        ""
    };
    let terrain = if terrain {
        r#","terrain":"terrain.json""#
    } else {
        ""
    };
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":12,"background":"#000000"{camera}}},
"random_seed":1,"start_screen":"main","max_objects":300,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{terrain}}}}}"##
    )
}

fn load(camera: bool, terrain: Option<&str>, objects: &str, rules: &str) -> Game {
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    load_game_from_texts_with_terrain(
        &game_json(camera, terrain.is_some()),
        PROPS,
        &scene,
        rules,
        SCREENS,
        None,
        &[],
        terrain,
    )
    .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
    .0
}

/// Игра 20×12 трёхмерной сцены без файла рельефа.
fn load_flat(objects: &str, rules: &str) -> Game {
    load(true, None, objects, rules)
}

/// Файл рельефа сцены 20×12 из высот `heights` (строки подряд) и воды.
fn terrain_file(heights: &[f64], water: Option<(f64, &str)>) -> String {
    let rows: Vec<String> = heights
        .chunks(41)
        .map(|row| {
            let cells: Vec<String> = row.iter().map(|h| format!("{h}")).collect();
            format!("[{}]", cells.join(","))
        })
        .collect();
    let water = water.map_or_else(String::new, |(level, color)| {
        format!(r#""water":{{"level":{level},"color":"{color}"}},"#)
    });
    format!(r#"{{{water}"heights":[{}]}}"#, rows.join(","))
}

/// Высоты сцены 20×12: `height_at(x, y)` в каждой точке сетки, строками сверху вниз.
fn grid(height_at: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    (0..25)
        .flat_map(|row| (0..41).map(move |column| (column as f64 / 2.0, row as f64 / 2.0)))
        .map(|(x, y)| height_at(x, y))
        .collect()
}

fn level(_: f64, _: f64) -> f64 {
    0.0
}

/// Холм: плато высоты 2 в середине сцены, склон в полторы клетки.
fn hill(x: f64, y: f64) -> f64 {
    let d = (x - 10.0).abs().max((y - 6.0).abs());
    if d <= 2.0 {
        2.0
    } else if d <= 3.5 {
        2.0 * (3.5 - d) / 1.5
    } else {
        0.0
    }
}

/// Холм и яма глубины 1 в западной части сцены.
fn hill_and_pit(x: f64, y: f64) -> f64 {
    let d = (x - 4.0).abs().max((y - 3.0).abs());
    match hill(x, y) {
        0.0 if d <= 1.5 => -1.0,
        0.0 if d <= 2.5 => -(2.5 - d),
        h => h,
    }
}

/// Хребет с подъёмом `rise` на клетку от `x = 8` до `x = 10` и таким же спуском до 12 во всей сцене.
fn ridge(rise: f64) -> impl Fn(f64, f64) -> f64 {
    move |x, _| rise * (x - 8.0).min(12.0 - x).clamp(0.0, 2.0)
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

fn z_of(game: &Game, id: u32) -> f64 {
    game.world.base_z(id)
}

fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{what}: {actual} вместо {expected}"
    );
}

fn tree(x: f64, y: f64) -> String {
    format!(
        r##"{{"name":"tree","position":[{x},{y}],"size":[0.6,0.6],"shape":"cylinder","height":3,"color":"#228833"}}"##
    )
}

fn rock(x: f64, y: f64) -> String {
    format!(
        r##"{{"name":"rock","position":[{x},{y}],"size":[0.6,0.6],"shape":"sphere","height":0.8,"color":"#777777"}}"##
    )
}

fn hero_json(x: f64, y: f64) -> String {
    format!(
        r##"{{"name":"hero","position":[{x},{y}],"size":[0.6,0.6],"shape":"capsule","height":1.8,"color":"#ff0000","hero":true,"walk_speed":8}}"##
    )
}

// -------------------------------------------------------------------------------------------
// set_terrain
// -------------------------------------------------------------------------------------------

#[test]
fn set_terrain_gives_new_heights_at_grid_points_and_between_them() {
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    let terrain = game.world.terrain();
    near(terrain.point_height(20, 12), 2.0, "плато в точке сетки");
    near(
        terrain.point_height(0, 0),
        0.0,
        "ровная земля в точке сетки",
    );
    near(terrain.height_at(10.0, 6.0), 2.0, "плато между точками");
    near(terrain.height_at(7.25, 6.0), 1.0, "склон между точками");
    assert_eq!(terrain.water(), None);
}

#[test]
fn set_terrain_puts_the_same_terrain_a_file_would() {
    let heights = grid(hill_and_pit);
    let text = terrain_file(&heights, Some((-0.5, "#3f7fd0")));
    let from_file = load(true, Some(&text), "", NO_RULES);
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &heights, Some((-0.5, "#3f7fd0"))).expect("рельеф поставлен");
    assert_eq!(game.world.terrain(), from_file.world.terrain());
}

#[test]
fn a_tree_on_a_raised_place_stands_higher_and_a_rock_in_a_dug_pit_lower() {
    let objects = format!("{},{}", tree(9.7, 5.7), rock(3.7, 2.7));
    let mut game = load(
        true,
        Some(&terrain_file(&grid(level), None)),
        &objects,
        NO_RULES,
    );
    near(z_of(&game, 0), 0.0, "дерево на ровной земле");
    near(z_of(&game, 1), 0.0, "камень на ровной земле");
    set_terrain(&mut game, &grid(hill_and_pit), None).expect("рельеф поставлен");
    near(z_of(&game, 0), 2.0, "дерево на холме");
    near(z_of(&game, 1), -1.0, "камень в яме");
    set_terrain(&mut game, &grid(level), None).expect("рельеф поставлен");
    near(z_of(&game, 0), 0.0, "холм срыт");
    near(z_of(&game, 1), 0.0, "яма засыпана");
}

#[test]
fn a_deck_without_z_takes_the_new_highest_point_under_it_and_an_object_with_z_keeps_its_own() {
    let objects = concat!(
        r##"{"name":"bridge","position":[8,5],"size":[4,2],"shape":"box","height":0.3,"color":"#8a6a3a","deck":true},"##,
        r##"{"name":"step","position":[3,3,1.25],"size":[1,1],"shape":"box","height":0.3,"color":"#886644","deck":true}"##,
    );
    let mut game = load_flat(objects, NO_RULES);
    near(z_of(&game, 0), 0.0, "мост на ровной земле");
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    near(z_of(&game, 0), 2.0, "мост на самой высокой точке под собой");
    near(z_of(&game, 1), 1.25, "ступень на своём z");
    set_terrain(&mut game, &grid(|_, _| 3.0), None).expect("рельеф поставлен");
    near(
        z_of(&game, 1),
        1.25,
        "земля выше z — ступень остаётся в земле",
    );
}

#[test]
fn a_flat_object_lies_on_the_new_terrain() {
    let trail = r##"{"name":"trail","position":[9,5],"size":[2,2],"color":"#aa8844"}"##;
    let mut game = load_flat(trail, NO_RULES);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    near(z_of(&game, 0), 2.0, "плоский объект на плато");
    assert!(matches!(
        surface::lies_on(&game.world, 0),
        Some(Lies::Terrain { .. })
    ));
}

#[test]
fn set_terrain_builds_the_world_from_the_scene_again() {
    let mut game = load_flat(&tree(9.7, 5.7), NO_RULES);
    game.move_object(0, [3.0, 3.0], None);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    assert_eq!(
        game.world.vec2(0, property::POSITION),
        Some([9.7, 5.7]),
        "объект вернулся на место сцены"
    );
}

#[test]
fn water_appears_and_goes_with_set_terrain() {
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &grid(hill), Some((-0.5, "#3f7fd0"))).expect("рельеф поставлен");
    let water = game.world.terrain().water().expect("вода есть");
    near(water.level, -0.5, "уровень воды");
    assert_eq!(Some(water.color), parse_color("#3f7fd0"));
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    assert_eq!(game.world.terrain().water(), None, "воды нет");
}

fn error_of(game: &mut Game, heights: &[f64], water: Option<(f64, &str)>) -> String {
    set_terrain(game, heights, water).expect_err("рельеф должен не встать")
}

#[test]
fn a_wrong_count_of_numbers_is_an_error_with_the_text() {
    let mut game = load_flat("", NO_RULES);
    let message = error_of(&mut game, &grid(hill)[1..], None);
    assert!(
        message.contains("heights") && message.contains("1024") && message.contains("1025"),
        "{message}"
    );
    assert!(game.world.terrain().is_trivial(), "рельеф не тронут");
}

#[test]
fn a_height_that_is_not_a_number_is_an_error_naming_its_place() {
    let mut game = load_flat("", NO_RULES);
    let mut heights = grid(level);
    heights[41 + 2] = f64::NAN;
    let message = error_of(&mut game, &heights, None);
    assert!(
        message.contains("heights[1][2]") && message.contains("ожидалось число"),
        "{message}"
    );
    heights[41 + 2] = f64::INFINITY;
    assert!(error_of(&mut game, &heights, None).contains("heights[1][2]"));
}

#[test]
fn a_color_that_is_not_a_color_is_an_error_with_the_text() {
    let mut game = load_flat("", NO_RULES);
    for color in ["blue", "#12345", ""] {
        let message = error_of(&mut game, &grid(level), Some((-1.0, color)));
        assert!(
            message.contains("water → color") && message.contains("#rrggbb"),
            "{message}"
        );
    }
}

#[test]
fn a_level_that_is_not_a_number_is_an_error_with_the_text() {
    let mut game = load_flat("", NO_RULES);
    let message = error_of(&mut game, &grid(level), Some((f64::NAN, "#3f7fd0")));
    assert!(
        message.contains("water → level") && message.contains("ожидалось число"),
        "{message}"
    );
}

#[test]
fn a_flat_scene_takes_no_terrain() {
    let mut game = load(false, None, "", NO_RULES);
    let message = error_of(&mut game, &grid(level), None);
    assert!(message.contains("трёхмерной"), "{message}");
}

#[test]
fn a_running_partiya_takes_no_terrain_but_a_stopped_one_does() {
    let mut game = load_flat("", NO_RULES);
    game.begin_session();
    let message = error_of(&mut game, &grid(hill), None);
    assert!(message.contains("партия"), "{message}");
    assert!(game.world.terrain().is_trivial(), "рельеф не тронут");
    game.end_session();
    set_terrain(&mut game, &grid(hill), None).expect("после «Стопа» рельеф ставится");
}

// -------------------------------------------------------------------------------------------
// Ходьба по новому рельефу
// -------------------------------------------------------------------------------------------

fn hero_x_after_walk(game: &mut Game) -> f64 {
    game.reset_for_play(true);
    game.world.set_walk_to(0, [16.0, 6.0], None);
    step(game, 1500);
    let position = game.world.vec2(0, property::POSITION).expect("position");
    position[0] + 0.3
}

#[test]
fn a_wall_raised_by_set_terrain_stops_the_hero_after_the_run_begins() {
    let mut game = load(
        true,
        Some(&terrain_file(&grid(level), None)),
        &hero_json(2.7, 5.7),
        WALK_RULES,
    );
    game.world.set_walk_to(0, [16.0, 6.0], None);
    step(&mut game, 5);
    let steep = 46.0_f64.to_radians().tan();
    set_terrain(&mut game, &grid(ridge(steep)), None).expect("рельеф поставлен");
    let x = hero_x_after_walk(&mut game);
    assert!(x < 8.5, "стена круче 45° не пустила: герой у x = {x}");
}

#[test]
fn a_hill_cut_down_by_set_terrain_is_walked_across_after_the_run_begins() {
    let steep = 46.0_f64.to_radians().tan();
    let mut game = load(
        true,
        Some(&terrain_file(&grid(ridge(steep)), None)),
        &hero_json(2.7, 5.7),
        WALK_RULES,
    );
    game.reset_for_play(true);
    game.world.set_walk_to(0, [16.0, 6.0], None);
    step(&mut game, 5);
    set_terrain(&mut game, &grid(level), None).expect("рельеф поставлен");
    let x = hero_x_after_walk(&mut game);
    near(x, 16.0, "герой дошёл");
}

// -------------------------------------------------------------------------------------------
// Камера редактора
// -------------------------------------------------------------------------------------------

fn orbit(target: [f64; 2], target_z: f64) -> EditorCamera {
    EditorCamera {
        target,
        target_z,
        yaw: 30.0,
        pitch: 50.0,
        distance: 14.0,
    }
}

fn pivot(game: &Game) -> f64 {
    game.editor_camera_3d(WINDOW_F32)
        .expect("трёхмерная сцена")
        .target_z
}

#[test]
fn the_pivot_height_of_the_editor_camera_is_the_terrain_under_it_or_exactly_the_named_one() {
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    let on_terrain = orbit([10.0, 6.0], 0.0).on_terrain(game.world.terrain());
    near(on_terrain.target_z, 2.0, "два числа — высота из рельефа");
    game.set_editor_camera(on_terrain);
    near(pivot(&game), 2.0, "камера вращается вокруг точки на плато");
    game.set_editor_camera(orbit([10.0, 6.0], 7.5));
    near(pivot(&game), 7.5, "три числа — ровно");
}

#[test]
fn set_terrain_and_show_scene_keep_the_pivot_height() {
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    game.set_editor_camera(orbit([10.0, 6.0], 2.0));
    set_terrain(&mut game, &grid(level), None).expect("рельеф поставлен");
    near(pivot(&game), 2.0, "после set_terrain");
    game.show_scene();
    near(pivot(&game), 2.0, "после show_scene");
    let camera = game.editor_camera_3d(WINDOW_F32).expect("камера");
    let centre = camera.project([10.0, 6.0, 2.0]).expect("перед камерой");
    assert!(
        (centre[0] - WINDOW[0] / 2.0).abs() < 1e-6 && (centre[1] - WINDOW[1] / 2.0).abs() < 1e-6,
        "точка вращения остаётся в середине окна"
    );
}

#[test]
fn fit_camera_hands_out_the_pivot_height_of_the_terrain_under_its_target() {
    let mut game = load_flat(&tree(3.0, 3.0), NO_RULES);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    let ground = game.fit_camera(None, WINDOW_F32).expect("камера");
    near(ground.target_z, 2.0, "вся земля — середина холма");
    let object = game.fit_camera(Some(0), WINDOW_F32).expect("камера");
    near(object.target_z, 0.0, "объект на ровной земле");
    let steered = object.camera(WINDOW);
    near(steered.target_z, 0.0, "камера стоит на высоте точки");
}

// -------------------------------------------------------------------------------------------
// terrain_heights
// -------------------------------------------------------------------------------------------

#[test]
fn terrain_heights_returns_the_terrain_of_the_file() {
    let heights = grid(hill_and_pit);
    let text = terrain_file(&heights, Some((-0.5, "#3f7fd0")));
    let game = load(true, Some(&text), "", NO_RULES);
    let snapshot = terrain_heights(&game).expect("сцена трёхмерная");
    assert_eq!((snapshot.columns, snapshot.rows), (41, 25));
    assert_eq!(snapshot.heights, heights);
    assert_eq!(snapshot.water, Some((-0.5, "#3f7fd0".to_string())));
}

#[test]
fn terrain_heights_without_a_file_are_zeros_of_the_scene_size_and_no_water() {
    let game = load_flat("", NO_RULES);
    let snapshot = terrain_heights(&game).expect("сцена трёхмерная");
    assert_eq!((snapshot.columns, snapshot.rows), (41, 25));
    assert_eq!(snapshot.heights, vec![0.0; 41 * 25]);
    assert_eq!(snapshot.water, None);
}

#[test]
fn terrain_heights_hand_back_what_set_terrain_took() {
    let mut game = load_flat("", NO_RULES);
    let heights = grid(hill_and_pit);
    set_terrain(&mut game, &heights, Some((-0.5, "#3f7fd0"))).expect("рельеф поставлен");
    let snapshot = terrain_heights(&game).expect("сцена трёхмерная");
    assert_eq!(snapshot.heights, heights);
    assert_eq!(snapshot.water, Some((-0.5, "#3f7fd0".to_string())));
}

#[test]
fn a_flat_scene_has_no_terrain_heights() {
    assert!(terrain_heights(&load(false, None, "", NO_RULES)).is_none());
}

// -------------------------------------------------------------------------------------------
// terrain_at и terrain_height
// -------------------------------------------------------------------------------------------

/// Овраг вдоль оси `y`: берега на −2,1 у стенок, дно −3 под водой уровня −2,3; мост поперёк.
fn gorge(x: f64, _: f64) -> f64 {
    match x {
        x if !(8.0..=12.0).contains(&x) => 0.0,
        x if x < 8.5 => -2.1 * (x - 8.0) / 0.5,
        x if x <= 9.5 || (10.5..=11.5).contains(&x) => -2.1,
        x if x < 10.0 => -2.1 - 0.9 * (x - 9.5) / 0.5,
        x if x <= 10.5 => -3.0 + 0.9 * (x - 10.0) / 0.5,
        x => -2.1 * (12.0 - x) / 0.5,
    }
}

const BRIDGE: &str = r##"{"name":"bridge","position":[6.5,5],"size":[7,3],"shape":"box","height":0.3,"color":"#8a6a3a","deck":true}"##;

fn gorge_game() -> Game {
    let text = terrain_file(&grid(gorge), Some((-2.3, "#35607a")));
    load(true, Some(&text), BRIDGE, NO_RULES)
}

fn pixel(camera: &Camera3d, point: [f64; 3]) -> [f64; 2] {
    camera.project(point).expect("точка перед камерой")
}

#[test]
fn terrain_at_through_the_bridge_lands_on_the_ground_beneath_it() {
    let game = gorge_game();
    let camera = Camera3d::looking_at([10.0, 6.5], 55.0, 12.0, WINDOW).raised(0.0);
    let window = pixel(&camera, [10.0, 6.5, 0.3]);
    let deck = pointer_hit(&game.world, &game.scene, &camera, window).expect("луч вниз");
    near(deck[2], 0.3, "щелчок ловит верх моста");
    let ground = terrain_hit(&game.world, &game.scene, &camera, window).expect("луч вниз");
    near(
        ground[2],
        game.world.terrain().height_at(ground[0], ground[1]),
        "точка лежит на рельефе",
    );
    assert!(ground[2] < -2.0, "под мостом, в овраге: {ground:?}");
}

#[test]
fn terrain_at_through_the_water_lands_on_the_bottom() {
    let game = gorge_game();
    let camera = Camera3d::looking_at([10.0, 6.5], 55.0, 12.0, WINDOW).raised(0.0);
    let window = pixel(&camera, [10.0, 10.0, -2.3]);
    let surface = pointer_hit(&game.world, &game.scene, &camera, window).expect("луч вниз");
    near(surface[2], -2.3, "щелчок ловит гладь воды");
    let bottom = terrain_hit(&game.world, &game.scene, &camera, window).expect("луч вниз");
    near(
        bottom[2],
        game.world.terrain().height_at(bottom[0], bottom[1]),
        "точка лежит на рельефе",
    );
    assert!(bottom[2] < -2.3, "дно ниже глади: {bottom:?}");
}

#[test]
fn terrain_at_on_the_plateau_is_the_point_itself() {
    let game = gorge_game();
    let camera = Camera3d::looking_at([10.0, 6.5], 55.0, 12.0, WINDOW).raised(0.0);
    let point = terrain_hit(
        &game.world,
        &game.scene,
        &camera,
        pixel(&camera, [3.0, 8.0, 0.0]),
    )
    .expect("луч вниз");
    assert!(
        (point[0] - 3.0).abs() < 1e-6 && (point[1] - 8.0).abs() < 1e-6 && point[2].abs() < 1e-6,
        "{point:?}"
    );
}

#[test]
fn terrain_at_misses_past_the_scene_and_above_the_horizon() {
    let game = gorge_game();
    let camera = Camera3d::looking_at([10.0, 2.0], 55.0, 12.0, WINDOW);
    assert_eq!(
        terrain_hit(&game.world, &game.scene, &camera, [WINDOW[0] / 2.0, 1.0]),
        None,
        "луч мимо сцены"
    );
    let low = Camera3d::orbiting([10.0, 6.0], 0.0, 10.0, 8.0, WINDOW);
    assert_eq!(
        terrain_hit(&game.world, &game.scene, &low, [WINDOW[0] / 2.0, 0.0]),
        None,
        "луч вверх"
    );
}

#[test]
fn terrain_at_in_a_scene_without_a_terrain_file_is_the_flat_ground() {
    let game = load_flat("", NO_RULES);
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    let point = terrain_hit(
        &game.world,
        &game.scene,
        &camera,
        pixel(&camera, [7.0, 5.0, 0.0]),
    )
    .expect("луч вниз");
    assert!(
        (point[0] - 7.0).abs() < 1e-6 && (point[1] - 5.0).abs() < 1e-6 && point[2].abs() < 1e-6,
        "{point:?}"
    );
}

#[test]
fn terrain_at_sees_the_terrain_set_by_set_terrain() {
    let mut game = load_flat("", NO_RULES);
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    let window = pixel(&camera, [10.0, 6.0, 2.0]);
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    let point = terrain_hit(&game.world, &game.scene, &camera, window).expect("луч вниз");
    near(point[2], 2.0, "луч встретил плато холма");
}

#[test]
fn terrain_height_is_height_at_and_outside_the_scene_the_height_of_the_edge() {
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &grid(|x, y| x + y), None).expect("рельеф поставлен");
    let terrain: &Terrain = game.world.terrain();
    near(terrain.height_at(3.0, 4.0), 7.0, "внутри сцены");
    near(terrain.height_at(-5.0, 4.0), 4.0, "за левым краем");
    near(
        terrain.height_at(100.0, 100.0),
        32.0,
        "за правым нижним углом",
    );
}

// -------------------------------------------------------------------------------------------
// Отрисовка без видеокарты
// -------------------------------------------------------------------------------------------

fn land_heights(mesh: &TerrainMesh) -> Vec<f32> {
    mesh.vertices[..mesh.land]
        .iter()
        .map(|v| v.position[2])
        .collect()
}

#[test]
fn the_terrain_mesh_after_set_terrain_holds_the_new_heights_in_the_same_number_of_vertices() {
    let mut game = load_flat("", NO_RULES);
    set_terrain(&mut game, &grid(level), None).expect("рельеф поставлен");
    let before = TerrainMesh::build(game.world.terrain(), [0.0; 4]).expect("файл высот есть");
    set_terrain(&mut game, &grid(hill), None).expect("рельеф поставлен");
    let after = TerrainMesh::build(game.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert_ne!(before.id, after.id, "новая сетка — новый номер");
    assert_eq!(
        before.vertices.len(),
        after.vertices.len(),
        "число вершин то же — буфер перезаписывается"
    );
    assert!(land_heights(&before).iter().all(|&z| z == 0.0));
    let top = land_heights(&after).into_iter().fold(f32::MIN, f32::max);
    assert_eq!(top, 2.0, "вершины сетки — новые высоты");
}

#[test]
fn the_terrain_mesh_of_a_scene_without_a_file_appears_with_the_first_set_terrain() {
    let mut game = load_flat("", NO_RULES);
    assert!(TerrainMesh::build(game.world.terrain(), [0.0; 4]).is_none());
    set_terrain(&mut game, &grid(hill), Some((-0.5, "#3f7fd0"))).expect("рельеф поставлен");
    let mesh = TerrainMesh::build(game.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert!(mesh.land > 0);
}

// -------------------------------------------------------------------------------------------
// Скорость на ролевой игре
// -------------------------------------------------------------------------------------------

fn rpg_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["..", "games", "rpg", name]);
    path
}

fn rpg_text(name: &str) -> String {
    let path = rpg_path(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

fn load_rpg() -> Game {
    let game_json = rpg_text("game.json");
    let (config, _) = read_entry(&game_json).expect("game.json ролевой игры разбирается");
    let tables: Vec<(String, Option<String>)> = config
        .files
        .tables
        .iter()
        .map(|(name, path)| (name.clone(), Some(rpg_text(path))))
        .collect();
    load_rest_with_terrain(
        &game_json,
        config,
        Some(&rpg_text("properties.json")),
        Some(&rpg_text("scene.json")),
        Some(&rpg_text("rules.json")),
        Some(&rpg_text("screens.json")),
        &[],
        &[],
        &[],
        &[],
        Some(&rpg_text("code.lua")),
        true,
        &tables,
        Some(&rpg_text("terrain.json")),
    )
    .unwrap_or_else(|e| panic!("ролевая игра не загрузилась: {e:?}"))
    .0
}

#[test]
fn set_terrain_on_the_rpg_game_takes_at_most_five_milliseconds_in_release() {
    let mut game = load_rpg();
    let mut snapshot = terrain_heights(&game).expect("сцена трёхмерная");
    let water = snapshot.water.clone().expect("в овраге вода");
    let (columns, rows) = (snapshot.columns, snapshot.rows);
    let hero = game
        .world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some("hero"))
        .expect("герой");
    let before = z_of(&game, hero);
    for (n, height) in snapshot.heights.iter_mut().enumerate() {
        *height += if (n % columns + n / columns) % 7 == 0 {
            0.5
        } else {
            0.0
        };
    }
    let started = Instant::now();
    set_terrain(
        &mut game,
        &snapshot.heights,
        Some((water.0, water.1.as_str())),
    )
    .expect("рельеф поставлен");
    let mesh = TerrainMesh::build(game.world.terrain(), game.scene.background).expect("сетка");
    let elapsed = started.elapsed();
    println!("set_terrain и сетка рельефа на ролевой игре: {elapsed:?}");
    assert_eq!(game.world.terrain().heights().len(), columns * rows);
    assert!(mesh.land > 0);
    assert!(
        (z_of(&game, hero) - before).abs() < 1.0,
        "мир собран заново по новому рельефу"
    );
    if !cfg!(debug_assertions) {
        assert!(elapsed.as_micros() <= 5000, "{elapsed:?}, дольше 5 мс");
    }
}

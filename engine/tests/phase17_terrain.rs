//! Фаза 17 — рельеф земли. Игры собираются в коде теста, как в `phase15_3d_scene_shapes.rs`: файл
//! высот `files.terrain` с водой и его проверка, высота основания и настил, посадка на поверхность,
//! столбы по высоте, ходьба по рельефу и мостам, камера с высотой, луч и точка под курсором, `z` в коде и
//! правилах, запись партии, вызовы редактора, отрисовка без видеокарты (сетка рельефа, плитки и плоские объекты
//! на нём, тени, свет, шейдеры).

use engine::core::camera::Camera3d;
use engine::core::footprint::Footprint;
use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::pathfind::WalkCaches;
use engine::core::property;
use engine::core::scene::{self, ObjectTransform};
use engine::core::surface;
use engine::core::terrain::{Terrain, Water};
use engine::core::value::Rotation;
use engine::core::walk3d::{self, Blocker, Deck, Goal, Surfaces, Walker};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::{
    ImageDecl, ImageVerdict, load_game_from_texts_with_terrain, load_rest_with_terrain, read_entry,
};
use engine::render::atlas::{self, AtlasImage, AtlasRect};
use engine::render::relief::{SurfaceVertex, TerrainMesh};
use engine::render::scene3d::{Frame3d, compose_frame3d};
use std::collections::HashSet;

const WINDOW: [f64; 2] = [1920.0, 1080.0];

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

const PROPS: &str = r#"{"properties":{"hero":"flag","wall":"flag","spawned":"flag","clicked":"number","goblin":"flag","blocked":"flag","marker":"flag"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const NO_RULES: &str = r#"{"rules":[]}"#;
const WALK_RULES: &str =
    r#"{"rules":[{"kind":"walk","for":{"has":["hero"]},"avoid":{"has":["wall"]}}]}"#;

fn game_json(width: u32, height: u32, camera: bool, terrain: bool) -> String {
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
        r##"{{"name":"T","scene":{{"width":{width},"height":{height},"background":"#000000"{camera}}},
"random_seed":1,"start_screen":"main","max_objects":300,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{terrain}}}}}"##
    )
}

type Loaded = Result<(Game, engine::core::screens::ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_full(
    width: u32,
    height: u32,
    camera: bool,
    terrain: Option<&str>,
    scene: &str,
    rules: &str,
    declare_terrain: bool,
) -> Loaded {
    load_game_from_texts_with_terrain(
        &game_json(width, height, camera, declare_terrain),
        PROPS,
        scene,
        rules,
        SCREENS,
        None,
        &[],
        terrain,
    )
}

/// Игра 20×12 трёхмерной сцены с рельефом.
fn load_terrain(terrain: &str, objects: &str, rules: &str) -> Game {
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    load_full(20, 12, true, Some(terrain), &scene, rules, true)
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
        .0
}

/// Игра 20×12 трёхмерной сцены без файла рельефа.
fn load_flat(objects: &str, rules: &str) -> Game {
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    load_full(20, 12, true, None, &scene, rules, false)
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
        .0
}

fn errors_of(result: Loaded) -> Vec<String> {
    match result {
        Ok(_) => panic!("игра должна была не загрузиться"),
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
            .collect(),
    }
}

fn expect_error(result: Loaded, fragments: &[&str]) {
    let errors = errors_of(result);
    assert!(
        errors
            .iter()
            .any(|e| fragments.iter().all(|f| e.contains(f))),
        "нет ошибки со словами {fragments:?}: {errors:#?}"
    );
}

/// Файл высот сцены `width × height` клеток: `height_at(x, y)` считается в каждой точке сетки.
fn terrain_text(
    width: u32,
    height: u32,
    water: Option<(f64, &str)>,
    height_at: impl Fn(f64, f64) -> f64,
) -> String {
    let rows: Vec<String> = (0..=2 * height)
        .map(|row| {
            let cells: Vec<String> = (0..=2 * width)
                .map(|column| {
                    let value = height_at(column as f64 / 2.0, row as f64 / 2.0);
                    format!("{}", (value * 1e6).round() / 1e6)
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();
    let water = water.map_or_else(String::new, |(level, color)| {
        format!(r#""water":{{"level":{level},"color":"{color}"}},"#)
    });
    format!(r#"{{{water}"heights":[{}]}}"#, rows.join(","))
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

fn hero_json(x: f64, y: f64) -> String {
    format!(
        r##"{{"name":"hero","position":[{x},{y}],"size":[0.6,0.6],"shape":"capsule","height":1.8,"color":"#ff0000","hero":true,"walk_speed":8}}"##
    )
}

fn z_of(game: &Game, id: u32) -> f64 {
    game.world.base_z(id)
}

fn center(game: &Game, id: u32) -> [f64; 2] {
    let p = game.world.vec2(id, property::POSITION).expect("position");
    let s = game.world.vec2(id, property::SIZE).expect("size");
    [p[0] + s[0] / 2.0, p[1] + s[1] / 2.0]
}

fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{what}: {actual} вместо {expected}"
    );
}

/// Посылает идущего в точку и ждёт, пока он дойдёт или пройдёт `limit` шагов; возвращает шаги.
fn walk(game: &mut Game, id: u32, to: [f64; 2], z: Option<f64>, limit: u32) -> u32 {
    game.world.set_walk_to(id, to, z);
    for n in 1..=limit {
        step(game, 1);
        if !game.world.has(id, property::WALK_TO) {
            return n;
        }
    }
    limit
}

// -------------------------------------------------------------------------------------------
// Высота земли
// -------------------------------------------------------------------------------------------

fn small_terrain() -> Terrain {
    // Сцена 2×1 клетки: 5 × 3 точки; высоты выбраны так, чтобы диагональ квадрата была видна.
    let rows = vec![
        vec![0.0, 1.0, 2.0, 3.0, 4.0],
        vec![0.0, 0.0, 2.0, 0.0, 0.0],
        vec![0.0, 0.0, 0.0, 0.0, 0.0],
    ];
    Terrain::from_rows([2, 1], &rows, None).expect("размеры сходятся")
}

#[test]
fn heights_at_grid_points_are_the_file_numbers_at_half_cell_steps() {
    let terrain = small_terrain();
    assert_eq!(terrain.height_at(0.0, 0.0), 0.0);
    assert_eq!(terrain.height_at(0.5, 0.0), 1.0);
    assert_eq!(terrain.height_at(1.0, 0.5), 2.0);
    assert_eq!(terrain.height_at(2.0, 0.0), 4.0);
    assert_eq!(terrain.height_at(1.5, 0.5), 0.0);
}

#[test]
fn the_square_is_cut_along_the_diagonal_from_its_top_left_to_its_bottom_right() {
    // Квадрат (1, 0): точки (0,5; 0) = 1, (1; 0) = 2, (0,5; 0,5) = 0, (1; 0,5) = 2.
    let terrain = small_terrain();
    // Над диагональю (x − 0,5 ≥ y): плоскость через (0,5; 0), (1; 0), (1; 0,5) — растёт вдоль x.
    let above = terrain.height_at(0.9, 0.1);
    let want_above = 1.0 + (0.9 - 0.5) * 2.0 + 0.1 * (2.0 - 2.0);
    assert!(
        (above - want_above).abs() < 1e-9,
        "{above} против {want_above}"
    );
    // Под диагональю: плоскость через (0,5; 0), (1; 0,5), (0,5; 0,5).
    let below = terrain.height_at(0.6, 0.4);
    let want_below = 1.0 + 0.4 * (0.0 - 1.0) * 2.0 + (0.6 - 0.5) * 2.0 * (2.0 - 0.0);
    assert!(
        (below - want_below).abs() < 1e-9,
        "{below} против {want_below}"
    );
}

#[test]
fn beyond_the_scene_the_height_is_the_nearest_edge_points() {
    let terrain = small_terrain();
    assert_eq!(terrain.height_at(-3.0, 0.0), 0.0);
    assert_eq!(terrain.height_at(7.0, 0.0), 4.0);
    assert_eq!(terrain.height_at(1.0, 9.0), 0.0);
    assert_eq!(
        terrain.height_at(1.0, -9.0),
        2.0 * 0.0 + terrain.height_at(1.0, 0.0)
    );
}

/// Самая низкая и самая высокая точка под местом, найденные перебором мелкой сетки точек внутри него.
fn sampled_range(terrain: &Terrain, place: &Footprint) -> (f64, f64) {
    let bbox = place.aabb();
    let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
    let step = 0.004;
    let mut x = bbox.x;
    while x <= bbox.x + bbox.w {
        let mut y = bbox.y;
        while y <= bbox.y + bbox.h {
            if place.contains([x, y]) {
                let h = terrain.height_at(x, y);
                low = low.min(h);
                high = high.max(h);
            }
            y += step;
        }
        x += step;
    }
    (low, high)
}

#[test]
fn the_lowest_and_highest_point_under_a_rectangle_match_a_dense_sampling_of_it() {
    let terrain = small_terrain();
    let places = [
        Footprint::flat([0.2, 0.1], [1.6, 0.6]),
        Footprint::flat([0.55, 0.05], [0.3, 0.4]),
        Footprint::rotated([1.4, 0.1], [0.5, 0.5], Rotation::from_degrees(45.0)),
        Footprint::rotated([0.3, 0.05], [1.2, 0.3], Rotation::from_degrees(20.0)),
        Footprint::rotated([0.6, 0.1], [0.8, 0.6], Rotation::from_degrees(-70.0)),
    ];
    for place in places {
        let (low, high) = terrain.range_under(&place);
        let (sampled_low, sampled_high) = sampled_range(&terrain, &place);
        assert!(
            low <= sampled_low + 1e-9 && sampled_low - low < 0.02,
            "низ {low} против перебора {sampled_low} под {place:?}"
        );
        assert!(
            high >= sampled_high - 1e-9 && high - sampled_high < 0.02,
            "верх {high} против перебора {sampled_high} под {place:?}"
        );
    }
}

#[test]
fn a_rectangle_over_the_top_left_point_of_a_square_reads_the_peak_at_the_grid_point() {
    let terrain = small_terrain();
    let over_peak = Footprint::flat([0.9, 0.4], [0.2, 0.2]);
    assert!((terrain.max_under(&over_peak) - 2.0).abs() < 1e-9);
}

#[test]
fn a_rectangle_partly_beyond_the_scene_reads_the_edge_height_beyond_it() {
    let terrain = small_terrain();
    let rect = Footprint::flat([1.8, 0.0], [1.0, 0.2]);
    let (low, high) = terrain.range_under(&rect);
    assert!(high >= 4.0 - 1e-9, "правый край сцены — 4: {high}");
    assert!(low < high);
}

// -------------------------------------------------------------------------------------------
// Проверка перед запуском
// -------------------------------------------------------------------------------------------

fn flat_terrain_text() -> String {
    terrain_text(20, 12, None, |_, _| 0.0)
}

#[test]
fn a_terrain_file_in_a_flat_scene_is_an_error() {
    let result = load_full(
        20,
        12,
        false,
        Some(&flat_terrain_text()),
        r#"{"objects":[]}"#,
        NO_RULES,
        true,
    );
    expect_error(result, &["files → terrain", "трёхмерной"]);
}

#[test]
fn a_missing_terrain_file_is_named() {
    let result = load_full(20, 12, true, None, r#"{"objects":[]}"#, NO_RULES, true);
    expect_error(result, &["terrain.json", "файл не найден"]);
}

#[test]
fn a_terrain_file_that_is_not_json_or_has_an_unknown_key_is_an_error() {
    let scene = r#"{"objects":[]}"#;
    expect_error(
        load_full(20, 12, true, Some("не json"), scene, NO_RULES, true),
        &["terrain.json", "JSON"],
    );
    let text = flat_terrain_text().replacen("{", r#"{"sea":1,"#, 1);
    expect_error(
        load_full(20, 12, true, Some(&text), scene, NO_RULES, true),
        &["terrain.json", "sea"],
    );
}

#[test]
fn heights_with_a_wrong_row_count_or_row_length_are_errors() {
    let scene = r#"{"objects":[]}"#;
    let short = terrain_text(20, 11, None, |_, _| 0.0);
    expect_error(
        load_full(20, 12, true, Some(&short), scene, NO_RULES, true),
        &["heights", "строк"],
    );
    let narrow = terrain_text(19, 12, None, |_, _| 0.0);
    expect_error(
        load_full(20, 12, true, Some(&narrow), scene, NO_RULES, true),
        &["heights[0]", "чисел"],
    );
}

#[test]
fn a_height_that_is_not_a_number_is_an_error_with_its_place() {
    let text = flat_terrain_text().replacen("0,0", r#""0",0"#, 1);
    let result = load_full(
        20,
        12,
        true,
        Some(&text),
        r#"{"objects":[]}"#,
        NO_RULES,
        true,
    );
    expect_error(result, &["heights[0][0]", "число"]);
}

#[test]
fn a_broken_water_is_an_error_for_each_way_it_can_break() {
    let scene = r#"{"objects":[]}"#;
    let heights = flat_terrain_text();
    let with = |water: &str| heights.replacen("{", &format!("{{{water},"), 1);
    for (water, fragments) in [
        (r##""water":{"color":"#000000"}"##, vec!["water", "level"]),
        (r#""water":{"level":0}"#, vec!["water", "color"]),
        (
            r##""water":{"level":0,"color":"#000000","tide":1}"##,
            vec!["water", "tide"],
        ),
        (
            r##""water":{"level":"low","color":"#000000"}"##,
            vec!["water → level", "число"],
        ),
        (
            r#""water":{"level":0,"color":"blue"}"#,
            vec!["water → color", "цвет"],
        ),
    ] {
        let text = with(water);
        expect_error(
            load_full(20, 12, true, Some(&text), scene, NO_RULES, true),
            &fragments,
        );
    }
}

#[test]
fn a_deck_that_is_not_a_flag_or_stands_in_a_flat_scene_or_lacks_a_place_is_an_error() {
    let deck = |body: &str| format!(r#"{{"objects":[{{{body}}}]}}"#);
    expect_error(
        load_full(
            20,
            12,
            true,
            Some(&flat_terrain_text()),
            &deck(r#""position":[1,1],"size":[2,1],"deck":"yes""#),
            NO_RULES,
            true,
        ),
        &["objects[0] → deck", "признак"],
    );
    expect_error(
        load_full(
            20,
            12,
            false,
            None,
            &deck(r#""position":[1,1],"size":[2,1],"deck":true"#),
            NO_RULES,
            false,
        ),
        &["objects[0] → deck", "трёхмерной"],
    );
    expect_error(
        load_full(
            20,
            12,
            true,
            None,
            &deck(r#""size":[2,1],"deck":true"#),
            NO_RULES,
            false,
        ),
        &["objects[0]", "deck", "position и size"],
    );
    expect_error(
        load_full(
            20,
            12,
            true,
            None,
            &deck(r#""position":[1,1],"deck":true"#),
            NO_RULES,
            false,
        ),
        &["objects[0]", "deck", "position и size"],
    );
}

#[test]
fn the_third_number_of_a_place_is_an_error_in_a_flat_scene_and_when_it_is_not_a_number() {
    let flat = |body: &str| {
        load_full(
            20,
            12,
            false,
            None,
            &format!(r#"{{"objects":[{{{body}}}]}}"#),
            NO_RULES,
            false,
        )
    };
    expect_error(
        flat(r#""position":[1,1,2],"size":[1,1]"#),
        &["objects[0] → position", "трёхмерной"],
    );
    expect_error(
        flat(r#""position":[1,1],"size":[1,1],"walk_to":[3,3,0]"#),
        &["objects[0] → walk_to", "трёхмерной"],
    );
    let solid = |body: &str| {
        load_full(
            20,
            12,
            true,
            None,
            &format!(r#"{{"objects":[{{{body}}}]}}"#),
            NO_RULES,
            false,
        )
    };
    expect_error(
        solid(r#""position":[1,1,"high"],"size":[1,1]"#),
        &["objects[0] → position → [2]", "число"],
    );
    expect_error(
        solid(r#""position":[1,1,0,4],"size":[1,1]"#),
        &["objects[0] → position", "элементов: 4"],
    );
    expect_error(
        solid(r#""position":[1,1],"size":[1,1],"walk_to":[1,1,1,1]"#),
        &["objects[0] → walk_to", "элементов: 4"],
    );
    solid(r#""position":[1,1,-2.5],"size":[1,1],"walk_to":[3,3,1]"#)
        .expect("тройка чисел в трёхмерной сцене — норма");
}

// -------------------------------------------------------------------------------------------
// Высота основания и посадка на поверхность
// -------------------------------------------------------------------------------------------

/// Холм: в середине сцены плато высоты 2, вокруг — ровная земля 0, склон в полторы клетки.
fn hill_terrain() -> String {
    terrain_text(20, 12, None, |x, y| {
        let d = ((x - 10.0).abs()).max((y - 6.0).abs());
        if d <= 2.0 {
            2.0
        } else if d <= 3.5 {
            2.0 * (3.5 - d) / 1.5
        } else {
            0.0
        }
    })
}

fn wall_box(name: &str, x: f64, y: f64, w: f64, h: f64, z: Option<f64>, extra: &str) -> String {
    let z = z.map_or_else(String::new, |z| format!(",{z}"));
    format!(
        r##"{{"name":"{name}","position":[{x},{y}{z}],"size":[{w},{h}],"shape":"box","height":1,"color":"#888888","wall":true{extra}}}"##
    )
}

#[test]
fn a_deck_without_z_stands_on_the_highest_terrain_point_under_it() {
    let deck = wall_box(
        "bridge",
        8.0,
        5.0,
        4.0,
        2.0,
        None,
        r#","deck":true,"height":0.3"#,
    )
    .replace(r#""height":1,"#, "");
    let game = load_terrain(&hill_terrain(), &deck, NO_RULES);
    near(z_of(&game, 0), 2.0, "низ настила на плато");
    near(surface::deck_top(&game.world, 0), 2.3, "верх настила");
}

#[test]
fn a_deck_with_z_in_the_data_stands_exactly_there_and_a_deck_without_shape_or_height_is_flat() {
    let flat =
        r##"{"name":"plank","position":[8,5,1.25],"size":[4,2],"color":"#886644","deck":true}"##;
    let game = load_terrain(&hill_terrain(), flat, NO_RULES);
    near(z_of(&game, 0), 1.25, "z из данных");
    near(
        surface::deck_top(&game.world, 0),
        1.25,
        "плоский настил — сам z",
    );
}

#[test]
fn a_shape_on_a_slope_stands_on_its_lowest_point_and_sinks_into_the_slope() {
    let hero = hero_json(6.6, 6.0);
    let game = load_terrain(&hill_terrain(), &hero, NO_RULES);
    // Тело 0,6 × 0,6 на склоне от x = 6,5: самая нижняя точка под ним.
    let footprint = scene::ground_footprint(&game.world, 0).expect("место");
    near(
        z_of(&game, 0),
        game.world.terrain().min_under(&footprint),
        "самая нижняя точка",
    );
    assert!(z_of(&game, 0) < game.world.terrain().max_under(&footprint));
}

#[test]
fn a_flat_object_gets_the_lowest_point_and_lies_on_the_terrain() {
    let flat = r##"{"name":"trail","position":[6.4,5.8],"size":[1,0.6],"color":"#aa8844"}"##;
    let game = load_terrain(&hill_terrain(), flat, NO_RULES);
    let footprint = scene::ground_footprint(&game.world, 0).expect("место");
    near(
        z_of(&game, 0),
        game.world.terrain().min_under(&footprint),
        "нижняя точка",
    );
    assert!(matches!(
        surface::lies_on(&game.world, 0),
        Some(surface::Lies::Terrain { .. })
    ));
}

#[test]
fn an_object_over_a_deck_stands_on_its_top_and_a_touching_edge_does_not_count() {
    let deck = r##"{"name":"bridge","position":[8,5],"size":[4,2],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    let over = wall_box("crate", 9.0, 5.5, 1.0, 1.0, None, "");
    let beside = wall_box("beside", 12.0, 5.5, 1.0, 1.0, None, "");
    let game = load_terrain(
        &terrain_text(20, 12, None, |_, _| 0.0),
        &format!("{deck},{over},{beside}"),
        NO_RULES,
    );
    near(z_of(&game, 1), 0.3, "на верх настила");
    near(z_of(&game, 2), 0.0, "касание краем — не настил");
}

#[test]
fn z_in_the_data_holds_and_a_moved_object_sits_down_again() {
    let high = wall_box("high", 3.0, 3.0, 1.0, 1.0, Some(5.0), "");
    let mut game = load_terrain(&hill_terrain(), &high, NO_RULES);
    near(z_of(&game, 0), 5.0, "z из данных держится");
    step(&mut game, 3);
    near(z_of(&game, 0), 5.0, "пока не сдвинется");
    game.world.set_vec2(0, property::POSITION, [3.0, 3.5]);
    near(z_of(&game, 0), 0.0, "полёта нет: встал на землю");
}

#[test]
fn a_step_of_four_tenths_is_climbed_and_four_tenths_and_a_hair_is_not() {
    let decks = |top: f64| {
        format!(
            r##"{{"name":"step","position":[5,3],"size":[2,2],"shape":"box","height":{top},"color":"#886644","deck":true}},{}"##,
            wall_box("pawn", 3.0, 3.5, 1.0, 1.0, None, "")
        )
    };
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let mut game = load_terrain(&flat, &decks(0.4), NO_RULES);
    game.world.set_vec2(1, property::POSITION, [4.5, 3.5]);
    near(z_of(&game, 1), 0.4, "ступенька 0,4 — на неё");
    let mut game2 = load_terrain(&flat, &decks(0.41), NO_RULES);
    game2.world.set_vec2(1, property::POSITION, [4.5, 3.5]);
    near(z_of(&game2, 1), 0.0, "0,41 — нет");
    step(&mut game, 1);
}

#[test]
fn stepping_off_a_deck_entirely_puts_the_object_down_and_going_under_a_high_deck_keeps_it_down() {
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let objects = format!(
        r##"{{"name":"table","position":[5,3],"size":[3,2],"shape":"box","height":0.3,"color":"#886644","deck":true}},{},{}"##,
        wall_box("pawn", 6.0, 3.5, 1.0, 1.0, None, ""),
        // Настил-навес с z в данных: низ на высоте 3, верх 3,3.
        r##"{"name":"roof","position":[10,3,3],"size":[3,2],"shape":"box","height":0.3,"color":"#886644","deck":true}"##
    );
    let mut game = load_terrain(&flat, &objects, NO_RULES);
    near(z_of(&game, 1), 0.3, "на столе");
    game.world.set_vec2(1, property::POSITION, [8.5, 3.5]);
    near(z_of(&game, 1), 0.0, "сошёл целиком — внизу");
    game.world.set_vec2(1, property::POSITION, [10.5, 3.5]);
    near(z_of(&game, 1), 0.0, "под навесом остаётся внизу");
}

#[test]
fn a_moved_deck_keeps_its_height_and_does_not_carry_what_stands_on_it() {
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let objects = format!(
        r##"{{"name":"plate","position":[5,3,0.2],"size":[3,2],"shape":"box","height":0.3,"color":"#886644","deck":true}},{}"##,
        wall_box("pawn", 6.0, 3.5, 1.0, 1.0, None, "")
    );
    let mut game = load_terrain(&flat, &objects, NO_RULES);
    near(z_of(&game, 1), 0.5, "на настиле");
    game.world.set_vec2(0, property::POSITION, [12.0, 8.0]);
    near(z_of(&game, 0), 0.2, "настил держит высоту");
    near(z_of(&game, 1), 0.5, "стоящий на нём не упал и не поехал");
    game.world.delete(0);
    near(z_of(&game, 1), 0.5, "убранный настил не роняет");
    game.world.set_vec2(1, property::POSITION, [6.0, 3.6]);
    near(z_of(&game, 1), 0.0, "первый же сдвиг ставит на землю");
}

#[test]
fn the_flat_world_without_terrain_keeps_every_object_at_zero() {
    let game = load_flat(
        &format!(
            "{},{}",
            hero_json(3.0, 3.0),
            wall_box("w", 6.0, 6.0, 1.0, 1.0, None, "")
        ),
        NO_RULES,
    );
    assert_eq!(z_of(&game, 0), 0.0);
    assert_eq!(z_of(&game, 1), 0.0);
}

#[test]
fn transform_object_without_a_third_number_sits_the_object_and_with_one_sets_it_exactly() {
    let mut game = load_terrain(&hill_terrain(), &hero_json(3.0, 3.0), NO_RULES);
    let transform = |z: Option<f64>| ObjectTransform {
        position: [10.0, 6.0],
        z,
        size: [0.6, 0.6],
        height: None,
        rotation: None,
    };
    scene::transform_object(&mut game.world, 0, transform(None));
    near(z_of(&game, 0), 2.0, "без z — посадка на плато");
    scene::transform_object(&mut game.world, 0, transform(Some(7.5)));
    near(z_of(&game, 0), 7.5, "с z — ровно");
    scene::move_object(&mut game.world, 0, [3.0, 3.0], None);
    near(z_of(&game, 0), 0.0, "перенос без z — на землю");
    scene::move_object(&mut game.world, 0, [3.0, 3.0], Some(1.5));
    near(z_of(&game, 0), 1.5, "перенос с z");
}

#[test]
fn rest_height_answers_for_the_object_at_a_new_place_with_and_without_a_previous_height() {
    let deck = r##"{"name":"bridge","position":[8,5],"size":[4,2],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    let game = load_terrain(
        &terrain_text(20, 12, None, |_, _| 0.0),
        &format!("{deck},{}", hero_json(3.0, 3.0)),
        NO_RULES,
    );
    let over = surface::rest_height_at(&game.world, 1, [9.0, 5.5], None).expect("трёхмерная сцена");
    near(over, 0.3, "без from — как без z в данных");
    let from_below = surface::rest_height_at(&game.world, 1, [9.0, 5.5], Some(-1.0)).unwrap();
    near(from_below, 0.0, "с from ниже ступеньки — не на настил");
    let from_near = surface::rest_height_at(&game.world, 1, [9.0, 5.5], Some(-0.1)).unwrap();
    near(from_near, 0.3, "с from в пределах ступеньки");
    let deck_now = surface::rest_height_at(&game.world, 0, [1.0, 1.0], Some(9.0)).unwrap();
    near(deck_now, 0.0, "настил — его нынешняя высота");
    assert_eq!(
        surface::rest_height_at(&game.world, 99, [1.0, 1.0], None),
        None
    );
}

#[test]
fn the_water_of_the_file_reaches_the_world_with_its_level_and_color() {
    let game = load_terrain(&gorge_terrain(), &hero_json(2.7, 2.7), NO_RULES);
    let water = game.world.terrain().water().expect("вода из файла");
    near(water.level, -2.3, "уровень");
    assert_eq!(
        Some(water.color),
        engine::core::value::parse_color("#35607a")
    );
    let dry = load_terrain(&hill_terrain(), &hero_json(2.7, 2.7), NO_RULES);
    assert_eq!(dry.world.terrain().water(), None::<Water>);
}

#[test]
fn an_object_without_size_stands_on_the_ground_at_its_point() {
    let point = r##"{"name":"marker","position":[10,6],"color":"#ffffff","marker":true}"##;
    let game = load_terrain(&hill_terrain(), point, NO_RULES);
    near(z_of(&game, 0), 2.0, "точка на плато");
}

// -------------------------------------------------------------------------------------------
// Ходьба
// -------------------------------------------------------------------------------------------

/// Овраг с востока на запад вдоль оси `y`: плато высоты 0, стенки круче 45°, по бокам от воды берега
/// на высоте −2,1, вода на уровне −2,3; на западе (`y ≤ 2`) и на востоке (`y ≥ 10`) к берегу ведёт
/// пологий спуск.
fn gorge_height(x: f64, y: f64) -> f64 {
    let bank = -2.1;
    let profile = |x: f64| {
        if x <= 8.0 || x >= 12.0 {
            0.0
        } else if x < 8.5 {
            bank * (x - 8.0) / 0.5
        } else if x <= 9.5 || (10.5..=11.5).contains(&x) {
            bank
        } else if x < 10.0 {
            bank + (-3.0 - bank) * (x - 9.5) / 0.5
        } else if x <= 10.5 {
            -3.0 + (3.0 + bank) * (x - 10.0) / 0.5
        } else {
            bank * (12.0 - x) / 0.5
        }
    };
    if y <= 2.0 && (4.5..8.5).contains(&x) {
        return bank * (x - 4.5) / 4.0;
    }
    if y >= 10.0 && (11.5..15.5).contains(&x) {
        return bank * (15.5 - x) / 4.0;
    }
    profile(x)
}

fn gorge_terrain() -> String {
    terrain_text(20, 12, Some((-2.3, "#35607a")), gorge_height)
}

const BRIDGE: &str = r##"{"name":"bridge","position":[6.5,5],"size":[7,3],"shape":"box","height":0.3,"color":"#8a6a3a","deck":true}"##;

fn gorge_game(hero: [f64; 2]) -> Game {
    load_terrain(
        &gorge_terrain(),
        &format!("{BRIDGE},{}", hero_json(hero[0] - 0.3, hero[1] - 0.3)),
        WALK_RULES,
    )
}

const HERO: u32 = 1;

#[test]
fn the_hero_crosses_the_gorge_by_the_bridge_and_never_leaves_its_deck_sideways() {
    let mut game = gorge_game([3.0, 6.5]);
    game.world.set_walk_to(HERO, [16.0, 6.5], None);
    let mut on_bridge = 0;
    for _ in 0..400 {
        step(&mut game, 1);
        let [cx, cy] = center(&game, HERO);
        assert!(
            z_of(&game, HERO) > -0.5,
            "герой не спускается в овраг: z = {}",
            z_of(&game, HERO)
        );
        if (8.3..11.7).contains(&cx) {
            on_bridge += 1;
            near(z_of(&game, HERO), 0.3, "на мосту стоит на его верху");
            assert!((5.3..=7.7).contains(&cy), "тело в пределах моста: y = {cy}");
        }
        if !game.world.has(HERO, property::WALK_TO) {
            break;
        }
    }
    assert!(on_bridge > 5, "герой прошёл по мосту");
    let [cx, cy] = center(&game, HERO);
    assert!(
        (cx - 16.0).abs() < 1e-6 && (cy - 6.5).abs() < 1e-6,
        "дошёл: {cx}, {cy}"
    );
    near(z_of(&game, HERO), 0.0, "сошёл на плато");
}

#[test]
fn a_click_on_the_bank_under_the_bridge_leads_under_it_and_a_click_on_the_bridge_leads_onto_it() {
    let mut game = gorge_game([3.0, 6.5]);
    let steps = walk(&mut game, HERO, [9.0, 6.5], Some(-2.1), 900);
    assert!(steps < 900, "герой дошёл под мост");
    let [cx, cy] = center(&game, HERO);
    assert!(
        (cx - 9.0).abs() < 1e-6 && (cy - 6.5).abs() < 1e-6,
        "{cx}, {cy}"
    );
    near(z_of(&game, HERO), -2.1, "стоит на берегу под мостом");

    let mut game = gorge_game([3.0, 6.5]);
    walk(&mut game, HERO, [9.0, 6.5], None, 900);
    near(z_of(&game, HERO), 0.3, "без z — верхняя поверхность: мост");
}

#[test]
fn the_way_to_the_bank_goes_down_the_ramp_and_never_through_the_water() {
    let mut game = gorge_game([3.0, 6.5]);
    game.world.set_walk_to(HERO, [9.0, 6.5], Some(-2.1));
    let mut lowest_y = f64::INFINITY;
    for _ in 0..900 {
        step(&mut game, 1);
        lowest_y = lowest_y.min(center(&game, HERO)[1]);
        assert!(
            z_of(&game, HERO) > -2.15,
            "не глубже берега: {}",
            z_of(&game, HERO)
        );
        if !game.world.has(HERO, property::WALK_TO) {
            break;
        }
    }
    assert!(lowest_y < 2.0, "спуск на западе (y ≤ 2): {lowest_y}");
}

#[test]
fn a_click_on_the_water_ends_at_its_edge() {
    let mut game = gorge_game([3.0, 3.0]);
    let steps = walk(&mut game, HERO, [10.0, 3.0], None, 900);
    assert!(steps < 900);
    let [cx, cy] = center(&game, HERO);
    assert!((9.2..9.45).contains(&cx), "у кромки воды: x = {cx}");
    assert!((cy - 3.0).abs() < 0.2, "y = {cy}");
    near(z_of(&game, HERO), -2.1, "на берегу");
}

/// Хребет с подъёмом `rise` на клетку по `x` от 8 до 10 и таким же спуском до 12 в полосе `y` от
/// `from` до `to`.
fn ramp_terrain(rise: f64, from: f64, to: f64) -> String {
    terrain_text(20, 12, None, move |x, y| {
        if (from..=to).contains(&y) {
            rise * (x - 8.0).min(12.0 - x).clamp(0.0, 2.0)
        } else {
            0.0
        }
    })
}

#[test]
fn a_slope_of_forty_four_degrees_is_climbed_and_one_of_forty_six_is_walked_around() {
    let climb = |degrees: f64, from: f64, to: f64| {
        let rise = degrees.to_radians().tan();
        let mut game = load_terrain(
            &ramp_terrain(rise, from, to),
            &hero_json(2.7, 5.7),
            WALK_RULES,
        );
        let steps = walk(&mut game, 0, [16.0, 6.0], None, 1500);
        (game, steps)
    };
    let (easy, easy_steps) = climb(44.0, 0.0, 12.0);
    assert!(easy_steps < 400, "44° проходимы: {easy_steps} шагов");
    let [cx, _] = center(&easy, 0);
    near(cx, 16.0, "дошёл");
    near(z_of(&easy, 0), 0.0, "спустился по другому склону");

    let (blocked, _) = climb(46.0, 0.0, 12.0);
    let [cx, _] = center(&blocked, 0);
    assert!(cx < 8.0, "46° по всей ширине — не пройти: x = {cx}");

    let (around, around_steps) = climb(46.0, 2.0, 10.0);
    let [cx, cy] = center(&around, 0);
    assert!(
        (cx - 16.0).abs() < 1e-6 && (cy - 6.0).abs() < 1e-6,
        "обошёл: {cx}, {cy}"
    );
    assert!(
        around_steps > easy_steps + 20,
        "обход длиннее: {around_steps} против {easy_steps}"
    );
}

// -------------------------------------------------------------------------------------------
// Столбы по высоте
// -------------------------------------------------------------------------------------------

fn collision_pairs(game: &Game) -> Vec<(u32, u32)> {
    engine::core::step::find_collision_pairs(
        &game.world,
        &mut engine::core::grid::SpatialGrid::new(),
    )
}

fn collider(x: f64, y: f64, z: f64, extra: &str) -> String {
    format!(r#"{{"position":[{x},{y},{z}],"size":[2,2],"collides":true{extra}}}"#)
}

#[test]
fn rectangles_on_different_heights_do_not_collide_and_touching_pillars_do_not_either() {
    // Столб объекта без фигуры — одна клетка вверх от основания.
    let objects = [
        collider(3.0, 3.0, 0.0, ""),
        collider(3.5, 3.5, 5.0, ""),
        collider(3.5, 3.5, 1.0, ""),
        collider(3.5, 3.5, 0.5, ""),
        collider(3.5, 3.5, -1.0, ""),
    ]
    .join(",");
    let game = load_flat(&objects, NO_RULES);
    let pairs = collision_pairs(&game);
    assert!(
        pairs.contains(&(0, 3)),
        "столбы 0–1 и 0,5–1,5 пересекаются: {pairs:?}"
    );
    assert!(!pairs.contains(&(0, 1)), "выше на пять клеток: {pairs:?}");
    assert!(
        !pairs.contains(&(0, 2)),
        "z₁ + h₁ = z₂ только касается: {pairs:?}"
    );
    assert!(
        !pairs.contains(&(0, 4)),
        "верх −1 + 1 = 0 касается основания: {pairs:?}"
    );
    assert!(pairs.contains(&(2, 3)), "0,5–1,5 и 1–2: {pairs:?}");
}

#[test]
fn a_flat_object_is_a_segment_with_both_ends_on_the_terrain_it_lies_on() {
    let slope = terrain_text(
        20,
        12,
        None,
        |x, _| if x < 3.0 { 0.0 } else { (x - 3.0).min(2.0) },
    );
    let flat = r##"{"position":[3,3],"size":[2,1],"color":"#aa8844","collides":true}"##;
    // Плоский объект на склоне занимает от 0 до 1 (от x = 3 до 5, высота x − 3 → 0..2); тут ровно 0–2.
    let above = collider(3.5, 3.0, 2.0, "");
    let top_hit = collider(3.5, 3.0, 1.9, "");
    let game = load_terrain(&slope, &format!("{flat},{above}"), NO_RULES);
    let pairs = collision_pairs(&game);
    assert!(
        pairs.contains(&(0, 1)),
        "верхний конец отрезка включён: {pairs:?}"
    );
    let game = load_terrain(&slope, &format!("{flat},{top_hit}"), NO_RULES);
    assert!(collision_pairs(&game).contains(&(0, 1)));
    let clear = collider(3.5, 3.0, 2.01, "");
    let game = load_terrain(&slope, &format!("{flat},{clear}"), NO_RULES);
    assert!(!collision_pairs(&game).contains(&(0, 1)), "выше отрезка");
}

const BALL_SHIFT: &str = r##"{"rules":[
    {"kind":"check","for":{"has":["hero"]},
     "do":[["shift",{"group":{"has":["hero"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                     "if_blocked":[["give","blocked",{"has":["hero"]}]]}]]}
]}"##;

#[test]
fn blocked_by_counts_the_pillars_of_the_group_and_of_the_blockers() {
    let run = |wall_z: f64| {
        let objects = format!(
            r##"{{"name":"ball","position":[3,8,0],"size":[1,1],"shape":"box","color":"#ff0000","hero":true}},
                {{"name":"wall","position":[4,8,{wall_z}],"size":[1,1],"shape":"box","color":"#888888","wall":true}}"##
        );
        let mut game = load_flat(&objects, BALL_SHIFT);
        step(&mut game, 1);
        let blocked = game.properties.resolve("blocked").unwrap();
        (
            game.world.flag(0, blocked),
            game.world.vec2(0, property::POSITION).unwrap(),
        )
    };
    assert_eq!(
        run(0.0),
        (true, [3.0, 8.0]),
        "стена на той же высоте преграждает"
    );
    assert_eq!(run(5.0), (false, [4.0, 8.0]), "стена выше столба не мешает");
    assert_eq!(
        run(1.0),
        (false, [4.0, 8.0]),
        "стена стоит на верху шара: столбы касаются"
    );
}

#[test]
fn a_blocked_shift_puts_the_group_back_on_its_old_height_too() {
    let terrain = terrain_text(20, 12, None, |x, _| if x >= 4.0 { 2.0 } else { 0.0 });
    let objects = r##"{"name":"ball","position":[3,8],"size":[0.4,0.4],"shape":"box","color":"#ff0000","hero":true},
            {"name":"wall","position":[3.9,8],"size":[1,1],"shape":"box","color":"#888888","wall":true}"##;
    let mut game = load_terrain(&terrain, objects, BALL_SHIFT);
    near(z_of(&game, 0), 0.0, "внизу");
    step(&mut game, 1);
    assert_eq!(game.world.vec2(0, property::POSITION), Some([3.0, 8.0]));
    near(
        z_of(&game, 0),
        0.0,
        "после отмены сдвига — на прежнем основании",
    );
}

#[test]
fn random_cell_takes_the_cell_when_the_pillar_of_the_newcomer_misses_the_occupant() {
    let rules = r##"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":9,"of":{"has":["spawned"]}}},
         "where":"random_cell","template":{"spawned":true,"size":[1,1],"collides":true}}
    ]}"##;
    let run = |wall_z: f64| {
        let scene = format!(
            r##"{{"objects":[{{"position":[-0.5,1.25,{wall_z}],"size":[4,0.5],"wall":true,"collides":true}}]}}"##
        );
        let (mut game, _s, _w) = load_full(3, 3, true, None, &scene, rules, false).unwrap();
        step(&mut game, 20);
        let spawned = game.properties.resolve("spawned").unwrap();
        game.world
            .ids()
            .filter(|&id| game.world.flag(id, spawned))
            .count()
    };
    assert_eq!(
        run(0.0),
        6,
        "стена на той же высоте занимает среднюю строку"
    );
    assert_eq!(run(5.0), 9, "стена на пять клеток выше не занимает ничего");
}

#[test]
fn an_object_created_at_its_parent_sits_as_if_it_had_moved_from_the_parents_height() {
    // Навес с низом на высоте 3 накрывает место, где стоит родитель; ребёнок, созданный в этом месте,
    // с высоты родителя (0) на навес не встаёт.
    let objects = r##"{"name":"mover","position":[5,3],"size":[1,1],"shape":"box","color":"#ff0000","velocity":[1,0],"hero":true},
        {"name":"roof","position":[4,2,3],"size":[4,3],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["hero"]}},
        {"kind":"spawn","when":{"after_move_of":{"has":["hero"]}},
         "where":"at_parent","template":{"spawned":true,"size":[0.5,0.5],"shape":"box","color":"#00ff00"}}
    ]}"##;
    let mut game = load_terrain(&terrain_text(20, 12, None, |_, _| 0.0), objects, rules);
    near(
        z_of(&game, 0),
        3.3,
        "родитель без z в данных встал на навес",
    );
    game.world.set_base_z(0, 0.0);
    step(&mut game, 1);
    let spawned = game.properties.resolve("spawned").unwrap();
    let child = game
        .world
        .ids()
        .find(|&id| game.world.flag(id, spawned))
        .expect("создан");
    near(
        z_of(&game, child),
        0.0,
        "с высоты родителя на навес не встаёт",
    );
}

#[test]
fn a_cell_position_in_a_spawn_sits_like_an_object_without_z_in_the_data() {
    let rules = r##"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["spawned"]}}},
         "where":{"at":[5,3]},"template":{"spawned":true,"size":[0.5,0.5],"shape":"box","color":"#00ff00"}}
    ]}"##;
    let roof = r##"{"name":"roof","position":[4,2,3],"size":[4,3],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    let mut game = load_terrain(&terrain_text(20, 12, None, |_, _| 0.0), roof, rules);
    step(&mut game, 1);
    let spawned = game.properties.resolve("spawned").unwrap();
    let child = game
        .world
        .ids()
        .find(|&id| game.world.flag(id, spawned))
        .expect("создан");
    near(z_of(&game, child), 3.3, "как без z в данных: на навес");
}

// -------------------------------------------------------------------------------------------
// Лестница, просвет, скорость
// -------------------------------------------------------------------------------------------

/// Три ступени по `rise` высотой одна над другой, потом площадка; ступени и площадка — настилы с `z`
/// в данных, идут вдоль `x` в полосе `y` от 4 до 8.
fn stairs(rise: f64) -> String {
    let step = |i: usize| {
        format!(
            r##"{{"name":"step{i}","position":[{},4,{}],"size":[1,4],"shape":"box","height":{rise},"color":"#886644","deck":true}}"##,
            6 + i,
            rise * i as f64
        )
    };
    let platform = format!(
        r##"{{"name":"platform","position":[9,4,{}],"size":[5,4],"shape":"box","height":{rise},"color":"#aa8855","deck":true}}"##,
        rise * 2.0
    );
    format!("{},{},{},{platform}", step(0), step(1), step(2))
}

fn climb(rise: f64) -> (Game, Vec<f64>) {
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let mut game = load_terrain(
        &flat,
        &format!("{},{}", stairs(rise), hero_json(2.7, 5.7)),
        WALK_RULES,
    );
    let hero = 4;
    game.world.set_walk_to(hero, [11.5, 6.0], None);
    let mut heights = Vec::new();
    for _ in 0..600 {
        step(&mut game, 1);
        heights.push(z_of(&game, hero));
        if !game.world.has(hero, property::WALK_TO) {
            break;
        }
    }
    (game, heights)
}

#[test]
fn a_stair_of_steps_no_higher_than_four_tenths_is_climbed_step_by_step() {
    let (game, heights) = climb(0.4);
    let hero = 4;
    let [cx, cy] = center(&game, hero);
    assert!(
        (cx - 11.5).abs() < 1e-6 && (cy - 6.0).abs() < 1e-6,
        "поднялся: {cx}, {cy}"
    );
    near(z_of(&game, hero), 1.2, "на площадке");
    for pair in heights.windows(2) {
        assert!(pair[1] >= pair[0] - 1e-9, "вверх без спусков: {heights:?}");
        assert!(pair[1] - pair[0] < 0.4 + 1e-9, "не выше ступеньки за шаг");
    }
    for level in [0.4, 0.8, 1.2] {
        assert!(
            heights.iter().any(|z| (z - level).abs() < 1e-9),
            "стоял на {level}"
        );
    }
}

#[test]
fn a_stair_with_a_step_of_four_tenths_and_a_hair_is_not_climbed() {
    let (game, heights) = climb(0.41);
    let hero = 4;
    assert!(
        heights.iter().all(|z| z.abs() < 1e-9),
        "остался внизу: {heights:?}"
    );
    let [cx, cy] = center(&game, hero);
    let on_platform = (9.0..14.0).contains(&cx) && (4.0..8.0).contains(&cy);
    assert!(!on_platform, "на площадку не попал: {cx}, {cy}");
}

/// Ступень выше земли на волосок больше 0,4 — стена: идущий по земле не заходит в её объём ни на одном
/// шаге, хотя путь к площадке за ней ищет.
#[test]
fn a_step_a_hair_above_four_tenths_is_a_wall_the_walker_never_enters() {
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let mut game = load_terrain(
        &flat,
        &format!("{},{}", stairs(0.4005), hero_json(2.7, 5.7)),
        WALK_RULES,
    );
    let hero = 4;
    game.world.set_walk_to(hero, [11.5, 6.0], None);
    for _ in 0..600 {
        step(&mut game, 1);
        let [cx, cy] = center(&game, hero);
        let inside_first_step = cx + 0.3 > 6.0 + 1e-6
            && cx - 0.3 < 7.0 - 1e-6
            && cy + 0.3 > 4.0 + 1e-6
            && cy - 0.3 < 8.0 - 1e-6;
        assert!(
            !(inside_first_step && z_of(&game, hero) < 0.4005),
            "прошёл сквозь ступень по земле: {cx}, {cy}, z {}",
            z_of(&game, hero)
        );
        if !game.world.has(hero, property::WALK_TO) {
            break;
        }
    }
}

/// Настил поперёк всей сцены с низом на `bottom`.
fn beam(bottom: f64) -> String {
    format!(
        r##"{{"name":"beam","position":[8,0,{bottom}],"size":[2,12],"shape":"box","height":0.3,"color":"#886644","deck":true}}"##
    )
}

#[test]
fn the_hero_passes_under_a_deck_whose_clearance_equals_his_height_and_not_under_a_lower_one() {
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let run = |bottom: f64| {
        let mut game = load_terrain(
            &flat,
            &format!("{},{}", beam(bottom), hero_json(2.7, 5.7)),
            WALK_RULES,
        );
        walk(&mut game, 1, [16.0, 6.0], None, 400);
        center(&game, 1)[0]
    };
    assert!(
        (run(1.8) - 16.0).abs() < 1e-6,
        "просвет ровно в рост — проходит"
    );
    assert!(run(1.79) < 8.0, "на 0,01 ниже — нет: x = {}", run(1.79));
}

/// Долина вдоль `x` с настилом поперёк неё: посередине просвет больше роста, у краёв ниже.
fn valley_with_a_span() -> (String, String) {
    let terrain = terrain_text(20, 12, None, |x, _| 0.5 * (x - 10.0).abs());
    let deck = r##"{"name":"span","position":[4,5],"size":[12,2],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    (terrain, deck.to_string())
}

/// Ведёт героя из `(x, 2)` в `(x, 10)` поперёк настила долины; сколько шагов он провёл под настилом.
fn cross_the_valley_span(x: f64) -> u32 {
    let (terrain, deck) = valley_with_a_span();
    let objects = format!("{deck},{}", hero_json(x - 0.3, 1.7));
    let mut game = load_terrain(&terrain, &objects, WALK_RULES);
    let span = scene::ground_footprint(&game.world, 0).expect("настил");
    let bottom = z_of(&game, 0);
    near(bottom, 3.0, "низ настила — на самой высокой точке под ним");
    game.world.set_walk_to(1, [x, 10.0], None);
    let mut under = 0;
    for _ in 0..400 {
        step(&mut game, 1);
        let hero = scene::ground_footprint(&game.world, 1).expect("герой");
        if hero.overlaps(&span) {
            under += 1;
            let clearance = bottom - z_of(&game, 1);
            assert!(
                clearance >= 1.8 - 1e-6,
                "под настилом просвет не меньше роста: {clearance} на {:?}",
                center(&game, 1)
            );
        }
        if !game.world.has(1, property::WALK_TO) {
            break;
        }
    }
    let [cx, cy] = center(&game, 1);
    assert!(
        (cx - x).abs() < 1e-6 && (cy - 10.0).abs() < 1e-6,
        "дошёл: {cx}, {cy}"
    );
    under
}

#[test]
fn a_deck_is_a_wall_only_where_the_hero_can_neither_stand_on_it_nor_pass_under_it() {
    assert!(
        cross_the_valley_span(10.0) > 15,
        "прошёл под серединой настила поперёк"
    );
    assert!(
        cross_the_valley_span(6.0) > 15,
        "цель по ту сторону у края, где под настилом тесно: шёл под серединой"
    );
}

#[test]
fn walking_advances_by_the_plane_on_a_slope_and_a_stair_alike() {
    let mut game = load_terrain(
        &ramp_terrain(44.0_f64.to_radians().tan(), 0.0, 12.0),
        &hero_json(2.7, 5.7),
        WALK_RULES,
    );
    game.world.set_walk_to(0, [16.0, 6.0], None);
    let mut previous = center(&game, 0);
    let mut climbed = 0;
    for _ in 0..80 {
        step(&mut game, 1);
        let now = center(&game, 0);
        let moved = (now[0] - previous[0]).hypot(now[1] - previous[1]);
        near(moved, 8.0 / 60.0, "за шаг по плоскости — как по ровному");
        if z_of(&game, 0) > 0.5 {
            climbed += 1;
        }
        previous = now;
    }
    assert!(climbed > 5, "герой поднимался: {climbed}");
}

#[test]
fn a_scene_with_a_flat_terrain_file_walks_exactly_like_one_without_the_file() {
    let objects = format!(
        "{},{},{}",
        hero_json(2.7, 5.7),
        wall_box("a", 6.0, 3.0, 1.0, 5.0, None, ""),
        wall_box("b", 10.0, 6.0, 2.0, 1.0, None, "")
    );
    let with_file = |trivial: bool| {
        let mut game = if trivial {
            load_terrain(
                &terrain_text(20, 12, None, |_, _| 0.0),
                &objects,
                WALK_RULES,
            )
        } else {
            load_flat(&objects, WALK_RULES)
        };
        game.world.set_walk_to(0, [15.0, 6.5], None);
        let mut trail = Vec::new();
        for _ in 0..300 {
            step(&mut game, 1);
            trail.push(center(&game, 0));
        }
        trail
    };
    assert_eq!(with_file(true), with_file(false));
}

#[test]
fn a_hero_stands_on_a_plateau_and_an_unreachable_click_leads_to_the_nearest_reachable_place() {
    // Плато 4 на 4 клетки, окружённое обрывом: на него не подняться.
    let terrain = terrain_text(20, 12, None, |x, y| {
        if (8.0..12.0).contains(&x) && (4.0..8.0).contains(&y) {
            3.0
        } else {
            0.0
        }
    });
    let mut game = load_terrain(&terrain, &hero_json(2.7, 5.7), WALK_RULES);
    walk(&mut game, 0, [9.0, 6.0], None, 600);
    let [cx, cy] = center(&game, 0);
    assert!(
        (cx - 7.2).abs() < 0.05 && (cy - 6.0).abs() < 0.5,
        "герой остановился у подножия: {cx}, {cy}"
    );
    near(z_of(&game, 0), 0.0, "внизу");
}

#[test]
fn a_hero_starting_on_a_bridge_can_walk_under_it_only_by_way_of_the_ramp() {
    let mut game = gorge_game([10.0, 6.5]);
    near(z_of(&game, HERO), 0.3, "стоит на мосту");
    game.world.set_walk_to(HERO, [9.0, 6.5], Some(-2.1));
    let mut lowest_y = f64::INFINITY;
    for _ in 0..1200 {
        step(&mut game, 1);
        lowest_y = lowest_y.min(center(&game, HERO)[1]);
        if !game.world.has(HERO, property::WALK_TO) {
            break;
        }
    }
    near(z_of(&game, HERO), -2.1, "внизу на берегу");
    assert!(lowest_y < 2.0, "пришёл по спуску на западе");
}

#[test]
fn an_avoided_object_on_the_bridge_does_not_stop_the_hero_under_it_but_does_on_the_bridge() {
    let goblin = r##"{"name":"goblin","position":[9.7,6.2],"size":[0.6,0.6],"shape":"box","height":1,"color":"#00aa00","wall":true}"##;
    let objects = format!("{BRIDGE},{},{goblin}", hero_json(2.7, 6.2));
    let mut game = load_terrain(&gorge_terrain(), &objects, WALK_RULES);
    // Гоблин на мосту. Идти по мосту — упирается в него: обходит по мосту нельзя (мост шириной 3, гоблин
    // посередине; тело 0,6 — обойти можно), поэтому проверяем иначе: под мостом ему всё равно.
    walk(&mut game, 1, [9.0, 6.5], Some(-2.1), 1500);
    near(z_of(&game, 1), -2.1, "под мостом");
    let [cx, cy] = center(&game, 1);
    assert!(
        (cx - 9.0).abs() < 1e-6 && (cy - 6.5).abs() < 1e-6,
        "гоблин на мосту не мешает: {cx}, {cy}"
    );
}

// -------------------------------------------------------------------------------------------
// Камера игры
// -------------------------------------------------------------------------------------------

const WINDOW_F32: [f32; 2] = [1920.0, 1080.0];

fn following_hero_json(x: f64, y: f64) -> String {
    hero_json(x, y).replace(r#""hero":true"#, r#""hero":true,"camera_follows":true"#)
}

fn camera_z(game: &Game) -> f64 {
    game.camera_3d(WINDOW_F32)
        .expect("трёхмерная сцена")
        .target_z
}

#[test]
fn the_game_camera_rises_with_the_hero_on_a_slope_without_lag() {
    let mut game = load_terrain(
        &ramp_terrain(44.0_f64.to_radians().tan(), 0.0, 12.0),
        &following_hero_json(2.7, 5.7),
        WALK_RULES,
    );
    near(camera_z(&game), 0.0, "внизу");
    game.world.set_walk_to(0, [16.0, 6.0], None);
    let mut highest = 0.0_f64;
    for _ in 0..80 {
        step(&mut game, 1);
        near(
            camera_z(&game),
            z_of(&game, 0),
            "камера на высоте основания героя",
        );
        highest = highest.max(camera_z(&game));
    }
    assert!(highest > 1.0, "поднялась на склон: {highest}");
}

#[test]
fn a_step_of_the_ground_is_walked_by_the_camera_in_twelve_equal_steps() {
    let objects = format!(
        r##"{{"name":"plate","position":[6,4],"size":[3,4],"shape":"box","height":0.3,"color":"#886644","deck":true}},{}"##,
        following_hero_json(3.7, 5.7)
    );
    let mut game = load_terrain(
        &terrain_text(20, 12, None, |_, _| 0.0),
        &objects,
        WALK_RULES,
    );
    game.world.set_walk_to(1, [7.5, 6.0], None);
    let mut trail = Vec::new();
    for _ in 0..80 {
        step(&mut game, 1);
        trail.push((z_of(&game, 1), camera_z(&game)));
    }
    let jump = trail
        .iter()
        .position(|&(z, _)| (z - 0.3).abs() < 1e-9)
        .expect("герой встал на настил");
    for k in 0..12 {
        near(
            trail[jump + k].1,
            0.025 * (k as f64 + 1.0),
            "равномерно, по 0,025 за шаг",
        );
    }
    near(trail[jump + 12].1, 0.3, "дошла ровно за 12 шагов");
    near(trail[jump + 13].1, 0.3, "и стоит");
}

#[test]
fn the_camera_stands_on_the_hero_at_once_after_the_world_is_built_or_edited_on_pause() {
    let objects = format!(
        r##"{{"name":"plate","position":[6,4,2],"size":[3,4],"shape":"box","height":0.3,"color":"#886644","deck":true}},{}"##,
        following_hero_json(7.0, 5.7)
    );
    let mut game = load_terrain(
        &terrain_text(20, 12, None, |_, _| 0.0),
        &objects,
        WALK_RULES,
    );
    near(camera_z(&game), 2.3, "после сборки мира");
    game.world.set_position_exact(1, [12.0, 5.7], 0.0);
    game.update_camera();
    near(camera_z(&game), 0.0, "правка на паузе — сразу");
    game.world.set_position_exact(1, [7.0, 5.7], 2.3);
    step(&mut game, 1);
    game.show_scene();
    near(camera_z(&game), 2.3, "после «Стопа»");
}

#[test]
fn a_replay_seek_to_the_step_after_a_jump_leaves_the_camera_on_the_hero_at_once() {
    let objects = format!(
        r##"{{"name":"plate","position":[6,4],"size":[3,4],"shape":"box","height":0.3,"color":"#886644","deck":true}},{}"##,
        following_hero_json(3.7, 5.7).replace(r#""hero":true"#, r#""hero":true,"walk_to":[7.5,6]"#)
    );
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    let (mut game, screens, _) = load_full(
        20,
        12,
        true,
        Some(&terrain_text(20, 12, None, |_, _| 0.0)),
        &scene,
        WALK_RULES,
        true,
    )
    .expect("грузится");
    let mut state = ScreenState::new(screens.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &screens, &mut state);
    let (mut queue, mut mouse) = (UiQueue::new(), MouseState::default());
    for _ in 0..80 {
        session.step_once(
            &mut queue,
            &mut mouse,
            &mut game,
            &screens,
            &mut state,
            WINDOW_F32,
            &[],
        );
    }
    let text = session.recording_text(&game);
    let mut replay =
        PlaySession::begin_replay(&text, &mut game, &screens, &mut state).expect("запись читается");
    (1..=80)
        .find(|&target| {
            replay.seek(
                target,
                &mut queue,
                &mut mouse,
                &mut game,
                &screens,
                &mut state,
                WINDOW_F32,
                &[],
            );
            (z_of(&game, 1) - 0.3).abs() < 1e-9
        })
        .expect("герой встал на настил");
    near(z_of(&game, 1), 0.3, "герой на настиле");
    near(
        camera_z(&game),
        0.3,
        "камера после перемотки сразу на основании героя",
    );
}

// -------------------------------------------------------------------------------------------
// Луч, точка под курсором, щелчок
// -------------------------------------------------------------------------------------------

fn pixel(camera: &Camera3d, point: [f64; 3]) -> [f64; 2] {
    camera.project(point).expect("точка перед камерой")
}

#[test]
fn the_point_under_the_cursor_is_where_the_ray_first_meets_the_terrain_water_or_a_deck() {
    let game = load_terrain(
        &gorge_terrain(),
        &format!("{BRIDGE},{}", hero_json(2.7, 2.7)),
        NO_RULES,
    );
    let camera = Camera3d::looking_at([10.0, 6.5], 55.0, 12.0, WINDOW).raised(0.0);
    let hit = |point: [f64; 3]| {
        scene::pointer_hit(&game.world, &game.scene, &camera, pixel(&camera, point))
            .expect("луч вниз")
    };
    let close = |a: [f64; 3], b: [f64; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
    // Верх моста.
    assert!(
        close(hit([10.0, 6.5, 0.3]), [10.0, 6.5, 0.3]),
        "{:?}",
        hit([10.0, 6.5, 0.3])
    );
    // Плато.
    assert!(close(hit([3.0, 8.0, 0.0]), [3.0, 8.0, 0.0]));
    // Вода: луч до её глади, а не до дна.
    let water = hit([10.0, 2.0, -2.3]);
    assert!(close(water, [10.0, 2.0, -2.3]), "{water:?}");
    // Берег вне моста.
    let bank = hit([9.0, 2.0, -2.1]);
    assert!(close(bank, [9.0, 2.0, -2.1]), "{bank:?}");
}

#[test]
fn a_ray_that_passes_under_the_top_of_the_bridge_lands_on_the_bank_beneath_it() {
    let game = load_terrain(&gorge_terrain(), BRIDGE, NO_RULES);
    // Камера вровень с берегом смотрит вдоль оврага: её луч идёт под настилом.
    let camera = Camera3d::orbiting([9.0, 6.5], 0.0, 10.0, 8.0, WINDOW).raised(-2.1);
    let point = scene::pointer_hit(
        &game.world,
        &game.scene,
        &camera,
        [WINDOW[0] / 2.0, WINDOW[1] / 2.0],
    )
    .expect("луч вниз");
    assert!((point[2] + 2.1).abs() < 1e-6, "берег под мостом: {point:?}");
    assert!(
        (point[0] - 9.0).abs() < 1e-6 && (point[1] - 6.5).abs() < 1e-6,
        "{point:?}"
    );
}

#[test]
fn a_ray_past_the_scene_gives_the_unclamped_point_of_the_zero_plane() {
    let game = load_terrain(&hill_terrain(), &hero_json(2.7, 2.7), NO_RULES);
    let camera = Camera3d::looking_at([10.0, 2.0], 55.0, 12.0, WINDOW);
    let above_edge = scene::pointer_hit(&game.world, &game.scene, &camera, [WINDOW[0] / 2.0, 1.0])
        .expect("луч вниз");
    assert!(
        above_edge[1] < 0.0 && above_edge[2] == 0.0,
        "{above_edge:?}"
    );
}

fn box_at(name: &str, x: f64, y: f64, extra: &str) -> String {
    format!(
        r##"{{"name":"{name}","position":[{x},{y}],"size":[1,1],"shape":"box","height":1,"color":"#cc4444"{extra}}}"##
    )
}

#[test]
fn a_hill_hides_the_ground_and_the_shape_behind_it_from_a_click_and_an_editor_pick() {
    let objects = format!(
        "{},{}",
        box_at("behind", 9.5, 2.6, r#","on_click":[["clicked",1]]"#),
        box_at("front", 9.5, 9.0, r#","on_click":[["clicked",1]]"#)
    );
    let game = load_terrain(&hill_terrain(), &objects, NO_RULES);
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    let pick = |id: u32| {
        let z = z_of(&game, id) + 0.5;
        let p = pixel(&camera, [10.0, if id == 0 { 3.1 } else { 9.5 }, z]);
        scene::editor_target_ray(&game.world, &game.scene, &camera, p)
    };
    assert_eq!(pick(1), Some(1), "объект перед холмом выбирается");
    assert_ne!(pick(0), Some(0), "объект за холмом холм заслоняет");
    let eye = camera.eye;
    let shot = scene::on_click_target_ray_at(&game.world, &game.scene, Some(eye), [10.0, 3.1, 0.5]);
    assert_ne!(
        shot,
        Some(0),
        "щелчок по объекту за холмом не попадает в него"
    );
}

#[test]
fn a_flat_object_on_a_slope_is_picked_by_its_surface_and_not_when_a_hill_stands_in_front() {
    let terrain = terrain_text(20, 12, None, |x, _| {
        if x < 6.0 {
            0.0
        } else {
            ((x - 6.0) * 0.5).min(1.5)
        }
    });
    let flat = r##"{"name":"trail","position":[8,5],"size":[2,1],"color":"#aa8844","on_click":[["clicked",1]]}"##;
    let game = load_terrain(&terrain, flat, NO_RULES);
    let camera = Camera3d::looking_at([9.0, 5.5], 55.0, 12.0, WINDOW);
    let on_surface = [9.0, 5.5, game.world.terrain().height_at(9.0, 5.5)];
    let p = pixel(&camera, on_surface);
    assert_eq!(
        scene::editor_target_ray(&game.world, &game.scene, &camera, p),
        Some(0)
    );
    let off = pixel(
        &camera,
        [9.0, 3.0, game.world.terrain().height_at(9.0, 3.0)],
    );
    assert_eq!(
        scene::editor_target_ray(&game.world, &game.scene, &camera, off),
        None
    );
}

// -------------------------------------------------------------------------------------------
// Код и правила
// -------------------------------------------------------------------------------------------

fn load_with_code(
    camera: bool,
    terrain: Option<&str>,
    objects: &str,
    rules: &str,
    code: &str,
) -> Game {
    let text = game_json(20, 12, camera, terrain.is_some())
        .replace(r#""fonts":{}"#, r#""fonts":{},"code":"code.lua""#);
    load_game_from_texts_with_terrain(
        &text,
        PROPS,
        &format!(r#"{{"objects":[{objects}]}}"#),
        rules,
        SCREENS,
        Some(code),
        &[],
        terrain,
    )
    .unwrap_or_else(|failure| panic!("не загрузилась: {:#?}", failure.errors))
    .0
}

const RUN_HERO: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["hero"]},"do":[["run","act"]]}]}"#;

fn act(code: &str, objects: &str, terrain: Option<&str>) -> Game {
    let mut game = load_with_code(true, terrain, objects, RUN_HERO, code);
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    game
}

#[test]
fn code_reads_the_base_height_of_position_and_writes_it_exactly() {
    let plateau = hill_terrain();
    let game = act(
        "function act(obj) assert(obj.position.z == 2) obj.position.z = 5 assert(obj.position.z == 5) end",
        &hero_json(9.7, 5.7),
        Some(&plateau),
    );
    near(z_of(&game, 0), 5.0, "запись z ставит основание ровно");
}

#[test]
fn writing_x_or_y_from_code_sits_the_object_and_a_table_without_z_does_too_and_with_z_sets_exactly()
{
    let plateau = hill_terrain();
    let seat = act(
        "function act(obj) obj.position.z = 7 obj.position.x = 3 end",
        &hero_json(9.7, 5.7),
        Some(&plateau),
    );
    near(z_of(&seat, 0), 0.0, "запись x сажает на землю");
    let table = act(
        "function act(obj) obj.position.z = 7 obj.position = {x = 3, y = 3} end",
        &hero_json(9.7, 5.7),
        Some(&plateau),
    );
    near(z_of(&table, 0), 0.0, "таблица без z сажает");
    let exact = act(
        "function act(obj) obj.position = {x = 3, y = 3, z = 1.25} assert(obj.position.z == 1.25) end",
        &hero_json(9.7, 5.7),
        Some(&plateau),
    );
    near(z_of(&exact, 0), 1.25, "таблица с z — ровно");
    let copied = act(
        "function act(obj) local other = find{has = {\"wall\"}}[1] obj.position = other.position end",
        &format!(
            "{},{}",
            hero_json(9.7, 5.7),
            wall_box("w", 3.0, 3.0, 1.0, 1.0, Some(0.75), "")
        ),
        Some(&plateau),
    );
    near(z_of(&copied, 0), 0.75, "пара целиком копирует и высоту");
}

#[test]
fn walk_to_has_a_z_that_code_reads_and_writes() {
    let game = act(
        "function act(obj) assert(obj.walk_to == nil) obj.walk_to = {x = 3, y = 4, z = 1} assert(obj.walk_to.z == 1) obj.walk_to.x = 5 assert(obj.walk_to.z == 1) end",
        &hero_json(9.7, 5.7),
        Some(&hill_terrain()),
    );
    assert_eq!(game.world.vec2(0, property::WALK_TO), Some([5.0, 4.0]));
    assert_eq!(game.world.walk_to_z(0), Some(1.0));
    let plain = act(
        "function act(obj) obj.walk_to = {x = 3, y = 4} assert(obj.walk_to.z == nil) end",
        &hero_json(9.7, 5.7),
        Some(&hill_terrain()),
    );
    assert_eq!(plain.world.walk_to_z(0), None);
}

#[test]
fn z_is_nil_in_a_flat_scene_and_writing_it_stops_the_game_with_a_code_error() {
    let object = r##"{"position":[3,3],"size":[1,1],"hero":true}"##;
    let mut game = load_with_code(
        false,
        None,
        object,
        RUN_HERO,
        "function act(obj) assert(obj.position.z == nil) end",
    );
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let mut bad = load_with_code(
        false,
        None,
        object,
        RUN_HERO,
        "function act(obj) obj.position.z = 2 end",
    );
    step(&mut bad, 1);
    let error = bad
        .code_error()
        .expect("запись z в плоской сцене — ошибка кода");
    assert!(error.message.contains("z"), "{}", error.message);
    let mut table = load_with_code(
        false,
        None,
        object,
        RUN_HERO,
        "function act(obj) obj.position = {x = 1, y = 1, z = 1} end",
    );
    step(&mut table, 1);
    assert!(table.code_error().is_some());
}

#[test]
fn a_deck_flag_is_written_by_code_and_a_bad_height_is_a_code_error() {
    let mut game = load_with_code(
        true,
        None,
        r##"{"position":[3,3],"size":[3,1],"color":"#886644","hero":true}"##,
        RUN_HERO,
        "function act(obj) obj.deck = true end",
    );
    step(&mut game, 1);
    assert!(game.world.flag(0, property::DECK));
    let mut bad = load_with_code(
        true,
        None,
        r##"{"position":[3,3],"size":[1,1],"hero":true}"##,
        RUN_HERO,
        "function act(obj) obj.position.z = 'high' end",
    );
    step(&mut bad, 1);
    assert!(bad.code_error().is_some());
}

#[test]
fn a_rule_sets_position_with_three_numbers_exactly_and_with_two_by_sitting() {
    let rules = |value: &str| {
        format!(
            r#"{{"rules":[{{"kind":"check","for":{{"has":["hero"]}},"do":[["set","position",{value}]]}}]}}"#
        )
    };
    let run = |value: &str| {
        let mut game = load_terrain(&hill_terrain(), &hero_json(2.7, 2.7), &rules(value));
        step(&mut game, 1);
        game
    };
    near(z_of(&run("[10, 6, 6.5]"), 0), 6.5, "три числа — ровно");
    near(
        z_of(&run("[10, 6]"), 0),
        2.0,
        "два числа — посадка на плато",
    );
}

// -------------------------------------------------------------------------------------------
// Курсор, щелчок и запись партии
// -------------------------------------------------------------------------------------------

use engine::core::screens::ScreenState;
use engine::data::recording::{self, ReplayEventKind};
use engine::data::session::PlaySession;

fn click(game: &mut Game, point: [f64; 3], eye: [f64; 3]) {
    game.set_cursor_point(point, Some(eye));
    game.key_down("MouseLeft");
    let snapshot = game.take_input_snapshot();
    game.step(snapshot);
}

#[test]
fn a_click_writes_walk_to_with_the_height_of_the_point_under_the_cursor() {
    let field = r##"{"name":"field","position":[0,0],"size":[20,12],"color":"#3b6e2c","on_click":[["walk_to","cursor"]]}"##;
    let hero = hero_json(2.0, 2.0);
    let mut game = load_terrain(&hill_terrain(), &format!("{field},{hero}"), NO_RULES);
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    click(&mut game, [10.0, 6.0, 2.0], camera.eye);
    assert_eq!(game.world.vec2(0, property::WALK_TO), Some([10.0, 6.0]));
    assert_eq!(game.world.walk_to_z(0), Some(2.0));
}

#[test]
fn follow_mouse_puts_the_middle_under_the_cursor_and_the_base_exactly_on_the_points_height() {
    let puck = r##"{"name":"puck","position":[2,2],"size":[1,1],"shape":"box","color":"#cc4444","follow_mouse":"xy"}"##;
    let mut game = load_flat(puck, NO_RULES);
    game.set_cursor_point([6.5, 4.5, 1.3], None);
    let snapshot = game.take_input_snapshot();
    game.step(snapshot);
    assert_eq!(game.world.vec2(0, property::POSITION), Some([6.0, 4.0]));
    near(
        z_of(&game, 0),
        1.3,
        "основание — на высоте точки, а не на земле",
    );
    game.step(StepInput::empty());
    near(z_of(&game, 0), 1.3, "и держится, пока объект не сдвинется");
}

#[test]
fn a_cursor_beyond_the_scene_is_clamped_and_takes_the_top_surface_there() {
    let terrain = terrain_text(20, 12, None, |x, _| if x < 4.0 { 1.5 } else { 0.0 });
    let puck = r##"{"name":"puck","position":[6,2],"size":[1,1],"shape":"box","color":"#cc4444","follow_mouse":"xy"}"##;
    let mut game = load_terrain(&terrain, puck, NO_RULES);
    game.set_cursor_point([-3.0, 5.0, 0.0], None);
    let snapshot = game.take_input_snapshot();
    game.step(snapshot);
    assert_eq!(game.world.vec2(0, property::POSITION), Some([0.0, 4.5]));
    near(z_of(&game, 0), 1.5, "верхняя поверхность в прижатой точке");
}

#[test]
fn the_recording_carries_the_height_of_the_point_beside_the_eye_and_a_short_point_reads_as_ground()
{
    let (mut game, screens, _warnings) = load_game_from_texts_with_terrain(
        &game_json(20, 12, true, true),
        PROPS,
        &format!(r#"{{"objects":[{}]}}"#, hero_json(2.7, 2.7)),
        NO_RULES,
        SCREENS,
        None,
        &[],
        Some(&hill_terrain()),
    )
    .expect("грузится");
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    let mut state = ScreenState::new(screens.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &screens, &mut state);
    game.set_cursor_point([10.0, 6.0, 2.0], Some(camera.eye));
    session.record_cursor(&game);
    let text = session.recording_text(&game);
    assert!(text.contains(r#""cursor":[10.0,6.0,2.0]"#), "{text}");
    let recorded = recording::parse(&text).expect("запись разбирается");
    let ReplayEventKind::Cursor(point, Some(eye)) = recorded.events[0].kind else {
        panic!("{:?}", recorded.events[0].kind);
    };
    assert_eq!(point, [10.0, 6.0, 2.0]);
    assert_eq!(eye, camera.eye);

    let old = r#"{"format":1,"steps":2,"events":[{"step":0,"cursor":[10,6],"eye":[10,20,12]}]}"#;
    let ReplayEventKind::Cursor(point, _) =
        recording::parse(old).expect("разбирается").events[0].kind
    else {
        panic!("курсор");
    };
    assert_eq!(point, [10.0, 6.0, 0.0], "два числа — высота 0");
}

// -------------------------------------------------------------------------------------------
// Вызовы редактора
// -------------------------------------------------------------------------------------------

#[test]
fn object_rect_lies_at_the_height_of_the_objects_base() {
    let game = load_terrain(&hill_terrain(), &hero_json(9.7, 5.7), NO_RULES);
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    let corners = scene::object_screen_corners(&game.world, 0, &camera).expect("углы");
    let footprint = scene::ground_footprint(&game.world, 0).expect("место");
    for (corner, ground) in corners.iter().zip(footprint.corners()) {
        let want = camera
            .project([ground[0], ground[1], 2.0])
            .expect("перед камерой");
        assert!((corner[0] - want[0]).abs() < 1e-9 && (corner[1] - want[1]).abs() < 1e-9);
    }
}

#[test]
fn the_editor_camera_turns_about_a_point_on_the_terrain() {
    let mut game = load_terrain(&hill_terrain(), &hero_json(2.7, 2.7), NO_RULES);
    game.set_editor_camera(engine::core::camera::EditorCamera {
        target: [10.0, 6.0],
        yaw: 30.0,
        pitch: 50.0,
        distance: 14.0,
    });
    let camera = game.editor_camera_3d(WINDOW_F32).expect("трёхмерная сцена");
    near(camera.target_z, 2.0, "высота точки — рельеф под ней");
    let centre = camera.project([10.0, 6.0, 2.0]).expect("перед камерой");
    assert!(
        (centre[0] - WINDOW[0] / 2.0).abs() < 1e-6 && (centre[1] - WINDOW[1] / 2.0).abs() < 1e-6
    );
}

fn in_window(p: [f64; 2]) -> bool {
    p[0] >= -1e-6 && p[0] <= WINDOW[0] + 1e-6 && p[1] >= -1e-6 && p[1] <= WINDOW[1] + 1e-6
}

fn fitted(game: &Game, id: Option<u32>) -> Camera3d {
    let editor = game.fit_camera(id, WINDOW_F32).expect("камера");
    let ground = game
        .world
        .terrain()
        .height_at(editor.target[0], editor.target[1]);
    editor.camera(WINDOW).raised(ground)
}

#[test]
fn fit_camera_sees_the_top_of_the_hill_and_the_bottom_of_the_gorge() {
    let hill = load_terrain(&hill_terrain(), &hero_json(2.7, 2.7), NO_RULES);
    let camera = fitted(&hill, None);
    assert!(in_window(
        camera.project([10.0, 6.0, 2.0]).expect("перед камерой")
    ));

    let gorge = load_terrain(&gorge_terrain(), &hero_json(2.7, 2.7), NO_RULES);
    let camera = fitted(&gorge, None);
    for corner in [
        [0.0, 0.0],
        [20.0, 0.0],
        [20.0, 12.0],
        [0.0, 12.0],
        [10.0, 6.0],
    ] {
        let z = gorge.world.terrain().height_at(corner[0], corner[1]);
        let p = camera
            .project([corner[0], corner[1], z])
            .expect("перед камерой");
        assert!(in_window(p), "{corner:?} → {p:?}");
    }
    assert!(in_window(
        camera.project([10.0, 6.0, -3.0]).expect("перед камерой")
    ));
}

#[test]
fn fit_camera_on_an_object_sees_its_volume_from_the_base_to_the_top() {
    let tower =
        wall_box("tower", 9.0, 5.0, 2.0, 2.0, None, r#","height":6"#).replace(r#""height":1,"#, "");
    let game = load_terrain(&hill_terrain(), &tower, NO_RULES);
    let camera = fitted(&game, Some(0));
    near(z_of(&game, 0), 2.0, "башня на плато");
    for z in [2.0, 8.0] {
        for corner in [[9.0, 5.0], [11.0, 7.0]] {
            let p = camera
                .project([corner[0], corner[1], z])
                .expect("перед камерой");
            assert!(in_window(p), "{corner:?} {z}: {p:?}");
        }
    }
}

#[test]
fn object_properties_show_three_numbers_of_position_in_a_three_dimensional_scene() {
    let mut game = load_terrain(&hill_terrain(), &hero_json(9.7, 5.7), NO_RULES);
    let json = engine::data::edit::object_properties_json(&game.world, &game.properties, &[], 0)
        .expect("жив");
    assert_eq!(json["position"], serde_json::json!([9.7, 5.7, 2.0]));
    engine::data::edit::set_property(
        &mut game.world,
        &game.properties,
        &[],
        0,
        "position",
        &serde_json::json!([3, 3, 4.5]),
    )
    .expect("три числа");
    near(z_of(&game, 0), 4.5, "три числа — ровно");
    engine::data::edit::set_property(
        &mut game.world,
        &game.properties,
        &[],
        0,
        "position",
        &serde_json::json!([10, 6]),
    )
    .expect("два числа");
    near(z_of(&game, 0), 2.0, "два числа — посадка на плато");
    let flat = load_flat(
        r##"{"position":[3,3],"size":[1,1],"color":"#ffffff"}"##,
        NO_RULES,
    );
    let json = engine::data::edit::object_properties_json(&flat.world, &flat.properties, &[], 0)
        .expect("жив");
    assert_eq!(json["position"], serde_json::json!([3.0, 3.0, 0.0]));
}

// -------------------------------------------------------------------------------------------
// Ещё о ходьбе и посадке
// -------------------------------------------------------------------------------------------

#[test]
fn an_avoided_object_blocks_only_when_its_pillar_meets_the_walkers_pillar() {
    let run = |wall_z: f64| {
        let objects = format!(
            "{},{}",
            hero_json(2.7, 5.7),
            wall_box("w", 8.0, 0.0, 1.0, 12.0, Some(wall_z), "")
        );
        let mut game = load_flat(&objects, WALK_RULES);
        walk(&mut game, 0, [15.0, 6.0], None, 500);
        center(&game, 0)[0]
    };
    assert!(run(0.0) < 8.0, "стена на той же высоте не пускает");
    assert!(
        (run(4.0) - 15.0).abs() < 1e-6,
        "стена выше головы идущего не мешает"
    );
    assert!(
        (run(-1.0) - 15.0).abs() < 1e-6,
        "стена, кончающаяся под ногами, не мешает: {}",
        run(-1.0)
    );
}

#[test]
fn the_edge_of_the_water_is_the_line_where_the_slope_meets_the_surface_and_not_a_whole_triangle() {
    // Склон вниз по x от 8 (высота 0) до 12 (−4): под уровнем −2 он уходит на x = 10.
    let terrain = terrain_text(20, 12, Some((-2.0, "#35607a")), |x, _| {
        -(x - 8.0).clamp(0.0, 4.0)
    });
    let mut game = load_terrain(&terrain, &hero_json(2.7, 5.7), WALK_RULES);
    walk(&mut game, 0, [16.0, 6.0], None, 800);
    let [cx, _] = center(&game, 0);
    assert!(
        (cx - 9.7).abs() < 1e-6,
        "тело упирается в линию воды: x = {cx}"
    );
    let dry = terrain_text(20, 12, Some((-5.0, "#35607a")), |x, _| {
        -(x - 8.0).clamp(0.0, 4.0)
    });
    let mut game = load_terrain(&dry, &hero_json(2.7, 5.7), WALK_RULES);
    walk(&mut game, 0, [16.0, 6.0], None, 800);
    near(center(&game, 0)[0], 16.0, "вода ниже дна — не мешает");
}

#[test]
fn a_turned_deck_is_a_turned_rectangle_for_the_object_that_would_stand_on_it() {
    let deck = r##"{"name":"ramp","position":[5,5],"size":[4,1],"shape":"box","height":0.3,"color":"#886644","deck":true,"rotation":45}"##;
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let pawn = wall_box("pawn", 3.0, 3.0, 0.6, 0.6, None, "");
    let mut game = load_terrain(&flat, &format!("{deck},{pawn}"), NO_RULES);
    // Середина настила (7; 5,5); пешка в углу описанного прямоугольника, но мимо повёрнутого.
    game.world.set_vec2(1, property::POSITION, [8.7, 4.2]);
    near(z_of(&game, 1), 0.0, "мимо повёрнутого настила");
    game.world.set_vec2(1, property::POSITION, [6.9, 5.2]);
    near(z_of(&game, 1), 0.3, "на повёрнутом настиле");
}

#[test]
fn turning_an_object_seats_it_like_a_shift() {
    let deck = r##"{"name":"plate","position":[5,3],"size":[3,3],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    let long = wall_box("bar", 4.4, 2.7, 0.4, 3.6, None, "");
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let mut game = load_terrain(&flat, &format!("{deck},{long}"), NO_RULES);
    near(z_of(&game, 1), 0.0, "рядом с настилом");
    let turned = Rotation::from_degrees(90.0).expect("градусы");
    game.world.set_rotation(1, property::ROTATION, turned);
    near(z_of(&game, 1), 0.3, "повёрнутая палка задела настил");
}

// -------------------------------------------------------------------------------------------
// Лестница к обрыву
// -------------------------------------------------------------------------------------------

/// Юг сцены ровный, потом обрыв круче 60° до вершины высоты 2,4, вершина, а к северному краю — пологий
/// спуск.
fn cliff_terrain() -> String {
    terrain_text(20, 12, None, |_, y| {
        if y <= 5.9 {
            0.0
        } else if y < 6.6 {
            2.4 * (y - 5.9) / 0.7
        } else if y <= 9.0 {
            2.4
        } else {
            2.4 * (12.0 - y) / 3.0
        }
    })
}

/// Восемь ступеней по 0,3 глубиной 0,7 и шириной 2 к обрыву; верх последней вровень с вершиной.
fn cliff_stairs() -> String {
    (0..8)
        .map(|i| {
            format!(
                r##"{{"name":"stair{i}","position":[9,{},{}],"size":[2,0.7],"shape":"box","height":0.3,"color":"#886644","deck":true}}"##,
                1.0 + 0.7 * i as f64,
                0.3 * i as f64
            )
        })
        .collect::<Vec<_>>()
        .join(",")
}

#[test]
fn the_hero_reaches_the_top_of_a_cliff_only_by_the_stairs_to_it() {
    let objects = format!("{},{}", cliff_stairs(), hero_json(9.7, 0.2));
    let mut game = load_terrain(&cliff_terrain(), &objects, WALK_RULES);
    let hero = 8;
    game.world.set_walk_to(hero, [10.0, 11.0], None);
    let mut heights = Vec::new();
    for _ in 0..900 {
        step(&mut game, 1);
        heights.push(z_of(&game, hero));
        if !game.world.has(hero, property::WALK_TO) {
            break;
        }
    }
    let [cx, cy] = center(&game, hero);
    assert!(
        (cx - 10.0).abs() < 1e-6 && (cy - 11.0).abs() < 1e-6,
        "дошёл: {cx}, {cy}"
    );
    for pair in heights.windows(2) {
        assert!(
            (pair[1] - pair[0]).abs() < 0.4 + 1e-6,
            "за шаг не выше ступеньки: {} → {}",
            pair[0],
            pair[1]
        );
    }
    let peak = heights.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    assert!(peak > 2.3, "поднимался по лестнице: {peak}");
}

#[test]
fn the_top_of_the_cliff_is_out_of_reach_without_the_stairs_and_the_hero_stops_at_its_foot() {
    let mut game = load_terrain(&cliff_terrain(), &hero_json(9.7, 2.7), WALK_RULES);
    walk(&mut game, 0, [10.0, 11.0], None, 900);
    let [_, cy] = center(&game, 0);
    assert!(cy < 5.9, "у подножия обрыва: y = {cy}");
    near(z_of(&game, 0), 0.0, "внизу");
}

// -------------------------------------------------------------------------------------------
// Лестница к обрыву из узких, повёрнутых и сдвинутых ступеней
// -------------------------------------------------------------------------------------------

/// Сколько вдоль лестницы, идущей под углом `angle` (градусы), занимает тело героя 0,6 × 0,6, стоящее
/// по осям сцены.
fn body_span(angle: f64) -> f64 {
    let radians = angle.to_radians();
    0.6 * (radians.cos().abs() + radians.sin().abs())
}

const FLIGHT_STEPS: usize = 8;
/// Высота вершины обрыва: восемь ступеней по 0,3.
const FLIGHT_TOP: f64 = 2.4;

/// Восемь настилов-ступеней по 0,3 выше друг друга под углом `angle` к оси `x`; верх последней вровень с
/// вершиной обрыва, что поперёк оси поднимается на те же 2,4 и дальше ровна. Ступени глубиной `depth`
/// вдоль оси идут с шагом `spacing` между серединами; `shift` — на сколько вся лестница сдвинута вбок.
struct Flight {
    angle: f64,
    width: f64,
    depth: f64,
    spacing: f64,
    shift: f64,
}

impl Flight {
    fn axis(&self) -> [f64; 2] {
        let radians = self.angle.to_radians();
        [radians.cos(), radians.sin()]
    }

    /// Точка сцены: `along` от середины нижней ступени вдоль оси, `aside` от оси вбок.
    fn at(&self, along: f64, aside: f64) -> [f64; 2] {
        let axis = self.axis();
        [
            3.5 + along * axis[0] - aside * axis[1],
            3.0 + along * axis[1] + aside * axis[0],
        ]
    }

    fn last_step(&self) -> f64 {
        self.spacing * (FLIGHT_STEPS - 1) as f64
    }

    fn terrain(&self) -> String {
        let axis = self.axis();
        let edge = self.last_step() - 0.2;
        terrain_text(20, 12, None, |x, y| {
            let along = (x - 3.5) * axis[0] + (y - 3.0) * axis[1];
            ((along - (edge - 0.7)) / 0.7).clamp(0.0, 1.0) * FLIGHT_TOP
        })
    }

    /// Ступени и герой у подножия, на `hero_aside` от оси.
    fn objects(&self, hero_aside: f64) -> String {
        let mut objects: Vec<String> = (0..FLIGHT_STEPS)
            .map(|i| {
                let middle = self.at(self.spacing * i as f64, self.shift);
                let rotation = if self.angle == 0.0 {
                    String::new()
                } else {
                    format!(r#","rotation":{}"#, self.angle)
                };
                format!(
                    r##"{{"name":"flight{i}","position":[{},{},{}],"size":[{},{}],"shape":"box","height":0.3,"color":"#886644","deck":true{rotation}}}"##,
                    middle[0] - self.depth / 2.0,
                    middle[1] - self.width / 2.0,
                    0.3 * i as f64,
                    self.depth,
                    self.width
                )
            })
            .collect();
        let foot = self.at(-1.3, hero_aside);
        objects.push(hero_json(foot[0] - 0.3, foot[1] - 0.3));
        objects.join(",")
    }
}

fn polyline_length(from: [f64; 2], points: &[[f64; 2]]) -> f64 {
    let mut previous = from;
    points
        .iter()
        .map(|&point| {
            let length =
                ((point[0] - previous[0]).powi(2) + (point[1] - previous[1]).powi(2)).sqrt();
            previous = point;
            length
        })
        .sum()
}

fn arrives(path: &[[f64; 2]], goal: [f64; 2]) -> bool {
    path.last()
        .is_some_and(|end| ((end[0] - goal[0]).powi(2) + (end[1] - goal[1]).powi(2)).sqrt() < 1e-6)
}

/// Герой у подножия лестницы идёт на точку за её последней ступенью (`goal_along` от середины ступени):
/// путь достигает цели, не длиннее ломаной через середины ступеней больше чем на 3 %, посадка по правилу
/// сдвига ни разу не роняет и не поднимает героя больше чем на ступеньку, а сам герой доходит и стоит на
/// высоте вершины.
fn assert_flight_is_climbed(flight: &Flight, goal_along: f64, what: &str) {
    let mut game = load_terrain(&flight.terrain(), &flight.objects(flight.shift), WALK_RULES);
    let hero = FLIGHT_STEPS as u32;
    let start = center(&game, hero);
    let goal = flight.at(flight.last_step() + goal_along, flight.shift);
    let middles: Vec<[f64; 2]> = (0..hero)
        .map(|id| {
            scene::ground_footprint(&game.world, id)
                .expect("ступень")
                .center()
        })
        .chain([goal])
        .collect();
    let path = planned_path(&game, hero, goal, None, &mut WalkCaches::new());
    assert!(
        arrives(&path, goal),
        "{what}: до {goal:?} путь не дошёл: {path:?}"
    );
    let (found, through_middles) = (
        polyline_length(start, &path),
        polyline_length(start, &middles),
    );
    assert!(
        found <= through_middles * 1.03,
        "{what}: путь {found:.3} длиннее ломаной через середины ступеней {through_middles:.3}"
    );
    assert_eq!(first_seat_jump(&game, hero, &path), None, "{what}");
    walk(&mut game, hero, goal, None, 1500);
    let [cx, cy] = center(&game, hero);
    assert!(
        (cx - goal[0]).abs() < 1e-6 && (cy - goal[1]).abs() < 1e-6,
        "{what}: герой встал в {cx}, {cy}"
    );
    near(z_of(&game, hero), FLIGHT_TOP, what);
}

/// Лестницы с такой глубиной и шагом ступеней под углом `angle`, шириной 1,4 и 1,2 и на волос шире тела,
/// стоящие ровно и сдвинутые вбок, при цели на площадке ступени и за ней.
fn assert_flights_are_climbed(angle: f64, depth: f64, spacing: f64) {
    let widths = [1.4, 1.2, body_span(angle) + 0.05, body_span(angle) + 0.01];
    for width in widths {
        for shift in [0.0, 0.13, 0.2] {
            for goal_along in [0.1, 1.2] {
                let flight = Flight {
                    angle,
                    width,
                    depth,
                    spacing,
                    shift,
                };
                let what = format!(
                    "угол {angle}, ширина {width:.3}, глубина {depth}, шаг {spacing}, вбок {shift}, цель {goal_along}"
                );
                assert_flight_is_climbed(&flight, goal_along, &what);
            }
        }
    }
}

#[test]
fn stairs_of_steps_along_an_axis_are_climbed_however_narrow_or_moved_aside() {
    assert_flights_are_climbed(0.0, 0.75, 0.65);
    assert_flights_are_climbed(0.0, 0.9, 0.8);
    assert_flights_are_climbed(90.0, 0.75, 0.7);
}

#[test]
fn a_stair_turned_like_the_stair_of_the_rpg_game_is_climbed_however_narrow_or_moved_aside() {
    assert_flights_are_climbed(26.5651, 0.96, 0.9);
}

#[test]
fn a_stair_turned_by_forty_five_degrees_is_climbed_however_narrow_or_moved_aside() {
    assert_flights_are_climbed(45.0, 1.0, 0.95);
}

#[test]
fn a_flight_is_climbed_by_the_short_way_from_either_side_of_its_foot() {
    for aside in [-0.4, 0.4] {
        let flight = Flight {
            angle: 26.5651,
            width: 1.4,
            depth: 0.96,
            spacing: 0.9,
            shift: 0.0,
        };
        let game = load_terrain(&flight.terrain(), &flight.objects(aside), WALK_RULES);
        let hero = FLIGHT_STEPS as u32;
        let goal = flight.at(flight.last_step() + 1.2, 0.0);
        let path = planned_path(&game, hero, goal, None, &mut WalkCaches::new());
        assert!(
            arrives(&path, goal),
            "вбок на {aside} путь не дошёл: {path:?}"
        );
        let start = center(&game, hero);
        assert!(
            polyline_length(start, &path) <= polyline_length(start, &[goal]) * 1.03,
            "вбок на {aside}: путь не длиннее прямой больше чем на 3 %"
        );
    }
}

#[test]
fn steps_closer_together_than_the_body_is_long_along_the_stair_are_not_climbed() {
    // Тело 0,6 × 0,6 вдоль лестницы под 26,57° занимает 0,805: при шаге 0,78 у взошедшего на следующую
    // ступень героя сзади торчит кусок над землёй ниже ступеньки — требование 17 не даёт так встать.
    let flight = Flight {
        angle: 26.5651,
        width: 1.4,
        depth: 0.96,
        spacing: 0.78,
        shift: 0.0,
    };
    assert!(body_span(flight.angle) > flight.spacing);
    let mut game = load_terrain(&flight.terrain(), &flight.objects(0.0), WALK_RULES);
    let hero = FLIGHT_STEPS as u32;
    let goal = flight.at(flight.last_step() + 1.2, 0.0);
    walk(&mut game, hero, goal, None, 1500);
    near(z_of(&game, hero), 0.0, "остался внизу");
    let [cx, cy] = center(&game, hero);
    assert!(
        ((cx - goal[0]).powi(2) + (cy - goal[1]).powi(2)).sqrt() > 1.0,
        "до цели не дошёл: {cx}, {cy}"
    );
}

// -------------------------------------------------------------------------------------------
// Выход из стены и край настила
// -------------------------------------------------------------------------------------------

/// Земля с бугром в углу: не ровная, поэтому идут по поверхностям, а не по плоскости.
fn bumpy_flat_terrain() -> String {
    terrain_text(
        20,
        12,
        None,
        |x, y| if x > 18.0 && y > 10.0 { 0.5 } else { 0.0 },
    )
}

#[test]
fn a_hero_inside_a_wall_leaves_it_without_crossing_another_wall() {
    // Ближайший выход из большой стены — вверх, но на этом пути стоит малая; героя она не держит.
    let big = wall_box("big", 6.0, 4.0, 6.0, 4.0, None, "");
    let small = wall_box("small", 8.2, 4.5, 0.6, 0.4, None, "");
    let objects = format!("{big},{small},{}", hero_json(8.2, 5.4));
    let mut game = load_terrain(&bumpy_flat_terrain(), &objects, WALK_RULES);
    let (hero, small) = (2, 1);
    let small_place = scene::ground_footprint(&game.world, small).expect("стена");
    let hero_place = scene::ground_footprint(&game.world, hero).expect("герой");
    assert!(hero_place.overlaps(&scene::ground_footprint(&game.world, 0).expect("стена")));
    assert!(
        !hero_place.overlaps(&small_place),
        "герою малая стена не мешает"
    );
    game.world.set_walk_to(hero, [17.0, 2.0], None);
    for n in 0..300 {
        step(&mut game, 1);
        let here = scene::ground_footprint(&game.world, hero).expect("герой");
        assert!(
            !here.overlaps(&small_place),
            "шаг {n}: герой прошёл сквозь малую стену: {:?}",
            center(&game, hero)
        );
    }
    let [cx, cy] = center(&game, hero);
    assert!(
        (cx - 17.0).abs() < 1e-6 && (cy - 2.0).abs() < 1e-6,
        "дошёл: {cx}, {cy}"
    );
}

/// Меняется ли основание идущего на пути `path` больше чем на ступеньку при посадке по правилу сдвига.
fn seat_jump_along(
    game: &Game,
    hero: u32,
    goal: [f64; 2],
    z: Option<f64>,
    caches: &mut WalkCaches,
) -> Option<String> {
    let path = planned_path(game, hero, goal, z, caches);
    first_seat_jump(game, hero, &path)
}

#[test]
fn walking_along_the_edge_of_a_stair_seats_the_hero_the_same_way_at_every_point() {
    // Впритык к боку ступеней (середина героя на 0,3 от края) касание краем — не наложение: он стоит на
    // земле весь путь, а не подпрыгивает на ступень и обратно от погрешности дробных чисел.
    let objects = format!("{},{}", cliff_stairs(), hero_json(9.7, 1.05));
    let mut game = load_terrain(&cliff_terrain(), &objects, WALK_RULES);
    let hero = 8;
    near(z_of(&game, hero), 0.3, "начал на нижней ступени");
    game.world.set_walk_to(hero, [11.0, 2.5], None);
    let mut heights = vec![z_of(&game, hero)];
    for _ in 0..300 {
        step(&mut game, 1);
        heights.push(z_of(&game, hero));
        if !game.world.has(hero, property::WALK_TO) {
            break;
        }
    }
    near(center(&game, hero)[0], 11.3, "встал у бока ступеней");
    for pair in heights.windows(2) {
        assert!(
            (pair[1] - pair[0]).abs() < 0.4 + 1e-9,
            "за шаг основание меняется не больше чем на ступеньку: {heights:?}"
        );
    }
    near(z_of(&game, hero), 0.0, "стоит на земле");
}

#[test]
fn no_path_around_stairs_over_a_cliff_drops_or_lifts_the_hero_by_more_than_a_step() {
    for step_number in [0, 3, 7] {
        let start = 1.05 + 0.7 * step_number as f64;
        let objects = format!("{},{}", cliff_stairs(), hero_json(9.7, start));
        let game = load_terrain(&cliff_terrain(), &objects, WALK_RULES);
        let mut caches = WalkCaches::new();
        for z in [None, Some(0.0), Some(2.4)] {
            for column in 0..=8 {
                for row in 0..=9 {
                    let goal = [7.0 + 0.75 * column as f64, 0.5 + 0.75 * row as f64];
                    assert_eq!(
                        seat_jump_along(&game, 8, goal, z, &mut caches),
                        None,
                        "со ступени {step_number} к {goal:?}, высота {z:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn a_deck_on_the_lip_of_a_drop_is_not_crossed_on_the_ground_and_left_over_the_drop() {
    // Низкий настил на краю спуска: с ровного места на него всходят, а сойти с другого края — в яму.
    let terrain = terrain_text(20, 12, None, |x, _| -(0.5 * (x - 8.0)).clamp(0.0, 3.0));
    let plate = r##"{"name":"plate","position":[6,4],"size":[4,4],"shape":"box","height":0.3,"color":"#886644","deck":true,"rotation":10}"##;
    let objects = format!("{plate},{}", hero_json(1.7, 5.7));
    let mut game = load_terrain(&terrain, &objects, WALK_RULES);
    let hero = 1;
    assert_eq!(
        seat_jump_along(&game, hero, [14.0, 3.0], None, &mut WalkCaches::new()),
        None,
        "путь не сажает героя с настила в яму"
    );
    game.world.set_walk_to(hero, [14.0, 3.0], None);
    let mut heights = vec![z_of(&game, hero)];
    for _ in 0..600 {
        step(&mut game, 1);
        heights.push(z_of(&game, hero));
        if !game.world.has(hero, property::WALK_TO) {
            break;
        }
    }
    let [cx, cy] = center(&game, hero);
    assert!(
        (cx - 14.0).abs() < 1e-6 && (cy - 3.0).abs() < 1e-6,
        "дошёл: {cx}, {cy}"
    );
    for pair in heights.windows(2) {
        assert!(
            (pair[1] - pair[0]).abs() < 0.4 + 0.5 * 8.0 / 60.0 + 1e-9,
            "за шаг основание не падает в яму: {pair:?}"
        );
    }
}

#[test]
fn an_object_a_hairs_breadth_over_a_deck_edge_stands_on_the_ground_and_a_visible_overlap_lifts_it()
{
    let deck = r##"{"name":"plate","position":[5,3],"size":[3,3],"shape":"box","height":0.3,"color":"#886644","deck":true}"##;
    let pawn = wall_box("pawn", 8.0, 4.0, 0.6, 0.6, None, "");
    let flat = terrain_text(20, 12, None, |_, _| 0.0);
    let mut game = load_terrain(&flat, &format!("{deck},{pawn}"), NO_RULES);
    game.world
        .set_vec2(1, property::POSITION, [8.0 - 1e-7, 4.0]);
    near(z_of(&game, 1), 0.0, "на волосок над краем — ещё на земле");
    game.world
        .set_vec2(1, property::POSITION, [8.0 - 0.001, 4.0]);
    near(
        z_of(&game, 1),
        0.3,
        "на тысячную над краем — уже на настиле",
    );
}

// -------------------------------------------------------------------------------------------
// Случайные сцены: ходьба не падает и не выходит за сцену
// -------------------------------------------------------------------------------------------

/// Ломаная героя от его середины до `to` — тем поиском, что ведёт правило `walk`; `z` — высота цели.
fn planned_path(
    game: &Game,
    hero: u32,
    to: [f64; 2],
    z: Option<f64>,
    caches: &mut WalkCaches,
) -> Vec<[f64; 2]> {
    let world = &game.world;
    let decks = surface::decks(world)
        .map(|(id, place, top)| Deck {
            id,
            place,
            bottom: world.base_z(id),
            top,
        })
        .collect();
    let wall = game.properties.resolve("wall").expect("wall объявлен");
    let blockers = world
        .ids()
        .filter(|&id| id != hero && world.flag(id, wall))
        .filter_map(|id| {
            Some(Blocker {
                id,
                place: scene::ground_footprint(world, id)?,
                pillar: surface::pillar(world, id),
            })
        })
        .collect();
    let surfaces = Surfaces {
        terrain: world.terrain(),
        decks,
        blockers,
    };
    let size = world.vec2(hero, property::SIZE).expect("size");
    let walker = Walker {
        id: hero,
        center: center(game, hero),
        size,
        rotation: None,
        height: surface::body_height(world, hero),
        z: z_of(game, hero),
    };
    let goal = Goal {
        point: to,
        named_z: z,
        wanted_z: z.unwrap_or_else(|| scene::top_surface_height(world, to)),
    };
    let scene_size = (f64::from(game.scene.width), f64::from(game.scene.height));
    walk3d::plan(&surfaces, &walker, &goal, scene_size, caches)
}

/// Запас на склон между двумя соседними точками ломаной: 0,02 клетки при наклоне не круче 45°.
const SAMPLE_SLOPE: f64 = 0.021;

/// Идущий, которого сажают правилом сдвига через каждые две сотых клетки ломаной, ни разу не падает и не
/// взлетает больше чем на ступеньку. Первая беда — её место и перепад.
fn first_seat_jump(game: &Game, hero: u32, path: &[[f64; 2]]) -> Option<String> {
    let world = &game.world;
    let size = world.vec2(hero, property::SIZE).expect("size");
    let mut z = z_of(game, hero);
    let mut from = center(game, hero);
    let mut points = vec![from];
    for &next in path {
        let length = ((next[0] - from[0]).powi(2) + (next[1] - from[1]).powi(2)).sqrt();
        let parts = (length / 0.02).ceil().max(1.0) as usize;
        for k in 1..=parts {
            let t = k as f64 / parts as f64;
            points.push([
                from[0] + t * (next[0] - from[0]),
                from[1] + t * (next[1] - from[1]),
            ]);
        }
        from = next;
    }
    for point in points {
        let place = Footprint::rotated(
            [point[0] - size[0] / 2.0, point[1] - size[1] / 2.0],
            size,
            None,
        );
        let seat = surface::rest_height(world, Some(hero), &place, Some(z));
        if (seat - z).abs() > surface::STEP + SAMPLE_SLOPE {
            return Some(format!("на {point:?}: {z} → {seat}"));
        }
        z = seat;
    }
    None
}

struct Lcg(u64);

impl Lcg {
    fn next(&mut self, low: f64, high: f64) -> f64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        low + (high - low) * ((self.0 >> 33) as f64 / (1_u64 << 31) as f64)
    }
}

/// Случайные рельеф, стены и настилы, две серии: герой не выходит за сцену, его путь не роняет и не
/// подбрасывает выше ступеньки, а на любой сцене, где он начинает вне преград, он не заходит ни в одну
/// стену, кроме тех, что задевал с самого начала. Серия 6 — та, где герой, начавший внутри одной
/// стены, прежде выходил из неё сквозь другую.
#[test]
fn walking_over_random_terrain_decks_and_obstacles_stays_inside_the_scene_and_never_panics() {
    for (seed, rounds) in [(20_260_929, 60), (6, 62)] {
        walk_over_random_scenes(seed, rounds);
    }
}

fn walk_over_random_scenes(seed: u64, rounds: u32) {
    let mut random = Lcg(seed);
    for round in 0..rounds {
        let gentle = round % 2 == 1;
        let bumps: Vec<[f64; 4]> = (0..4)
            .map(|_| {
                let (width, height) = if gentle {
                    (random.next(2.5, 4.0), 1.0)
                } else {
                    (random.next(1.0, 4.0), 3.0)
                };
                [
                    random.next(0.0, 20.0),
                    random.next(0.0, 12.0),
                    width,
                    random.next(-height, height),
                ]
            })
            .collect();
        let terrain = terrain_text(
            20,
            12,
            (round % 3 == 0 && !gentle).then_some((random.next(-1.0, 1.0), "#35607a")),
            |x, y| {
                bumps
                    .iter()
                    .map(|b| {
                        b[3] * (-((x - b[0]).powi(2) + (y - b[1]).powi(2)) / (b[2] * b[2])).exp()
                    })
                    .sum()
            },
        );
        let mut objects: Vec<String> = (0..6)
            .map(|i| {
                wall_box(
                    &format!("w{i}"),
                    random.next(0.0, 18.0),
                    random.next(0.0, 10.0),
                    random.next(0.5, 3.0),
                    random.next(0.5, 3.0),
                    None,
                    &format!(r#","rotation":{}"#, random.next(0.0, 90.0)),
                )
            })
            .collect();
        for i in 0..3 {
            objects.push(format!(
                r##"{{"name":"d{i}","position":[{},{}],"size":[{},{}],"shape":"box","height":{},"color":"#886644","deck":true,"rotation":{}}}"##,
                random.next(0.0, 16.0),
                random.next(0.0, 9.0),
                random.next(1.0, 4.0),
                random.next(0.5, 3.0),
                random.next(0.1, if gentle { 0.3 } else { 0.6 }),
                random.next(0.0, 45.0),
            ));
        }
        objects.push(hero_json(random.next(1.0, 18.0), random.next(1.0, 10.0)));
        let hero = 9;
        let mut game = load_terrain(&terrain, &objects.join(","), WALK_RULES);
        let started_inside: Vec<bool> = (0..6)
            .map(|wall| {
                let hero_place = scene::ground_footprint(&game.world, hero).unwrap();
                let wall_place = scene::ground_footprint(&game.world, wall).unwrap();
                hero_place.overlaps(&wall_place)
            })
            .collect();
        let mut caches = WalkCaches::new();
        let free_start = gentle || {
            let at = center(&game, hero);
            planned_path(&game, hero, at, None, &mut caches).is_empty()
        };
        for _ in 0..3 {
            let to = [random.next(0.0, 20.0), random.next(0.0, 12.0)];
            let z = (round % 2 == 0).then(|| random.next(-2.0, 2.0));
            let planned = planned_path(&game, hero, to, z, &mut caches);
            assert_eq!(
                first_seat_jump(&game, hero, &planned),
                None,
                "серия {seed}, раунд {round}: цель {to:?} z {z:?} от {:?} z {}",
                center(&game, hero),
                z_of(&game, hero)
            );
            game.world.set_walk_to(hero, to, z);
            for _ in 0..200 {
                step(&mut game, 1);
                let [cx, cy] = center(&game, hero);
                assert!(
                    (0.29..=19.71).contains(&cx) && (0.29..=11.71).contains(&cy),
                    "серия {seed}, раунд {round}: герой вышел за сцену: {cx}, {cy}"
                );
                assert!(z_of(&game, hero).is_finite(), "серия {seed}, раунд {round}");
                if free_start {
                    let hero_place = scene::ground_footprint(&game.world, hero).unwrap();
                    for wall in (0..6).filter(|&wall| !started_inside[wall as usize]) {
                        let wall_place = scene::ground_footprint(&game.world, wall).unwrap();
                        assert!(
                            !(hero_place.overlaps(&wall_place)
                                && surface::pillar(&game.world, hero)
                                    .overlaps(&surface::pillar(&game.world, wall))),
                            "серия {seed}, раунд {round}: герой зашёл в стену {wall}: {cx}, {cy}"
                        );
                    }
                }
                if !game.world.has(hero, property::WALK_TO) {
                    break;
                }
            }
        }
    }
}

// -------------------------------------------------------------------------------------------
// Отрисовка без видеокарты
// -------------------------------------------------------------------------------------------

/// Игра 20×12 с плитками травы на всей земле и, если дан, файлом рельефа.
fn load_tiled(terrain: Option<&str>, objects: &str) -> (Game, Vec<ImageDecl>, Vec<AtlasRect>) {
    let text = game_json(20, 12, true, terrain.is_some()).replace(
        r#""fonts":{}"#,
        r#""fonts":{},"images":{"grass":{"path":"grass.png","frames":4,"columns":2}}"#,
    );
    let row = "[0,1,2,3,0,1,2,3,0,1,2,3,0,1,2,3,0,1,2,3]";
    let rows = [row; 12].join(",");
    let scene =
        format!(r#"{{"objects":[{objects}],"ground":[{{"image":"grass","cells":[{rows}]}}]}}"#);
    let (config, _) = read_entry(&text).expect("game.json разбирается");
    let verdicts = vec![(
        "grass".to_string(),
        ImageVerdict::Ok {
            width: 4,
            height: 4,
            pixels: vec![255; 64],
        },
    )];
    let (game, _screens, _warnings, images) = load_rest_with_terrain(
        &text,
        config,
        Some(PROPS),
        Some(&scene),
        Some(NO_RULES),
        Some(SCREENS),
        &[],
        &[],
        &[],
        &verdicts,
        None,
        false,
        &[],
        terrain,
    )
    .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors));
    let packed = atlas::pack(&[AtlasImage {
        width: 4,
        height: 4,
        pixels: vec![255; 64],
    }])
    .expect("влезает");
    (game, images, packed.rects)
}

fn frame_from(
    game: &Game,
    camera: &Camera3d,
    images: &[ImageDecl],
    rects: &[AtlasRect],
) -> Frame3d {
    compose_frame3d(game, camera, 0.0, images, rects)
}

fn game_frame(game: &Game, images: &[ImageDecl], rects: &[AtlasRect]) -> Frame3d {
    let camera = game.camera_3d(WINDOW_F32).expect("камера");
    frame_from(game, &camera, images, rects)
}

fn xy_area(vertices: &[SurfaceVertex]) -> f64 {
    vertices
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (t[0].position, t[1].position, t[2].position);
            let cross = f64::from(b[0] - a[0]) * f64::from(c[1] - a[1])
                - f64::from(b[1] - a[1]) * f64::from(c[0] - a[0]);
            cross.abs() / 2.0
        })
        .sum()
}

const MAT: &str = r##"{"name":"mat","position":[7.5,5],"size":[3,2],"color":"#ffffff"}"##;

#[test]
fn the_terrain_mesh_holds_the_heights_of_the_file_and_no_water_without_it() {
    let game = load_terrain(&hill_terrain(), "", NO_RULES);
    let terrain = game.world.terrain();
    let mesh = TerrainMesh::build(terrain, [0.2, 0.3, 0.4, 1.0]).expect("файл высот есть");
    assert_eq!(
        mesh.vertices.len(),
        40 * 24 * 2 * 3,
        "по два треугольника на квадрат"
    );
    assert_eq!(mesh.land, mesh.vertices.len());
    assert!(!mesh.has_water());
    let mut peak = f32::NEG_INFINITY;
    for vertex in &mesh.vertices {
        let (column, row) = (vertex.position[0] * 2.0, vertex.position[1] * 2.0);
        assert_eq!(
            (column.fract(), row.fract()),
            (0.0, 0.0),
            "вершина в точке сетки"
        );
        let file = terrain.point_height(column as usize, row as usize);
        assert_eq!(vertex.position[2], file as f32, "высота из файла");
        assert_eq!(vertex.color, [0.2, 0.3, 0.4], "земля залита фоном");
        let length = vertex.normal.iter().map(|c| c * c).sum::<f32>().sqrt();
        assert!((length - 1.0).abs() < 1e-5 && vertex.normal[2] > 0.0);
        peak = peak.max(vertex.position[2]);
    }
    assert_eq!(peak, 2.0, "плато холма");
    let again = TerrainMesh::build(terrain, [0.0; 4]).expect("файл высот есть");
    assert_ne!(mesh.id, again.id, "у каждой сетки свой номер");
}

fn water_covers(sheet: &[engine::render::relief::TerrainVertex], x: f32, y: f32) -> bool {
    sheet.chunks(3).any(|t| {
        let side =
            |a: [f32; 3], b: [f32; 3]| (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]);
        let signs = [
            side(t[0].position, t[1].position),
            side(t[1].position, t[2].position),
            side(t[2].position, t[0].position),
        ];
        signs.iter().all(|s| *s >= 0.0) || signs.iter().all(|s| *s <= 0.0)
    })
}

fn water_area(mesh: &TerrainMesh) -> f64 {
    mesh.vertices[mesh.land..]
        .chunks(3)
        .map(|t| {
            let (a, b, c) = (t[0].position, t[1].position, t[2].position);
            let cross = f64::from(b[0] - a[0]) * f64::from(c[1] - a[1])
                - f64::from(b[1] - a[1]) * f64::from(c[0] - a[0]);
            cross.abs() / 2.0
        })
        .sum()
}

#[test]
fn the_terrain_mesh_draws_water_only_over_land_below_its_level() {
    let game = load_terrain(&gorge_terrain(), "", NO_RULES);
    let terrain = game.world.terrain();
    let water = terrain.water().expect("вода в файле");
    let mesh = TerrainMesh::build(terrain, [0.0, 0.0, 0.0, 1.0]).expect("файл высот есть");
    assert!(mesh.has_water());
    let sheet = &mesh.vertices[mesh.land..];
    assert_eq!(sheet.len() % 3, 0);
    for vertex in sheet {
        assert_eq!(vertex.position[2], -2.3, "уровень воды");
        assert_eq!(vertex.normal, [0.0, 0.0, 1.0]);
        assert_eq!(
            vertex.color,
            [water.color[0], water.color[1], water.color[2]]
        );
    }
    for triangle in sheet.chunks(3) {
        let x = triangle
            .iter()
            .map(|v| f64::from(v.position[0]))
            .sum::<f64>()
            / 3.0;
        let y = triangle
            .iter()
            .map(|v| f64::from(v.position[1]))
            .sum::<f64>()
            / 3.0;
        assert!(
            terrain.height_at(x, y) < -2.3,
            "вода над землёй выше глади: {x} {y}"
        );
    }
    assert!(water_covers(sheet, 10.0, 6.0), "русло под водой");
    assert!(water_covers(sheet, 10.0, 0.1), "русло у края сцены");
    for (x, y) in [
        (2.0, 6.0),
        (19.0, 11.0),
        (8.6, 6.0),
        (11.0, 6.0),
        (6.0, 1.0),
    ] {
        assert!(
            !water_covers(sheet, x, y),
            "под берегом и плато воды нет: {x} {y}"
        );
    }

    let dry = load_terrain(&hill_terrain(), "", NO_RULES);
    let dry_mesh = TerrainMesh::build(dry.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert!(!dry_mesh.has_water(), "воды нет, пока её нет в файле");

    let sunk = load_terrain(
        &terrain_text(20, 12, Some((-1.0, "#35607a")), |_, _| 0.0),
        "",
        NO_RULES,
    );
    let sunk_mesh = TerrainMesh::build(sunk.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert!(!sunk_mesh.has_water(), "вода ниже всей земли не видна");

    let flooded = load_terrain(
        &terrain_text(20, 12, Some((3.0, "#35607a")), |_, _| 0.0),
        "",
        NO_RULES,
    );
    let flooded_mesh =
        TerrainMesh::build(flooded.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert!(
        (water_area(&flooded_mesh) - 240.0).abs() < 1e-3,
        "вода выше всей земли — на всю сцену"
    );

    let flat = load_flat("", NO_RULES);
    assert!(
        TerrainMesh::build(flat.world.terrain(), [0.0; 4]).is_none(),
        "без файла высот сетки нет — земля заливается фоном кадра"
    );
}

#[test]
fn a_ground_tile_is_the_eight_triangles_of_its_cell_with_the_picture_stretched_over_them() {
    let (game, images, rects) = load_tiled(Some(&hill_terrain()), "");
    let frame = game_frame(&game, &images, &rects);
    let tiles: Vec<_> = frame.ground.iter().filter(|r| r.object.is_none()).collect();
    assert!(!tiles.is_empty() && tiles.len() <= 240, "{}", tiles.len());
    assert_eq!(
        frame.surface.len(),
        tiles.len() * 24,
        "восемь треугольников на клетку"
    );
    let terrain = game.world.terrain();
    for (index, tile) in tiles.iter().enumerate() {
        let cell = [tile.position[0] as usize, tile.position[1] as usize];
        let mut corners = Vec::new();
        for dy in 0..2 {
            for dx in 0..2 {
                for which in 0..2 {
                    corners.extend(terrain.triangle(2 * cell[0] + dx, 2 * cell[1] + dy, which));
                }
            }
        }
        for (vertex, corner) in frame.surface[index * 24..(index + 1) * 24]
            .iter()
            .zip(&corners)
        {
            let expected = [corner[0] as f32, corner[1] as f32, corner[2] as f32];
            assert_eq!(vertex.position, expected, "клетка {cell:?}");
            let unit = [corner[0] - cell[0] as f64, corner[1] - cell[1] as f64];
            assert_eq!(
                vertex.uv,
                [
                    tile.atlas_rect.x as f32 + unit[0] as f32 * tile.atlas_rect.w as f32,
                    tile.atlas_rect.y as f32 + unit[1] as f32 * tile.atlas_rect.h as f32,
                ],
                "картинка тянется по склону"
            );
        }
    }
}

#[test]
fn without_a_terrain_file_a_tile_and_a_flat_object_are_two_flat_triangles_at_zero() {
    let (game, images, rects) = load_tiled(None, MAT);
    let frame = game_frame(&game, &images, &rects);
    assert_eq!(frame.surface.len(), frame.ground.len() * 6);
    assert!(
        frame
            .surface
            .iter()
            .all(|v| v.position[2] == 0.0 && v.normal == [0.0, 0.0, 1.0])
    );
    let mat = &frame.surface[frame.surface.len() - 6..];
    assert!(
        (xy_area(mat) - 6.0).abs() < 1e-5,
        "прямоугольник 3 × 2 целиком"
    );
}

#[test]
fn a_flat_object_on_a_slope_follows_the_triangles_of_the_terrain_without_gaps() {
    for turn in ["", r#","rotation":30"#] {
        let mat = MAT.replace('}', &format!("{turn}}}"));
        let (game, images, rects) = load_tiled(Some(&hill_terrain()), &mat);
        let frame = game_frame(&game, &images, &rects);
        let tiles = frame.ground.iter().filter(|r| r.object.is_none()).count();
        let mat_vertices = &frame.surface[tiles * 24..];
        assert!(
            mat_vertices.len() > 6,
            "прямоугольник режется по треугольникам рельефа"
        );
        let area = xy_area(mat_vertices);
        assert!((area - 6.0).abs() < 1e-4, "{turn}: площадь {area}");
        let terrain = game.world.terrain();
        for vertex in mat_vertices {
            let [x, y, z] = vertex.position.map(f64::from);
            assert!(
                (terrain.height_at(x, y) - z).abs() < 1e-4,
                "{turn}: вершина ({x}, {y}) на высоте {z}, а земля {}",
                terrain.height_at(x, y)
            );
            assert!(vertex.normal[2] > 0.0);
        }
    }
}

#[test]
fn a_flat_object_on_a_bridge_lies_flat_on_its_top() {
    let mat = r##"{"name":"mat","position":[9,6],"size":[1,1],"color":"#ffffff"}"##;
    let (game, images, rects) = load_tiled(Some(&gorge_terrain()), &format!("{BRIDGE},{mat}"));
    let frame = game_frame(&game, &images, &rects);
    let sheet = &frame.surface[frame.surface.len() - 6..];
    for vertex in sheet {
        assert!(
            (vertex.position[2] - 0.3).abs() < 1e-6,
            "верх моста: {}",
            vertex.position[2]
        );
        assert_eq!(vertex.normal, [0.0, 0.0, 1.0]);
    }
    assert!((xy_area(sheet) - 1.0).abs() < 1e-5);
}

#[test]
fn a_shape_stands_on_its_base_height() {
    let (game, images, rects) = load_tiled(Some(&hill_terrain()), &hero_json(9.7, 5.7));
    let frame = game_frame(&game, &images, &rects);
    assert_eq!(frame.shapes.len(), 1);
    assert_eq!(f64::from(frame.shapes[0].base), z_of(&game, 0));
    assert_eq!(frame.shapes[0].base, 2.0, "на плато холма");

    let (game, images, rects) = load_tiled(Some(&gorge_terrain()), BRIDGE);
    let frame = game_frame(&game, &images, &rects);
    assert_eq!(frame.shapes[0].base, 0.0);
    assert!((frame.shapes[0].height - 0.3).abs() < 1e-6);
}

fn hilly_gorge_terrain() -> String {
    terrain_text(20, 12, Some((-2.3, "#35607a")), |x, y| {
        gorge_height(x, y) + 3.0 * (-((x - 4.0).powi(2) + (y - 8.0).powi(2)) / 4.0).exp()
    })
}

/// Точки сетки рельефа с высотами и точки в середине квадратов сетки.
fn terrain_samples(terrain: &Terrain) -> Vec<[f64; 3]> {
    let mut points = Vec::new();
    for row in 0..=24 {
        for column in 0..=40 {
            let (x, y) = (f64::from(column) / 2.0, f64::from(row) / 2.0);
            points.push([x, y, terrain.height_at(x, y)]);
            let (x, y) = (x + 0.25, y + 0.25);
            points.push([x, y, terrain.height_at(x, y)]);
        }
    }
    points
}

fn on_screen(camera: &Camera3d, point: [f64; 3]) -> bool {
    camera
        .project(point)
        .is_some_and(|p| (0.0..=WINDOW[0]).contains(&p[0]) && (0.0..=WINDOW[1]).contains(&p[1]))
}

/// Случайная камера редактора над рельефом `terrain`, смотрящая на точку земли.
fn random_camera(random: &mut Lcg, terrain: &Terrain) -> Camera3d {
    let target = [random.next(3.0, 17.0), random.next(3.0, 9.0)];
    Camera3d::orbiting(
        target,
        random.next(0.0, 360.0),
        random.next(25.0, 80.0),
        random.next(8.0, 30.0),
        WINDOW,
    )
    .raised(terrain.height_at(target[0], target[1]))
}

#[test]
fn the_shadow_box_holds_the_highest_and_the_lowest_visible_point_and_the_top_of_a_tall_shape() {
    let tower = r##"{"name":"tower","position":[3.5,7.5],"size":[1,1],"shape":"cylinder","height":5,"color":"#00ff00"}"##;
    let (game, images, rects) = load_tiled(Some(&hilly_gorge_terrain()), tower);
    let terrain = game.world.terrain();
    let mut random = Lcg(11);
    for round in 0..40 {
        let camera = random_camera(&mut random, terrain);
        let frame = frame_from(&game, &camera, &images, &rects);
        let mut seen = (f64::INFINITY, f64::NEG_INFINITY);
        let mut points = terrain_samples(terrain);
        points.push([4.0, 8.0, z_of(&game, 0) + 5.0]);
        for point in points.into_iter().filter(|&p| on_screen(&camera, p)) {
            seen = (seen.0.min(point[2]), seen.1.max(point[2]));
            let clip = engine::core::math3::transform_point(&frame.light_view_proj, point);
            let (x, y, z) = (clip[0] / clip[3], clip[1] / clip[3], clip[2] / clip[3]);
            assert!(
                x.abs() <= 1.0 && y.abs() <= 1.0 && (0.0..=1.0).contains(&z),
                "раунд {round}: {point:?} → {x} {y} {z}"
            );
        }
        assert!(seen.0.is_finite(), "раунд {round}: на экране что-то есть");
    }
}

#[test]
fn the_tiles_cover_every_place_of_the_terrain_the_camera_sees_from_any_side() {
    let (game, images, rects) = load_tiled(Some(&hilly_gorge_terrain()), "");
    let terrain = game.world.terrain();
    let mut random = Lcg(23);
    for round in 0..80 {
        let camera = random_camera(&mut random, terrain);
        let frame = frame_from(&game, &camera, &images, &rects);
        let cells: HashSet<(u32, u32)> = frame
            .ground
            .iter()
            .filter(|r| r.object.is_none())
            .map(|r| (r.position[0] as u32, r.position[1] as u32))
            .collect();
        for point in terrain_samples(terrain)
            .into_iter()
            .filter(|&p| on_screen(&camera, p) && p[0] < 20.0 && p[1] < 12.0)
        {
            assert!(
                cells.contains(&(point[0] as u32, point[1] as u32)),
                "раунд {round}: клетка точки {point:?} не выложена плиткой"
            );
        }
    }
}

/// Освещённость по формуле требования 33: `(1 − shadow) + shadow × освещено × min(1, max(0, cos
/// угла нормали к солнцу) / sin высоты солнца)`; `lit` — 1, когда карта теней тени не даёт.
fn light_of(frame: &Frame3d, normal: [f32; 3], lit: f32) -> f32 {
    let cos = normal[0] * frame.sun[0] + normal[1] * frame.sun[1] + normal[2] * frame.sun[2];
    (1.0 - frame.shadow) + frame.shadow * lit * (cos.max(0.0) / frame.sun[3]).min(1.0)
}

#[test]
fn flat_ground_in_the_sun_is_fully_lit_and_a_slope_turned_from_the_sun_gets_the_shadow_color() {
    let steep_hill = terrain_text(20, 12, None, |x, y| {
        let d = (x - 10.0).abs().max((y - 6.0).abs());
        (4.0 * (2.5 - d) / 0.5).clamp(0.0, 4.0)
    });
    let (game, images, rects) = load_tiled(Some(&steep_hill), "");
    let frame = game_frame(&game, &images, &rects);
    let mesh = TerrainMesh::build(game.world.terrain(), [0.0; 4]).expect("файл высот есть");
    let (mut flat, mut turned_away, mut toward) = (0, 0, 0);
    for vertex in &mesh.vertices {
        let light = light_of(&frame, vertex.normal, 1.0);
        if vertex.normal == [0.0, 0.0, 1.0] {
            assert_eq!(light, 1.0, "ровная земля на солнце — как без рельефа");
            flat += 1;
            continue;
        }
        let cos = vertex.normal[0] * frame.sun[0]
            + vertex.normal[1] * frame.sun[1]
            + vertex.normal[2] * frame.sun[2];
        if cos <= 0.0 {
            assert!(
                (light - (1.0 - frame.shadow)).abs() < 1e-6,
                "склон от солнца — цвет × (1 − shadow)"
            );
            turned_away += 1;
        } else {
            toward += 1;
        }
    }
    assert!(
        flat > 0 && turned_away > 0 && toward > 0,
        "{flat} {turned_away} {toward}"
    );
}

fn parse_and_validate(source: &str) -> (naga::Module, naga::valid::ModuleInfo) {
    let module = naga::front::wgsl::parse_str(source).expect("WGSL разбирается");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::empty(),
    )
    .validate(&module)
    .expect("WGSL проходит проверку");
    (module, info)
}

/// Как `wgpu-hal` собирает GLSL для WebGL2: биндинг → слот из раскладки группы, `es 300`.
fn to_webgl2_glsl(
    module: &naga::Module,
    info: &naga::valid::ModuleInfo,
    stage: naga::ShaderStage,
    entry_point: &str,
    bindings: &[u32],
) -> String {
    use naga::back::glsl;
    let mut binding_map = glsl::BindingMap::default();
    let mut slots = [0u8; 3];
    for &binding in bindings {
        let kind = match binding {
            0 => 0,
            1 | 3 => 1,
            _ => 2,
        };
        binding_map.insert(naga::ResourceBinding { group: 0, binding }, slots[kind]);
        slots[kind] += 1;
    }
    let options = glsl::Options {
        version: glsl::Version::Embedded {
            version: 300,
            is_webgl: true,
        },
        writer_flags: glsl::WriterFlags::ADJUST_COORDINATE_SPACE
            | glsl::WriterFlags::FORCE_POINT_SIZE,
        binding_map,
        zero_initialize_workgroup_memory: true,
    };
    let pipeline = glsl::PipelineOptions {
        shader_stage: stage,
        entry_point: entry_point.to_string(),
        multiview: None,
    };
    let mut out = String::new();
    glsl::Writer::new(
        &mut out,
        module,
        info,
        &options,
        &pipeline,
        naga::proc::BoundsCheckPolicies::default(),
    )
    .unwrap_or_else(|e| panic!("{entry_point}: писатель GLSL не создан: {e}"))
    .write()
    .unwrap_or_else(|e| panic!("{entry_point}: GLSL не записан: {e}"));
    out
}

#[test]
fn the_terrain_shaders_validate_translate_to_glsl_es_300_and_light_the_ground_like_the_shapes() {
    let source = include_str!("../shaders/scene3d.wgsl");
    let (module, info) = parse_and_validate(source);
    let all = [0, 1, 2, 3, 4];
    for (stage, entry, bindings) in [
        (naga::ShaderStage::Vertex, "vs_shadow_terrain", &[0][..]),
        (naga::ShaderStage::Vertex, "vs_terrain", &[0][..]),
        (naga::ShaderStage::Fragment, "fs_terrain", &all[..]),
        (naga::ShaderStage::Vertex, "vs_ground", &[0][..]),
        (naga::ShaderStage::Fragment, "fs_ground", &all[..]),
        (naga::ShaderStage::Vertex, "vs_shadow", &[0][..]),
        (naga::ShaderStage::Vertex, "vs_shape", &[0][..]),
        (naga::ShaderStage::Fragment, "fs_shape", &all[..]),
    ] {
        let glsl = to_webgl2_glsl(&module, &info, stage, entry, bindings);
        assert!(glsl.starts_with("#version 300 es"), "{entry}: {glsl}");
    }
    let terrain = to_webgl2_glsl(
        &module,
        &info,
        naga::ShaderStage::Fragment,
        "fs_terrain",
        &all,
    );
    assert!(
        terrain.contains("sampler2DShadow"),
        "тень рельефа — выборка сравнением"
    );
    assert!(
        !terrain.contains("sampler2DArray"),
        "рельеф атлас не читает: одна текстура — одна выборка"
    );
    for entry in ["fs_terrain", "fs_ground", "fs_shape"] {
        let body = source
            .split_once(&format!("fn {entry}("))
            .and_then(|(_, rest)| rest.split_once("\n}"))
            .map(|(body, _)| body)
            .unwrap_or_else(|| panic!("{entry} не найден"));
        assert!(
            body.contains("lighting(in.world, normalize(in.normal))")
                || body.contains("lighting(in.world, normal)"),
            "{entry}: земля и фигуры освещаются одной функцией с нормалью поверхности"
        );
    }
}

/// Настилы нулевого размера над ямой ничего не убирают из земли под ступенькой: сборка мира с ними не
/// растёт вчетверо на каждый — шесть таких прежде грузились минуты.
#[test]
fn zero_sized_decks_over_a_pit_do_not_slow_the_world_build_down() {
    let pit = terrain_text(20, 12, None, |x, y| {
        if (8.0..=12.0).contains(&x) && (4.0..=8.0).contains(&y) {
            -3.0
        } else {
            0.0
        }
    });
    let decks: Vec<String> = (0..6)
        .map(|i| {
            format!(
                r#"{{"name":"flat_{i}","position":[{},6,0],"size":[0,0],"deck":true}}"#,
                9.0 + f64::from(i) * 0.4
            )
        })
        .collect();
    let objects = format!("{},{}", hero_json(2.0, 2.0), decks.join(","));
    let started = std::time::Instant::now();
    let game = load_terrain(&pit, &objects, WALK_RULES);
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "сборка мира с шестью настилами нулевого размера шла {elapsed:?}"
    );
    near(z_of(&game, 0), 0.0, "герой на земле");
}

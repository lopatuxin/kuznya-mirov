//! Фаза 24 — вода на разной высоте: запись `water` с морем, озёрами и реками, гладь в точках сетки,
//! вода в ходьбе и в луче мыши, ошибки и предупреждения, `terrain_heights` и `set_terrain` с таблицей
//! воды, шейдер воды и сетка воды для рисования. Игры собираются в коде теста, как в `phase17_terrain.rs`.

use engine::core::camera::Camera3d;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::scene;
use engine::core::terrain::Terrain;
use engine::core::water::Body;
use engine::data::edit::{set_terrain, terrain_heights, terrain_readings};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::load_game_from_texts_with_terrain;
use engine::render::relief::TerrainMesh;
use serde_json::{Value as Json, json};

const WINDOW: [f64; 2] = [1920.0, 1080.0];

// -------------------------------------------------------------------------------------------
// Игры из текстов
// -------------------------------------------------------------------------------------------

const PROPS: &str = r#"{"properties":{"hero":"flag"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const WALK_RULES: &str = r#"{"rules":[{"kind":"walk","for":{"has":["hero"]}}]}"#;

const WIDTH: u32 = 20;
const HEIGHT: u32 = 12;
const DENSITY: usize = 4;
const COLUMNS: usize = DENSITY * WIDTH as usize + 1;
const ROWS: usize = DENSITY * HEIGHT as usize + 1;

fn game_json() -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":{WIDTH},"height":{HEIGHT},"background":"#000000","view_height":12,"camera":{{"pitch":55}}}},
"random_seed":1,"start_screen":"main","max_objects":300,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"terrain":"terrain.json"}}}}"##
    )
}

type Loaded = Result<(Game, engine::core::screens::ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_result(terrain: &str, objects: &str, rules: &str) -> Loaded {
    let scene = format!(r#"{{"objects":[{objects}]}}"#);
    load_game_from_texts_with_terrain(
        &game_json(),
        PROPS,
        &scene,
        rules,
        SCREENS,
        None,
        &[],
        Some(terrain),
    )
}

fn load(terrain: &str, objects: &str, rules: &str) -> Game {
    load_result(terrain, objects, rules)
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
        .0
}

fn warnings_of(terrain: &str) -> Vec<String> {
    load_result(terrain, HERO_SOUTH, WALK_RULES)
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
        .2
        .iter()
        .map(|w| format!("{} → {}: {}", w.file, w.path, w.message))
        .collect()
}

fn errors_of(terrain: &str) -> Vec<String> {
    match load_result(terrain, "", WALK_RULES) {
        Ok(_) => panic!("игра должна была не загрузиться"),
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} → {}: {}", e.file, e.path, e.message))
            .collect(),
    }
}

/// Файл рельефа сцены 20×12: `water` как есть (текст JSON) и высоты `height_at` в каждой точке сетки.
fn terrain_text(water: &str, height_at: impl Fn(f64, f64) -> f64) -> String {
    let rows: Vec<String> = (0..ROWS)
        .map(|row| {
            let cells: Vec<String> = (0..COLUMNS)
                .map(|column| {
                    let value = height_at(column as f64 / 4.0, row as f64 / 4.0);
                    format!("{}", (value * 1e6).round() / 1e6)
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();
    let water = if water.is_empty() {
        String::new()
    } else {
        format!(r#""water":{water},"#)
    };
    format!(r#"{{{water}"heights":[{}]}}"#, rows.join(","))
}

/// Те же высоты, что пишет `terrain_text`, — с той же точностью файла.
fn heights_of(height_at: impl Fn(f64, f64) -> f64) -> Vec<f64> {
    (0..ROWS)
        .flat_map(|row| (0..COLUMNS).map(move |column| (column as f64 / 4.0, row as f64 / 4.0)))
        .map(|(x, y)| (height_at(x, y) * 1e6).round() / 1e6)
        .collect()
}

fn step(game: &mut Game, n: u32) {
    for _ in 0..n {
        game.step(StepInput::empty());
    }
}

fn near(actual: f64, expected: f64, what: &str) {
    assert!(
        (actual - expected).abs() < 1e-6,
        "{what}: {actual} вместо {expected}"
    );
}

/// Номер точки сетки по месту сцены.
fn point(x: f64, y: f64) -> usize {
    (y * 4.0).round() as usize * COLUMNS + (x * 4.0).round() as usize
}

fn terrain_of(game: &Game) -> &Terrain {
    game.world.terrain()
}

// -------------------------------------------------------------------------------------------
// Сцена из трёх высот: плато с озером, река сквозь порог в низину и море
// -------------------------------------------------------------------------------------------

/// Высота гладей реки по её звеньям: на плато 4,5, у порога обрыв до 1,2, в низине 1 и 0,2 у моря.
fn river_h(x: f64) -> f64 {
    let points = [(5.0, 4.6), (8.0, 4.5), (9.0, 1.2), (16.0, 1.0), (19.5, 0.2)];
    for pair in points.windows(2) {
        let ((x0, h0), (x1, h1)) = (pair[0], pair[1]);
        if (x0..=x1).contains(&x) {
            return h0 + (h1 - h0) * (x - x0) / (x1 - x0);
        }
    }
    points[0].1
}

const RIVER: &str = r#"{"points":[[5.0,4.0,4.6,1.2],[8.0,4.0,4.5,1.2],[9.0,4.0,1.2,1.2],[16.0,4.0,1.0,1.4],[19.5,4.0,0.2,1.6]]}"#;

/// Плато высоты 5 до `x = 8`, обрыв до низины 1 к `x = 10`, дальше низина уходит под уровень моря;
/// чаша озера вокруг (3, 4); русло реки вдоль y = 4 прорезано на 0,3 ниже её глади.
fn terraces(x: f64, y: f64) -> f64 {
    let base = if x <= 8.0 {
        5.0
    } else if x <= 10.0 {
        5.0 - 2.0 * (x - 8.0)
    } else {
        1.0 - 0.75 * (x - 16.0).max(0.0)
    };
    let bowl = if (x - 3.0).hypot(y - 4.0) < 2.0 {
        4.0
    } else {
        base
    };
    if (5.0..=19.5).contains(&x) && (y - 4.0).abs() <= 0.6 {
        bowl.min(river_h(x) - 0.3)
    } else {
        bowl
    }
}

fn terraces_water() -> String {
    format!(
        r##"{{"color":"#35607a","clarity":0.8,"ripple":0.6,"level":-0.5,"lakes":[{{"at":[3.0,4.0],"level":4.6}}],"rivers":[{RIVER}]}}"##
    )
}

fn terraces_game(objects: &str) -> Game {
    load(
        &terrain_text(&terraces_water(), terraces),
        objects,
        WALK_RULES,
    )
}

#[test]
fn every_body_of_water_stands_at_its_own_height() {
    let game = terraces_game("");
    let field = terrain_of(&game).water_field().expect("вода есть");
    assert_eq!(field.body(point(3.0, 4.0)), Body::Lake);
    near(
        field.surface(point(3.0, 4.0)).expect("озеро"),
        4.6,
        "озеро на плато",
    );
    assert_eq!(field.body(point(13.0, 4.0)), Body::River);
    near(
        field.surface(point(13.0, 4.0)).expect("река"),
        river_h(13.0),
        "река в низине",
    );
    assert_eq!(field.body(point(19.0, 10.0)), Body::Sea);
    near(
        field.surface(point(19.0, 10.0)).expect("море"),
        -0.5,
        "море",
    );
    assert_eq!(field.body(point(2.0, 10.0)), Body::Dry);
    assert!(terrain_of(&game).water_warnings().is_empty());
}

#[test]
fn the_surface_range_holds_the_lake_the_river_and_the_sea() {
    let game = terraces_game("");
    let (low, high) = terrain_of(&game).water_range().expect("вода есть");
    near(low, -0.5, "наименьшая гладь");
    near(high, 4.6, "наибольшая гладь");
}

// -------------------------------------------------------------------------------------------
// Старая запись воды
// -------------------------------------------------------------------------------------------

fn bowl_and_hill(x: f64, y: f64) -> f64 {
    let hill = (3.0 - (x - 14.0).hypot(y - 6.0)).max(0.0);
    let pit = -(2.5 - (x - 5.0).hypot(y - 5.0)).clamp(0.0, 1.0);
    hill + pit
}

#[test]
fn the_old_sea_record_floods_the_same_points_and_pieces_as_before() {
    let level = -0.4;
    let water = format!(r##"{{"level":{level},"color":"#3f7fd0"}}"##);
    let game = load(&terrain_text(&water, bowl_and_hill), "", WALK_RULES);
    let terrain = terrain_of(&game);
    let field = terrain.water_field().expect("вода есть");
    let ground = terrain.heights();
    for (index, &height) in ground.iter().enumerate() {
        assert_eq!(field.is_flooded(index), height < level, "точка {index}");
    }
    let old: f64 = terrain
        .pieces_below(level, [[0.0, 0.0], [f64::from(WIDTH), f64::from(HEIGHT)]])
        .iter()
        .map(|piece| area(piece))
        .sum();
    let new: f64 = terrain
        .water_polygons()
        .iter()
        .map(|piece| area_of(piece))
        .sum();
    assert!(old > 1.0, "яма есть: {old}");
    assert!((old - new).abs() < 1e-6, "{old} против {new}");
    let pieces = terrain.water_polygons();
    assert!(
        pieces
            .iter()
            .flatten()
            .all(|v| (v.position[2] - level).abs() < 1e-9 && v.depth >= -1e-9),
        "гладь моря плоская, глубина не отрицательна"
    );
}

fn area(polygon: &[[f64; 2]]) -> f64 {
    (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            a[0] * b[1] - b[0] * a[1]
        })
        .sum::<f64>()
        .abs()
        / 2.0
}

fn area_of(polygon: &[engine::core::terrain::WaterVertex]) -> f64 {
    let flat: Vec<[f64; 2]> = polygon
        .iter()
        .map(|v| [v.position[0], v.position[1]])
        .collect();
    area(&flat)
}

// -------------------------------------------------------------------------------------------
// Ход
// -------------------------------------------------------------------------------------------

const HERO_SOUTH: &str = r##"{"name":"hero","position":[12.7,9.7],"size":[0.6,0.6],"shape":"capsule","height":1.8,"color":"#ff0000","hero":true,"walk_speed":8}"##;
const BRIDGE: &str = r##"{"name":"bridge","position":[11.5,3,1],"size":[3,2],"shape":"box","height":0.2,"color":"#8a6a3a","deck":true}"##;

fn hero_center(game: &Game) -> [f64; 2] {
    let p = game.world.vec2(0, property::POSITION).expect("position");
    [p[0] + 0.3, p[1] + 0.3]
}

fn walk_to(game: &mut Game, to: [f64; 2], z: Option<f64>) {
    game.world.set_walk_to(0, to, z);
    for _ in 0..1500 {
        step(game, 1);
        if !game.world.has(0, property::WALK_TO) {
            break;
        }
    }
}

#[test]
fn the_hero_does_not_wade_into_the_river_and_does_not_get_across_without_a_bridge() {
    let mut game = terraces_game(HERO_SOUTH);
    walk_to(&mut game, [13.0, 1.5], None);
    let [_, y] = hero_center(&game);
    assert!(y > 4.0, "по ту сторону реки герой не оказался: y = {y}");
    let field = terrain_of(&game).water_field().expect("вода");
    let on = point(hero_center(&game)[0].clamp(0.0, 20.0), y.clamp(0.0, 12.0));
    assert!(!field.is_flooded(on), "герой стоит на суше");
}

#[test]
fn the_hero_crosses_the_river_by_a_bridge_over_the_water() {
    let mut game = terraces_game(&format!("{HERO_SOUTH},{BRIDGE}"));
    walk_to(&mut game, [13.0, 1.5], None);
    let [_, y] = hero_center(&game);
    assert!((y - 1.5).abs() < 1e-6, "герой дошёл по мосту: y = {y}");
}

#[test]
fn a_pool_below_the_fall_blocks_the_walk_and_a_dry_bank_does_not() {
    let game = terraces_game("");
    let blocked = terrain_of(&game).walk_blocked();
    let inside = |polygon: &Vec<[f64; 2]>, x: f64, y: f64| {
        (0..polygon.len()).all(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            (b[0] - a[0]) * (y - a[1]) - (b[1] - a[1]) * (x - a[0]) >= -1e-9
        })
    };
    assert!(
        blocked.iter().any(|piece| inside(piece, 13.0, 4.0)),
        "река закрыта для ходьбы"
    );
    assert!(
        blocked.iter().any(|piece| inside(piece, 3.0, 4.0)),
        "озеро на плато закрыто"
    );
    assert!(
        !blocked.iter().any(|piece| inside(piece, 13.0, 9.0)),
        "сухая низина открыта"
    );
}

// -------------------------------------------------------------------------------------------
// Луч мыши
// -------------------------------------------------------------------------------------------

fn pixel(camera: &Camera3d, point: [f64; 3]) -> [f64; 2] {
    camera.project(point).expect("точка перед камерой")
}

#[test]
fn the_ray_meets_the_lake_on_the_plateau_and_the_river_below_not_the_bed() {
    let game = terraces_game("");
    let camera = Camera3d::looking_at([10.0, 6.0], 55.0, 12.0, WINDOW);
    let hit = |p: [f64; 3]| {
        scene::pointer_hit(&game.world, &game.scene, &camera, pixel(&camera, p)).expect("луч вниз")
    };
    let lake = hit([3.0, 4.0, 4.6]);
    near(lake[2], 4.6, "гладь озера на плато");
    let river = hit([13.0, 4.0, river_h(13.0)]);
    near(river[2], river_h(13.0), "гладь реки в низине");
    let sea = hit([19.0, 10.0, -0.5]);
    near(sea[2], -0.5, "гладь моря");
    let bank = hit([13.0, 8.0, terraces(13.0, 8.0)]);
    near(bank[2], terraces(13.0, 8.0), "сухой берег");
}

// -------------------------------------------------------------------------------------------
// Ошибки и предупреждения
// -------------------------------------------------------------------------------------------

fn flat_text(water: &str) -> String {
    terrain_text(water, |_, _| 0.0)
}

#[test]
fn a_broken_water_is_an_error_for_each_way_it_can_break() {
    let color = r##""color":"#000000""##;
    for (water, fragments) in [
        ("5".to_string(), vec!["water", "объект"]),
        (
            format!("{{{color},\"level\":0,\"tide\":1}}"),
            vec!["water", "tide"],
        ),
        (r#"{"level":0}"#.to_string(), vec!["water", "color"]),
        (
            r#"{"color":"blue","level":0}"#.to_string(),
            vec!["water → color", "цвет"],
        ),
        (
            format!("{{{color}}}"),
            vec!["water", "level", "lakes", "rivers"],
        ),
        (
            format!("{{{color},\"lakes\":[],\"rivers\":[]}}"),
            vec!["water", "level"],
        ),
        (
            format!("{{{color},\"level\":\"low\"}}"),
            vec!["water → level", "число"],
        ),
        (
            format!("{{{color},\"level\":0,\"clarity\":0}}"),
            vec!["water → clarity", "больше нуля"],
        ),
        (
            format!("{{{color},\"level\":0,\"ripple\":-1}}"),
            vec!["water → ripple", "больше нуля"],
        ),
        (
            format!("{{{color},\"lakes\":{{}}}}"),
            vec!["water → lakes", "массив"],
        ),
        (
            format!("{{{color},\"rivers\":3}}"),
            vec!["water → rivers", "массив"],
        ),
        (
            format!("{{{color},\"lakes\":[{{\"at\":[1,1],\"level\":1,\"depth\":1}}]}}"),
            vec!["lakes[0]", "depth"],
        ),
        (
            format!("{{{color},\"lakes\":[{{\"level\":1}}]}}"),
            vec!["lakes[0]", "at"],
        ),
        (
            format!("{{{color},\"lakes\":[{{\"at\":[1,1]}}]}}"),
            vec!["lakes[0]", "level"],
        ),
        (
            format!("{{{color},\"lakes\":[{{\"at\":[1],\"level\":1}}]}}"),
            vec!["lakes[0] → at", "пара"],
        ),
        (
            format!("{{{color},\"lakes\":[{{\"at\":[50,1],\"level\":1}}]}}"),
            vec!["lakes[0] → at", "вне сцены"],
        ),
        (
            format!("{{{color},\"lakes\":[{{\"at\":[1,1],\"level\":\"x\"}}]}}"),
            vec!["lakes[0] → level", "число"],
        ),
        (
            format!("{{{color},\"rivers\":[{{\"points\":[[1,1,1,1]]}}]}}"),
            vec!["rivers[0]", "двух точек"],
        ),
        (
            format!("{{{color},\"rivers\":[{{\"points\":[[1,1,1,1],[2,2,1]]}}]}}"),
            vec!["rivers[0] → points[1]", "четыре числа"],
        ),
        (
            format!("{{{color},\"rivers\":[{{\"points\":[[1,1,1,1],[2,99,1,1]]}}]}}"),
            vec!["points[1]", "вне сцены"],
        ),
        (
            format!("{{{color},\"rivers\":[{{\"points\":[[1,1,1,1],[2,2,1,0]]}}]}}"),
            vec!["points[1][3]", "ширина"],
        ),
        (
            format!("{{{color},\"rivers\":[{{\"points\":[[1,1,1,1],[2,2,2,1]]}}]}}"),
            vec!["points[1]", "выше предыдущей"],
        ),
        (
            format!("{{{color},\"rivers\":[{{\"points\":[[1,1,1,1],[2,2,1,1]],\"flow\":1}}]}}"),
            vec!["rivers[0]", "flow"],
        ),
    ] {
        let errors = errors_of(&flat_text(&water));
        assert!(
            errors
                .iter()
                .any(|e| fragments.iter().all(|f| e.contains(f))),
            "{water}: нет ошибки со словами {fragments:?}: {errors:#?}"
        );
    }
}

#[test]
fn a_clean_water_record_with_every_field_loads_without_warnings() {
    let game = terraces_game("");
    assert!(game.world.terrain().water().is_some());
    let text = terrain_text(&terraces_water(), terraces);
    assert_eq!(warnings_of(&text), Vec::<String>::new());
}

#[test]
fn each_water_warning_names_its_lake_or_river_and_the_points_of_the_stretch() {
    let color = r##""color":"#35607a""##;
    let dry_lake = format!("{{{color},\"lakes\":[{{\"at\":[2,2],\"level\":3}}]}}");
    let warnings = warnings_of(&terrain_text(&dry_lake, |_, _| 5.0));
    assert!(
        warnings.iter().any(|w| w.contains("lakes[0]")
            && w.contains("озеро 1")
            && w.contains("воды в озере нет")),
        "{warnings:#?}"
    );
    let open_lake = format!("{{{color},\"lakes\":[{{\"at\":[2,2],\"level\":3}}]}}");
    let warnings = warnings_of(&terrain_text(&open_lake, |_, _| 0.0));
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("озеро 1") && w.contains("не замкнута")),
        "{warnings:#?}"
    );
    let dry_river = format!(
        "{{{color},\"rivers\":[{{\"points\":[[2,6,1,1],[8,6,1,1],[16,6,1,1],[18,6,1,1]]}}]}}"
    );
    let ridge = |x: f64, _: f64| if (9.0..13.0).contains(&x) { 4.0 } else { -2.0 };
    let warnings = warnings_of(&terrain_text(&dry_river, ridge));
    assert!(
        warnings.iter().any(|w| w.contains("rivers[0]")
            && w.contains("река 1")
            && w.contains("точками 2 и 3")),
        "{warnings:#?}"
    );
}

// -------------------------------------------------------------------------------------------
// terrain_heights, terrain_readings и set_terrain
// -------------------------------------------------------------------------------------------

fn parsed_water() -> Json {
    serde_json::from_str(&terraces_water()).expect("вода — JSON")
}

#[test]
fn terrain_heights_hand_back_the_water_table_with_the_fields_that_were_written() {
    let game = terraces_game("");
    let snapshot = terrain_heights(&game).expect("сцена трёхмерная");
    assert_eq!(snapshot.water, Some(parsed_water()));
    let only_sea = load(
        &terrain_text(r##"{"level":-0.5,"color":"#3f7fd0"}"##, terraces),
        "",
        WALK_RULES,
    );
    assert_eq!(
        terrain_heights(&only_sea).expect("сцена").water,
        Some(json!({"level": -0.5, "color": "#3f7fd0"})),
        "clarity и ripple не появляются, пока автор их не писал"
    );
}

#[test]
fn terrain_readings_give_the_same_table_without_a_game() {
    let text = terrain_text(&terraces_water(), terraces);
    let readings = terrain_readings(&game_json(), Some(&text), &[]).expect("прочитан");
    assert_eq!(readings.water, Some(parsed_water()));
}

#[test]
fn set_terrain_takes_the_water_table_and_puts_the_same_terrain_a_file_would() {
    let text = terrain_text(&terraces_water(), terraces);
    let from_file = load(&text, "", WALK_RULES);
    let mut game = load(&flat_text(""), "", WALK_RULES);
    set_terrain(&mut game, &heights_of(terraces), Some(&parsed_water()), &[])
        .expect("вода принята");
    assert!(
        game.world.terrain() == from_file.world.terrain(),
        "set_terrain ставит тот же рельеф, что файл"
    );
    let snapshot = terrain_heights(&game).expect("сцена");
    assert_eq!(snapshot.water, Some(parsed_water()));
}

#[test]
fn set_terrain_with_a_wrong_water_is_an_error_and_leaves_the_terrain_as_it_was() {
    let mut game = terraces_game("");
    let before = game.world.terrain().clone();
    for (water, fragments) in [
        (json!({"color": "#35607a"}), vec!["water", "level"]),
        (
            json!({"color": "blue", "level": 0}),
            vec!["water → color", "цвет"],
        ),
        (
            json!({"color": "#35607a", "level": null}),
            vec!["water → level", "число"],
        ),
        (
            json!({"color": "#35607a", "lakes": [{"at": [99, 1], "level": 1}]}),
            vec!["lakes[0] → at", "вне сцены"],
        ),
        (
            json!({"color": "#35607a", "rivers": [{"points": [[1, 1, 1, 1], [2, 2, 2, 1]]}]}),
            vec!["points[1]", "выше предыдущей"],
        ),
        (json!(7), vec!["water", "объект"]),
    ] {
        let message = set_terrain(&mut game, &heights_of(terraces), Some(&water), &[])
            .expect_err("вода не должна встать");
        assert!(
            fragments.iter().all(|f| message.contains(f)),
            "{water}: {message}"
        );
        assert!(game.world.terrain() == &before, "рельеф не тронут");
    }
}

#[test]
fn a_new_terrain_recomputes_the_water_from_the_same_records() {
    let mut game = terraces_game("");
    assert_eq!(
        terrain_of(&game)
            .water_field()
            .expect("вода")
            .body(point(13.0, 4.0)),
        Body::River
    );
    let mut raised = heights_of(terraces);
    for (index, height) in raised.iter_mut().enumerate() {
        let (column, row) = (index % COLUMNS, index / COLUMNS);
        if (50..56).contains(&column) && (14..18).contains(&row) {
            *height = 9.0;
        }
    }
    set_terrain(&mut game, &raised, Some(&parsed_water()), &[]).expect("вода принята");
    let field = terrain_of(&game).water_field().expect("вода");
    assert_eq!(
        field.body(point(13.0, 4.0)),
        Body::Dry,
        "дамба выше глади реки: вода пересчитана"
    );
}

// -------------------------------------------------------------------------------------------
// Рисование
// -------------------------------------------------------------------------------------------

#[test]
fn the_terrain_mesh_carries_the_depth_the_flow_and_the_foam_of_the_water_and_the_surface_over_the_land()
 {
    let game = terraces_game("");
    let mesh = TerrainMesh::build(game.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert!(mesh.has_water());
    let water = &mesh.vertices[mesh.land..];
    assert!(
        water.iter().all(|v| v.extra[0] >= -1e-6),
        "глубина не отрицательна"
    );
    assert!(water.iter().any(|v| v.extra[1].abs() > 0.1), "река течёт");
    assert!(
        water.iter().any(|v| v.extra[3] > 0.0),
        "на пороге есть пена"
    );
    assert!(
        water.iter().any(|v| (v.position[2] - 4.6).abs() < 1e-5),
        "озеро на плато"
    );
    let land = &mesh.vertices[..mesh.land];
    // Вдали от воды гладь — на `clarity` ниже своей земли: полоса мокрого берега там кончилась и гаснет ровно.
    let clarity = game.world.terrain().water().expect("вода").clarity() as f32;
    // Точка берега чаши озера вдали от реки: у истока реки гладь тоже 4,6, поэтому место задано.
    let shore: Vec<_> = land
        .iter()
        .filter(|v| (v.position[0] - 3.0).abs() < 1e-4 && (v.position[1] - 1.75).abs() < 1e-4)
        .collect();
    assert!(!shore.is_empty(), "точка берега есть в сетке");
    assert!(
        shore
            .iter()
            .all(|v| (v.extra[0] - 4.6).abs() < 1e-5 && v.position[2] > 4.6),
        "земля на берегу озера знает гладь озера"
    );
    assert!(
        land.iter()
            .any(|v| (v.extra[0] - (v.position[2] - clarity)).abs() < 1e-4),
        "земля вдали от воды стоит выше глади на clarity"
    );
    assert!(
        land.iter().all(|v| v.extra[0] > -100.0),
        "в сцене с водой гладь есть у каждой точки земли"
    );
}

#[test]
fn the_scene_without_water_gives_the_land_no_surface_and_no_water_mesh() {
    let game = load(&flat_text(""), "", WALK_RULES);
    let mesh = TerrainMesh::build(game.world.terrain(), [0.0; 4]).expect("файл высот есть");
    assert!(!mesh.has_water());
    assert!(mesh.vertices.iter().all(|v| v.extra[0] < -100.0));
}

// -------------------------------------------------------------------------------------------
// Луч в гладь: перебор всех треугольников против хода по квадратам
// -------------------------------------------------------------------------------------------

type Vec3 = [f64; 3];

fn sub(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: Vec3, b: Vec3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Расстояние до пересечения луча с треугольником и доли его вершин в точке (Мёллер — Трумбор).
fn brute_triangle_hit(triangle: &[Vec3; 3], origin: Vec3, direction: Vec3) -> Option<(f64, Vec3)> {
    let (e1, e2) = (sub(triangle[1], triangle[0]), sub(triangle[2], triangle[0]));
    let p = cross(direction, e2);
    let det = dot(e1, p);
    if det.abs() < 1e-14 {
        return None;
    }
    let s = sub(origin, triangle[0]);
    let u = dot(s, p) / det;
    let q = cross(s, e1);
    let v = dot(direction, q) / det;
    let t = dot(e2, q) / det;
    (u >= 0.0 && v >= 0.0 && u + v <= 1.0 && t >= 0.0).then_some((t, [1.0 - u - v, u, v]))
}

/// Первое попадание луча в гладь, перебором всех треугольников глади с водой над землёй.
fn brute_water_hit(terrain: &Terrain, origin: Vec3, direction: Vec3) -> Option<f64> {
    let [columns, rows] = terrain.squares();
    let mut best: Option<f64> = None;
    for row in 0..rows {
        for column in 0..columns {
            for which in 0..2 {
                let ground = terrain.triangle(column, row, which);
                let surfaces = terrain.triangle_surfaces(column, row, which);
                let [Some(a), Some(b), Some(c)] = surfaces else {
                    continue;
                };
                let surface = [0, 1, 2].map(|i| [ground[i][0], ground[i][1], [a, b, c][i]]);
                let Some((t, weights)) = brute_triangle_hit(&surface, origin, direction) else {
                    continue;
                };
                let depth: f64 = (0..3)
                    .map(|i| weights[i] * (surface[i][2] - ground[i][2]))
                    .sum();
                if depth > 0.0 && best.is_none_or(|b| t < b) {
                    best = Some(t);
                }
            }
        }
    }
    best
}

#[test]
fn the_ray_into_the_surface_agrees_with_a_brute_force_over_every_triangle() {
    let game = terraces_game("");
    let terrain = terrain_of(&game);
    let mut seed = 12345_u64;
    let mut random = move || {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 11) as f64 / (1_u64 << 53) as f64
    };
    let (mut hits, mut misses) = (0, 0);
    for _ in 0..1500 {
        let origin = [
            -2.0 + 24.0 * random(),
            -2.0 + 16.0 * random(),
            7.0 + 5.0 * random(),
        ];
        let direction = [
            0.9 * (random() - 0.5),
            0.9 * (random() - 0.5),
            -0.2 - random(),
        ];
        let fast = terrain.water_hit(origin, direction);
        let slow = brute_water_hit(terrain, origin, direction);
        match (fast, slow) {
            (Some(a), Some(b)) => {
                assert!(
                    (a - b).abs() < 1e-6,
                    "{origin:?} {direction:?}: {a} против {b}"
                );
                hits += 1;
            }
            (None, None) => misses += 1,
            other => panic!("{origin:?} {direction:?}: {other:?}"),
        }
    }
    assert!(
        hits > 80 && misses > 80,
        "{hits} попаданий, {misses} промахов"
    );
}

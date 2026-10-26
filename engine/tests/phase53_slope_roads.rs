//! Фаза 53 — дороги дугой по склону. Ось-дуга, профиль, поправки земли, посадка и наклон машины, щелчок по полосе
//! дороги, проверки перед запуском: сквозные проверки через загрузку игры, как `tests/phase38_roads.rs`. Раскладка
//! кусков и перекрёстков проверена в `mod tests` рядом с кодом (`core::road_kinds`).

use engine::core::camera::Camera3d;
use engine::core::game::Game;
use engine::core::input::StepInput;
use engine::core::property;
use engine::core::roads::MAX_BEND_DEGREES;
use engine::core::scene;
use engine::data::edit;
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::load_game_from_texts_with_terrain;

const WIDTH: u32 = 40;
const HEIGHT: u32 = 30;
const PROPS: &str =
    r#"{"properties":{"car":"flag","probe":"flag","length":"number","time":"number"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

type Loaded = Result<(Game, Vec<GameError>), LoadFailure>;

fn game_json(flat: bool) -> String {
    let (camera, terrain) = if flat {
        ("", "")
    } else {
        (
            r#","view_height":24,"camera":{"pitch":60}"#,
            r#","terrain":"terrain.json""#,
        )
    };
    format!(
        r##"{{"name":"T","scene":{{"width":{WIDTH},"height":{HEIGHT},"background":"#000000"{camera}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}{terrain},"code":"code.lua"}}}}"##
    )
}

/// Файл высот сцены: `height_at(x, y)` в каждой точке сетки через полклетки, с морем на `sea`, если оно названо.
fn terrain_text(sea: Option<f64>, height_at: impl Fn(f64, f64) -> f64) -> String {
    let rows: Vec<String> = (0..=2 * HEIGHT)
        .map(|row| {
            let cells: Vec<String> = (0..=2 * WIDTH)
                .map(|column| {
                    let value = height_at(f64::from(column) / 2.0, f64::from(row) / 2.0);
                    format!("{}", (value * 1e6).round() / 1e6)
                })
                .collect();
            format!("[{}]", cells.join(","))
        })
        .collect();
    let water = sea.map_or_else(String::new, |level| {
        format!(r##""water":{{"level":{level},"color":"#35607a"}},"##)
    });
    format!(r#"{{{water}"heights":[{}]}}"#, rows.join(","))
}

fn scene_of(objects: &[String]) -> String {
    format!(r#"{{"objects":[{}]}}"#, objects.join(","))
}

fn load_with(terrain: &str, objects: &[String], rules: &[&str], code: &str, flat: bool) -> Loaded {
    load_game_from_texts_with_terrain(
        &game_json(flat),
        PROPS,
        &scene_of(objects),
        &format!(r#"{{"rules":[{}]}}"#, rules.join(",")),
        SCREENS,
        Some(code),
        &[],
        (!flat).then_some(terrain),
    )
    .map(|(game, _screens, warnings)| (game, warnings))
}

fn game_of(terrain: &str, objects: &[String], rules: &[&str]) -> Game {
    load_with(terrain, objects, rules, "", false)
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors))
        .0
}

fn errors_of(result: Loaded) -> Vec<String> {
    match result {
        Err(failure) => failure
            .errors
            .iter()
            .map(|e| format!("{} ({}): {}", e.file, e.path, e.message))
            .collect(),
        Ok(_) => Vec::new(),
    }
}

fn expect_error(result: Loaded, fragment: &str) {
    let errors = errors_of(result);
    assert!(
        errors.iter().any(|e| e.contains(fragment)),
        "нет ошибки со словами \"{fragment}\": {errors:#?}"
    );
}

/// Дорога без вида: прямоугольник `position`/`size` и свойства `extra` через запятую.
fn road(position: [f64; 2], size: [f64; 2], extra: &str) -> String {
    format!(
        r##"{{"position":[{},{}],"size":[{},{}],"color":"#8a8f96","road":true{extra}}}"##,
        position[0], position[1], size[0], size[1]
    )
}

fn car(center: [f64; 2], target: [f64; 2], speed: f64) -> String {
    format!(
        r##"{{"name":"car","position":[{},{}],"size":[0.2,0.6],"shape":"box","height":0.4,"color":"#f2f2f2","car":true,"drive_speed":{speed},"drive_to":[{},{}]}}"##,
        center[0] - 0.1,
        center[1] - 0.3,
        target[0],
        target[1]
    )
}

fn drive_rule(side: &str) -> String {
    format!(r#"{{"kind":"drive","for":{{"has":["car"]}},"side":"{side}"}}"#)
}

fn step(game: &mut Game, count: usize) {
    for _ in 0..count {
        game.step(StepInput::empty());
    }
}

fn near(actual: f64, expected: f64, eps: f64) {
    assert!(
        (actual - expected).abs() <= eps,
        "ожидалось {expected} ± {eps}, вышло {actual}"
    );
}

fn center(game: &Game, id: u32) -> [f64; 2] {
    let position = game.world.vec2(id, property::POSITION).expect("position");
    let size = game.world.vec2(id, property::SIZE).expect("size");
    [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0]
}

/// Склон: высота 0,1 клетки на клетку вдоль `x`.
fn ramp(x: f64, _y: f64) -> f64 {
    0.1 * x
}

fn ramp_game(objects: &[String], rules: &[&str]) -> Game {
    game_of(&terrain_text(None, ramp), objects, rules)
}

#[test]
fn a_road_without_z_stands_by_its_ends_on_the_ground_with_one_slope() {
    let game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], "")], &[]);
    near(game.world.base_z(0), 0.4, 1e-9);
    assert_eq!(
        game.world.placed_z(0, property::POSITION),
        Some(game.world.base_z(0))
    );
    let mut game = game;
    let links = game.road_links(0).expect("дорога");
    let points = &links.axis.points;
    assert_eq!(points.len(), 2);
    near(points[0][2], 0.4, 1e-9);
    near(points[1][2], 2.4, 1e-9);
    near(links.handle[2], 1.4, 1e-9);
}

#[test]
fn a_road_with_z_in_the_data_is_level_and_leaves_the_ground_alone() {
    let level =
        r##"{"position":[4,14.5,3],"size":[20,1],"color":"#8a8f96","road":true}"##.to_string();
    let game = ramp_game(&[level], &[]);
    near(game.world.base_z(0), 3.0, 1e-9);
    let mut game = game;
    let links = game.road_links(0).expect("дорога");
    near(links.axis.points[0][2], 3.0, 1e-9);
    near(links.axis.points[1][2], 3.0, 1e-9);
    assert_eq!(
        game.world.terrain().heights(),
        game.world.terrain().ground_heights()
    );
}

#[test]
fn a_road_shifted_without_z_stands_on_the_ground_again_and_a_written_z_makes_it_level() {
    let mut game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], "")], &[]);
    game.world.set_vec2(0, property::POSITION, [10.0, 14.5]);
    near(game.world.base_z(0), 1.0, 1e-9);
    game.world.set_position_exact(0, [10.0, 14.5], 5.0);
    near(game.world.base_z(0), 5.0, 1e-9);
    game.world.set_vec2(0, property::POSITION, [6.0, 14.5]);
    near(game.world.base_z(0), 5.0, 1e-9);
    let links = game.road_links(0).expect("дорога");
    near(links.axis.points[1][2], 5.0, 1e-9);
}

/// Сдвиг группы упёрся в стену и откатился: дорога без `z` стоит концами на земле, как стояла, а ровная держит `z`.
#[test]
fn a_road_put_back_by_a_blocked_shift_keeps_standing_on_the_ground_and_a_level_one_keeps_its_z() {
    let shift = r#"{"kind":"check","for":{"has":["probe"]},
        "do":[["shift",{"group":{"has":["probe"]},"by":[1,0],"blocked_by":{"has":["car"]}}]]}"#;
    let wall = r##"{"position":[24.5,14],"size":[1,2],"shape":"box","height":1,"color":"#ffffff","car":true}"##;
    let mut game = ramp_game(
        &[
            road([4.0, 14.5], [20.0, 1.0], r#","probe":true"#),
            wall.to_string(),
        ],
        &[shift],
    );
    step(&mut game, 1);
    assert_eq!(game.world.vec2(0, property::POSITION), Some([4.0, 14.5]));
    assert!(!game.world.is_level(0), "откат сделал дорогу ровной");
    near(game.world.base_z(0), 0.4, 1e-9);

    let level =
        r##"{"position":[4,14.5,3],"size":[20,1],"color":"#8a8f96","road":true,"probe":true}"##;
    let mut game = ramp_game(&[level.to_string(), wall.to_string()], &[shift]);
    step(&mut game, 1);
    assert!(game.world.is_level(0));
    near(game.world.base_z(0), 3.0, 1e-9);
}

/// Редактор перетащил дорогу без записи `z` — концы встали на землю заново; с `z` — дорога ровная на нём.
#[test]
fn a_road_dragged_in_the_editor_without_z_stands_on_the_ground_and_with_z_is_level() {
    let mut game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], "")], &[]);
    let drag = |position: [f64; 2], z: Option<f64>| scene::ObjectTransform {
        position,
        z,
        size: [20.0, 1.0],
        height: None,
        rotation: None,
    };
    scene::transform_object(&mut game.world, 0, drag([10.0, 14.5], None));
    assert!(!game.world.is_level(0));
    near(game.world.base_z(0), 1.0, 1e-9);
    scene::transform_object(&mut game.world, 0, drag([6.0, 14.5], Some(5.0)));
    assert!(game.world.is_level(0));
    near(game.world.base_z(0), 5.0, 1e-9);
}

#[test]
fn the_ground_under_a_road_is_corrected_without_touching_the_file_heights() {
    let hill = |x: f64, y: f64| (1.5 * (1.0 - (x - 20.0).hypot(y - 14.5) / 6.0)).max(0.0);
    let text = terrain_text(None, hill);
    let game = game_of(&text, &[road([5.0, 14.0], [30.0, 2.0], "")], &[]);
    let plain = game.properties.terrain().expect("рельеф").clone();
    let terrain = game.world.terrain();
    near(terrain.height_at(20.0, 14.5), 0.0, 1e-9);
    near(terrain.height_at(20.0, 15.5), 0.0, 1e-9);
    assert!(plain.height_at(20.0, 14.5) > 1.4, "холм в файле остался");
    assert_eq!(plain.heights(), plain.ground_heights());
    assert_eq!(terrain.base_heights(), plain.base_heights());
    assert!(terrain.height_at(20.0, 20.5) > 0.0 || plain.height_at(20.0, 20.5) == 0.0);
}

#[test]
fn the_correction_follows_a_deleted_road() {
    let hill = |x: f64, y: f64| (1.5 * (1.0 - (x - 20.0).hypot(y - 14.5) / 6.0)).max(0.0);
    let text = terrain_text(None, hill);
    let mut game = game_of(&text, &[road([5.0, 14.0], [30.0, 2.0], "")], &[]);
    let plain = game.properties.terrain().expect("рельеф").clone();
    assert_ne!(game.world.terrain().heights(), plain.heights());
    game.world.delete(0);
    assert_eq!(game.world.terrain().heights(), plain.heights());
}

#[test]
fn a_bridge_with_z_does_not_correct_the_ground_and_the_street_under_it_follows_the_slope() {
    let bridge =
        r##"{"position":[10,14.5,6],"size":[10,1],"color":"#8a8f96","road":true}"##.to_string();
    let street = road([5.0, 14.5], [25.0, 1.0], "");
    let mut game = ramp_game(&[street, bridge], &[]);
    near(game.world.base_z(0), 0.5, 1e-9);
    near(game.world.base_z(1), 6.0, 1e-9);
    assert!(game.road_links(0).expect("дорога").links.is_empty());
    assert!(game.road_links(1).expect("мост").links.is_empty());
}

#[test]
fn the_water_sees_the_cut_and_floods_it() {
    // Сухой гребень выше моря между двумя низинами; дорога, концы которой стоят в низинах, срезает гребень до профиля.
    let ridge = |x: f64, _y: f64| {
        if (14.0..26.0).contains(&x) { 1.0 } else { 0.3 }
    };
    let text = terrain_text(Some(0.5), ridge);
    let mut with_road = game_of(&text, &[road([6.0, 14.5], [28.0, 1.0], "")], &[]);
    let without = game_of(&text, &[], &[]);
    let at = |game: &Game, x: f64, y: f64| {
        let terrain = game.world.terrain();
        let (column, row) = ((x * 2.0) as usize, (y * 2.0) as usize);
        terrain
            .water_field()
            .is_some_and(|field| field.is_flooded(row * terrain.columns() + column))
    };
    assert!(!at(&without, 20.0, 14.5), "гребень сух");
    assert!(
        at(&with_road, 20.0, 14.5),
        "врезанная дорога заливается, как низина"
    );
    with_road.world.delete(0);
    assert!(!at(&with_road, 20.0, 14.5), "дороги нет — гребень сух");
}

/// Дорога кончается у самого берега: ядро и откос её конца заходят в море, но дно под водой она не насыпает.
#[test]
fn a_road_ending_at_the_shore_does_not_fill_the_sea() {
    let shore = |x: f64, _y: f64| 0.1 * (x - 10.0);
    let text = terrain_text(Some(0.0), shore);
    let game = game_of(&text, &[road([10.5, 14.5], [10.0, 1.0], "")], &[]);
    let terrain = game.world.terrain();
    for x in [8.5, 9.0, 9.5] {
        near(
            terrain.height_at(x, 15.0),
            terrain.ground_height_at(x, 15.0),
            1e-9,
        );
    }
    near(terrain.height_at(11.0, 15.0), 0.1, 1e-9);
}

#[test]
fn a_brush_stroke_near_a_road_is_a_new_ground_for_its_ends() {
    let mut game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], "")], &[]);
    near(game.world.base_z(0), 0.4, 1e-9);
    let readings = edit::terrain_heights(&game).expect("рельеф");
    let mut heights = readings.heights.clone();
    for row in 0..readings.rows {
        for column in 0..readings.columns {
            if (column as f64 / 2.0 - 4.0).abs() < 1.5 {
                heights[row * readings.columns + column] += 2.0;
            }
        }
    }
    edit::set_terrain(&mut game, &heights, None, &[]).expect("рельеф поставлен");
    near(game.world.base_z(0), 2.4, 1e-9);
}

#[test]
fn a_bend_written_by_code_is_taken_as_the_nearest_allowed_one() {
    let mut game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], r#","bend":30"#)], &[]);
    near(game.road_links(0).expect("дорога").axis.bend, 30.0, 1e-9);
    game.world.set_number(0, property::BEND, 500.0);
    near(
        game.road_links(0).expect("дорога").axis.bend,
        MAX_BEND_DEGREES,
        1e-9,
    );
    game.world.set_number(0, property::BEND, -500.0);
    near(
        game.road_links(0).expect("дорога").axis.bend,
        -MAX_BEND_DEGREES,
        1e-9,
    );
    game.world.set_number(0, property::BEND, f64::NAN);
    near(game.road_links(0).expect("дорога").axis.bend, 0.0, 1e-9);
    let mut tight = ramp_game(&[road([4.0, 14.5], [3.0, 1.0], "")], &[]);
    tight.world.set_number(0, property::BEND, 170.0);
    let bend = tight.road_links(0).expect("дорога").axis.bend;
    let radius = 3.0 / (2.0 * (bend.to_radians() / 2.0).sin());
    assert!(radius >= 1.0 - 1e-9, "радиус {radius} меньше ширины");
}

#[test]
fn the_ends_of_a_bent_road_stay_and_the_axis_is_an_arc_with_the_profile() {
    let mut game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], r#","bend":90"#)], &[]);
    let links = game.road_links(0).expect("дорога");
    let points = &links.axis.points;
    assert_eq!(points.len(), 46, "90° по 2°");
    near(points[0][0], 4.0, 1e-9);
    near(points[0][1], 15.0, 1e-9);
    near(points[45][0], 24.0, 1e-9);
    near(points[45][1], 15.0, 1e-9);
    near(points[0][2], 0.4, 1e-9);
    near(points[45][2], 2.4, 1e-9);
    assert!(
        points[22][1] < 15.0 - 1.0,
        "положительный изгиб — к меньшему y, как положительный rotation"
    );
    near(links.handle[0], 14.0, 1e-6);
}

#[test]
fn a_car_on_a_sloping_road_stands_on_its_profile_and_leans_along_the_slope() {
    let mut scene = vec![road([4.0, 14.5], [20.0, 1.0], "")];
    scene.push(car([4.5, 15.0], [23.0, 15.0], 4.0));
    let mut game = ramp_game(&scene, &[&drive_rule("middle")]);
    let mut leaned = false;
    for _ in 0..600 {
        step(&mut game, 1);
        let at = center(&game, 1);
        let profile = 0.4 + 0.1 * (at[0] - 4.0);
        if at[0] > 6.0 && at[0] < 22.0 {
            near(game.world.base_z(1), profile, 1e-6);
            near(game.world.tilt(1), 0.1_f64.atan(), 1e-9);
            leaned = true;
        }
        if !game.world.has(1, property::DRIVE_TO) {
            break;
        }
    }
    assert!(leaned);
    near(game.world.tilt(1), 0.0, 1e-12);
    assert!(!game.world.has(1, property::DRIVE_TO));
}

#[test]
fn a_car_going_down_leans_the_other_way_and_one_driven_off_the_road_is_level() {
    let mut scene = vec![road([4.0, 14.5], [20.0, 1.0], "")];
    scene.push(car([23.5, 15.0], [5.0, 15.0], 4.0));
    let mut game = ramp_game(&scene, &[&drive_rule("middle")]);
    step(&mut game, 30);
    near(game.world.tilt(1), -(0.1_f64.atan()), 1e-9);
    game.world.clear_property(1, property::DRIVE_TO);
    step(&mut game, 1);
    near(game.world.tilt(1), 0.0, 1e-12);
}

#[test]
fn a_car_on_a_bent_road_keeps_to_its_arc_and_its_height() {
    let mut scene = vec![road([4.0, 14.5], [20.0, 1.0], r#","bend":90"#)];
    scene.push(car([4.0, 15.0], [24.0, 15.0], 4.0));
    let mut game = ramp_game(&scene, &[&drive_rule("middle")]);
    let mut worst: f64 = 0.0;
    let mut deepest = f64::INFINITY;
    for _ in 0..1200 {
        step(&mut game, 1);
        let at = center(&game, 1);
        let radius = 20.0 / (2.0 * (45.0_f64).to_radians().sin());
        let arc_center = [14.0, 15.0 + radius * (45.0_f64).to_radians().cos()];
        worst = worst.max(((at[0] - arc_center[0]).hypot(at[1] - arc_center[1]) - radius).abs());
        deepest = deepest.min(at[1]);
        if !game.world.has(1, property::DRIVE_TO) {
            break;
        }
    }
    assert!(worst < 2e-3, "машина ушла с дуги на {worst}");
    assert!(deepest < 15.0 - 1.0, "дуга прогнута");
    near(center(&game, 1)[0], 24.0, 1e-6);
}

const ARC_CODE: &str = r#"
function measure(probe)
    local r = road_route({x = 4, y = 15}, {x = 24, y = 15}, 4)
    probe.length = r.length
    probe.time = r.time
end
"#;

#[test]
fn road_route_measures_along_the_arc() {
    let mut scene = vec![road([4.0, 14.5], [20.0, 1.0], r#","bend":90"#)];
    scene.push(
        r#"{"name":"probe","probe":true,"position":[2,10],"size":[1,1],"length":0,"time":0}"#
            .to_string(),
    );
    let rule = r#"{"kind":"check","for":{"has":["probe"]},"do":[["run","measure"]]}"#;
    let (mut game, _) = load_with(&terrain_text(None, ramp), &scene, &[rule], ARC_CODE, false)
        .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors));
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let radius = 20.0 / (2.0 * (45.0_f64).to_radians().sin());
    let arc = radius * std::f64::consts::FRAC_PI_2;
    let length = game.properties.resolve("length").expect("length");
    let time = game.properties.resolve("time").expect("time");
    near(
        game.world.number_like(1, length).expect("length"),
        arc,
        1e-6,
    );
    near(
        game.world.number_like(1, time).expect("time"),
        arc / 4.0,
        1e-6,
    );
}

#[test]
fn a_click_on_the_lane_of_a_road_on_a_slope_and_on_an_arc_hits_it_and_beside_it_does_not() {
    let objects = vec![
        road([4.0, 6.5], [20.0, 1.0], ""),
        road([4.0, 20.5], [20.0, 1.0], r#","bend":60"#),
    ];
    let mut game = ramp_game(&objects, &[]);
    let camera = Camera3d::looking_at([14.0, 15.0], 60.0, 24.0, [1280.0, 800.0]);
    let pick = |game: &Game, point: [f64; 3]| {
        let window = camera.project(point).expect("точка в кадре");
        scene::editor_target_ray(&game.world, &game.scene, &camera, window)
    };
    for x in [6.0, 12.0, 20.0, 23.0] {
        let z = 0.4 + 0.1 * (x - 4.0);
        assert_eq!(pick(&game, [x, 7.0, z]), Some(0), "x = {x}");
    }
    assert_eq!(pick(&game, [12.0, 8.5, 0.1 * 12.0]), None, "мимо полосы");
    let points = game.road_links(1).expect("дорога").axis.points;
    for at in [
        points[5],
        points[points.len() / 2],
        points[points.len() - 6],
    ] {
        assert_eq!(pick(&game, at), Some(1), "{at:?}");
    }
    let chord_middle = [14.0, 21.0, 0.1 * 14.0 + 0.1];
    assert_eq!(
        pick(&game, chord_middle),
        None,
        "середина хорды лежит мимо дуги"
    );
}

#[test]
fn the_bend_errors_stop_the_game() {
    let bent = |extra: &str| road([4.0, 14.5], [20.0, 1.0], extra);
    let terrain = terrain_text(None, ramp);
    expect_error(
        load_with(&terrain, &[bent(r#","bend":"много""#)], &[], "", false),
        "ожидалось число",
    );
    expect_error(
        load_with(&terrain, &[bent(r#","bend":180"#)], &[], "", false),
        "меньше 180",
    );
    expect_error(
        load_with(&terrain, &[bent(r#","bend":-200"#)], &[], "", false),
        "меньше 180",
    );
    expect_error(
        load_with(
            &terrain,
            &[road([4.0, 14.5], [1.5, 1.0], r#","bend":170"#)],
            &[],
            "",
            false,
        ),
        "радиус дуги",
    );
    let not_a_road =
        r##"{"position":[4,14],"size":[10,1],"color":"#ffffff","bend":20}"##.to_string();
    expect_error(
        load_with(&terrain, &[not_a_road], &[], "", false),
        "без road",
    );
    let flat = r##"{"position":[4,14],"size":[10,1],"color":"#ffffff","bend":20}"##.to_string();
    expect_error(load_with(&terrain, &[flat], &[], "", true), "bend");
    assert!(
        errors_of(load_with(
            &terrain,
            &[bent(r#","bend":45"#)],
            &[],
            "",
            false
        ))
        .is_empty()
    );
}

#[test]
fn road_links_of_a_bent_road_give_the_polyline_and_the_bend() {
    let mut game = ramp_game(&[road([4.0, 14.5], [20.0, 1.0], r#","bend":45"#)], &[]);
    let links = game.road_links(0).expect("дорога");
    assert!(links.axis.points.len() > 2);
    near(links.axis.bend, 45.0, 1e-9);
    assert!(links.ends.len() == 2 && links.neighbours.is_empty());
}

#[test]
fn a_street_ending_on_the_axis_of_a_road_takes_the_height_of_that_road_there() {
    let hill = |x: f64, y: f64| (1.5 * (1.0 - (x - 14.0).hypot(y - 14.5) / 5.0)).max(0.0);
    let trunk = road([4.0, 14.0], [20.0, 1.0], "");
    let street = road([13.65, 6.5], [0.7, 8.0], "");
    let game = game_of(&terrain_text(None, hill), &[trunk, street], &[]);
    let on_hill = game
        .properties
        .terrain()
        .expect("рельеф")
        .height_at(14.0, 14.5);
    assert!(on_hill > 1.4, "земля на холме: {on_hill}");
    near(game.world.base_z(0), 0.0, 1e-9);
    near(game.world.base_z(1), 0.0, 1e-9);
    let form = *game.world.road_form(1).expect("улица");
    near(form.end_z, 0.0, 1e-9);
}

#[test]
fn a_bend_is_set_by_the_editor_only_on_a_road() {
    let mut game = ramp_game(
        &[
            road([4.0, 14.5], [20.0, 1.0], ""),
            r##"{"position":[4,2],"size":[2,2],"color":"#ffffff"}"##.to_string(),
        ],
        &[],
    );
    let ok = |game: &mut Game, id: u32, value: f64| {
        edit::set_property(
            &mut game.world,
            &game.properties,
            &[],
            id,
            "bend",
            &serde_json::json!(value),
        )
    };
    assert!(ok(&mut game, 0, 45.0).is_ok());
    near(game.road_links(0).expect("дорога").axis.bend, 45.0, 1e-9);
    assert!(ok(&mut game, 1, 45.0).is_err(), "не дорога");
    assert!(
        ok(&mut game, 0, 200.0).is_err(),
        "180 и больше — ошибка проверки"
    );
}

const BEND_CODE: &str = r#"
function bend_it(road)
    road.bend = 500
end
"#;

#[test]
fn a_bend_written_by_the_code_of_the_game_is_taken_as_the_nearest_allowed_one() {
    let objects = [road([4.0, 14.5], [20.0, 1.0], r#","probe":true"#)];
    let rule = r#"{"kind":"check","for":{"has":["probe"]},"do":[["run","bend_it"]]}"#;
    let (mut game, _) = load_with(
        &terrain_text(None, ramp),
        &objects,
        &[rule],
        BEND_CODE,
        false,
    )
    .unwrap_or_else(|failure| panic!("игра не загрузилась: {:#?}", failure.errors));
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    near(
        game.world.number_like(0, property::BEND).expect("bend"),
        500.0,
        0.0,
    );
    near(
        game.road_links(0).expect("дорога").axis.bend,
        MAX_BEND_DEGREES,
        1e-9,
    );
}

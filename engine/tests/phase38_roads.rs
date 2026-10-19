//! Фаза 38 — дороги (требования 1–31). Каждый тест собирает свою маленькую трёхмерную игру в коде,
//! как `tests/phase37_run_jump.rs`: сеть, путь и дуги проверены и в `mod tests` рядом с кодом
//! (`core::roads`), здесь — сквозные проверки через загрузку игры: езда машины шаг за шагом, поворот
//! по ходу, мост, закрытие дороги на ходу, вызов кода `road_route`, `road_links`, проверки перед
//! запуском и повтор записи партии.

use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::property;
use engine::core::report::RuleFired;
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::data::error::{GameError, LoadFailure};
use engine::data::load::load_game_from_texts_with_code;
use engine::data::session::PlaySession;

const VIEWPORT: [f32; 2] = [800.0, 600.0];
const SCENE_EXTRA: &str = r#","view_height":24,"camera":{"pitch":60}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const PROPS: &str = r#"{"properties":{"car":"flag","watcher":"flag","probe":"flag","count":"number","length":"number","time":"number","blocker":"flag"}}"#;

fn game_json(scene_extra: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":40,"height":30,"background":"#000000"{scene_extra}}},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}},"code":"code.lua"}}}}"##
    )
}

fn scene_of(objects: &[String]) -> String {
    format!(r#"{{"objects":[{}]}}"#, objects.join(","))
}

fn rules_of(rules: &[&str]) -> String {
    format!(r#"{{"rules":[{}]}}"#, rules.join(","))
}

fn drive_rule(side: &str) -> String {
    format!(r#"{{"kind":"drive","for":{{"has":["car"]}},"side":"{side}"}}"#)
}

type Loaded = Result<(Game, ScreensConfig, Vec<GameError>), LoadFailure>;

fn load_with_code(scene_extra: &str, scene: &[String], rules: &[&str], code: &str) -> Loaded {
    load_game_from_texts_with_code(
        &game_json(scene_extra),
        PROPS,
        &scene_of(scene),
        &rules_of(rules),
        SCREENS,
        Some(code),
    )
}

fn load(scene_extra: &str, scene: &[String], rules: &[&str]) -> Loaded {
    load_with_code(scene_extra, scene, rules, "")
}

fn game_of(scene: &[String], rules: &[&str]) -> Game {
    let (game, _screens, _warnings) =
        load(SCENE_EXTRA, scene, rules).expect("игра должна загрузиться");
    game
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

fn road(position: [f64; 2], size: [f64; 2], extra: &str) -> String {
    format!(
        r##"{{"position":[{},{}],"size":[{},{}],"color":"#8a8f96","road":true{extra}}}"##,
        position[0], position[1], size[0], size[1]
    )
}

/// Машина 0,2 × 0,6 с серединой в `center`.
fn car(center: [f64; 2], target: Option<[f64; 2]>, speed: f64) -> String {
    let drive_to = target.map_or(String::new(), |t| {
        format!(r#","drive_to":[{},{}]"#, t[0], t[1])
    });
    format!(
        r##"{{"name":"car","position":[{},{}],"size":[0.2,0.6],"shape":"box","height":0.4,"color":"#f2f2f2","car":true,"drive_speed":{speed}{drive_to}}}"##,
        center[0] - 0.1,
        center[1] - 0.3
    )
}

/// Улица с запада на восток (ось y = 10) и улица с юга на север (ось x = 10), ширина 1.
fn crossing(second_extra: &str) -> Vec<String> {
    vec![
        road([0.0, 9.5], [20.0, 1.0], ""),
        road([9.5, 0.0], [1.0, 20.0], second_extra),
    ]
}

fn step(game: &mut Game, count: usize) {
    for _ in 0..count {
        game.step(StepInput::empty());
    }
}

fn center(game: &Game, id: u32) -> [f64; 2] {
    let position = game.world.vec2(id, property::POSITION).expect("position");
    let size = game.world.vec2(id, property::SIZE).expect("size");
    [position[0] + size[0] / 2.0, position[1] + size[1] / 2.0]
}

fn rotation(game: &Game, id: u32) -> f64 {
    game.world
        .rotation(id, property::ROTATION)
        .expect("rotation")
        .angle()
}

fn driving(game: &Game, id: u32) -> bool {
    game.world.has(id, property::DRIVE_TO)
}

/// Шагов, за которые машина `id` доехала; ошибка теста, если не доехала за `limit`.
fn steps_to_arrive(game: &mut Game, id: u32, limit: usize) -> usize {
    for count in 1..=limit {
        step(game, 1);
        if !driving(game, id) {
            return count;
        }
    }
    panic!("машина не доехала за {limit} шагов");
}

fn near(actual: f64, expected: f64, eps: f64) {
    assert!(
        (actual - expected).abs() <= eps,
        "ожидалось {expected} ± {eps}, вышло {actual}"
    );
}

fn near_point(actual: [f64; 2], expected: [f64; 2], eps: f64) {
    near(actual[0], expected[0], eps);
    near(actual[1], expected[1], eps);
}

/// Пример требования 14: машина в (2, 10) едет в (10, 2) по осям; машина — объект 2.
const CAR_ID: u32 = 2;

fn example(side: &str, second_extra: &str) -> Game {
    let mut scene = crossing(second_extra);
    scene.push(car([2.0, 10.0], Some([10.0, 2.0]), 4.0));
    game_of(&scene, &[&drive_rule(side)])
}

#[test]
fn the_middle_line_takes_237_steps_and_the_car_ends_exactly_on_the_goal() {
    let mut game = example("middle", "");
    let steps = steps_to_arrive(&mut game, CAR_ID, 400);
    near(steps as f64, 237.0, 3.0);
    near_point(center(&game, CAR_ID), [10.0, 2.0], 1e-9);
    assert!(!driving(&game, CAR_ID));
}

#[test]
fn a_slow_second_street_makes_it_361_steps() {
    let mut game = example("middle", r#","road_speed":2"#);
    near(steps_to_arrive(&mut game, CAR_ID, 600) as f64, 361.0, 4.0);
}

#[test]
fn the_right_side_runs_a_quarter_width_from_the_axis() {
    let mut game = example("right", "");
    let mut east = f64::NEG_INFINITY;
    let mut north = f64::NEG_INFINITY;
    for _ in 0..400 {
        step(&mut game, 1);
        let at = center(&game, CAR_ID);
        if (4.0..9.0).contains(&at[0]) {
            east = east.max((at[1] - 10.25).abs());
        }
        if (4.0..9.0).contains(&at[1]) && at[0] > 9.0 {
            north = north.max((at[0] - 10.25).abs());
        }
        if !driving(&game, CAR_ID) {
            break;
        }
    }
    near(east, 0.0, 1e-9);
    near(north, 0.0, 1e-9);
}

#[test]
fn the_left_side_runs_on_the_other_side_of_the_axis() {
    let mut game = example("left", "");
    step(&mut game, 60);
    near(center(&game, CAR_ID)[1], 9.75, 1e-9);
}

fn heading_of_a_straight_trip(from: [f64; 2], to: [f64; 2]) -> f64 {
    let mut scene = crossing("");
    scene.push(car(from, Some(to), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 20);
    rotation(&game, CAR_ID)
}

#[test]
fn the_car_turns_to_face_the_way_it_goes() {
    near(
        heading_of_a_straight_trip([2.0, 10.0], [18.0, 10.0]),
        270.0,
        1e-9,
    );
    near(
        heading_of_a_straight_trip([10.0, 18.0], [10.0, 2.0]),
        180.0,
        1e-9,
    );
    near(
        heading_of_a_straight_trip([10.0, 2.0], [10.0, 18.0]),
        0.0,
        1e-9,
    );
    near(
        heading_of_a_straight_trip([18.0, 10.0], [2.0, 10.0]),
        90.0,
        1e-9,
    );
}

#[test]
fn on_the_arc_the_heading_runs_through_the_angles_between() {
    let mut game = example("middle", "");
    let mut seen_between = false;
    for _ in 0..400 {
        step(&mut game, 1);
        let angle = rotation(&game, CAR_ID);
        if angle > 190.0 && angle < 260.0 {
            seen_between = true;
        }
        if !driving(&game, CAR_ID) {
            break;
        }
    }
    assert!(
        seen_between,
        "поворот должен идти плавно, через промежуточные углы"
    );
    near(rotation(&game, CAR_ID), 180.0, 1e-9);
}

#[test]
fn a_standing_car_keeps_its_heading_and_a_car_never_sent_gets_none() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([8.0, 10.0]), 4.0));
    scene.push(car([2.0, 12.0], None, 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 200);
    assert!(!driving(&game, CAR_ID));
    near(rotation(&game, CAR_ID), 270.0, 1e-9);
    assert!(
        game.world
            .rotation(CAR_ID + 1, property::ROTATION)
            .is_none()
    );
    near_point(center(&game, CAR_ID + 1), [2.0, 12.0], 1e-12);
}

#[test]
fn a_car_with_no_speed_stands_and_keeps_its_goal() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([8.0, 10.0]), 0.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 30);
    near_point(center(&game, CAR_ID), [2.0, 10.0], 1e-12);
    assert!(driving(&game, CAR_ID));
}

#[test]
fn after_move_of_sees_the_shift_and_the_report_lists_the_driver() {
    let watcher = r#"{"name":"watcher","watcher":true,"count":0}"#.to_string();
    let watch = r#"{"kind":"check","for":{"has":["watcher"]},"when":{"after_move_of":{"has":["car"]}},"do":[["add","count",1]]}"#;
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([4.0, 10.0]), 4.0));
    scene.push(watcher);
    let mut game = game_of(&scene, &[&drive_rule("middle"), watch]);
    game.begin_session();
    step(&mut game, 1);
    let report = game.last_report().expect("отчёт");
    assert!(report.fired.iter().any(|fired| matches!(
        fired,
        RuleFired::Drive { objects, .. } if objects == &vec![CAR_ID]
    )));
    step(&mut game, 100);
    let count = game.properties.resolve("count").expect("count");
    let moves = game.world.number_like(CAR_ID + 1, count).expect("count");
    near(moves, 30.0, 1.0);
}

#[test]
fn a_car_far_from_the_roads_first_drives_straight_to_the_nearest_one() {
    let mut scene = crossing("");
    scene.push(car([2.0, 14.0], Some([10.0, 2.0]), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 30);
    let at = center(&game, CAR_ID);
    near(at[0], 2.0, 1e-9);
    assert!(at[1] < 14.0 && at[1] > 10.0, "{at:?}");
    steps_to_arrive(&mut game, CAR_ID, 600);
    near_point(center(&game, CAR_ID), [10.0, 2.0], 1e-9);
}

#[test]
fn a_goal_far_from_the_roads_is_reached_by_a_straight_piece() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([10.0, 26.0]), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    steps_to_arrive(&mut game, CAR_ID, 900);
    near_point(center(&game, CAR_ID), [10.0, 26.0], 1e-9);
}

#[test]
fn without_roads_the_car_drives_straight_at_its_speed() {
    let scene = vec![car([2.0, 10.0], Some([14.0, 10.0]), 6.0)];
    let mut game = game_of(&scene, &[&drive_rule("right")]);
    near(steps_to_arrive(&mut game, 0, 400) as f64, 120.0, 1.0);
    near_point(center(&game, 0), [14.0, 10.0], 1e-9);
}

#[test]
fn a_closed_street_is_not_driven_and_closing_one_midway_reroutes_next_step() {
    let around = vec![
        road([0.0, 9.5], [10.0, 1.0], ""),
        road([10.0, 9.5], [10.0, 1.0], ""),
        road([0.0, 0.0], [1.0, 10.0], ""),
        road([0.0, 0.0], [20.0, 1.0], ""),
        road([19.0, 0.0], [1.0, 10.0], ""),
    ];
    let mut scene = around.clone();
    scene.push(car([0.5, 10.0], Some([19.5, 10.0]), 5.0));
    let rules = [drive_rule("middle")];
    let rule_refs: Vec<&str> = rules.iter().map(String::as_str).collect();
    let mut game = game_of(&scene, &rule_refs);
    let car_id = 5;
    step(&mut game, 30);
    assert!(
        center(&game, car_id)[1] > 9.9,
        "по короткому пути на север не едут"
    );
    let road_speed = property::ROAD_SPEED;
    game.world.set_number(1, road_speed, 0.0);
    let mut north_most = f64::INFINITY;
    for _ in 0..800 {
        step(&mut game, 1);
        north_most = north_most.min(center(&game, car_id)[1]);
        if !driving(&game, car_id) {
            break;
        }
    }
    assert!(
        north_most < 1.5,
        "машина должна объехать закрытую дорогу по северной: {north_most}"
    );
    assert!(!driving(&game, car_id));
    near_point(center(&game, car_id), [19.5, 10.0], 1e-9);
}

#[test]
fn a_street_closed_from_the_start_is_out_of_the_network() {
    let mut scene = crossing(r#","road_speed":0"#);
    scene.push(car([2.0, 10.0], Some([10.0, 2.0]), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    steps_to_arrive(&mut game, CAR_ID, 600);
    near_point(center(&game, CAR_ID), [10.0, 2.0], 1e-9);
    let links = game.road_links(1).expect("дорога");
    assert!(
        links.links.len() == 1,
        "закрытая дорога в сети связей остаётся"
    );
}

#[test]
fn a_shift_by_code_gives_a_new_path_from_the_new_place() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([10.0, 2.0]), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 10);
    game.world.set_vec2(CAR_ID, property::POSITION, [14.9, 9.7]);
    step(&mut game, 1);
    near_point(center(&game, CAR_ID), [15.0 - 4.0 / 60.0, 10.0], 1e-9);
}

#[test]
fn a_new_goal_gives_a_new_path() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([10.0, 2.0]), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 10);
    game.world
        .set_vec2(CAR_ID, property::DRIVE_TO, [10.0, 18.0]);
    steps_to_arrive(&mut game, CAR_ID, 600);
    near_point(center(&game, CAR_ID), [10.0, 18.0], 1e-9);
}

#[test]
fn several_drive_rules_share_the_paths_and_forget_only_cars_that_stopped_driving() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([10.0, 2.0]), 4.0));
    scene.push(
        car([2.0, 10.0], Some([10.0, 18.0]), 4.0).replace(r#""car":true"#, r#""watcher":true"#),
    );
    let second_rule = drive_rule("middle").replace("car", "watcher");
    let mut game = game_of(&scene, &[&drive_rule("middle"), &second_rule]);
    step(&mut game, 5);
    assert_eq!(game.road_path_count(), 2, "путь помнят машины обоих правил");
    step(&mut game, 5);
    assert_eq!(game.road_path_count(), 2);

    game.world.clear_property(CAR_ID + 1, property::DRIVE_TO);
    step(&mut game, 1);
    assert_eq!(game.road_path_count(), 1, "перестала ехать — путь забыт");
}

#[test]
fn the_slower_way_is_avoided_when_a_longer_one_is_faster() {
    let mut scene = vec![
        road([0.0, 9.5], [10.0, 1.0], ",\"road_speed\":1"),
        road([10.0, 9.5], [10.0, 1.0], ",\"road_speed\":1"),
        road([0.0, 0.0], [1.0, 10.0], ""),
        road([0.0, 0.0], [20.0, 1.0], ""),
        road([19.0, 0.0], [1.0, 10.0], ""),
    ];
    scene.push(car([0.5, 10.0], Some([19.5, 10.0]), 5.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    let mut north_most = f64::INFINITY;
    for _ in 0..800 {
        step(&mut game, 1);
        north_most = north_most.min(center(&game, 5)[1]);
        if !driving(&game, 5) {
            break;
        }
    }
    assert!(north_most < 1.5, "{north_most}");
}

/// Мост: улица с юга на север по x = 20 поднимается тремя ступенями по 0,3, идёт поверху над улицей
/// с запада на восток (y = 15) и спускается так же.
fn bridge_scene() -> Vec<String> {
    let deck = |y: f64, length: f64, height: f64| {
        format!(
            r##"{{"position":[19.5,{y}],"size":[1,{length}],"deck":true,"shape":"box","height":{height},"color":"#777777"}}"##
        )
    };
    let lane = |y: f64, length: f64| road([19.5, y], [1.0, length], "");
    let mut scene = vec![
        r##"{"position":[0,14.5,0],"size":[40,1],"color":"#8a8f96","road":true}"##.to_string(),
        lane(24.0, 6.0),
        lane(0.0, 6.0),
    ];
    for (y, length, height) in [
        (22.5, 1.5, 0.3),
        (21.0, 1.5, 0.6),
        (19.5, 1.5, 0.9),
        (10.5, 9.0, 1.2),
        (9.0, 1.5, 0.9),
        (7.5, 1.5, 0.6),
        (6.0, 1.5, 0.3),
    ] {
        scene.push(deck(y, length, height));
        scene.push(lane(y, length));
    }
    scene
}

#[test]
fn a_car_climbs_the_ramps_crosses_over_the_street_and_comes_down() {
    let mut scene = bridge_scene();
    let bridge_car = scene.len() as u32;
    scene.push(car([20.0, 29.0], Some([20.0, 1.0]), 5.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    let mut highest = 0.0_f64;
    let mut over_the_street = false;
    for _ in 0..1200 {
        step(&mut game, 1);
        let at = center(&game, bridge_car);
        near(at[0], 20.0, 1e-9);
        let z = game.world.base_z(bridge_car);
        highest = highest.max(z);
        if (14.5..15.5).contains(&at[1]) {
            over_the_street = true;
            near(z, 1.2, 1e-9);
        }
        if !driving(&game, bridge_car) {
            break;
        }
    }
    assert!(over_the_street);
    near(highest, 1.2, 1e-9);
    assert!(!driving(&game, bridge_car));
    near_point(center(&game, bridge_car), [20.0, 1.0], 1e-9);
    near(game.world.base_z(bridge_car), 0.0, 1e-9);
}

#[test]
fn a_car_on_the_street_below_passes_under_the_bridge() {
    let mut scene = bridge_scene();
    let street_car = scene.len() as u32;
    scene.push(car([2.0, 15.0], Some([38.0, 15.0]), 5.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    for _ in 0..1200 {
        step(&mut game, 1);
        near(game.world.base_z(street_car), 0.0, 1e-9);
        near(center(&game, street_car)[1], 15.0, 1e-9);
        if !driving(&game, street_car) {
            break;
        }
    }
    near_point(center(&game, street_car), [38.0, 15.0], 1e-9);
}

#[test]
fn roads_on_the_bridge_and_under_it_are_not_linked() {
    let mut game = game_of(&bridge_scene(), &[]);
    let under = game.road_links(0).expect("дорога");
    assert!(under.links.is_empty(), "{under:?}");
    assert_eq!(under.ends.len(), 2);
    assert!(under.neighbours.is_empty());
}

#[test]
fn road_links_tell_the_axis_the_links_the_dead_ends_and_the_neighbours() {
    let scene = vec![
        road([0.0, 9.5], [20.0, 1.0], ""),
        road([9.5, 0.0], [1.0, 20.0], ""),
        r##"{"position":[2,2],"size":[1,1],"color":"#ffffff"}"##.to_string(),
    ];
    let mut game = game_of(&scene, &[]);
    let links = game.road_links(0).expect("дорога");
    assert_eq!(links.axis, [[0.0, 10.0], [20.0, 10.0]]);
    assert_eq!(links.links, vec![([10.0, 10.0], 1)]);
    assert_eq!(links.ends, vec![[0.0, 10.0], [20.0, 10.0]]);
    assert_eq!(links.neighbours.len(), 1);
    assert_eq!(links.neighbours[0].0, 1);
    assert_eq!(links.neighbours[0].1, [[10.0, 0.0], [10.0, 20.0]]);
    assert!(game.road_links(2).is_none(), "не дорога");
    assert!(game.road_links(99).is_none(), "нет такого объекта");
}

#[test]
fn road_links_follow_a_street_that_was_edited() {
    let scene = vec![
        road([0.0, 9.5], [20.0, 1.0], ""),
        road([9.5, 0.0], [1.0, 8.0], ""),
    ];
    let mut game = game_of(&scene, &[]);
    assert!(game.road_links(0).expect("дорога").links.is_empty());
    game.world.set_vec2(1, property::SIZE, [1.0, 20.0]);
    assert_eq!(game.road_links(0).expect("дорога").links.len(), 1);
}

const ROUTE_CODE: &str = r#"
function measure(probe)
    local r = road_route({x = 2, y = 10}, {x = 10, y = 2}, 4)
    probe.length = r.length
    probe.time = r.time
end

function pairs_too(probe)
    local r = road_route(probe.position, {x = 10, y = 2}, 4)
    probe.length = r.length
end

function bad_speed(probe)
    road_route({x = 2, y = 10}, {x = 10, y = 2}, 0)
end

function bad_point(probe)
    road_route(1, {x = 10, y = 2}, 4)
end
"#;

fn code_game(function: &str) -> Game {
    let mut scene = crossing("");
    scene.push(
        r#"{"name":"probe","probe":true,"position":[2,10],"size":[1,1],"length":0,"time":0}"#
            .to_string(),
    );
    let rule =
        format!(r#"{{"kind":"check","for":{{"has":["probe"]}},"do":[["run","{function}"]]}}"#);
    let (game, _screens, _warnings) =
        load_with_code(SCENE_EXTRA, &scene, &[&rule], ROUTE_CODE).expect("игра должна загрузиться");
    game
}

#[test]
fn road_route_gives_the_length_and_time_of_the_example() {
    let mut game = code_game("measure");
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let length = game.properties.resolve("length").expect("length");
    let time = game.properties.resolve("time").expect("time");
    near(
        game.world.number_like(2, length).expect("length"),
        16.0,
        1e-9,
    );
    near(game.world.number_like(2, time).expect("time"), 4.0, 1e-9);
}

#[test]
fn road_route_takes_a_pair_read_off_an_object() {
    let mut game = code_game("pairs_too");
    step(&mut game, 1);
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let length = game.properties.resolve("length").expect("length");
    near(
        game.world.number_like(2, length).expect("length"),
        16.0,
        1e-9,
    );
}

#[test]
fn road_route_with_wrong_arguments_is_a_code_error() {
    for function in ["bad_speed", "bad_point"] {
        let mut game = code_game(function);
        step(&mut game, 1);
        assert!(
            game.code_error().is_some(),
            "{function} должна быть ошибкой кода"
        );
    }
}

#[test]
fn road_route_reads_the_speed_of_the_street_at_the_moment_of_the_call() {
    let mut game = code_game("measure");
    game.world.set_number(1, property::ROAD_SPEED, 1.0);
    step(&mut game, 1);
    let length = game.properties.resolve("length").expect("length");
    let time = game.properties.resolve("time").expect("time");
    near(
        game.world.number_like(2, length).expect("length"),
        16.0,
        1e-9,
    );
    near(game.world.number_like(2, time).expect("time"), 10.0, 1e-9);
}

fn load_scene(scene_extra: &str, scene: &[String], rules: &[&str]) -> Loaded {
    load(scene_extra, scene, rules)
}

#[test]
fn drive_in_a_flat_scene_is_an_error() {
    let scene = vec![car([2.0, 10.0], None, 4.0)];
    expect_error(
        load_scene("", &scene, &[&drive_rule("right")]),
        "drive есть только в трёхмерной сцене",
    );
}

#[test]
fn a_side_that_is_not_right_left_or_middle_is_an_error() {
    let scene = vec![car([2.0, 10.0], None, 4.0)];
    expect_error(
        load_scene(SCENE_EXTRA, &scene, &[&drive_rule("up")]),
        "side должен быть",
    );
}

#[test]
fn an_object_under_drive_needs_position_size_and_drive_speed() {
    let no_speed =
        r##"{"position":[2,10],"size":[0.2,0.6],"shape":"box","color":"#ffffff","car":true}"##;
    expect_error(
        load_scene(
            SCENE_EXTRA,
            &[no_speed.to_string()],
            &[&drive_rule("right")],
        ),
        "drive_speed",
    );
    let no_size = r##"{"position":[2,10],"car":true,"drive_speed":4}"##;
    expect_error(
        load_scene(SCENE_EXTRA, &[no_size.to_string()], &[&drive_rule("right")]),
        "size",
    );
}

#[test]
fn an_object_under_drive_and_under_move_walk_or_run_jump_is_an_error() {
    let mut mover = car([2.0, 10.0], None, 4.0);
    mover = mover.replace(
        r#""car":true"#,
        r#""car":true,"velocity":[1,0],"walk_speed":2"#,
    );
    let move_rule = r#"{"kind":"move","for":{"has":["velocity"]}}"#;
    expect_error(
        load_scene(
            SCENE_EXTRA,
            &[mover.clone()],
            &[&drive_rule("right"), move_rule],
        ),
        "под move",
    );
    let walk_rule = r#"{"kind":"walk","for":{"has":["walk_speed"]}}"#;
    expect_error(
        load_scene(SCENE_EXTRA, &[mover], &[&drive_rule("right"), walk_rule]),
        "под walk",
    );
}

#[test]
fn drive_speed_and_road_speed_must_be_numbers_not_below_zero() {
    let negative = car([2.0, 10.0], None, -1.0);
    expect_error(
        load_scene(SCENE_EXTRA, &[negative], &[&drive_rule("right")]),
        "не может быть меньше нуля",
    );
    let slow = road([0.0, 9.5], [20.0, 1.0], r#","road_speed":-2"#);
    expect_error(load_scene(SCENE_EXTRA, &[slow], &[]), "road_speed");
    let text = car([2.0, 10.0], None, 4.0).replace(r#""drive_speed":4"#, r#""drive_speed":"fast""#);
    expect_error(
        load_scene(SCENE_EXTRA, &[text], &[&drive_rule("right")]),
        "число",
    );
}

#[test]
fn road_must_be_a_flag_on_an_object_with_position_and_size_and_in_a_3d_scene() {
    let text = road([0.0, 9.5], [20.0, 1.0], "").replace(r#""road":true"#, r#""road":5"#);
    expect_error(load_scene(SCENE_EXTRA, &[text], &[]), "признак");
    let bare = r##"{"position":[0,0],"road":true}"##.to_string();
    expect_error(
        load_scene(SCENE_EXTRA, &[bare], &[]),
        "road разрешён только",
    );
    let flat = vec![road([0.0, 9.5], [20.0, 1.0], "")];
    expect_error(
        load_scene("", &flat, &[]),
        "road есть только в трёхмерной сцене",
    );
}

fn counting_car(target: Option<[f64; 2]>) -> String {
    car([2.0, 10.0], target, 4.0).replace(r#""car":true"#, r#""car":true,"count":0"#)
}

#[test]
fn naming_road_without_any_road_is_a_warning() {
    let named =
        r#"{"kind":"check","for":{"has":["car"],"without":["road"]},"do":[["add","count",1]]}"#;
    let scene = vec![counting_car(None)];
    let (_game, _screens, warnings) =
        load(SCENE_EXTRA, &scene, &[&drive_rule("right"), named]).expect("игра загрузится");
    assert!(
        warnings
            .iter()
            .any(|w| w.message.contains("ни одной дороги")),
        "{warnings:#?}"
    );
    let with_road = vec![road([0.0, 9.5], [20.0, 1.0], ""), counting_car(None)];
    let (_game, _screens, warnings) =
        load(SCENE_EXTRA, &with_road, &[&drive_rule("right"), named]).expect("игра загрузится");
    assert!(
        !warnings
            .iter()
            .any(|w| w.message.contains("ни одной дороги"))
    );
}

#[test]
fn a_selector_without_drive_to_finds_the_standing_cars() {
    let mut scene = crossing("");
    scene.push(counting_car(Some([6.0, 10.0])));
    let standing =
        r#"{"kind":"check","for":{"has":["car"],"without":["drive_to"]},"do":[["add","count",1]]}"#;
    let (mut game, _screens, warnings) =
        load(SCENE_EXTRA, &scene, &[&drive_rule("middle"), standing]).expect("игра загрузится");
    assert!(
        !warnings.iter().any(|w| w.message.contains("заведомо")),
        "{warnings:#?}"
    );
    let count = game.properties.resolve("count").expect("count");
    step(&mut game, 30);
    near(
        game.world.number_like(CAR_ID, count).expect("count"),
        0.0,
        0.0,
    );
    step(&mut game, 100);
    assert!(game.world.number_like(CAR_ID, count).expect("count") > 0.0);
}

#[test]
fn two_cars_on_one_street_drive_through_each_other() {
    let mut scene = crossing("");
    scene.push(car([2.0, 10.0], Some([18.0, 10.0]), 4.0));
    scene.push(car([18.0, 10.0], Some([2.0, 10.0]), 4.0));
    let mut game = game_of(&scene, &[&drive_rule("middle")]);
    step(&mut game, 120);
    near_point(center(&game, CAR_ID), center(&game, CAR_ID + 1), 0.2);
}

const KEYED_CAR: &str = r##"{"name":"car","position":[1.9,9.7],"size":[0.2,0.6],"shape":"box","height":0.4,"color":"#f2f2f2","car":true,"drive_speed":4,
"keys":{"KeyG":{"press":[["drive_to",[10,2]]]}}}"##;

#[test]
fn a_recorded_drive_replays_to_the_same_places_step_for_step() {
    let mut scene = crossing("");
    scene.push(KEYED_CAR.to_string());
    let rules = [drive_rule("right")];
    let rule_refs: Vec<&str> = rules.iter().map(String::as_str).collect();
    let (mut game, config, _warnings) =
        load(SCENE_EXTRA, &scene, &rule_refs).expect("игра загрузится");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    let mut live = Vec::new();
    for count in 0..300 {
        if count == 5 {
            queue.push_key_down("KeyG");
        }
        session.step_once(
            &mut queue,
            &mut mouse,
            &mut game,
            &config,
            &mut state,
            VIEWPORT,
            &[],
        );
        live.push((center(&game, CAR_ID), rotation_or_zero(&game)));
    }
    let text = session.recording_text(&game);
    let (mut again, config2, _warnings) =
        load(SCENE_EXTRA, &scene, &rule_refs).expect("игра загрузится");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut again, &config2, &mut state2)
        .expect("запись читается");
    for (count, expected) in live.iter().enumerate() {
        replay.step_once(
            &mut UiQueue::new(),
            &mut MouseState::default(),
            &mut again,
            &config2,
            &mut state2,
            VIEWPORT,
            &[],
        );
        assert_eq!(
            (center(&again, CAR_ID), rotation_or_zero(&again)),
            *expected,
            "повтор разошёлся на шаге {count}"
        );
    }
    assert!(!driving(&again, CAR_ID), "в повторе машина тоже доехала");
}

fn rotation_or_zero(game: &Game) -> f64 {
    game.world
        .rotation(CAR_ID, property::ROTATION)
        .map_or(0.0, |r| r.angle())
}

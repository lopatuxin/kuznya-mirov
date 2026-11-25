use engine::core::game::Game;
use engine::core::input::{MouseState, StepInput, UiQueue};
use engine::core::property;
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::data::edit;
use engine::data::load::{load_game_from_texts_with_code, read_entry};
use engine::data::session::PlaySession;
use serde_json::json;

const PROPERTIES: &str = r#"{"properties":{"entry":"flag","serial":"number"}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;
const CODE: &str = r#"
function records()
  for i=1,25 do create{entry=true,serial=i} end
end
function physical()
  create{position={1,1},size={1,1}}
  create{entry=true,position={2,1},size={1,1},velocity={1,0}}
  create{serial=9}
end
"#;

fn entry(exemption: bool) -> String {
    let mut entry = json!({
        "name":"Limits", "scene":{"width":100,"height":100,"background":"#000000"},
        "random_seed":1,"start_screen":"main","max_objects":3,
        "files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
            "screens":"screens.json","code":"code.lua","fonts":{}}
    });
    if exemption {
        entry["max_objects_exempt_flag"] = json!("entry");
    }
    entry.to_string()
}

fn scene() -> String {
    let mut objects = (0..350)
        .map(|n| json!({"entry":true,"serial":n}))
        .collect::<Vec<_>>();
    objects.push(json!({"name":"unmarked_data"}));
    objects.push(json!({"entry":true,"position":[0,0],"size":[1,1],"velocity":[1,0]}));
    json!({"objects":objects}).to_string()
}

fn load(exemption: bool, rules: &str) -> (Game, ScreensConfig) {
    let (game, config, _) = load_game_from_texts_with_code(
        &entry(exemption),
        PROPERTIES,
        &scene(),
        rules,
        SCREENS,
        Some(CODE),
    )
    .expect("fixture loads");
    (game, config)
}

#[test]
fn hundreds_of_records_allow_data_creates_while_physical_creates_keep_the_same_limit() {
    let (mut game, _) = load(true, r#"{"rules":[]}"#);
    game.run_screen_function("records", None, "test");
    assert_eq!(game.world.alive_count(), 377);
    game.run_screen_function("physical", None, "test");
    assert_eq!(game.world.alive_count(), 378);
    assert_eq!(
        game.world
            .ids()
            .filter(|&id| game.world.has(id, property::POSITION))
            .count(),
        2
    );
    game.run_screen_function("records", None, "test");
    assert_eq!(
        game.world.alive_count(),
        403,
        "data still fits while the physical budget is full"
    );
    assert_eq!(
        game.messages()
            .iter()
            .filter(|line| line.contains("потолок объектов"))
            .count(),
        1
    );
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
}

#[test]
fn omission_of_the_field_keeps_the_old_limit_for_all_records() {
    let (mut game, _) = load(false, r#"{"rules":[]}"#);
    game.run_screen_function("records", None, "test");
    game.run_screen_function("physical", None, "test");
    assert_eq!(game.world.alive_count(), 352);
    assert_eq!(
        game.messages()
            .iter()
            .filter(|line| line.contains("потолок объектов"))
            .count(),
        1
    );
}

#[test]
fn rule_spawns_are_physical_even_when_their_template_has_the_exempt_flag() {
    let rules = r#"{"rules":[{"kind":"spawn","when":{"fewer_than":{"count":1000,"of":{"has":["entry"]}}},
        "where":"random_cell","template":{"entry":true,"size":[1,1]}},
        {"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let (mut game, _) = load(true, rules);
    for _ in 0..5 {
        game.step(StepInput::empty());
    }
    assert_eq!(game.world.alive_count(), 353);
    assert_eq!(
        game.world
            .ids()
            .filter(|&id| game.world.has(id, property::POSITION))
            .count(),
        2
    );
    assert!(
        game.world.vec2(351, property::POSITION).unwrap()[0] > 0.0,
        "moving flagged objects remain physical"
    );
}

#[test]
fn editor_session_and_replay_apply_the_same_budget_to_existing_and_new_records() {
    let (mut game, config) = load(true, r#"{"rules":[]}"#);
    let mut state = ScreenState::from_config(&config);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let record = session
        .add_object(&mut game, &[], &json!({"entry":true}))
        .unwrap();
    session
        .add_object(
            &mut game,
            &[],
            &json!({"entry":true,"position":[4,4],"size":[1,1]}),
        )
        .unwrap();
    assert_eq!(
        session
            .add_object(&mut game, &[], &json!({"entry":false}))
            .unwrap_err(),
        "Достигнут предел объектов"
    );
    assert!(
        session
            .add_object(&mut game, &[], &json!({"entry":true}))
            .is_ok()
    );
    assert!(
        edit::add_object(
            &mut game.world,
            &game.properties,
            &[],
            3,
            &json!({"position":[5,5]})
        )
        .is_err()
    );
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        [800.0, 600.0],
        &[],
    );
    let expected = game.world.alive_count();
    let recording = session.recording_text(&game);
    let mut replay = PlaySession::begin_replay(&recording, &mut game, &config, &mut state).unwrap();
    replay.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        [800.0, 600.0],
        &[],
    );
    assert_eq!(game.world.alive_count(), expected);
    edit::set_property(
        &mut game.world,
        &game.properties,
        &[],
        record,
        "position",
        &json!([7, 7]),
    )
    .unwrap();
    assert!(edit::add_object(&mut game.world, &game.properties, &[], 3, &json!({})).is_err());
    edit::remove_property(&mut game.world, &game.properties, record, "position").unwrap();
    edit::delete_object(&mut game.world, 354);
    assert!(
        edit::add_object(&mut game.world, &game.properties, &[], 3, &json!({})).is_err(),
        "unmarked data also uses the budget"
    );
    edit::delete_object(&mut game.world, 353);
    assert!(edit::add_object(&mut game.world, &game.properties, &[], 3, &json!({})).is_ok());
}

#[test]
fn the_exemption_names_an_author_declared_flag_and_rejects_other_json_types() {
    for name in ["absent", "serial", "collides", "position", ""] {
        let mut game_json: serde_json::Value = serde_json::from_str(&entry(true)).unwrap();
        game_json["max_objects_exempt_flag"] = json!(name);
        let failure = load_game_from_texts_with_code(
            &game_json.to_string(),
            PROPERTIES,
            &scene(),
            r#"{"rules":[]}"#,
            SCREENS,
            Some(CODE),
        )
        .err()
        .unwrap();
        assert!(
            failure
                .errors
                .iter()
                .any(|error| error.file == "game.json" && error.path == "max_objects_exempt_flag")
        );
    }
    for value in [json!(3), json!(true), json!(null), json!([])] {
        let mut game_json: serde_json::Value = serde_json::from_str(&entry(true)).unwrap();
        game_json["max_objects_exempt_flag"] = value;
        let failure = read_entry(&game_json.to_string()).err().unwrap();
        assert!(
            failure
                .errors
                .iter()
                .any(|error| error.path == "max_objects_exempt_flag")
        );
    }
}

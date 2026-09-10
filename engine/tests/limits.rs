use engine::core::input::StepInput;
use engine::data::load::load_game_from_texts;

/// A rule that spawns one object per step, forever, hits `max_objects` and starts dropping
/// requests instead of growing without bound.
#[test]
fn max_objects_caps_creation_and_warns_once() {
    let game_json = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"max_objects":3,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json"}}"##;
    let props = r#"{"properties":{"thing":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    // Always fewer than 1000 things, so this fires every single step.
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1000,"of":{"has":["thing"]}}},
         "where":"random_cell","template":{"size":[1,1],"thing":true}}
    ]}"#;
    let (mut game, _warnings) =
        load_game_from_texts(game_json, props, scene, rules).expect("должно загрузиться");

    for _ in 0..10 {
        game.step(StepInput::empty());
    }

    assert_eq!(game.world.alive_count(), 3, "не больше max_objects");
    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| m.contains("потолок"))
            .count(),
        1,
        "сообщение только один раз"
    );
}

/// `random_cell` with a scene that has no free cell left drops the request and warns once,
/// without crashing or looping.
#[test]
fn random_cell_with_no_free_cell_drops_the_request_and_warns_once() {
    let game_json = r##"{"name":"T","scene":{"width":1,"height":1,"background":"#000000"},
"random_seed":1,"max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json"}}"##;
    let props = r#"{"properties":{"thing":"flag"}}"#;
    // The single cell of a 1x1 scene is already occupied.
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"collides":true}]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1000,"of":{"has":["thing"]}}},
         "where":"random_cell","template":{"size":[1,1],"collides":true,"thing":true}}
    ]}"#;
    let (mut game, _warnings) =
        load_game_from_texts(game_json, props, scene, rules).expect("должно загрузиться");

    for _ in 0..5 {
        game.step(StepInput::empty());
    }

    assert_eq!(
        game.world.alive_count(),
        1,
        "заявки отброшены, свободной клетки нет"
    );
    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| m.contains("random_cell"))
            .count(),
        1,
        "сообщение только один раз"
    );
}

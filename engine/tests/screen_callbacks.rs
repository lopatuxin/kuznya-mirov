#[path = "ui_support/mod.rs"]
mod support;

use engine::core::interface::{self, Part};
use engine::core::screens::Element;
use engine::data::session::PlaySession;
use support::{Files, Ui, VIEWPORT};

const PROPERTIES: &str = r#"{"properties":{"item":"flag","taken":"flag","n":"number","preview":"number","enters":"number","leaves":"number","clicks":"number"}}"#;
const SCENE: &str = r#"{"objects":[
    {"name":"status","preview":-1,"enters":0,"leaves":0,"clicks":0},
    {"name":"a","item":true,"n":1},{"name":"b","item":true,"n":2},
    {"name":"c","item":true,"n":3},{"name":"d","item":true,"n":4},
    {"name":"e","item":true,"n":5},{"name":"f","item":true,"n":6},
    {"name":"g","item":true,"n":7}] }"#;
const CODE: &str = r#"
function status() return find{has={"enters"}}[1] end
function open() status().enters = status().enters + 1 end
function preview(o) status().preview = o.n end
function top(o) status().preview = o.n + 10 end
function leave(o)
  status().preview = -1
  status().leaves = status().leaves + 1
end
function take(o)
  if status().preview ~= -1 then error("preview was not cleared") end
  status().clicks = status().clicks + 1
  o.taken = true
end
"#;

fn screens(columns: Option<&str>) -> String {
    let columns = columns
        .map(|n| format!(r#", "columns":{n}"#))
        .unwrap_or_default();
    format!(
        r##"{{"screens":[
      {{"name":"main","world_runs":true,"on_enter":["run","open"],
        "keys":{{"KeyC":["show_screen","book"]}},"elements":[]}},
      {{"name":"book","world_runs":false,"on_enter":["run","open"],
        "keys":{{"Escape":["resume"]}},"elements":[
          {{"kind":"list","anchor":"top_left","offset":[0,0],"size":[300,40],
            "for":{{"has":["item"],"without":["taken"]}},"row_height":20{columns},"row":[
              {{"kind":"button","offset":[0,0],"size":[100,20],"text":"{{n}}","font":"ui",
                "color":"#ffffff","on_hover":["run","preview"],"on_leave":["run","leave"],
                "on_click":["run","take"]}}
            ]}}
        ]}}
    ]}}"##
    )
}

fn load() -> Ui {
    Files::new(PROPERTIES, SCENE, &screens(Some("3")))
        .with_code(CODE)
        .load()
}

fn stat(ui: &Ui, name: &str) -> f64 {
    ui.number(ui.id_of("status"), name).unwrap()
}

#[test]
fn entering_is_synchronous_before_world_steps_and_reentering_runs_again() {
    let mut ui = load();
    let _session = PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    assert_eq!(stat(&ui, "enters"), 1.0);
    ui.show("book");
    assert_eq!(stat(&ui, "enters"), 2.0);
    assert_eq!(ui.game.step_count(), 0);
    ui.tick(0.0);
    ui.tick(1.0);
    assert_eq!(stat(&ui, "enters"), 2.0);
    assert_eq!(ui.game.step_count(), 0);
    ui.key("Escape");
    assert_eq!(stat(&ui, "enters"), 3.0);
    ui.show("book");
    assert_eq!(stat(&ui, "enters"), 4.0);
}

#[test]
fn equal_columns_scroll_by_visual_rows_including_a_partial_last_row() {
    let mut ui = load();
    ui.show("book");
    let positions = |ui: &Ui| {
        interface::layout(
            &ui.config.screens[ui.state.active()],
            ui.state.active(),
            &ui.game.world,
            &ui.state,
            VIEWPORT,
        )
        .into_iter()
        .filter_map(|part| match part {
            Part::Element(placed) => Some((placed.object.unwrap(), placed.rect)),
            _ => None,
        })
        .collect::<Vec<_>>()
    };
    let first = positions(&ui);
    assert_eq!(first.len(), 6);
    assert_eq!(first[1].1, [100.0, 0.0, 100.0, 20.0]);
    assert_eq!(first[3].1, [0.0, 20.0, 100.0, 20.0]);
    assert!(ui.wheel(10.0, 10.0, 100.0));
    let last = positions(&ui);
    assert_eq!(last.len(), 4);
    assert_eq!(last[0].0, ui.id_of("d"));
    assert_eq!(last[3], (ui.id_of("g"), [0.0, 20.0, 100.0, 20.0]));
    let plain = Files::new(PROPERTIES, SCENE, &screens(None))
        .with_code(CODE)
        .load();
    assert!(matches!(
        plain.config.screens[1].elements[0],
        Element::List { columns: 1, .. }
    ));
}

#[test]
fn paused_hover_follows_scroll_object_changes_clicks_and_screen_changes() {
    let mut ui = load();
    ui.show("book");
    ui.move_to(10.0, 10.0);
    assert_eq!(stat(&ui, "preview"), 1.0);
    ui.tick(0.0);
    assert_eq!(stat(&ui, "leaves"), 0.0);
    ui.wheel(10.0, 10.0, 100.0);
    ui.tick(0.0);
    assert_eq!(stat(&ui, "preview"), 4.0);
    assert_eq!(stat(&ui, "leaves"), 1.0);
    ui.move_to(-1.0, -1.0);
    assert_eq!(stat(&ui, "preview"), -1.0);
    ui.click(10.0, 10.0);
    assert_eq!(stat(&ui, "clicks"), 1.0);
    assert!(ui.flag(ui.id_of("d"), "taken"));
    assert_eq!(stat(&ui, "preview"), -1.0);
    ui.move_to(110.0, 10.0);
    assert_eq!(stat(&ui, "preview"), 2.0);
    let taken = ui.game.properties.resolve("taken").unwrap();
    ui.game.world.set_flag(ui.id_of("b"), taken, true);
    ui.tick(0.0);
    assert_eq!(stat(&ui, "preview"), 3.0);
    ui.key("Escape");
    assert_eq!(stat(&ui, "preview"), -1.0);
    assert_eq!(ui.game.step_count(), 0);
    ui.show("book");
    ui.tick(0.0);
    assert_eq!(stat(&ui, "preview"), 3.0);
}

#[test]
fn callbacks_share_click_hit_testing_and_locked_buttons_block_hover() {
    let mut json: serde_json::Value = serde_json::from_str(&screens(Some("3"))).unwrap();
    let mut top = json["screens"][1]["elements"][0]["row"][0].clone();
    top["on_hover"] = serde_json::json!(["run", "top"]);
    json["screens"][1]["elements"][0]["row"]
        .as_array_mut()
        .unwrap()
        .push(top);
    let mut ui = Files::new(PROPERTIES, SCENE, &json.to_string())
        .with_code(CODE)
        .load();
    ui.show("book");
    ui.move_to(1.0, 1.0);
    assert_eq!(stat(&ui, "preview"), 11.0);
    ui.press(1.0, 1.0);
    let Element::List { row, .. } = &mut ui.config.screens[1].elements[0] else {
        panic!("list");
    };
    let Element::Button { needs_choice, .. } = &mut row[1] else {
        panic!("button");
    };
    *needs_choice = Some("empty".to_string());
    ui.tick(0.0);
    ui.release();
    assert_eq!(stat(&ui, "clicks"), 0.0);
    assert_eq!(stat(&ui, "preview"), -1.0);
    assert_eq!(stat(&ui, "leaves"), 1.0);
    ui.move_to(101.0, 1.0);
    assert_eq!(stat(&ui, "preview"), -1.0);
}

#[test]
fn leaving_the_window_clears_a_plain_object_button_even_outside_the_viewport() {
    let mut json: serde_json::Value = serde_json::from_str(&screens(Some("3"))).unwrap();
    let mut button = json["screens"][1]["elements"][0]["row"][0].clone();
    button["anchor"] = serde_json::json!("top_left");
    button["offset"] = serde_json::json!([-10, -10]);
    button["for"] = serde_json::json!({"has":["item"]});
    json["screens"][1]["elements"] = serde_json::json!([button]);
    let mut ui = Files::new(PROPERTIES, SCENE, &json.to_string())
        .with_code(CODE)
        .load();
    ui.show("book");
    ui.move_to(-1.0, -1.0);
    assert_eq!(stat(&ui, "preview"), 1.0);
    ui.queue.push_mouse_leave();
    ui.tick(0.0);
    assert_eq!(stat(&ui, "preview"), -1.0);
    ui.tick(0.0);
    assert_eq!(stat(&ui, "leaves"), 1.0);
    ui.move_to(1.0, 1.0);
    assert_eq!(stat(&ui, "preview"), 1.0);
}

#[test]
fn columns_and_callbacks_are_validated_before_start() {
    for invalid in ["0", "-1", "1.5", "\"3\""] {
        let json = screens(Some(invalid));
        let failure = Files::new(PROPERTIES, SCENE, &json)
            .with_code(CODE)
            .try_load()
            .err()
            .unwrap();
        assert!(
            failure
                .errors
                .iter()
                .any(|error| error.path.ends_with("columns"))
        );
    }
    for field in ["on_enter", "on_hover", "on_leave"] {
        for invalid in [r#"["resume"]"#, r#"["run","absent"]"#] {
            let mut json: serde_json::Value = serde_json::from_str(&screens(Some("3"))).unwrap();
            let value = serde_json::from_str(invalid).unwrap();
            if field == "on_enter" {
                json["screens"][1][field] = value;
            } else {
                json["screens"][1]["elements"][0]["row"][0][field] = value;
            }
            let text = json.to_string();
            let failure = Files::new(PROPERTIES, SCENE, &text)
                .with_code(CODE)
                .try_load()
                .err()
                .unwrap();
            assert!(
                failure
                    .errors
                    .iter()
                    .any(|error| error.path.ends_with(field)),
                "{field}: {:?}",
                failure.errors
            );
        }
    }
}

#[test]
fn paused_ui_records_hover_leave_click_and_enter_once_for_replay() {
    let mut ui = load();
    let mut session = PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    let drain = |ui: &mut Ui, session: &mut PlaySession| {
        session.process_ui(
            &mut ui.queue,
            &mut ui.mouse,
            &mut ui.game,
            &ui.config,
            &mut ui.state,
            VIEWPORT,
            &[],
        );
    };
    ui.queue.push_key_up("KeyC");
    drain(&mut ui, &mut session);
    ui.queue.push_mouse_move(10.0, 10.0);
    drain(&mut ui, &mut session);
    ui.wheel(10.0, 10.0, 100.0);
    drain(&mut ui, &mut session);
    ui.queue.push_mouse_down();
    ui.queue.push_mouse_up();
    drain(&mut ui, &mut session);
    ui.queue.push_key_up("Escape");
    drain(&mut ui, &mut session);
    session.step_once(
        &mut ui.queue,
        &mut ui.mouse,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
        VIEWPORT,
        &[],
    );
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
    let expected = (
        stat(&ui, "enters"),
        stat(&ui, "leaves"),
        stat(&ui, "clicks"),
        stat(&ui, "preview"),
    );
    let text = session.recording_text(&ui.game);
    assert!(text.contains("preview"), "{text}");
    let mut replay =
        PlaySession::begin_replay(&text, &mut ui.game, &ui.config, &mut ui.state).unwrap();
    replay.step_once(
        &mut ui.queue,
        &mut ui.mouse,
        &mut ui.game,
        &ui.config,
        &mut ui.state,
        VIEWPORT,
        &[],
    );
    assert_eq!(
        (
            stat(&ui, "enters"),
            stat(&ui, "leaves"),
            stat(&ui, "clicks"),
            stat(&ui, "preview")
        ),
        expected,
        "{text}"
    );
    assert!(ui.flag(ui.id_of("d"), "taken"));
}

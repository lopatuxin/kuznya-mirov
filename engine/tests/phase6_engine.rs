//! Фаза 06 — новые общие средства движка (требования 1–33), вне контекста конкретной игры.
//! `games/*` уже покрыты полной загрузкой (`*_demo.rs`) и записанным вводом (`replays.rs`);
//! здесь — прицельные тесты на сам механизм: `timer`, `rotation`, `follow_mouse`, `check`,
//! составные условия, `set`/`give`/`take` в `do`, число-таблица/множитель, `shift`/`turn`,
//! создание по клетке/`pick_one`, курсор в мире, `timer`/`rotation` в коде, начальные значения
//! `new_game`, и часть ошибок проверки перед запуском.

use engine::core::input::StepInput;
use engine::core::property;
use engine::core::value::Value;
use engine::data::load::{load_game_from_texts, load_game_from_texts_with_code};

const GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
const GAME_CODE: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","code":"code.lua","fonts":{}}}"##;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

fn err_text(result: &Result<impl std::fmt::Debug, engine::data::load::LoadFailure>) -> String {
    match result {
        Err(f) => f
            .errors
            .iter()
            .map(|e| e.message.clone())
            .collect::<Vec<_>>()
            .join(" | "),
        Ok(_) => String::new(),
    }
}

// ---------------------------------------------------------------------------------------------
// timer — требование 1
// ---------------------------------------------------------------------------------------------

#[test]
fn timer_counts_down_by_one_step_and_stops_at_zero() {
    let props = r#"{"properties":{"t":"timer"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"t":0.05}]}"#; // 3 steps
    let rules = r#"{"rules":[]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let t = game.properties.resolve("t").unwrap();
    for expected in [2, 1, 0, 0] {
        game.step(StepInput::empty());
        assert_eq!(game.world.timer(0, t), Some(expected));
    }
}

#[test]
fn timer_add_and_set_write_seconds_and_clamp_negative_to_zero() {
    let props = r#"{"properties":{"marker":"flag","t":"timer"}}"#;
    let scene = r#"{"objects":[
        {"position":[-100,-100],"size":[1,1],"marker":true},
        {"position":[0,0],"size":[1,1],"t":1.0}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["marker"]},"when":"outside_scene",
         "do":[["add","t",-2.0]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let t = game.properties.resolve("t").unwrap();
    game.step(StepInput::empty());
    // 60 steps (1s) - 1 tick - 120 steps (2s delta) clamps to 0, not negative.
    assert_eq!(game.world.timer(1, t), Some(0));
}

#[test]
fn negative_timer_in_scene_is_a_prestart_error() {
    let props = r#"{"properties":{"t":"timer"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"t":-1.0}]}"#;
    let rules = r#"{"rules":[]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
    assert!(err_text(&result).contains("timer"), "{}", err_text(&result));
}

/// «Формат игры», требование 1: сравнение с `timer` в условии — в секундах, как у `time`, а не
/// в шагах (`World::number_like` отдаёт для `timer` шаги, порог переводится при разборе).
#[test]
fn timer_compare_condition_is_in_seconds_not_steps() {
    let props = r#"{"properties":{"t":"timer","n":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"t":1.0,"n":0}]}"#; // 60 steps
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["n"]},"when":["t","<",0.5],"do":[["add","n",1]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    let mut first_hit = None;
    for step in 1..=60 {
        game.step(StepInput::empty());
        if first_hit.is_none() && game.world.number_like(0, n) != Some(0.0) {
            first_hit = Some(step);
        }
    }
    // t < 0.5s истинно, когда осталось меньше 30 шагов — впервые после шага 31.
    assert_eq!(first_hit, Some(31));
}

// ---------------------------------------------------------------------------------------------
// rotation, follow_mouse — требования 2–3
// ---------------------------------------------------------------------------------------------

#[test]
fn rotation_not_one_of_four_values_is_a_prestart_error() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"rotation":45}]}"#;
    let rules = r#"{"rules":[]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
    assert!(err_text(&result).contains("rotation"));
}

#[test]
fn follow_mouse_without_size_is_a_prestart_error() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"follow_mouse":"x"}]}"#;
    let rules = r#"{"rules":[]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
    assert!(err_text(&result).contains("follow_mouse"));
}

#[test]
fn follow_mouse_moves_the_object_only_on_the_step_the_cursor_actually_moved() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[2,2],"follow_mouse":"xy"}]}"#;
    let rules = r#"{"rules":[]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    // Cursor never moved: object stays put.
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([0.0, 0.0]));

    game.set_cursor_cell([5.0, 5.0]);
    let snap = game.take_input_snapshot();
    game.step(snap);
    assert_eq!(game.world.vec2(0, property::POSITION), Some([4.0, 4.0]));

    // Cursor did not move since the last step it was delivered to — no further snap.
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([4.0, 4.0]));
}

// ---------------------------------------------------------------------------------------------
// check, all/any/not — требования 4–9
// ---------------------------------------------------------------------------------------------

#[test]
fn check_do_runs_once_per_matching_object_and_a_candidate_that_stops_matching_is_skipped() {
    let props = r#"{"properties":{"active":"flag","fired":"number"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"active":true,"fired":0},
        {"position":[1,0],"size":[1,1],"active":true,"fired":0}
    ]}"#;
    // «Правила игры», требование 5: candidates are fixed up front (both here), but a
    // candidate's own turn re-checks `for` — `take` (TakeAll) clears `active` from every
    // current holder the first time `do` runs, so the second candidate no longer matches `for`
    // by the time its own turn comes and its own `do` never runs at all. `run` gets exactly the
    // checked object, unlike `add`/`set`/`give`/`take`, which all reach every holder/selector
    // match regardless of which candidate triggered them — the only precise per-candidate
    // witness available.
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["active"]},
         "do":[["run","mark"],["take","active"]]}
    ]}"#;
    let code = "function mark(obj) obj.fired = obj.fired + 1 end";
    let (mut game, _s, _w) =
        load_game_from_texts_with_code(GAME_CODE, props, scene, rules, SCREENS, Some(code))
            .expect("должно загрузиться");
    let fired = game.properties.resolve("fired").unwrap();
    let active = game.properties.resolve("active").unwrap();
    game.step(StepInput::empty());
    assert!(game.is_running(), "{:?}", game.code_error());
    assert_eq!(
        game.world.number_like(0, fired),
        Some(1.0),
        "первый кандидат — do выполнен"
    );
    assert_eq!(
        game.world.number_like(1, fired),
        Some(0.0),
        "второй кандидат уже не подходит под for к своей очереди — do пропущен"
    );
    assert!(!game.world.has(0, active));
    assert!(!game.world.has(1, active), "take снял active у обоих разом");
}

#[test]
fn check_when_absent_means_always_true() {
    let props = r#"{"properties":{"n":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"n":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["n"]},"do":[["add","n",1]]}]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    game.step(StepInput::empty());
    assert_eq!(game.world.number_like(0, n), Some(1.0));
}

#[test]
fn check_missing_do_is_a_prestart_error() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":[]}}]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
}

/// «Правила игры», требование 6: `all`/`any`/`not` nest and combine correctly — one object per
/// game (`add` applies to every holder of a property, so a shared scene would let a second
/// object's own firing add onto the first; a lone object sidesteps that entirely).
fn all_any_not_case(a: i32, b: i32) -> f64 {
    let props = r#"{"properties":{"a":"number","b":"number","n":"number"}}"#;
    let scene =
        format!(r#"{{"objects":[{{"position":[0,0],"size":[1,1],"a":{a},"b":{b},"n":0}}]}}"#);
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["n"]},
         "when":{"any":[{"all":[["a","==",1],{"not":["b","==",1]}]},{"not":["a","==",1]}]},
         "do":[["add","n",1]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, &scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    game.step(StepInput::empty());
    game.world.number_like(0, n).unwrap()
}

#[test]
fn all_any_not_conditions_nest_and_combine() {
    assert_eq!(all_any_not_case(1, 1), 0.0, "a and not b — ложно для (1,1)");
    assert_eq!(
        all_any_not_case(1, 0),
        1.0,
        "a and not b — истинно для (1,0)"
    );
    assert_eq!(all_any_not_case(0, 0), 1.0, "not a — истинно для (0,0)");
}

#[test]
fn empty_all_is_a_prestart_error() {
    let props = r#"{"properties":{"n":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"n":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["n"]},"when":{"all":[]},"do":[]}]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------------------------
// set/give/take в do, число-таблица/множитель — требования 10–11
// ---------------------------------------------------------------------------------------------

#[test]
fn set_give_take_in_do_apply_to_every_holder_and_selector_match() {
    let props = r#"{"properties":{"n":"number","flag_a":"flag","target":"flag","trigger":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[-100,-100],"size":[1,1],"trigger":true},
        {"position":[0,0],"size":[1,1],"n":1},
        {"position":[1,0],"size":[1,1],"n":1,"flag_a":true},
        {"position":[2,0],"size":[1,1]}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["trigger"]},"when":"outside_scene",
         "do":[
           ["set","n",9],
           ["give","target",{"has":["n"]}],
           ["take","flag_a"]
         ]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    let target = game.properties.resolve("target").unwrap();
    let flag_a = game.properties.resolve("flag_a").unwrap();
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(1, n),
        Some(9.0),
        "set пишет всем носителям n"
    );
    assert_eq!(game.world.number_like(2, n), Some(9.0));
    assert!(game.world.has(1, target), "give по отбору has=[n]");
    assert!(game.world.has(2, target));
    assert!(
        !game.world.has(3, target),
        "у объекта 3 нет n — give его не коснулся"
    );
    assert!(!game.world.has(2, flag_a), "take снимает у всех носителей");
}

/// Воспроизведённый баг: `target` не стоит ни у кого в `scene.json` и ни в одном шаблоне
/// «создать» — только `check`-правило раздаёт его через `give` в `do`. До фикса
/// `widen_shapes_by_rule_actions` знал только про `give`/`take`/`set` в `effects` столкновения,
/// не в `do` любого другого правила, и `move`-правило с отбором `has:["target"]` получало ложное
/// «отбор заведомо не подходит ни одному объекту».
#[test]
fn give_in_a_check_rules_do_widens_shapes_so_a_later_selector_is_not_reported_as_matching_nobody() {
    let props = r#"{"properties":{"tag":"flag","target":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":[0,0],"tag":true}]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["tag"]},"do":[["give","target",{"has":["tag"]}]]},
        {"kind":"move","for":{"has":["target"]}}
    ]}"#;
    let (mut game, _s, warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "give в do check-правила должен избавить отбор move от ложного предупреждения: {warnings:?}"
    );
    let target = game.properties.resolve("target").unwrap();
    game.step(StepInput::empty());
    assert!(game.world.has(0, target), "give в do реально сработал");
}

/// Тот же механизм, но `give` спрятан в `if_blocked` внутри `shift` — виден он должен быть точно
/// так же, как прямо в `do`.
#[test]
fn give_inside_an_if_blocked_widens_shapes_so_a_later_selector_is_not_reported_as_matching_nobody()
{
    let props = r#"{"properties":{"stuck":"flag","falling":"flag","wall":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"falling":true,"collides":true},
        {"position":[1,0],"size":[1,1],"wall":true,"collides":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["falling"]},"do":[
            ["shift",{"group":{"has":["falling"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                      "if_blocked":[["give","stuck",{"has":["falling"]}]]}]
        ]},
        {"kind":"move","for":{"has":["stuck"]}}
    ]}"#;
    let (_game, _s, warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "give в if_blocked должен избавить отбор move от ложного предупреждения: {warnings:?}"
    );
}

#[test]
fn number_table_picks_by_floor_and_clamps_at_the_ends() {
    let props = r#"{"properties":{"idx":"number","n":"number","trigger":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[-100,-100],"size":[1,1],"trigger":true},
        {"position":[0,0],"size":[1,1],"idx":-5,"n":0},
        {"position":[1,0],"size":[1,1],"idx":1.9,"n":0},
        {"position":[2,0],"size":[1,1],"idx":99,"n":0}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["trigger"]},"when":"outside_scene",
         "do":[["add","n",{"table":[10,20,30],"by":"idx"}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    game.step(StepInput::empty());
    assert_eq!(
        game.world.number_like(1, n),
        Some(10.0),
        "меньше нуля — первый элемент"
    );
    assert_eq!(
        game.world.number_like(2, n),
        Some(20.0),
        "1.9 округляется вниз до 1"
    );
    assert_eq!(
        game.world.number_like(3, n),
        Some(30.0),
        "за концом — последний элемент"
    );
}

#[test]
fn number_multiplier_uses_the_actors_own_property() {
    let props = r#"{"properties":{"mult":"number","n":"number","trigger":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[-100,-100],"size":[1,1],"trigger":true},
        {"position":[0,0],"size":[1,1],"mult":3,"n":0}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["trigger"]},"when":"outside_scene",
         "do":[["add","n",{"value":5,"times":"mult"}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    game.step(StepInput::empty());
    assert_eq!(game.world.number_like(1, n), Some(15.0));
}

#[test]
fn empty_table_is_a_prestart_error() {
    let props = r#"{"properties":{"n":"number","idx":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"n":0,"idx":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["n"]},"do":[["add","n",{"table":[],"by":"idx"}]]}]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------------------------
// shift, turn — требования 12–18
// ---------------------------------------------------------------------------------------------

#[test]
fn shift_moves_the_whole_group_and_marks_moved_for_after_move_of() {
    let props = r#"{"properties":{"g":"flag","trig":"flag","moved_out":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true,"trig":true},
        {"position":[1,0],"size":[1,1],"g":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["trig"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[2,3]}]]},
        {"kind":"check","for":{"has":["g"]},
         "when":{"after_move_of":{"has":["g"]}},
         "do":[["give","moved_out",{"has":["g"]}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let moved_out = game.properties.resolve("moved_out").unwrap();
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([2.0, 3.0]));
    assert_eq!(game.world.vec2(1, property::POSITION), Some([3.0, 3.0]));
    assert!(
        game.world.has(0, moved_out) && game.world.has(1, moved_out),
        "after_move_of должен увидеть перемещение shift этого же шага"
    );
}

#[test]
fn shift_blocked_reverts_everything_and_runs_if_blocked_touching_is_not_blocked() {
    let props = r#"{"properties":{"g":"flag","wall":"flag","blocked_ran":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true},
        {"position":[2,0],"size":[1,1],"wall":true}
    ]}"#;
    // Shifting by 1 lands the group's right edge exactly on the wall's left edge (touching, no
    // overlap) — must NOT block.
    let rules_touch = r#"{"rules":[
        {"kind":"check","for":{"has":["g"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                          "if_blocked":[["give","blocked_ran",{"has":["g"]}]]}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules_touch, SCREENS).expect("должно загрузиться");
    let blocked_ran = game.properties.resolve("blocked_ran").unwrap();
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([1.0, 0.0]));
    assert!(!game.world.has(0, blocked_ran), "касание краями — не упор");

    // Now a real overlap (dx smaller than the gap): must revert and run if_blocked.
    let scene2 = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true},
        {"position":[0.5,0],"size":[1,1],"wall":true}
    ]}"#;
    let rules_block = r#"{"rules":[
        {"kind":"check","for":{"has":["g"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[1,0],"blocked_by":{"has":["wall"]},
                          "if_blocked":[["give","blocked_ran",{"has":["g"]}]]}]]}
    ]}"#;
    let (mut game2, _s2, _w2) = load_game_from_texts(GAME, props, scene2, rules_block, SCREENS)
        .expect("должно загрузиться");
    game2.step(StepInput::empty());
    assert_eq!(
        game2.world.vec2(0, property::POSITION),
        Some([0.0, 0.0]),
        "упор — позиция возвращена"
    );
    assert!(
        game2.world.has(0, blocked_ran),
        "упор — if_blocked выполняется"
    );
}

#[test]
fn shift_ignores_a_blocked_by_match_that_is_itself_in_the_group() {
    let props = r#"{"properties":{"g":"flag","trig":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true,"trig":true},
        {"position":[1,0],"size":[1,1],"g":true}
    ]}"#;
    // Both group members also match blocked_by (has=g) — the second sits exactly where the
    // first would overlap it if it counted, but a group member must not block its own group.
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["trig"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[0.5,0],"blocked_by":{"has":["g"]}}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([0.5, 0.0]));
}

#[test]
fn turn_rotates_the_group_around_the_pivots_center_clockwise_and_counterclockwise() {
    let props = r#"{"properties":{"g":"flag","p":"flag"}}"#;
    // Pivot at [0,0]-[1,1] (center 0.5,0.5); one other cube directly to its right.
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"g":true,"p":true},
        {"position":[1,0],"size":[1,1],"g":true}
    ]}"#;
    let rules_cw = r#"{"rules":[
        {"kind":"check","for":{"has":["p"]},
         "do":[["turn",{"group":{"has":["g"]},"around":{"has":["p"]},"dir":1}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules_cw, SCREENS).expect("должно загрузиться");
    game.step(StepInput::empty());
    // Right neighbor rotated clockwise around the pivot's own center should end up below it.
    assert_eq!(
        game.world.vec2(0, property::POSITION),
        Some([0.0, 0.0]),
        "пивот не сдвигается"
    );
    assert_eq!(game.world.vec2(1, property::POSITION), Some([0.0, 1.0]));
    let rotation_prop = property::ROTATION;
    assert_eq!(
        game.world.rotation(1, rotation_prop).map(|r| r.degrees()),
        Some(90)
    );
}

#[test]
fn turn_with_no_around_match_does_nothing_and_skips_if_blocked() {
    let props = r#"{"properties":{"g":"flag","ran":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"g":true}]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["g"]},
         "do":[["turn",{"group":{"has":["g"]},"around":{"has":["ran"]},"dir":1,
                        "blocked_by":{"has":["ran"]},"if_blocked":[["give","ran",{"has":["g"]}]]}]]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let ran = game.properties.resolve("ran").unwrap();
    game.step(StepInput::empty());
    assert_eq!(game.world.vec2(0, property::POSITION), Some([0.0, 0.0]));
    assert!(
        !game.world.has(0, ran),
        "around не нашёл никого — if_blocked не выполняется"
    );
}

#[test]
fn shift_with_if_blocked_but_no_blocked_by_is_a_prestart_error() {
    let props = r#"{"properties":{"g":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"check","for":{"has":["g"]},
         "do":[["shift",{"group":{"has":["g"]},"by":[1,0],
                          "if_blocked":[["give","g",{"has":["g"]}]]}]]}
    ]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------------------------
// создание по клетке и pick_one — требования 19–22
// ---------------------------------------------------------------------------------------------

#[test]
fn spawn_where_at_places_at_the_literal_cell() {
    let props = r#"{"properties":{"born":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["born"]}}},
         "where":{"at":[3,4]},"template":{"size":[1,1],"born":true}}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    game.step(StepInput::empty());
    let born = game.properties.resolve("born").unwrap();
    let id = game
        .world
        .ids()
        .find(|&id| game.world.has(id, born))
        .unwrap();
    assert_eq!(game.world.vec2(id, property::POSITION), Some([3.0, 4.0]));
}

#[test]
fn pick_one_layers_template_variant_and_cell_fields() {
    let props = r#"{"properties":{"born":"flag","tag":"number"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["born"]}}},
         "where":{"at":[0,0]},"template":{"size":[1,1],"born":true,"tag":1},
         "pick_one":[
           {"cells":[[0,0],{"at":[1,0],"tag":9}]}
         ]}
    ]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    game.step(StepInput::empty());
    let born = game.properties.resolve("born").unwrap();
    let tag = game.properties.resolve("tag").unwrap();
    let mut cells: Vec<_> = game
        .world
        .ids()
        .filter(|&id| game.world.has(id, born))
        .map(|id| {
            (
                game.world.vec2(id, property::POSITION).unwrap(),
                game.world.number_like(id, tag),
            )
        })
        .collect();
    cells.sort_by(|a, b| a.0[0].partial_cmp(&b.0[0]).unwrap());
    assert_eq!(
        cells,
        vec![([0.0, 0.0], Some(1.0)), ([1.0, 0.0], Some(9.0))]
    );
}

#[test]
fn empty_pick_one_is_a_prestart_error() {
    let props = r#"{"properties":{"born":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["born"]}}},
         "where":{"at":[0,0]},"template":{"size":[1,1],"born":true},"pick_one":[]}
    ]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(result.is_err());
}

/// Воспроизведённый баг: проверка «`from_parent` без `after_move_of`» (требование 7) смотрела
/// только `template`, не поля варианта или клетки `pick_one` — так `from_parent` у клетки/варианта
/// без родителя молча проходило.
#[test]
fn pick_one_variant_field_from_parent_without_after_move_of_is_a_prestart_error() {
    let props = r#"{"properties":{"born":"flag","tag":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["born"]}}},
         "where":{"at":[0,0]},"template":{"size":[1,1],"born":true},
         "pick_one":[
           {"tag":{"from_parent":"tag"},"cells":[[0,0]]}
         ]}
    ]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(
        err_text(&result).contains("from_parent недоступен"),
        "{}",
        err_text(&result)
    );
}

#[test]
fn pick_one_cell_field_from_parent_without_after_move_of_is_a_prestart_error() {
    let props = r#"{"properties":{"born":"flag","tag":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["born"]}}},
         "where":{"at":[0,0]},"template":{"size":[1,1],"born":true},
         "pick_one":[
           {"cells":[{"at":[0,0],"tag":{"from_parent":"tag"}}]}
         ]}
    ]}"#;
    let result = load_game_from_texts(GAME, props, scene, rules, SCREENS);
    assert!(
        err_text(&result).contains("from_parent недоступен"),
        "{}",
        err_text(&result)
    );
}

// ---------------------------------------------------------------------------------------------
// timer/rotation/follow_mouse в коде — требование 28
// ---------------------------------------------------------------------------------------------

#[test]
fn code_reads_and_writes_timer_in_seconds_and_clamps_negative_to_zero() {
    let props = r#"{"properties":{"t":"timer"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"t":1.0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["t"]},"do":[["run","touch"]]}]}"#;
    let code = "function touch(obj) obj.t = obj.t - 3 end";
    let (mut game, _s, _w) =
        load_game_from_texts_with_code(GAME_CODE, props, scene, rules, SCREENS, Some(code))
            .expect("должно загрузиться");
    game.step(StepInput::empty());
    let t = game.properties.resolve("t").unwrap();
    assert_eq!(game.world.timer(0, t), Some(0), "отрицательное — ноль");
}

#[test]
fn code_writing_rotation_not_one_of_four_is_a_runtime_code_error() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"rotation":0}]}"#;
    let rules = r#"{"rules":[{"kind":"check","for":{"has":["rotation"]},"do":[["run","touch"]]}]}"#;
    let code = "function touch(obj) obj.rotation = 45 end";
    let (mut game, _s, _w) =
        load_game_from_texts_with_code(GAME_CODE, props, scene, rules, SCREENS, Some(code))
            .expect("должно загрузиться");
    game.step(StepInput::empty());
    assert!(game.code_error().is_some());
}

#[test]
fn code_reading_follow_mouse_is_a_runtime_code_error() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"follow_mouse":"x"}]}"#;
    let rules =
        r#"{"rules":[{"kind":"check","for":{"has":["follow_mouse"]},"do":[["run","touch"]]}]}"#;
    let code = "function touch(obj) local x = obj.follow_mouse end";
    let (mut game, _s, _w) =
        load_game_from_texts_with_code(GAME_CODE, props, scene, rules, SCREENS, Some(code))
            .expect("должно загрузиться");
    game.step(StepInput::empty());
    assert!(game.code_error().is_some());
}

// ---------------------------------------------------------------------------------------------
// начальные значения у new_game — требование 29
// ---------------------------------------------------------------------------------------------

#[test]
fn new_game_with_values_writes_over_the_freshly_built_world_and_can_add_a_new_property() {
    let props = r#"{"properties":{"n":"number"}}"#;
    let scene = r#"{"objects":[{"name":"obj","position":[0,0],"size":[1,1]}]}"#;
    let rules = r#"{"rules":[]}"#;
    let (mut game, _s, _w) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let n = game.properties.resolve("n").unwrap();
    assert!(
        !game.world.has(0, n),
        "свойства n ещё нет — объект его не объявлял"
    );
    game.new_game_with_values(&[("obj".to_string(), n, Value::Number(42.0))]);
    assert_eq!(
        game.world.number_like(0, n),
        Some(42.0),
        "у объекта появилось новое свойство"
    );
}

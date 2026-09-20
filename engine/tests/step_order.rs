use engine::core::input::StepInput;
use engine::data::load::load_game_from_texts;

const GAME: &str = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":10,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

/// «Формат игры»: перевод секунд в шаги для длительностей зажат снизу единицей, но
/// `["add", "<time>", число]` — это дельта, не длительность: знак и ноль должны сохраняться.
#[test]
fn do_add_time_delta_keeps_sign_and_zero() {
    let props = r#"{"properties":{"dummy":"flag","c_pos":"time","c_zero":"time","c_neg":"time"}}"#;
    let scene = r#"{"objects":[
        {"position":[-100,-100],"size":[1,1],"dummy":true},
        {"position":[0,0],"size":[1,1],"c_pos":0.5,"c_zero":0.5,"c_neg":1.0}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["dummy"]},"when":"outside_scene",
         "do":[["add","c_pos",0.12],["add","c_zero",0],["add","c_neg",-0.5]]}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    let c_pos = game.properties.resolve("c_pos").unwrap();
    let c_zero = game.properties.resolve("c_zero").unwrap();
    let c_neg = game.properties.resolve("c_neg").unwrap();
    let id = game.world.ids().next().unwrap();
    assert_eq!(
        game.world.time(id, c_pos),
        Some(37),
        "положительная дельта: 30 + 7"
    );
    assert_eq!(
        game.world.time(id, c_zero),
        Some(30),
        "нулевая дельта не должна становиться +1"
    );
    assert_eq!(
        game.world.time(id, c_neg),
        Some(30),
        "отрицательная дельта: 60 - 30, а не 60 + 1"
    );
}

/// Тот же запрет на зажим, только через `effects` правила «столкнуть», а не через `do`.
#[test]
fn collide_effect_add_time_delta_keeps_sign_and_zero() {
    let props =
        r#"{"properties":{"counter":"flag","c_pos":"time","c_zero":"time","c_neg":"time"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"counter":true,
         "c_pos":0.5,"c_zero":0.5,"c_neg":1.0},
        {"position":[0,0],"size":[1,1],"collides":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["collides","counter"]},"b":{"has":["collides"]},
         "effects":{"a":[["add","c_pos",0.12],["add","c_zero",0],["add","c_neg",-0.5]]}}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    let c_pos = game.properties.resolve("c_pos").unwrap();
    let c_zero = game.properties.resolve("c_zero").unwrap();
    let c_neg = game.properties.resolve("c_neg").unwrap();
    assert_eq!(
        game.world.time(0, c_pos),
        Some(37),
        "положительная дельта: 30 + 7"
    );
    assert_eq!(
        game.world.time(0, c_zero),
        Some(30),
        "нулевая дельта не должна становиться +1"
    );
    assert_eq!(
        game.world.time(0, c_neg),
        Some(30),
        "отрицательная дельта: 60 - 30, а не 60 + 1"
    );
}

/// «Формат игры»: время в файлах пишется секундами, `world.number_like` для `Column::Time`
/// отдаёт шаги — сравнение `["c", "<", 2]` должно значить «меньше двух секунд» (120 шагов), не
/// «меньше двух шагов».
#[test]
fn delete_condition_compares_a_time_property_in_seconds_not_steps() {
    let props = r#"{"properties":{"c":"time"}}"#;
    // 1.5с — это 90 шагов, заведомо больше двух шагов, но меньше 120.
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"c":1.5}]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["c"]},"when":["c","<",2]}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let id = game.world.ids().next().unwrap();

    game.step(StepInput::empty());

    assert!(
        !game.world.is_alive(id),
        "90 шагов (1.5с) меньше 120 шагов (2с) — объект должен быть удалён"
    );
}

/// Тот же перевод, только порог — ноль: `seconds_to_steps_delta` не должен подтягивать его до
/// минимума в один шаг, как это делает длительность.
#[test]
fn delete_condition_threshold_of_zero_seconds_stays_zero_steps() {
    let props = r#"{"properties":{"c":"time"}}"#;
    // 1/60с округляется к одному шагу — ровно на единицу больше нулевого порога.
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"c":0.016666667}]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["c"]},"when":["c","<=",0]}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");
    let id = game.world.ids().next().unwrap();

    game.step(StepInput::empty());

    assert!(
        game.world.is_alive(id),
        "один шаг не меньше и не равен нулю шагов — объект не должен быть удалён"
    );
}

/// Created objects only start moving on the step *after* they were spawned.
#[test]
fn object_created_on_step_n_first_moves_on_step_n_plus_1() {
    let props = r#"{"properties":{"mover":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["position","velocity"]}},
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["mover"]}}},
         "where":"random_cell","template":{"size":[1,1],"velocity":[1,0],"mover":true}}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    assert_eq!(game.world.alive_count(), 0);

    game.step(StepInput::empty()); // step 1: the object is born here
    assert_eq!(game.world.alive_count(), 1);
    let mover_prop = game.properties.resolve("mover").unwrap();
    let id = game
        .world
        .ids()
        .find(|&id| game.world.flag(id, mover_prop))
        .expect("объект должен существовать");
    let born_at = game
        .world
        .vec2(id, engine::core::property::POSITION)
        .unwrap();

    game.step(StepInput::empty()); // step 2: born on step 1, it should move for the first time now
    let after = game
        .world
        .vec2(id, engine::core::property::POSITION)
        .unwrap();
    assert!(
        (after[0] - born_at[0] - 1.0 / 60.0).abs() < 1e-9,
        "born={born_at:?} after={after:?}"
    );
}

/// Stage 8 applies deletions before creations, so a freed slot is available to the same
/// step's spawn even under a tight `max_objects`.
#[test]
fn deletions_are_applied_before_creations_in_the_same_step() {
    let props = r#"{"properties":{"dying":"flag","fresh":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"dying":true,"lifetime":0.001}]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["fresh"]}}},
         "where":"random_cell","template":{"size":[1,1],"fresh":true}}
    ]}"#;
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":1,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(game_json, props, scene, rules, SCREENS).expect("должно загрузиться");
    assert_eq!(game.world.alive_count(), 1);

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        1,
        "потолок объектов не должен помешать: старый объект уже удалён к моменту создания"
    );
    let fresh_prop = game.properties.resolve("fresh").unwrap();
    let id = game.world.ids().next().unwrap();
    assert!(
        game.world.flag(id, fresh_prop),
        "выживший объект должен быть новым, а не старым"
    );
}

/// «Формат игры»: `do` выполняется по разу на каждое срабатывание, у «удалить» — на объект.
#[test]
fn delete_rule_runs_do_once_per_deleted_object() {
    let props = r#"{"properties":{"score":"number"}}"#;
    let scene = r#"{"objects":[
        {"position":[1,1],"size":[1,1],"score":0},
        {"position":[-100,-100],"size":[1,1]},
        {"position":[-100,-100],"size":[1,1]},
        {"position":[-100,-100],"size":[1,1]}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["position","size"]},"when":"outside_scene",
         "do":[["add","score",1]]}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        1,
        "три объекта вне сцены должны быть удалены"
    );
    let score_prop = game.properties.resolve("score").unwrap();
    let id = game.world.ids().next().unwrap();
    assert_eq!(
        game.world.number_like(id, score_prop),
        Some(3.0),
        "do должен сработать по разу на каждый удалённый объект"
    );
}

/// «Формат игры»: `do` выполняется по разу на каждое срабатывание, у «создать» — на объект.
#[test]
fn spawn_rule_runs_do_once_per_created_object() {
    let game_json = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":20,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let props = r#"{"properties":{"mover":"flag","score":"number"}}"#;
    let scene = r#"{"objects":[
        {"position":[5,5],"score":0},
        {"position":[0,0],"velocity":[1,0],"mover":true},
        {"position":[1,0],"velocity":[1,0],"mover":true},
        {"position":[2,0],"velocity":[1,0],"mover":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["position","velocity"]}},
        {"kind":"spawn","when":{"after_move_of":{"has":["mover"]}},
         "where":"at_parent","template":{"size":[1,1]},
         "do":[["add","score",1]]}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(game_json, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        7,
        "счётчик + 3 родителя + 3 порождения"
    );
    let score_prop = game.properties.resolve("score").unwrap();
    let id = game
        .world
        .ids()
        .find(|&id| game.world.number_like(id, score_prop).is_some())
        .expect("счётчик должен существовать");
    assert_eq!(
        game.world.number_like(id, score_prop),
        Some(3.0),
        "do должен сработать по разу на каждое порождение"
    );
}

/// «Формат игры»: `after_move_of` спрашивает, сместился ли на этом шаге объект из отбора `of` —
/// не обязан быть сам кандидат на удаление. `for` и `of` называют разные объекты здесь.
#[test]
fn delete_rule_after_move_of_checks_selector_not_the_candidate() {
    let props = r#"{"properties":{"mover":"flag","target":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true},
        {"position":[2,2],"size":[1,1],"target":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["target"]},"when":{"after_move_of":{"has":["mover"]}}}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        1,
        "мовер сдвинулся, значит цель удаляется — хотя сама цель не движется"
    );
}

/// «Исполнение игры»: `random_cell` — свободная клетка целиком, не только её центр.
#[test]
fn random_cell_considers_the_whole_object_rectangle_not_just_its_center() {
    let game_json = r##"{"name":"T","scene":{"width":1,"height":1,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let props = r#"{"properties":{"thing":"flag"}}"#;
    // The object doesn't cover the cell's center point [0.5, 0.5], but it does occupy part of
    // the cell — a center-only check would wrongly call the cell free.
    let scene = r#"{"objects":[{"position":[0.6,0.6],"size":[0.3,0.3],"collides":true}]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1000,"of":{"has":["thing"]}}},
         "where":"random_cell","template":{"size":[1,1],"collides":true,"thing":true}}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(game_json, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        1,
        "клетка занята частично — свободной клетки для random_cell нет"
    );
}

/// «Исполнение игры»/`World::has`: заявка на создание с `"marker": false` в шаблоне не должна
/// засчитываться в `fewer_than` того же шага так, будто у неё `marker` есть — иначе одно правило
/// «создать» глушит следующее, посчитав чужую заявку за свою.
#[test]
fn pending_create_with_false_flag_does_not_satisfy_fewer_than_of_the_same_step() {
    let props = r#"{"properties":{"marker":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":5,"of":{"has":["marker"]}}},
         "where":"random_cell","template":{"size":[1,1],"marker":false}},
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["marker"]}}},
         "where":"random_cell","template":{"size":[1,1],"marker":true}}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        2,
        "заявка с marker:false не должна засчитаться в fewer_than второго правила"
    );
    let marker = game.properties.resolve("marker").unwrap();
    assert!(
        game.world.ids().any(|id| game.world.flag(id, marker)),
        "второе правило должно было создать объект с marker:true"
    );
}

/// «Формат игры»: `after_move_of` смотрит только на отбор `of`, а не на кандидата `for` — значит
/// удаляет и кандидата, у которого вовсе нет `position`.
#[test]
fn after_move_of_delete_removes_a_candidate_without_position() {
    let props = r#"{"properties":{"mover":"flag","fadeaway":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true},
        {"fadeaway":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["fadeaway"]},"when":{"after_move_of":{"has":["mover"]}}}
    ]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.step(StepInput::empty());

    assert_eq!(
        game.world.alive_count(),
        1,
        "мовер сдвинулся — объект без position тоже должен быть удалён"
    );
}

/// «Исполнение игры»: счётчик хода снова равен интервалу сразу после прыжка. Нулевая скорость
/// не даёт прыжка, поэтому счётчик не должен сбрасываться сам по себе.
#[test]
fn grid_hop_counter_does_not_reset_without_an_actual_hop() {
    let props = r#"{"properties":{}}"#;
    let scene = format!(
        r#"{{"objects":[{{"position":[0,0],"size":[1,1],"velocity":[0,0],"grid":{{"interval":{}}}}}]}}"#,
        5.0 / 60.0
    );
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, &scene, rules, SCREENS).expect("должно загрузиться");

    for _ in 0..5 {
        game.step(StepInput::empty());
    }
    let pos_after_5 = game
        .world
        .vec2(0, engine::core::property::POSITION)
        .unwrap();
    assert_eq!(pos_after_5, [0.0, 0.0], "нулевая скорость — прыжка не было");

    game.world
        .set_vec2(0, engine::core::property::VELOCITY, [1.0, 0.0]);
    game.step(StepInput::empty());

    let pos_after_6 = game
        .world
        .vec2(0, engine::core::property::POSITION)
        .unwrap();
    assert_eq!(
        pos_after_6,
        [1.0, 0.0],
        "счётчик хода не должен был сброситься на пятом шаге без настоящего прыжка"
    );
}

/// «Исполнение игры»: a release and a press of the same key landing in the same real-time gap
/// must reach the world in that same order — the world input queue used to split them into two
/// lists and apply every release after every press, regardless of which the player actually did
/// last. Hold the key across one frame, then in a single gap release and immediately re-press it,
/// and run another frame: the binding must still be on.
#[test]
fn a_release_then_a_press_of_the_same_key_in_one_gap_leaves_it_held() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],
         "keys":{"KeyA":{"press":[["velocity",[5,0]]],"release":[["velocity",[0,0]]]}}}
    ]}"#;
    let rules = r#"{"rules":[]}"#;
    let (mut game, _screens, _warnings) =
        load_game_from_texts(GAME, props, scene, rules, SCREENS).expect("должно загрузиться");

    game.key_down("KeyA");
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, engine::core::property::VELOCITY),
        Some([5.0, 0.0]),
        "нажатие должно было задать скорость"
    );

    // One gap, both events for the same key, release arriving first — the way a fast repeat of
    // the same key can land between two animation frames.
    game.key_up("KeyA");
    game.key_down("KeyA");
    let step_input = game.take_input_snapshot();
    game.step(step_input);
    assert_eq!(
        game.world.vec2(0, engine::core::property::VELOCITY),
        Some([5.0, 0.0]),
        "клавиша физически зажата — привязка обязана остаться включённой"
    );
}

//! Фаза 41 — создание объектов кодом и скорость времени: вызов `create`, его заявки, встроенное свойство
//! `time_scale` и ход времени мира. Игра собрана из текстов, её ведёт `ui_support::Ui` теми же вызовами, что
//! `wasm::Engine`.

#[path = "ui_support/mod.rs"]
mod support;

use engine::core::property;
use engine::core::runner::catchup_limit;
use engine::data::session::PlaySession;
use support::{Files, Ui, VIEWPORT};

const PROPS: &str = r#"{"properties":{"ctl":"flag","bumped":"number","clock":"flag","ticks":"number",
    "maker":"flag","made":"flag","seen":"number","base_name":"text","go":"flag","car":"flag",
    "speed":"number"}}"#;

const SCENE: &str = r#"{"objects":[
    {"name":"ctl","ctl":true,"time_scale":1,"bumped":0},
    {"name":"maker","maker":true,"seen":-1,"base_name":"",
     "keys":{"KeyA":{"press":[["go",true]],"release":[["go",false]]}}},
    {"name":"clock","clock":true,"ticks":0,"position":[7,8],"size":[1,1]}
]}"#;

const TEMPLATES: &str =
    r#"{"car":{"name":"car","car":true,"position":[1,2],"size":[2,1],"speed":3}}"#;

const RULES_TICK: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["clock"]},"do":[["add","ticks",1]]}]}"#;
const RULES_MAKE: &str =
    r#"{"rules":[{"kind":"check","for":{"has":["maker"]},"do":[["run","make"]]}]}"#;
const RULES_STOP_AT_THREE: &str = r#"{"rules":[
    {"kind":"check","for":{"has":["clock"]},"do":[["add","ticks",1]]},
    {"kind":"check","for":{"has":["clock"]},"when":["ticks",">=",3],"do":[["run","stop"]]}
]}"#;

const CODE: &str = r#"
local function ctl() return find{has = {"ctl"}}[1] end
function stop() ctl().time_scale = 0 end
function normal() ctl().time_scale = 1 end
function fast() ctl().time_scale = 4 end
function bump() ctl().bumped = ctl().bumped + 1 end
function make(m)
  m.seen = #find{has = {"made"}}
  create{name = "m", made = true}
end
function from_base(m)
  create(tables.templates.car, {name = "car2", position = {x = 5, y = 6}})
  create(tables.templates.car, {size = {3, 4}})
  m.base_name = tables.templates.car.name
end
function twin()
  local c = find{has = {"clock"}}[1]
  create{name = "twin", position = c.position, size = c.size}
end
function many()
  for i = 1, 5 do create{name = "n" .. i, made = true} end
end
function with_keys()
  create{name = "k", position = {1, 1}, size = {1, 1},
         keys = {Space = {press = {{"go", true}}, release = {{"go", false}}}},
         on_click = {{"go", true}}}
end
function bad_name() create{nosuchprop = 1} end
function bad_size() create{name = "x", size = "big"} end
function bad_shape() create{shape = "box"} end
function bad_scale() create{time_scale = 20} end
function bad_argument() create(5) end
function self_ref()
  local t = {}
  t.a = t
  create{name = "x", keys = t}
end
function too_deep()
  local t = {}
  for _ = 1, 40 do t = {inner = t} end
  create{name = "x", keys = t}
end
function too_wide()
  local t = {1}
  for _ = 1, 30 do t = {t, t} end
  create{name = "x", keys = t}
end
function too_fast() ctl().time_scale = 20 end
"#;

const SCREENS: &str = r##"{"screens":[{"name":"main","world_runs":true,"elements":[
    {"kind":"button","anchor":"top_left","offset":[0,0],"size":[60,30],"text":"Стоп","font":"ui",
     "color":"#ffffff","on_click":["run","stop"]},
    {"kind":"button","anchor":"top_left","offset":[70,0],"size":[60,30],"text":"x1","font":"ui",
     "color":"#ffffff","on_click":["run","normal"]},
    {"kind":"button","anchor":"top_left","offset":[140,0],"size":[60,30],"text":"x4","font":"ui",
     "color":"#ffffff","on_click":["run","fast"]},
    {"kind":"button","anchor":"top_left","offset":[210,0],"size":[60,30],"text":"Бамп","font":"ui",
     "color":"#ffffff","on_click":["run","bump"]}
]}]}"##;

const STOP: (f32, f32) = (30.0, 15.0);
const NORMAL: (f32, f32) = (100.0, 15.0);
const FAST: (f32, f32) = (170.0, 15.0);
const BUMP: (f32, f32) = (240.0, 15.0);

fn files<'a>() -> Files<'a> {
    Files::new(PROPS, SCENE, SCREENS)
        .with_code(CODE)
        .with_tables(&[("templates", TEMPLATES)])
}

fn load() -> Ui {
    files().with_rules(RULES_TICK).load()
}

fn set_scale(ui: &mut Ui, scale: f64) {
    let ctl = ui.id_of("ctl");
    ui.game.world.set_number(ctl, property::TIME_SCALE, scale);
}

fn run(ui: &mut Ui, function: &str, object: Option<u32>) {
    ui.game.run_screen_function(function, object, "проверка");
}

/// Сообщение ошибки кода, которой остановилась игра.
fn code_error_of(function: &str) -> String {
    let mut ui = load();
    run(&mut ui, function, None);
    ui.game
        .code_error()
        .unwrap_or_else(|| panic!("{function}: ошибки кода нет"))
        .message
        .clone()
}

// -------------------------------------------------------------------------------------------
// Создание кодом
// -------------------------------------------------------------------------------------------

#[test]
fn a_created_object_appears_at_the_end_of_the_step_and_is_data_without_a_position() {
    let mut ui = files().with_rules(RULES_MAKE).load();
    let maker = ui.id_of("maker");
    let before = ui.game.world.alive_count();
    ui.tick(1.0 / 60.0);
    assert_eq!(ui.game.step_count(), 1);
    assert_eq!(
        ui.number(maker, "seen"),
        Some(0.0),
        "find не видел созданного в этом же шаге"
    );
    assert_eq!(
        ui.game.world.alive_count(),
        before + 1,
        "объект появился на этапе 8"
    );
    let made = ui.id_of("m");
    assert_eq!(
        ui.game.world.vec2(made, property::POSITION),
        None,
        "без position — данные"
    );
    assert!(ui.flag(made, "made"));

    ui.tick(1.0 / 60.0);
    assert_eq!(
        ui.number(maker, "seen"),
        Some(1.0),
        "со следующего шага find его видит"
    );
    assert_eq!(ui.game.world.alive_count(), before + 2);
}

#[test]
fn create_with_a_base_replaces_whole_keys_and_leaves_the_base_alone() {
    let mut ui = load();
    let maker = ui.id_of("maker");
    run(&mut ui, "from_base", Some(maker));
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
    let car2 = ui.id_of("car2");
    assert_eq!(
        ui.game.world.vec2(car2, property::POSITION),
        Some([5.0, 6.0]),
        "{{x = 5, y = 6}} — пара"
    );
    assert_eq!(
        ui.game.world.vec2(car2, property::SIZE),
        Some([2.0, 1.0]),
        "размер из заготовки"
    );
    assert_eq!(ui.number(car2, "speed"), Some(3.0));
    assert!(ui.flag(car2, "car"));

    let plain = ui
        .game
        .world
        .ids()
        .find(|&id| id != car2 && ui.text(id, "name").as_deref() == Some("car"))
        .expect("вторая машина из той же заготовки");
    assert_eq!(
        ui.game.world.vec2(plain, property::SIZE),
        Some([3.0, 4.0]),
        "size заменён целиком"
    );
    assert_eq!(
        ui.game.world.vec2(plain, property::POSITION),
        Some([1.0, 2.0])
    );
    assert_eq!(
        ui.text(maker, "base_name").as_deref(),
        Some("car"),
        "заготовка осталась прежней"
    );
}

#[test]
fn keys_and_on_click_tables_become_bindings_of_the_new_object() {
    let mut ui = load();
    run(&mut ui, "with_keys", None);
    assert!(ui.game.code_error().is_none(), "{:?}", ui.game.code_error());
    let made = ui.id_of("k");
    let keys = ui
        .game
        .world
        .keys(made, property::KEYS)
        .expect("привязки клавиш");
    let binding = keys.get("Space").expect("клавиша Space");
    assert_eq!((binding.press.len(), binding.release.len()), (1, 1));
    let clicks = ui
        .game
        .world
        .on_click(made, property::ON_CLICK)
        .expect("on_click");
    assert_eq!(clicks.len(), 1);
    // Привязки работают со следующего шага: нажатие Space ставит признак.
    ui.key_down("Space");
    ui.tick(1.0 / 60.0);
    assert!(ui.flag(made, "go"));
}

#[test]
fn a_pair_read_from_another_object_can_be_written_into_a_record() {
    let mut ui = load();
    run(&mut ui, "twin", None);
    let twin = ui.id_of("twin");
    assert_eq!(
        ui.game.world.vec2(twin, property::POSITION),
        Some([7.0, 8.0])
    );
    assert_eq!(ui.game.world.vec2(twin, property::SIZE), Some([1.0, 1.0]));
}

#[test]
fn a_wrong_record_is_a_code_error_that_names_the_property() {
    let message = code_error_of("bad_name");
    assert!(message.contains("nosuchprop"), "{message}");
    let message = code_error_of("bad_size");
    assert!(message.contains("size"), "{message}");
    let message = code_error_of("bad_shape");
    assert!(
        message.contains("shape"),
        "запрещённое в плоской сцене: {message}"
    );
    let message = code_error_of("bad_scale");
    assert!(message.contains("time_scale"), "{message}");
    let message = code_error_of("bad_argument");
    assert!(message.contains("create"), "{message}");
}

#[test]
fn a_table_that_refers_to_itself_or_nests_too_deep_is_a_code_error() {
    let message = code_error_of("self_ref");
    assert!(message.contains("ссылается сама на себя"), "{message}");
    assert!(message.contains("keys"), "{message}");
    let message = code_error_of("too_deep");
    assert!(message.contains("слишком глубоко вложена"), "{message}");
    let message = code_error_of("too_wide");
    assert!(message.contains("значений"), "{message}");
}

#[test]
fn create_in_a_button_function_makes_the_object_at_once_even_while_time_stands() {
    let mut ui = load();
    set_scale(&mut ui, 0.0);
    let before = ui.game.world.alive_count();
    let maker = ui.id_of("maker");
    run(&mut ui, "from_base", Some(maker));
    assert_eq!(
        ui.game.world.alive_count(),
        before + 2,
        "объекты появились сразу после функции"
    );
    assert_eq!(ui.game.step_count(), 0);
}

#[test]
fn the_object_ceiling_drops_the_request_and_says_so_once() {
    let mut ui = files().with_rules(RULES_TICK).with_max_objects(6).load();
    assert_eq!(ui.game.world.alive_count(), 3);
    run(&mut ui, "many", None);
    assert_eq!(ui.game.world.alive_count(), 6, "три из пяти заявок приняты");
    run(&mut ui, "many", None);
    assert_eq!(ui.game.world.alive_count(), 6);
    let ceiling = ui
        .game
        .messages()
        .iter()
        .filter(|message| message.contains("потолок объектов"))
        .count();
    assert_eq!(ceiling, 1, "{:?}", ui.game.messages());
}

#[test]
fn the_ceiling_message_from_a_button_function_reaches_the_session_journal() {
    let mut ui = files().with_rules(RULES_TICK).with_max_objects(6).load();
    ui.game.begin_session();
    run(&mut ui, "many", None);
    let journal = ui
        .game
        .session_messages()
        .iter()
        .filter(|(_, message)| message.contains("потолок объектов"))
        .count();
    assert_eq!(journal, 1, "{:?}", ui.game.session_messages());
}

// -------------------------------------------------------------------------------------------
// Скорость времени
// -------------------------------------------------------------------------------------------

#[test]
fn the_world_makes_sixty_times_the_scale_steps_a_second() {
    let mut ui = load();
    set_scale(&mut ui, 4.0);
    ui.ticks(60, 1.0 / 60.0);
    assert_eq!(
        ui.game.step_count(),
        240,
        "60 вызовов по 1/60 секунды при скорости 4"
    );
    let clock = ui.id_of("clock");
    assert_eq!(ui.number(clock, "ticks"), Some(240.0));

    let mut ui = load();
    set_scale(&mut ui, 4.0);
    ui.tick(1.0 / 30.0);
    assert_eq!(ui.game.step_count(), 8, "при 30 кадрах — 8 шагов за кадр");

    let mut ui = load();
    set_scale(&mut ui, 0.5);
    ui.ticks(60, 1.0 / 60.0);
    assert_eq!(ui.game.step_count(), 30);
}

#[test]
fn steps_per_call_are_capped_at_five_per_whole_unit_of_the_scale() {
    assert_eq!(catchup_limit(1.0), 5);
    assert_eq!(catchup_limit(0.5), 5);
    assert_eq!(catchup_limit(4.0), 20);
    assert_eq!(catchup_limit(2.5), 15);
    assert_eq!(catchup_limit(16.0), 80);
    let mut ui = load();
    set_scale(&mut ui, 16.0);
    ui.tick(0.2);
    assert_eq!(ui.game.step_count(), 80, "остаток времени выброшен");
    ui.tick(0.0);
    assert_eq!(ui.game.step_count(), 80);
}

#[test]
fn a_timer_of_ten_seconds_runs_out_in_a_quarter_of_the_time_at_scale_four() {
    let scene = r#"{"objects":[
        {"name":"ctl","ctl":true,"time_scale":4,"bumped":0},
        {"name":"temp","clock":true,"ticks":0,"lifetime":10}
    ]}"#;
    let mut ui = Files::new(PROPS, scene, SCREENS)
        .with_code(CODE)
        .with_rules(RULES_TICK)
        .load();
    let temp = ui.id_of("temp");
    ui.ticks(149, 1.0 / 60.0);
    assert!(
        ui.game.world.is_alive(temp),
        "через 149 вызовов срок ещё не вышел"
    );
    ui.tick(1.0 / 60.0);
    assert!(!ui.game.world.is_alive(temp), "через 150 вызовов вышел");
    assert_eq!(ui.game.step_count(), 600);
}

#[test]
fn at_scale_zero_the_world_stands_but_the_buttons_with_run_work() {
    let mut ui = load();
    set_scale(&mut ui, 0.0);
    ui.ticks(10, 1.0 / 60.0);
    assert_eq!(ui.game.step_count(), 0, "мира нет шагов");
    let ctl = ui.id_of("ctl");
    ui.click(BUMP.0, BUMP.1);
    assert_eq!(ui.number(ctl, "bumped"), Some(1.0), "кнопка с run работает");
    ui.ticks(10, 1.0 / 60.0);
    assert_eq!(ui.game.step_count(), 0);
    ui.click(FAST.0, FAST.1);
    ui.ticks(2, 1.0 / 60.0);
    assert_eq!(
        ui.game.step_count(),
        8,
        "скорость сменена кнопкой — шаги пошли с ближайшего кадра"
    );
}

#[test]
fn the_lowest_object_with_a_scale_rules_and_losing_it_means_normal_speed() {
    let mut ui = load();
    set_scale(&mut ui, 2.0);
    let clock = ui.id_of("clock");
    ui.game.world.set_number(clock, property::TIME_SCALE, 8.0);
    assert_eq!(
        ui.game.time_scale(),
        2.0,
        "действует объект с меньшим номером"
    );
    ui.game.world.delete(ui.id_of("ctl"));
    assert_eq!(ui.game.time_scale(), 8.0, "остался один");
    ui.game.world.delete(clock);
    assert_eq!(ui.game.time_scale(), 1.0, "ни у кого нет — один");
}

#[test]
fn code_can_stop_time_in_the_middle_of_a_burst() {
    let mut ui = files().with_rules(RULES_STOP_AT_THREE).load();
    set_scale(&mut ui, 4.0);
    ui.tick(1.0 / 30.0);
    assert_eq!(ui.game.step_count(), 3, "после третьего шага время встало");
    assert_eq!(ui.game.time_scale(), 0.0);
    ui.tick(1.0);
    assert_eq!(ui.game.step_count(), 3);
}

#[test]
fn stopping_time_releases_held_keys_and_drops_keys_while_it_stands() {
    let mut ui = load();
    let maker = ui.id_of("maker");
    ui.key_down("KeyA");
    ui.tick(1.0 / 60.0);
    assert!(ui.flag(maker, "go"), "клавиша мира работает");
    ui.click(STOP.0, STOP.1);
    ui.tick(1.0 / 60.0);
    assert!(!ui.flag(maker, "go"), "зажатая клавиша отпущена");
    ui.queue.push_key_up("KeyA");
    ui.key_down("KeyA");
    ui.tick(1.0 / 60.0);
    assert!(
        !ui.flag(maker, "go"),
        "пока время стоит, клавиши мира отбрасываются"
    );
    ui.click(NORMAL.0, NORMAL.1);
    ui.key_down("KeyA");
    ui.tick(1.0 / 60.0);
    assert!(ui.flag(maker, "go"), "время пошло — клавиша снова работает");
}

#[test]
fn a_scale_in_the_data_is_a_number_from_zero_to_sixteen() {
    let scene = |value: &str| {
        format!(r#"{{"objects":[{{"name":"ctl","ctl":true,"time_scale":{value},"bumped":0}}]}}"#)
    };
    let errors = |value: &str| {
        let scene = scene(value);
        match Files::new(PROPS, &scene, SCREENS)
            .with_code(CODE)
            .try_load()
        {
            Ok(_) => Vec::new(),
            Err(failure) => failure.errors,
        }
    };
    for bad in ["\"fast\"", "-1", "17", "16.5"] {
        let found = errors(bad);
        assert!(
            found.iter().any(|e| e.path.ends_with("time_scale")),
            "{bad}: {found:?}"
        );
    }
    for good in ["0", "0.25", "1", "16"] {
        assert_eq!(errors(good), Vec::new(), "{good}");
    }
    let message = code_error_of("too_fast");
    assert!(
        message.contains("time_scale"),
        "запись кодом — ошибка исполнения: {message}"
    );
}

// -------------------------------------------------------------------------------------------
// Запись партии
// -------------------------------------------------------------------------------------------

fn snapshot(ui: &Ui) -> (u64, Option<f64>, f64, usize) {
    let clock = ui.id_of("clock");
    (
        ui.game.session_step_count(),
        ui.number(clock, "ticks"),
        ui.game.time_scale(),
        ui.game.world.alive_count(),
    )
}

#[test]
fn a_replay_with_speed_changes_gives_the_same_partiya_and_runs_at_the_recorded_speed() {
    let mut ui = load();
    let mut session = PlaySession::begin_live(&mut ui.game, &ui.config, &mut ui.state);
    let frame = 1.0 / 60.0;
    let live = |ui: &mut Ui, session: &mut PlaySession, click: Option<(f32, f32)>| {
        if let Some((x, y)) = click {
            ui.queue.push_mouse_move(x, y);
            ui.queue.push_mouse_down();
            ui.queue.push_mouse_up();
        }
        session.tick_live(
            &mut ui.queue,
            &mut ui.mouse,
            &mut ui.runner,
            &mut ui.game,
            &ui.config,
            &mut ui.state,
            VIEWPORT,
            frame,
            &[],
        );
    };
    live(&mut ui, &mut session, None);
    live(&mut ui, &mut session, Some(FAST));
    live(&mut ui, &mut session, None);
    live(&mut ui, &mut session, Some(STOP));
    live(&mut ui, &mut session, None);
    live(&mut ui, &mut session, Some(NORMAL));
    live(&mut ui, &mut session, None);
    let steps = ui.game.session_step_count();
    assert_eq!(
        steps,
        1 + 4 + 4 + 1 + 1,
        "после ×4 — по четыре шага за вызов, на стопе ни одного"
    );
    session.end(&mut ui.game);
    let text = session.recording_text(&ui.game);
    let expected = snapshot(&ui);

    let mut again = load();
    let mut replay =
        PlaySession::begin_replay(&text, &mut again.game, &again.config, &mut again.state)
            .expect("запись читается");
    let mut calls = 0;
    while again.game.session_step_count() < steps && calls < 200 {
        replay.tick_replay(
            &mut again.queue,
            &mut again.mouse,
            &mut again.runner,
            &mut again.game,
            &again.config,
            &mut again.state,
            VIEWPORT,
            frame,
            &[],
        );
        calls += 1;
    }
    assert_eq!(snapshot(&again), expected, "повтор дал ту же партию");
    assert!(
        calls < 14,
        "повтор шёл с записанной скоростью, а не по шагу за вызов: {calls} вызовов"
    );
}

//! «Код игры»: поведение исполнителя кода во время партии — чтение и запись свойств каждого
//! вида, `find`/`delete`/`play_sound`/`print`/`math.random`, пределы, запрет глобальных, ошибка
//! во время партии. Ошибки проверки перед запуском — в `code_prestart.rs`.

use engine::core::input::StepInput;
use engine::data::load::load_game_from_texts_with_code;

const GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"main","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

fn load(
    props: &str,
    scene: &str,
    rules: &str,
    code: &str,
) -> Result<
    (
        engine::core::game::Game,
        engine::core::screens::ScreensConfig,
        Vec<engine::data::error::GameError>,
    ),
    engine::data::error::LoadFailure,
> {
    load_game_from_texts_with_code(GAME, props, scene, rules, SCREENS, Some(code))
}

/// Читает и пишет по одному свойству каждого вида, кроме пары (своя проверка ниже) и картинки
/// (своя, требует настоящих байт PNG) — «Код игры»: число, время, признак, слой, цвет; отсутствие
/// свойства читается как `nil`/`false`, запись `nil` его убирает.
#[test]
fn reads_and_writes_every_scalar_property_kind() {
    let props = r#"{"properties":{"score":"number","tag":"flag","cool":"time","marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[2,3],"collides":true,"marker":true,
         "score":5,"tag":true,"cool":2.0,"color":"#112233","layer":2},
        {"position":[1,1],"size":[2,3],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},
         "do":[["run","touch"]]}
    ]}"#;
    let code = r##"
function touch(obj)
    assert(obj.score == 5)
    obj.score = obj.score + 1
    assert(obj.tag == true)
    obj.tag = false
    assert(obj.cool == 2.0)
    obj.cool = 1.5
    assert(obj.layer == 2)
    obj.layer = 3
    assert(obj.color == "#112233")
    obj.color = "#ff0000"
    assert(obj.name == nil)
    obj.name = "hi"
end
"##;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());

    let score = game.properties.resolve("score").unwrap();
    let tag = game.properties.resolve("tag").unwrap();
    let cool = game.properties.resolve("cool").unwrap();
    let layer = engine::core::property::LAYER;
    let color = engine::core::property::COLOR;
    let name = engine::core::property::NAME;
    let id = 0;
    assert_eq!(game.world.number_like(id, score), Some(6.0));
    assert!(!game.world.flag(id, tag));
    assert_eq!(game.world.time(id, cool), Some(90));
    assert_eq!(game.world.layer(id, layer), Some(3));
    assert_eq!(game.world.color(id, color), Some([1.0, 0.0, 0.0, 1.0]));
    assert_eq!(game.world.text(id, name), Some("hi"));
}

/// «Код игры»: «запись `nil` убирает свойство» — у свойства любого вида, заданного сценой, а
/// после записи оно читается как `nil` (признак — как `false`) и в том же вызове, и в мире.
#[test]
fn writing_nil_removes_a_property_of_every_kind() {
    let props = r#"{"properties":{"score":"number","tag":"flag","cool":"time","marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[2,3],"velocity":[3,-4],"collides":true,"marker":true,
         "score":5,"tag":true,"cool":2.0,"color":"#112233","layer":2,"name":"hero"},
        {"position":[1,1],"size":[2,3],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},
         "do":[["run","clear"]]}
    ]}"#;
    let code = r##"
function clear(obj)
    obj.score = nil
    assert(obj.score == nil, "score")
    obj.tag = nil
    assert(obj.tag == false, "tag")
    obj.cool = nil
    assert(obj.cool == nil, "cool")
    obj.layer = nil
    assert(obj.layer == nil, "layer")
    obj.color = nil
    assert(obj.color == nil, "color")
    obj.name = nil
    assert(obj.name == nil, "name")
    obj.velocity = nil
    assert(obj.velocity == nil, "velocity")
end
"##;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());

    let score = game.properties.resolve("score").unwrap();
    let tag = game.properties.resolve("tag").unwrap();
    let cool = game.properties.resolve("cool").unwrap();
    let id = 0;
    assert_eq!(game.world.number_like(id, score), None);
    assert!(!game.world.flag(id, tag));
    assert_eq!(game.world.time(id, cool), None);
    assert_eq!(game.world.layer(id, engine::core::property::LAYER), None);
    assert_eq!(game.world.color(id, engine::core::property::COLOR), None);
    assert_eq!(game.world.text(id, engine::core::property::NAME), None);
    assert_eq!(game.world.vec2(id, engine::core::property::VELOCITY), None);
}

/// «Код игры» → требование 8: своя таблица кода игры (не объект, не `_ENV`/`_G`) с
/// метатаблицей-ловушкой на `__newindex` — запись `nil` в отсутствующий в ней ключ вызывает
/// ловушку, а не молча считается сделанной.
#[test]
fn own_table_with_a_newindex_trap_fires_it_on_a_nil_write_to_a_missing_key() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","touch"]]}
    ]}"#;
    let code = r#"
function touch()
    local t = setmetatable({}, { __newindex = function(tbl, k, v) print("trap", k) end })
    t.missing = nil
    print("done")
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert!(
        game.messages().iter().any(|m| m == "print: trap\tmissing"),
        "{:?}",
        game.messages()
    );
}

/// «Код игры»: запись в поле пары `nil` — значение не того вида, ошибка, а не тихий пропуск;
/// сама пара объекта при этом не меняется.
#[test]
fn writing_nil_into_a_pair_field_is_a_code_error() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[1,1],"velocity":[3,-4],"collides":true,"marker":true},
        {"position":[1,1],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","wipe"]]}
    ]}"#;
    let code = "function wipe(obj) obj.velocity.x = nil end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("запись nil в поле пары должна остановить партию");
    assert_eq!(err.line, Some(1), "{err:?}");
    assert!(err.message.contains("числа"), "{err:?}");
    assert_eq!(
        game.world
            .vec2(0, engine::core::property::VELOCITY)
            .unwrap(),
        [3.0, -4.0]
    );
}

/// «Код игры» → «Пара»: пару, прочитанную у одного объекта, можно записать в свойство другого —
/// записывается её значение, а не связь: дальнейшая правка источника копию не меняет.
#[test]
fn a_pair_read_from_one_object_can_be_written_into_another() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[1,1],"velocity":[3,-4],"collides":true,"marker":true},
        {"position":[1,1],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","copy"]]}
    ]}"#;
    let code = "function copy(a, b) b.velocity = a.velocity; a.velocity.x = 7 end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let velocity = engine::core::property::VELOCITY;
    assert_eq!(game.world.vec2(0, velocity).unwrap(), [7.0, -4.0]);
    assert_eq!(game.world.vec2(1, velocity).unwrap(), [3.0, -4.0]);
}

/// «Код игры» → «Пара»: `ball.velocity.y = -ball.velocity.y` меняет сам объект, не копию.
#[test]
fn vec2_write_through_a_read_pair_mutates_the_object() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[1,1],"size":[1,1],"velocity":[3,-4],"collides":true,"marker":true},
        {"position":[1,1],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","flip"]]}
    ]}"#;
    let code = "function flip(obj) obj.velocity.y = -obj.velocity.y end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.world
            .vec2(0, engine::core::property::VELOCITY)
            .unwrap(),
        [3.0, 4.0]
    );
}

/// «Код игры» → «Что ещё доступно коду»: `find` отбирает по `has`/`without`, по возрастанию
/// номеров, и всё ещё видит объект, на который в этом же вызове уже подана заявка на удаление
/// («объект с заявкой на удаление находится до этапа 8»).
#[test]
fn find_orders_by_id_respects_selector_and_still_sees_a_pending_delete() {
    let props = r#"{"properties":{"tag":"flag","other":"flag","val":"number","count":"number","first":"number","without_count":"number","after_count":"number","wall":"flag","trigger":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"tag":true,"val":10},
        {"position":[1,0],"size":[1,1],"collides":true,"tag":true,"other":true,"val":20},
        {"position":[2,0],"size":[1,1],"collides":true,"tag":true,"val":30},
        {"position":[5,5],"size":[1,1],"collides":true,"trigger":true,
         "count":0,"first":0,"without_count":0,"after_count":0},
        {"position":[5,5],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["trigger"]},"b":{"has":["wall"]},"do":[["run","check"]]}
    ]}"#;
    let code = r##"
function check(a, b)
    local all = find{has = {"tag"}}
    a.count = #all
    a.first = all[1].val
    local rest = find{has = {"tag"}, without = {"other"}}
    a.without_count = #rest
    delete(all[1])
    local after = find{has = {"tag"}}
    a.after_count = #after
end
"##;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());

    let count = game.properties.resolve("count").unwrap();
    let first = game.properties.resolve("first").unwrap();
    let without_count = game.properties.resolve("without_count").unwrap();
    let after_count = game.properties.resolve("after_count").unwrap();
    assert_eq!(game.world.number_like(3, count), Some(3.0));
    assert_eq!(
        game.world.number_like(3, first),
        Some(10.0),
        "по возрастанию номеров — первым найден объект 0"
    );
    assert_eq!(game.world.number_like(3, without_count), Some(2.0));
    assert_eq!(
        game.world.number_like(3, after_count),
        Some(3.0),
        "заявка на удаление ещё не применена — find видит объект до этапа 8"
    );
    assert!(
        !game.world.is_alive(0),
        "а после конца шага заявка уже применена"
    );
}

/// «Код игры»: `play_sound` поднимает отметку так же, как `["play_sound", "<имя>"]`, а неизвестное
/// имя — ошибка во время партии, которая останавливает игру.
#[test]
fn play_sound_raises_a_mark_and_unknown_name_stops_the_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","ping"]]}
    ]}"#;
    let code = "function ping() play_sound(\"beep\") end";
    // «Звук»: files.sounds не объявлена в этой игре, значит play_sound("beep") ссылается на
    // неизвестный звук — ровно то поведение, которое проверяет этот тест.
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("неизвестный звук должен остановить партию");
    assert!(err.message.contains("beep"), "{}", err.message);
    assert!(!game.is_running());
}

/// «Код игры»: `print(...)` кладёт `print: <текст>` в `messages`, аргументы через табуляцию.
#[test]
fn print_pushes_into_messages_with_tab_join() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","say"]]}
    ]}"#;
    let code = "function say() print(\"a\", 1, true) end";
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert!(
        game.messages().iter().any(|m| m == "print: a\t1\ttrue"),
        "{:?}",
        game.messages()
    );
}

/// «Код игры»: `print` форматирует числа так же, как `tostring` — `1.0` для `1.0`, а не `1` из
/// `Display`, и `-nan` для `0/0`, а не `NaN`.
#[test]
fn print_formats_floats_like_tostring() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[]}"#;
    let code = "print(1.0, 0/0)";
    let (game, _screens, _warnings) = load(props, scene, rules, code).expect("должно загрузиться");
    assert!(
        game.messages().iter().any(|m| m == "print: 1.0\t-nan"),
        "{:?}",
        game.messages()
    );
}

/// «Исполнение игры» → «Повторяемость»: `math.random` в коде берёт числа у того же счётчика,
/// что и правила — тот же посев, та же последовательность, и вторая партия повторяет первую.
#[test]
fn math_random_same_seed_gives_the_same_sequence_every_party() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","roll":"number"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true,"roll":0},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","roll"]]}
    ]}"#;
    let code = "function roll(a, b) a.roll = math.random(1, 1000000) end";
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    let roll = game.properties.resolve("roll").unwrap();

    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let first = game.world.number_like(0, roll).unwrap();

    game.new_game();
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let second = game.world.number_like(0, roll).unwrap();

    assert_eq!(first, second, "вторая партия должна повторить первую");
}

/// «Код игры»: запись в глобальную переменную из функции — ошибка во время партии.
#[test]
fn writing_a_global_from_a_function_stops_the_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","leak"]]}
    ]}"#;
    let code = "function leak() oops = 1 end";
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("запись в глобальную должна остановить партию");
    assert_eq!(err.line, Some(1), "{err:?}");
    assert!(
        err.message
            .starts_with("запись в глобальную переменную из функции: oops"),
        "в тексте не должно быть служебной приставки обёртки: {err:?}"
    );
}

/// «Код игры» → «Память кода»: таблица, заведённая файлом при загрузке, помнит изменения между
/// вызовами внутри одной партии — и сбрасывается новой партией («каждая партия создаёт свежий
/// исполнитель»).
#[test]
fn a_table_declared_at_load_persists_across_calls_and_resets_on_new_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","seen":"number"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true,"seen":0},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","bump"]]}
    ]}"#;
    let code = r#"
local state = { count = 0 }
function bump(a, b)
    state.count = state.count + 1
    a.seen = state.count
end
"#;
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    let seen = game.properties.resolve("seen").unwrap();

    game.step(StepInput::empty());
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.world.number_like(0, seen),
        Some(2.0),
        "второй вызов в той же партии видит state.count из первого"
    );

    game.new_game();
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert_eq!(
        game.world.number_like(0, seen),
        Some(1.0),
        "новая партия начинает с заново загруженного кода"
    );
}

/// «Код игры»: бесконечный цикл, в том числе обёрнутый в `pcall`, упирается в предел операций и
/// останавливает игру — а не вешает страницу.
#[test]
fn infinite_loop_even_wrapped_in_pcall_hits_the_instruction_limit() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","loop"]]}
    ]}"#;
    let code = r#"
function loop()
    pcall(function()
        local i = 0
        while true do
            i = i + 1
        end
    end)
end
"#;
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("предел операций должен остановить партию");
    // «Код игры»: не английское `sandbox instruction limit exceeded` от `SandboxConfig::
    // instruction_limit` — только русский текст общего счётного хука.
    assert!(
        err.message.contains("превышен предел операций кода"),
        "{}",
        err.message
    );
    assert!(err.line.is_some(), "{err:?}");
}

/// «Код игры»: `while true do f() end` — цикл из частых вызовов функции упирается в предел так
/// же, как пустой цикл, и называет строку кода игры.
#[test]
fn while_true_calling_a_function_hits_the_instruction_limit_and_names_the_line() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","loop"]]}
    ]}"#;
    let code = "function f() end\nfunction loop()\n    while true do f() end\nend\n";
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("предел операций должен остановить партию");
    assert!(
        err.message.contains("превышен предел операций кода"),
        "{}",
        err.message
    );
    assert_eq!(err.line, Some(3), "{err:?}");
}

/// «Код игры»: функция, жадная до памяти, упирается в предел памяти исполнителя.
#[test]
fn memory_hungry_function_hits_the_memory_limit() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","hog"]]}
    ]}"#;
    let code = r#"
function hog()
    local t = {}
    local i = 0
    while true do
        i = i + 1
        t[i] = string.rep("x", 1024)
    end
end
"#;
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "жадная до памяти функция должна остановить партию"
    );
}

/// «Код игры» → требование 20: ошибка во время партии называет файл и строку кода, функцию,
/// правило (`rules[N]`) и номер шага — не только «что не так». Опечатка в имени свойства внутри
/// функции воспроизводит ровно ту ошибку, что находит ручная проверка в критериях готовности
/// (опечатка в имени свойства внутри `paddle_bounce`); правило-цель стоит вторым (`rules[1]`),
/// чтобы индекс правила не совпал с индексом по случайности.
#[test]
fn error_during_a_step_names_file_line_function_rule_and_step() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["marker"]}},
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","touch"]]}
    ]}"#;
    let code = "\nfunction touch(a, b)\n    a.nosuch = 1\nend\n";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("неизвестное свойство должно остановить партию");
    assert!(!game.is_running());
    assert_eq!(game.code_path(), "code.lua");
    assert_eq!(err.line, Some(3), "{err:?}");
    assert_eq!(err.function.as_deref(), Some("touch"), "{err:?}");
    assert_eq!(err.rule.as_deref(), Some("rules[1]"), "{err:?}");
    assert_eq!(err.step, Some(1), "{err:?}");
    assert_eq!(
        err.message, "неизвестное свойство \"nosuch\"",
        "{}",
        err.message
    );
}

/// «Код игры»: обращение к удалённому объекту — ошибка, даже когда его номер уже занял новый
/// объект («Номера объектов»).
#[test]
fn accessing_a_deleted_object_errors_even_after_its_slot_is_reused() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","trigger":"flag","child":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true},
        {"position":[5,5],"size":[1,1],"collides":true,"trigger":true},
        {"position":[5,5],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","hold_and_delete"]]},
        {"kind":"collide","a":{"has":["trigger"]},"b":{"has":["wall"]},"do":[["run","touch_stale"]]},
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["child"]}}},"where":"random_cell",
         "template":{"size":[1,1],"collides":true,"child":true}}
    ]}"#;
    let code = r#"
local held = nil
function hold_and_delete(a, b)
    held = a
    delete(a)
end
function touch_stale()
    if held ~= nil then
        local _ = held.marker
    end
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    // Step 1: marker×wall deletes object 0 and remembers it in `held`; the spawn rule queues a
    // new "child" object, which takes the just-freed slot 0 right back at stage 8.
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let child = game.properties.resolve("child").unwrap();
    assert!(
        game.world.has(0, child),
        "слот 0 должен достаться новому объекту"
    );

    // Step 2: touch_stale reads the stale `held` handle — same slot, new generation.
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("обращение к удалённому объекту должно остановить партию");
    assert!(err.message.contains("удал"), "{}", err.message);
}

/// «Код игры»: предел операций — на все вызовы `run` одного шага вместе, не на каждый отдельно.
/// Две пары, обе срабатывающие тем же правилом в одном шаге, тратят ~0,6 млн операций каждая —
/// вместе за миллион, игра стоит.
#[test]
fn instruction_budget_is_shared_by_every_run_call_in_one_step() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true},
        {"position":[5,5],"size":[1,1],"collides":true,"marker":true},
        {"position":[5,5],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","burn"]]}
    ]}"#;
    let code = "function burn() local s = 0 for i = 1, 300000 do s = s + i end end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("два вызова по ~0,6 млн операций в одном шаге должны превысить миллион");
    assert!(!err.message.is_empty());
    assert!(!game.is_running());
}

/// То же ~0,6 млн операций, но каждый вызов — в своём шаге: предел на шаг сбрасывается заново,
/// так что оба шага проходят без ошибки.
#[test]
fn instruction_budget_resets_between_steps() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","burn"]]}
    ]}"#;
    let code = "function burn() local s = 0 for i = 1, 300000 do s = s + i end end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert!(game.is_running());
}

/// «Код игры»: запись в глобальную переменную — ошибка для ЛЮБОЙ функции, включая `local
/// function` верхнего уровня, вызванную из именованной глобальной функции.
#[test]
fn writing_a_global_from_a_local_function_stops_the_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","on_hit"]]}
    ]}"#;
    let code = r#"
counter = 0
local function helper()
    counter = counter + 1
end
function on_hit()
    helper()
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "запись в глобальную из local function должна остановить партию"
    );
}

/// «Код игры»: запись через `_G.x = ...` — тот же запрет, что прямая запись `x = ...`.
#[test]
fn writing_a_global_through_underscore_g_stops_the_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","on_hit"]]}
    ]}"#;
    let code = r#"
counter = 0
function on_hit()
    _G.counter = counter + 1
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "запись через _G должна остановить партию"
    );
}

/// «Код игры»: запись `nil` — тоже запись в глобальную: и в уже объявленную переменную, и в
/// никогда не объявленную, напрямую и через `_G`.
#[test]
fn writing_nil_to_a_global_from_a_function_stops_the_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","leak"]]}
    ]}"#;
    for (code, name) in [
        ("counter = 5\nfunction leak() counter = nil end", "counter"),
        ("function leak() ghost = nil end", "ghost"),
        (
            "counter = 5\nfunction leak() _G.counter = nil end",
            "counter",
        ),
    ] {
        let (mut game, _screens, _warnings) =
            load(props, scene, rules, code).expect("должно загрузиться");
        game.step(StepInput::empty());
        let err = game
            .code_error()
            .unwrap_or_else(|| panic!("запись nil в глобальную должна остановить партию: {code}"));
        assert!(
            err.message.starts_with(&format!(
                "запись в глобальную переменную из функции: {name}"
            )),
            "{code}: {err:?}"
        );
    }
}

/// «Код игры»: `math.random` с аргументами отдаёт Lua integer, не дробное —
/// `tostring(math.random(3))` даёт `"1"`/`"2"`/`"3"`, не `"1.0"`.
#[test]
fn math_random_with_arguments_returns_a_lua_integer() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","roll"]]}
    ]}"#;
    let code = r#"
function roll(a, b)
    assert(math.type(math.random(3)) == "integer")
    assert(math.type(math.random(1, 5)) == "integer")
    assert(math.type(math.random()) == "float")
    a.name = tostring(math.random(3))
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let name = game.world.text(0, engine::core::property::NAME).unwrap();
    assert!(
        ["1", "2", "3"].contains(&name),
        "ожидалось целое 1..3 без дробной части, получено {name:?}"
    );
}

/// «Код игры»: `print` принимает любое число аргументов, включая `nil` посередине, и печатает их
/// через табуляцию, как обычный Lua.
#[test]
fn print_accepts_any_number_of_arguments_including_a_nil_in_the_middle() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","say"]]}
    ]}"#;
    let code = r#"
function say()
    print(1, nil, 3)
    print(1, 2, 3, 4, 5, 6)
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    assert!(
        game.messages().iter().any(|m| m == "print: 1\tnil\t3"),
        "{:?}",
        game.messages()
    );
    assert!(
        game.messages()
            .iter()
            .any(|m| m == "print: 1\t2\t3\t4\t5\t6"),
        "{:?}",
        game.messages()
    );
}

/// «Код игры»: `nil` в конце и единственный аргумент `nil`, а не только посередине — и вовсе без
/// аргументов, `print` печатает настоящее число переданных значений, не арность.
#[test]
fn print_reports_the_real_argument_count_with_nil_at_the_end_or_alone() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","say"]]}
    ]}"#;
    let code = r#"
function say()
    print(1, nil)
    print(nil)
    print()
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let messages: Vec<&str> = game.messages().iter().map(String::as_str).collect();
    assert_eq!(messages, vec!["print: 1\tnil", "print: nil", "print: "]);
}

/// «Код игры»: пара — не объект: `delete(obj.velocity)` — ошибка кода, и текст называет пару.
#[test]
fn deleting_a_pair_is_a_code_error_naming_the_pair() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","drop"]]}
    ]}"#;
    let code = "function drop(obj) delete(obj.position) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("delete у пары должен дать ошибку кода");
    assert!(
        err.message.contains("ожидался объект, получено пара"),
        "{err:?}"
    );
}

/// «Код игры»: два обработчика одного объекта (аргумент и найденный `find`) равны в обе стороны,
/// а разные объекты — нет.
#[test]
fn two_handles_of_one_object_are_equal_and_different_objects_are_not() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","ok":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","check"]]}
    ]}"#;
    let code = r#"
function check(a, b)
    local found = find{has = {"marker"}}[1]
    a.ok = (a == found) and (found == a) and (a ~= b) and (b ~= found)
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let ok = game.properties.resolve("ok").unwrap();
    assert!(game.world.flag(0, ok));
}

/// «Код игры»: `obj == {}` — `false`, не ошибка Lua: таблица не объект.
#[test]
fn comparing_an_object_to_an_empty_table_is_false_not_an_error() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","ok":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","check"]]}
    ]}"#;
    let code = r#"
function check(a, b)
    assert(a == a)
    a.ok = (a == {})
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let ok = game.properties.resolve("ok").unwrap();
    assert!(!game.world.flag(0, ok), "obj == {{}} должно дать false");
}

/// «Код игры» → «Экраны и состояние»: код грузится ровно один раз на партию — у игры со
/// стартовым экраном-меню он не должен выполниться и при загрузке (мира ещё нет), и повторно
/// при `new_game`. Строка `print` верхнего уровня должна появиться в `messages` ровно один раз.
#[test]
fn code_with_a_menu_start_screen_runs_exactly_once_per_party() {
    let game_json = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"menu","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[]}"#;
    let screens = r#"{"screens":[
        {"name":"menu","world_runs":false,"keys":{"KeyP":["new_game","game"]},"elements":[]},
        {"name":"game","world_runs":true,"elements":[]}
    ]}"#;
    let code = "print(\"loaded\")";
    let (mut game, _screens, warnings) =
        load_game_from_texts_with_code(game_json, props, scene, rules, screens, Some(code))
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| **m == "print: loaded")
            .count(),
        0,
        "код игры со стартовым экраном-меню не должен исполниться до new_game: {:?}",
        game.messages()
    );

    game.new_game();
    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| **m == "print: loaded")
            .count(),
        1,
        "код должен исполниться ровно один раз на партию: {:?}",
        game.messages()
    );
}

/// «Код игры»: у игры со стартовым экраном, который сам живой (без меню), код должен выполниться
/// ровно один раз на партию — при самой загрузке (`Game::new`), не заново на каждом шаге.
#[test]
fn code_with_a_live_start_screen_runs_exactly_once_per_party() {
    let code = "print(\"loaded\")";
    let (mut game, _screens, warnings) = load(
        r#"{"properties":{}}"#,
        r#"{"objects":[]}"#,
        r#"{"rules":[]}"#,
        code,
    )
    .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| **m == "print: loaded")
            .count(),
        1,
        "живой стартовый экран должен выполнить код сразу при загрузке: {:?}",
        game.messages()
    );

    game.step(StepInput::empty());
    game.step(StepInput::empty());
    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| **m == "print: loaded")
            .count(),
        1,
        "повторные шаги не должны исполнять верхний уровень файла заново: {:?}",
        game.messages()
    );
}

/// «Код игры» → регрессия: стартовый экран — меню, `show_screen` уводит сразу на живой экран без
/// `new_game` («Экраны и состояние»: `show_screen` не трогает мир — движок этого не запрещает).
/// Исполнителя тогда ещё нет («Game::new» строит его только при живом стартовом экране), а шаг
/// всё равно идёт: `run` не должен давать ложную ошибку «run в игре без files.code» — исполнитель
/// должен появиться лениво, в начале первого же шага, и код — выполниться ровно один раз.
#[test]
fn code_lazily_loads_on_the_first_live_step_reached_without_new_game() {
    let game_json = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"menu","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
    let props = r#"{"properties":{"marker":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["marker"]}}},
         "where":"random_cell","template":{"marker":true,"size":[1,1]},
         "do":[["run","spawned"]]}
    ]}"#;
    let screens = r#"{"screens":[
        {"name":"menu","world_runs":false,"keys":{"KeyP":["show_screen","game"]},"elements":[]},
        {"name":"game","world_runs":true,"elements":[]}
    ]}"#;
    let code = "print(\"loaded\")\nfunction spawned() print(\"spawned\") end";
    let (mut game, _screens, warnings) =
        load_game_from_texts_with_code(game_json, props, scene, rules, screens, Some(code))
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    // Мимикрирует `show_screen` сразу на живой экран без `new_game` — `Game::step` ничего не
    // знает про экраны, только про то, есть ли уже исполнитель.
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_none(),
        "лениво собранный исполнитель не должен давать «run в игре без files.code»: {:?}",
        game.code_error()
    );
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());

    assert_eq!(
        game.messages()
            .iter()
            .filter(|m| **m == "print: loaded")
            .count(),
        1,
        "верхний уровень файла должен выполниться ровно один раз на партию: {:?}",
        game.messages()
    );
    assert!(
        game.messages().iter().any(|m| m == "print: spawned"),
        "правило spawn должно было вызвать run: {:?}",
        game.messages()
    );
}

/// «Код игры»: `quit` выбрасывает исполнитель вместе с миром — следующий живой шаг без
/// `new_game` (тот же обходной путь, что и выше, но после `quit`) должен собрать исполнитель
/// заново, со счётчиком случайности в том же состоянии, что и сразу после `new_game`: повторный
/// живой шаг повторяет то, что дала бы первая партия.
#[test]
fn quit_throws_the_executor_away_and_a_later_live_step_rebuilds_it_like_new_game() {
    let props = r#"{"properties":{"marker":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["marker"]}}},
         "where":"random_cell","template":{"marker":true,"size":[1,1]},
         "do":[["run","roll"]]}
    ]}"#;
    let code = "function roll() print(math.random(1, 1000000)) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let first_roll = game.messages().last().cloned();

    game.quit();
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_none(),
        "лениво собранный после quit исполнитель не должен давать «run в игре без files.code»: {:?}",
        game.code_error()
    );
    let second_roll = game.messages().last().cloned();

    assert_eq!(
        first_roll,
        second_roll,
        "после quit без new_game повторный живой шаг должен повторить первую партию: {:?}",
        game.messages()
    );
}

/// «Код игры»: `rawset` не смотрит на `__newindex` — `rawset(_ENV, ...)` не должен обходить
/// запрет «запись в глобальную переменную из функции».
#[test]
fn rawset_on_the_env_proxy_is_blocked() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","leak"]]}
    ]}"#;
    let code = "function leak() rawset(_ENV, 'counter', 5) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("rawset(_ENV, ...) должен остановить партию, как обычная запись в глобальную");
    assert!(
        err.message
            .starts_with("запись в глобальную переменную из функции: counter"),
        "{err:?}"
    );
}

/// «Код игры»: `rawset` на значении, которое не таблица, — тот же текст и та же строка кода
/// игры, что у обычного Lua, а не место замыкания, которое ставит эту защиту.
#[test]
fn rawset_on_a_non_table_names_the_lua_message_and_the_game_code_line() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","bad"]]}
    ]}"#;
    let code = "function bad() rawset(1, 2, 3) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("rawset(1, 2, 3) должен остановить партию");
    assert_eq!(
        err.message, "bad argument #1 to 'rawset' (table expected)",
        "{err:?}"
    );
    assert_eq!(err.line, Some(1), "{err:?}");
}

/// «Код игры»: `error({})` — объект ошибки не строка, без текста места, но со строкой кода игры,
/// откуда он брошен.
#[test]
fn error_with_a_non_string_object_names_the_game_code_line() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","bad"]]}
    ]}"#;
    let code = "function bad()\n    error({})\nend";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("error({{}}) должен остановить партию");
    assert_eq!(err.line, Some(2), "{err:?}");
}

/// «Код игры»: `getmetatable(_ENV).__index` — тот же настоящий стол глобальных, без защиты, если
/// его отдать как есть; `__metatable` на прокси должен закрыть и этот путь.
#[test]
fn mutating_the_env_metatable_index_is_blocked() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","leak"]]}
    ]}"#;
    let code = "function leak() getmetatable(_ENV).__index.counter = 7 end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "getmetatable(_ENV).__index не должен отдавать настоящий стол глобальных"
    );
}

/// «Код игры»: `getmetatable(_ENV).__newindex = nil` не должен снимать защиту от записи в
/// глобальные — `__metatable` на прокси должен запретить менять саму метатаблицу.
#[test]
fn disabling_the_env_newindex_guard_is_blocked() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","leak"]]}
    ]}"#;
    let code = "function leak()\n    getmetatable(_ENV).__newindex = nil\n    brand_new = 3\nend";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "getmetatable(_ENV).__newindex = nil не должен снять защиту"
    );
}

/// «Код игры»: `setmetatable(_ENV, ...)` своей метатаблицей не снимает запрет записи в глобальные.
#[test]
fn replacing_the_env_metatable_does_not_lift_the_guard() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","leak"]]}
    ]}"#;
    let code = "function leak()\n    pcall(setmetatable, _ENV, {__newindex = function() end})\n    brand_new = 3\nend";
    let (mut game, _screens, _warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("setmetatable(_ENV, ...) не должен снять защиту");
    assert!(
        err.message
            .starts_with("запись в глобальную переменную из функции: brand_new"),
        "{err:?}"
    );
}

/// «Код игры»: `getmetatable(obj)` не должен отдавать настоящую, общую на все объекты игры
/// метатаблицу — иначе её можно поменять разом для каждого объекта.
#[test]
fn mutating_an_objects_shared_metatable_is_blocked() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","tamper"]]}
    ]}"#;
    let code = r#"
function tamper(obj)
    local mt = getmetatable(obj)
    mt.__index = function() return 42 end
end
"#;
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "getmetatable(obj) не должен отдавать метатаблицу, которую можно поменять"
    );
}

/// «Код игры»: `setmetatable(obj, {})` — объект userdata, не таблица, обычный Lua отказывает
/// `setmetatable` на любом значении кроме таблицы.
#[test]
fn setmetatable_on_an_object_stops_the_game() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","tamper"]]}
    ]}"#;
    let code = "function tamper(obj) setmetatable(obj, {}) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "setmetatable(obj, {{}}) должен остановить партию, obj — не таблица"
    );
}

/// «Код игры»: пара того же объекта не должна оказаться равна самому объекту — у них один номер
/// и поколение, но это разные значения.
#[test]
fn an_object_is_never_equal_to_its_own_pair_property() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","ok":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","check"]]}
    ]}"#;
    let code = "function check(a, b) a.ok = (a == a.velocity) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let ok = game.properties.resolve("ok").unwrap();
    assert!(
        !game.world.flag(0, ok),
        "объект не должен быть равен своей же паре velocity"
    );
}

/// «Код игры»: `math.random` с дробным аргументом без целого представления — ошибка кода, как в
/// Lua 5.4 (`bad argument ... number has no integer representation`).
#[test]
fn math_random_with_a_non_integer_argument_is_a_code_error() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","roll"]]}
    ]}"#;
    let code = "function roll() math.random(2.7) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(
        game.code_error().is_some(),
        "math.random(2.7) должен быть ошибкой кода"
    );
}

/// «Код игры»: `math.random(m, n)` с `m > n` — «interval is empty», как в настоящем Lua.
#[test]
fn math_random_with_an_empty_interval_is_a_code_error() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","roll"]]}
    ]}"#;
    let code = "function roll() math.random(5, 1) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    let err = game
        .code_error()
        .expect("math.random(5, 1) должен быть ошибкой кода");
    assert!(err.message.contains("interval is empty"), "{}", err.message);
}

/// «Код игры»: `math.random(1, 2^32)` — не должен урезать верхнюю границу через `as u32`
/// (2^32 не влезает в `u32`), должен дать целое в границах диапазона.
#[test]
fn math_random_with_a_range_wider_than_u32_does_not_truncate() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag","roll":"number"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true,"roll":0},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","roll"]]}
    ]}"#;
    let code = "function roll(a, b) a.roll = math.random(1, 2^32) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
    let roll = game.properties.resolve("roll").unwrap();
    let value = game.world.number_like(0, roll).unwrap();
    assert!(
        (1.0..=2f64.powi(32)).contains(&value),
        "значение должно остаться в границах диапазона, получено {value}"
    );
}

/// «Код игры»: `math.random(math.mininteger, math.maxinteger)` — самая широкая пара, не должна
/// переполнять вычитание в `i64` (паника в debug-сборке при старом `n - m`).
#[test]
fn math_random_with_mininteger_and_maxinteger_does_not_panic() {
    let props = r#"{"properties":{"marker":"flag","wall":"flag"}}"#;
    let scene = r##"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true,"marker":true},
        {"position":[0,0],"size":[1,1],"collides":true,"wall":true}
    ]}"##;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["marker"]},"b":{"has":["wall"]},"do":[["run","roll"]]}
    ]}"#;
    let code = "function roll() math.random(math.mininteger, math.maxinteger) end";
    let (mut game, _screens, warnings) =
        load(props, scene, rules, code).expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    game.step(StepInput::empty());
    assert!(game.code_error().is_none(), "{:?}", game.code_error());
}

use engine::data::load::{LoadFailure, load_game_from_texts, read_entry};

const GAME: &str = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
const PROPS_EMPTY: &str = r#"{"properties":{}}"#;
const SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

/// Computes the 1-based `(line, column)` of `needle`'s first occurrence in `text`, counting bytes
/// the same way `serde_json`'s own syntax-error positions do. Used by the location tests below to
/// check the engine's answer against an independent count rather than mirroring its algorithm.
fn expect_location(text: &str, needle: &str) -> (usize, usize) {
    let byte_pos = text.find(needle).expect("needle must appear in text");
    let prefix = &text[..byte_pos];
    let line = prefix.matches('\n').count() + 1;
    let column = match prefix.rfind('\n') {
        Some(last_newline) => byte_pos - last_newline,
        None => byte_pos + 1,
    };
    (line, column)
}

#[test]
fn broken_json_reports_line_and_column() {
    let broken = "{ \"objects\": [ { \"position\": [0,0]";
    let result = load_game_from_texts(GAME, PROPS_EMPTY, broken, r#"{"rules":[]}"#, SCREENS);
    let LoadFailure { errors, .. } = result.expect_err("сломанный JSON должен быть ошибкой");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "scene.json" && e.message.contains("строка")),
        "{errors:?}"
    );
}

#[test]
fn unknown_property_is_reported() {
    let scene = r#"{"objects":[{"positon":[0,0],"size":[1,1]}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("опечатка в свойстве — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("positon")),
        "{errors:?}"
    );
}

#[test]
fn wrong_value_kind_is_reported() {
    let scene = r#"{"objects":[{"position":"oops","size":[1,1]}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("строка вместо пары чисел — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("position")),
        "{errors:?}"
    );
}

#[test]
fn negative_size_is_reported() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[-1,1]}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("отрицательный размер — не ошибка сейчас?");
    assert!(!errors.is_empty());
}

#[test]
fn zero_grid_interval_is_reported() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"grid":{"interval":0}}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("interval <= 0 — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("interval")),
        "{errors:?}"
    );
}

#[test]
fn missing_grid_interval_is_reported() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"grid":{}}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("grid без interval — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("interval")),
        "{errors:?}"
    );
}

#[test]
fn unknown_rule_kind_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"teleport"}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("неизвестный вид правила — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("teleport")),
        "{errors:?}"
    );
}

#[test]
fn missing_required_rule_field_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"move"}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("нет \"for\" — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("for")),
        "{errors:?}"
    );
}

#[test]
fn condition_of_wrong_shape_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"delete","for":{"has":[]},"when":{"nonsense":true}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("условие не той формы — ошибка");
    assert!(!errors.is_empty());
}

#[test]
fn object_missing_property_needed_by_rule_is_reported() {
    let scene = r#"{"objects":[{"name":"o","position":[0,0],"size":[1,1]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position"]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("нет velocity — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("velocity")),
        "{errors:?}"
    );
}

#[test]
fn image_property_is_rejected_outright() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"image":"foo.png"}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("image должно быть ошибкой");
    assert!(
        errors
            .iter()
            .any(|e| e.message == "картинки в этой версии не поддержаны"),
        "{errors:?}"
    );
}

#[test]
fn all_errors_are_collected_in_one_pass_not_just_the_first() {
    let scene = r#"{"objects":[{"positon":[0,0]},{"position":"oops"}]}"#;
    let rules = r#"{"rules":[{"kind":"nope"}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("тут явно есть ошибки");
    assert!(
        errors.len() >= 3,
        "ожидались все ошибки разом, получили {errors:?}"
    );
}

#[test]
fn valid_minimal_game_loads_without_error() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":[0,0]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let (game, _screens, _warnings) =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
            .expect("валидная игра должна загрузиться");
    assert_eq!(game.world.alive_count(), 1);
}

#[test]
fn broken_random_seed_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":"oops","start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("random_seed не того вида — ошибка, а не молчаливый ноль");
    assert!(
        errors.iter().any(|e| e.path.contains("random_seed")),
        "{errors:?}"
    );
}

#[test]
fn fractional_random_seed_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1.5,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("дробное random_seed — ошибка");
    assert!(
        errors.iter().any(|e| e.path.contains("random_seed")),
        "{errors:?}"
    );
}

#[test]
fn missing_random_seed_is_allowed() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let scene = r#"{"objects":[]}"#;
    let (game, _screens, _warnings) =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect("отсутствие random_seed в игре допустимо — «Формат игры» не требует его");
    assert_eq!(game.world.alive_count(), 0);
}

/// Новый контракт `read_entry`: как и `load_rest`, на успехе она возвращает предупреждения
/// вторым элементом кортежа, а не только конфиг — сегодня их по game.json не бывает, но тип
/// возврата больше не даёт их молча потерять, если такое предупреждение когда-нибудь появится.
#[test]
fn read_entry_returns_warnings_alongside_config_on_success() {
    let (config, warnings) = read_entry(GAME).expect("валидный game.json должен разобраться");
    assert_eq!(config.random_seed, 1);
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
}

/// Тот же контракт на отказе: `read_entry` возвращает `LoadFailure`, у которого, как и у
/// `load_rest`, есть и `errors`, и `warnings` — а не голый список ошибок, из которого предупреждения
/// достать было бы неоткуда.
#[test]
fn read_entry_returns_warnings_alongside_errors_on_failure() {
    let broken = r##"{"scene":{"width":4,"height":4,"background":"#000000"}}"##;
    let Err(LoadFailure { errors, warnings }) = read_entry(broken) else {
        panic!("нет max_objects и files — должна быть ошибка");
    };
    assert!(!errors.is_empty(), "{errors:?}");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
}

/// «Формат игры» не ограничивает диапазон random_seed. Отрицательное зерно принимается и даёт
/// детерминированное, пусть и неочевидное, значение через оборачивание в u64.
#[test]
fn negative_random_seed_wraps_into_u64_deterministically() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":-1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let (config, _warnings) = read_entry(game_json).expect("отрицательное random_seed допустимо");
    assert_eq!(config.random_seed, u64::MAX);
}

/// Целое число больше `i64::MAX`, но представимое в `u64` (как хранит его JSON), — по-прежнему
/// целое число и не должно отвергаться как «не целое».
#[test]
fn random_seed_larger_than_i64_max_is_accepted() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":18446744073709551615,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let (config, _warnings) =
        read_entry(game_json).expect("u64::MAX как random_seed — целое число, не ошибка");
    assert_eq!(config.random_seed, u64::MAX);
}

/// «Формат игры» больше не знает поля `cell_pixels` — сцена вписывается в холст целиком, а не по
/// фиксированному размеру клетки. `GAME`, используемый почти всеми тестами этого файла, уже не
/// содержит этого поля; этот тест называет требование явно.
#[test]
fn game_json_without_cell_pixels_loads() {
    let scene = r#"{"objects":[]}"#;
    let (game, _screens, _warnings) =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect("game.json без cell_pixels должен загружаться");
    assert_eq!(game.world.alive_count(), 0);
}

/// `game.json → scene` теперь проверяет свои ключи на незнакомость так же, как `resolve_property`
/// ловит опечатку в объектах `scene.json`: лишний `cell_pixels` — та же тихая подмена, которую
/// «Формат игры» запрещает, а не безобидная мелочь.
#[test]
fn stray_cell_pixels_field_in_game_json_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"cell_pixels":24,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("незнакомое поле cell_pixels — ошибка, а не тихий пропуск");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "game.json" && e.message.contains("cell_pixels")),
        "{errors:?}"
    );
}

/// Опечатка в имени необязательного поля `random_seed` не должна молча превращаться в зерно 0:
/// «Формат игры» запрещает подстановку значения по умолчанию вместо сломанного, а `random_sed`
/// без общей проверки незнакомых ключей выглядел бы как «поле отсутствует» и тихо давал 0.
#[test]
fn misspelled_random_seed_field_is_reported_not_silently_defaulted() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_sed":20260907,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("опечатка random_sed — ошибка, а не молчаливый ноль");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "game.json" && e.message.contains("random_sed")),
        "{errors:?}"
    );
}

/// `files → images` и верхнеуровневое `name` — часть документированного формата (см. «Формат
/// игры»), просто пока не используются загрузчиком; общая проверка незнакомых ключей не должна
/// отвергать их как опечатку.
#[test]
fn documented_but_unused_name_and_images_fields_are_accepted() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{},"images":"images/"}}"##;
    let scene = r#"{"objects":[]}"#;
    let (game, _screens, _warnings) =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect("name и files.images — документированные поля, не ошибка");
    assert_eq!(game.world.alive_count(), 0);
}

#[test]
fn non_string_object_name_is_reported() {
    let scene = r#"{"objects":[{"name":42,"position":[0,0],"size":[1,1]}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS).expect_err(
            "число вместо строки в name — ошибка, а не молчаливая замена на objects[N]",
        );
    let name_errors: Vec<_> = errors.iter().filter(|e| e.path.contains("name")).collect();
    assert_eq!(
        name_errors.len(),
        1,
        "ровно одно сообщение про name, а не дубль: {errors:?}"
    );
}

#[test]
fn missing_object_name_is_allowed() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1]}]}"#;
    let (game, _screens, _warnings) =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect("name необязателен, отсутствие не ошибка");
    assert_eq!(game.world.alive_count(), 1);
}

/// «Исполнение игры»/`World::has`: признак со значением `false` отсутствует у объекта так же,
/// как если бы его не было вовсе — отбор `has` не должен подходить под такой объект ни во время
/// игры, ни на предстартовой проверке.
#[test]
fn false_flag_does_not_satisfy_has_selector_at_prestart() {
    let props = r#"{"properties":{"deadly":"flag","score":"number"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"deadly":false}]}"#;
    // Only objects actually selected by `for: {"has": ["deadly"]}` need `score`; this object
    // isn't one of them, because its `deadly` is false.
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["deadly"]},"when":["score","<=",0]}
    ]}"#;
    let (game, _screens, _warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect(
            "deadly=false не подходит под has:[\"deadly\"], score от этого объекта не требуется",
        );
    assert_eq!(game.world.alive_count(), 1);
}

/// Тот же запрет на «`false` = признака нет», только у шаблона «создать», а не у объекта сцены:
/// объект, который правило `spawn` создаст с `"collides": false`, никогда не подойдёт под
/// `has: ["collides"]`, значит и требовать от шаблона свойств этого отбора не за что.
#[test]
fn false_flag_in_spawn_template_does_not_satisfy_has_selector_at_prestart() {
    let scene = r#"{"objects":[]}"#;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["collides"]}},
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["size"]}}},
         "where":"random_cell",
         "template":{"size":[1,1],"collides":false,"color":"#ffffff","layer":1}}
    ]}"##;
    let (game, _screens, _warnings) = load_game_from_texts(
        GAME,
        PROPS_EMPTY,
        scene,
        rules,
        SCREENS,
    )
    .expect(
        "collides:false в шаблоне не подходит под has:[\"collides\"], velocity для move не нужен",
    );
    assert_eq!(game.world.alive_count(), 0);
}

/// Значение признака в шаблоне `from_parent` не известно на этапе загрузки: родитель мог бы
/// нести его, а мог бы и нет. «Формат игры» велит брать «все объекты, какие вообще могут
/// существовать» — значит кандидат раздваивается на форму с признаком и форму без него, и форма
/// с признаком обязана подходить под `has: ["deadly"]` и нести всё, что этот отбор требует.
/// В этих данных у неё нет `damage`, а его требует `when: ["damage", ">", 0]` правила `delete` —
/// предстартовая проверка обязана это заметить: иначе такой потомок никогда бы не удалялся
/// (`core/step.rs`: сравнение с отсутствующим числом молча даёт «ложь»), и автор игры об этом
/// не узнал бы.
#[test]
fn from_parent_flag_in_spawn_template_may_satisfy_has_selector_at_prestart() {
    let props = r#"{"properties":{"deadly":"flag","damage":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true,"deadly":true,"damage":5}
    ]}"#;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["deadly"]},"when":["damage",">",0]},
        {"kind":"spawn","when":{"after_move_of":{"has":["mover"]}},
         "where":"at_parent",
         "template":{"deadly":{"from_parent":"deadly"}}}
    ]}"##;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS).expect_err(
        "deadly через from_parent может оказаться true у потомка — этой форме не хватает damage для delete",
    );
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("damage") && e.message.contains("шаблон rules[2]")),
        "{errors:?}"
    );
}

/// Симметричный случай: `matches_shape` учитывает и `without`, значит форма-кандидат может
/// подойти под `without: ["deadly"]` вне зависимости от того, унаследует ли конкретный потомок
/// `deadly` от родителя. Различающий случай против трактовки «from_parent-признак всегда есть»:
/// без константного `damage` в шаблоне у формы, подходящей под `without:["deadly"]`, `damage`
/// нет — предстартовая проверка обязана дать ошибку. При трактовке «признак всегда есть»
/// `without:["deadly"]` не подошёл бы вовсе, и эта ошибка была бы пропущена.
#[test]
fn from_parent_flag_in_spawn_template_without_selector_still_needs_damage() {
    let props = r#"{"properties":{"deadly":"flag","damage":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true,"deadly":true}
    ]}"#;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"without":["deadly"]},"when":["damage",">",0]},
        {"kind":"spawn","when":{"after_move_of":{"has":["mover"]}},
         "where":"at_parent",
         "template":{"deadly":{"from_parent":"deadly"}}}
    ]}"##;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err(
            "форма без deadly подходит под without:[\"deadly\"], а damage в шаблоне нет — ошибка",
        );
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("damage") && e.message.contains("шаблон rules[2]")),
        "{errors:?}"
    );
}

/// Тот же шаблон, но с константным `damage` — форма, подходящая под `without:["deadly"]`, несёт
/// всё, что нужно, и ошибки быть не должно.
#[test]
fn from_parent_flag_in_spawn_template_without_selector_finds_matching_shape_sufficient() {
    let props = r#"{"properties":{"deadly":"flag","damage":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true,"deadly":true}
    ]}"#;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"without":["deadly"]},"when":["damage",">",0]},
        {"kind":"spawn","when":{"after_move_of":{"has":["mover"]}},
         "where":"at_parent",
         "template":{"deadly":{"from_parent":"deadly"},"damage":5}}
    ]}"##;
    let (game, _screens, _warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS).expect(
        "форма без deadly подходит под without:[\"deadly\"] и несёт damage — ошибки быть не должно",
    );
    assert_eq!(game.world.alive_count(), 1);
}

/// Раньше несколько `from_parent`-признаков в одном шаблоне разбивались полным перебором на
/// 2^N форм-кандидатов под одним и тем же именем, и каждая подошедшая форма давала своё дословно
/// совпадающее сообщение об одной и той же нехватке `damage`. Теперь шаблон — одна форма с двумя
/// множествами, значит и сообщение ровно одно, а не по одному на каждое сочетание `a`/`b`.
#[test]
fn from_parent_flags_missing_property_reports_exactly_one_message() {
    let props = r#"{"properties":{"a":"flag","b":"flag","damage":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true}
    ]}"#;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["a"]},"when":["damage",">",0]},
        {"kind":"spawn","when":{"after_move_of":{"has":["mover"]}},
         "where":"at_parent",
         "template":{"a":{"from_parent":"a"},"b":{"from_parent":"b"}}}
    ]}"##;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err(
            "отбору has:[\"a\"] может подойти форма шаблона, а damage в шаблоне нет — ошибка",
        );
    let damage_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("damage"))
        .collect();
    assert_eq!(
        damage_errors.len(),
        1,
        "ровно одно сообщение про damage, а не по одному на сочетание a/b: {errors:?}"
    );
}

/// Раньше это было 2^N форм-кандидатов, теперь одна форма на шаблон. 32 флага — не «страховка от
/// зависания» (перебор долго считал бы и на меньшем числе), а различающий случай: на прежней
/// реализации `1u32 << 32` детерминированно паникует переполнением сдвига, так что этот тест
/// провалился бы конкретной, а не расплывчатой по времени, ошибкой при возврате к перебору.
#[test]
fn spawn_template_with_thirty_two_from_parent_flags_loads() {
    let flag_names: Vec<String> = (0..32).map(|i| format!("f{i}")).collect();
    let props = format!(
        "{{\"properties\":{{{},\"mover\":\"flag\"}}}}",
        flag_names
            .iter()
            .map(|n| format!("\"{n}\":\"flag\""))
            .collect::<Vec<_>>()
            .join(",")
    );
    let scene_flags = flag_names
        .iter()
        .map(|n| format!("\"{n}\":true"))
        .collect::<Vec<_>>()
        .join(",");
    let scene = format!(
        "{{\"objects\":[{{\"position\":[0,0],\"size\":[1,1],\"velocity\":[1,0],\"mover\":true,{scene_flags}}}]}}"
    );
    let template_flags = flag_names
        .iter()
        .map(|n| format!("\"{n}\":{{\"from_parent\":\"{n}\"}}"))
        .collect::<Vec<_>>()
        .join(",");
    let rules = format!(
        "{{\"rules\":[\
            {{\"kind\":\"move\",\"for\":{{\"has\":[\"mover\"]}}}},\
            {{\"kind\":\"spawn\",\"when\":{{\"after_move_of\":{{\"has\":[\"mover\"]}}}},\
             \"where\":\"at_parent\",\"template\":{{{template_flags}}}}}\
        ]}}"
    );
    let (game, _screens, _warnings) = load_game_from_texts(GAME, &props, &scene, &rules, SCREENS)
        .expect("шаблон с двумя десятками from_parent-признаков должен грузиться");
    assert_eq!(game.world.alive_count(), 1);
}

/// Отбор, требующий одно и то же свойство и в `has`, и в `without`, не может подойти ни одному
/// объекту: подмножество без признака валит `has`, подмножество с признаком валит `without`.
/// «Формат игры» относит «правило, чей отбор заведомо никого не находит» к предупреждениям, при
/// которых игра всё равно запускается, — значит такой отбор не должен требовать от кандидатов
/// ничего, включая `damage`, которого нет ни у сценного объекта, ни у формы шаблона.
#[test]
fn selector_with_property_in_both_has_and_without_matches_nobody_and_is_not_an_error() {
    let props = r#"{"properties":{"deadly":"flag","damage":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true,"deadly":true}
    ]}"#;
    let rules = r##"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["deadly"],"without":["deadly"]},"when":["damage",">",0]},
        {"kind":"spawn","when":{"after_move_of":{"has":["mover"]}},
         "where":"at_parent",
         "template":{"deadly":{"from_parent":"deadly"}}}
    ]}"##;
    let (game, _screens, _warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS).expect(
        "has и without на одном и том же свойстве не подходят никому — предупреждение, не ошибка",
    );
    assert_eq!(game.world.alive_count(), 1);
}

/// `require` вызывался по разу на каждый `add`/`set` в `effects`, поэтому два `add` на одно и то
/// же отсутствующее свойство давали два дословно одинаковых сообщения. Дедупликация идёт по паре
/// «кандидат плюс свойство» внутри правила.
#[test]
fn two_adds_of_same_missing_property_report_one_message_not_two() {
    let props = r#"{"properties":{"score":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"mover":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["mover"]},"b":{"has":["mover"]},
         "effects":{"a":[["add","score",1],["add","score",2]]}}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("объекту не хватает score для add — ошибка");
    let score_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("score"))
        .collect();
    assert_eq!(
        score_errors.len(),
        1,
        "ровно одно сообщение про score, а не дубль на каждый add: {errors:?}"
    );
}

/// То же дублирование, но между разными require-проверками одного и того же правила: `bounce`
/// требует `velocity` сам по себе, а рядом `set velocity` требует его повторно через цикл по
/// `effects`. Обе проверки бьют по одному и тому же кандидату и свойству — сообщение должно быть
/// одно.
#[test]
fn bounce_next_to_set_velocity_reports_one_message_not_two() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1]}]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},
         "effects":{"a":[["bounce"],["set","velocity",[1,0]]]}}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("объекту не хватает velocity для bounce/set — ошибка");
    let velocity_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("velocity"))
        .collect();
    assert_eq!(
        velocity_errors.len(),
        1,
        "ровно одно сообщение про velocity, а не дубль bounce+set: {errors:?}"
    );
}

/// «Формат игры»: `after_move_of` спрашивает, сместился ли на этом шаге объект из отбора `of` —
/// положения самого удаляемого кандидата не читает, значит и требовать `position` у его `for`
/// не за что.
#[test]
fn after_move_of_delete_does_not_require_position_on_candidate() {
    let props = r#"{"properties":{"mover":"flag","fadeaway":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[1,0],"mover":true},
        {"fadeaway":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["fadeaway"]},"when":{"after_move_of":{"has":["mover"]}}}
    ]}"#;
    let (game, _screens, _warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("удаляемый объект без position не должен отвергаться предстартовой проверкой");
    assert_eq!(game.world.alive_count(), 2);
}

/// Та же асимметрия, что чинилась у «создать»: пустой `of` внутри `after_move_of` осмыслен только
/// у `fewer_than` (0 меньше count). У `after_move_of` пустой отбор означает, что правило не
/// сработает никогда — то же самое, что у `for`, и у правила «удалить» это тоже должно быть
/// предупреждением, а не тишиной.
#[test]
fn delete_rule_after_move_of_selector_matching_nobody_is_reported_as_a_warning() {
    let props = r#"{"properties":{"tag":"flag","fadeaway":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"fadeaway":true}]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":["fadeaway"]},
         "when":{"after_move_of":{"has":["tag"],"without":["tag"]}}}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect(
            "отбор в after_move_of, заведомо никого не находящий, — предупреждение, а не ошибка",
        );
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().any(|w| w.file == "rules.json"
            && w.path.contains("rules[0]")
            && w.path.contains("after_move_of")),
        "{warnings:?}"
    );
}

/// «Формат игры»: прибавке в `do` довольно, чтобы свойство было объявлено и встречалось хоть у
/// одного объекта. Сообщение об этом — одно на свойство, а не по одному на каждую прибавку.
#[test]
fn repeated_do_add_of_same_unused_property_reports_one_message_per_property() {
    let props = r#"{"properties":{"gold":"number","silver":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"mover":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["mover"]},"b":{"has":["mover"]},
         "do":[["add","gold",1],["add","gold",2],["add","silver",1]]}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("gold и silver не встречаются ни у одного объекта — ошибка");
    let gold: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("gold"))
        .collect();
    let silver: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("silver"))
        .collect();
    assert_eq!(
        gold.len(),
        1,
        "ровно одно сообщение про gold, а не дубль на каждую прибавку: {errors:?}"
    );
    assert_eq!(
        silver.len(),
        1,
        "silver не должен пропасть из-за отметки, поставленной для gold: {errors:?}"
    );
}

/// Отметка о выданном сообщении живёт внутри одного правила: два правила с одинаковой ошибкой —
/// две разные строки в списке, потому что чинить автору игры надо оба.
#[test]
fn same_do_add_error_in_two_rules_reports_two_messages() {
    let props = r#"{"properties":{"gold":"number","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"mover":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["mover"]},"b":{"has":["mover"]},"do":[["add","gold",1]]},
        {"kind":"collide","a":{"has":["mover"]},"b":{"has":["mover"]},"do":[["add","gold",1]]}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("gold не встречается ни у одного объекта — ошибка");
    let gold: Vec<_> = errors
        .iter()
        .filter(|e| e.message.contains("gold"))
        .collect();
    assert_eq!(gold.len(), 2, "по сообщению на каждое правило: {errors:?}");
}

/// `name` в `scene.json` не уникален — сорок кирпичей арканоида все названы "brick". Список ошибок
/// читает не человек, а агент, правящий файлы игры снаружи: сообщение обязано называть каждый
/// сломанный объект его местом в `objects`, а не только именем, иначе несколько одинаково
/// названных и одинаково сломанных объектов дадут дословно одинаковые, неразличимые сообщения.
#[test]
fn same_named_objects_get_distinguishable_messages() {
    let scene = r#"{"objects":[
        {"name":"brick","size":[1,1]},
        {"name":"brick","size":[1,1]},
        {"name":"brick","size":[1,1]}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":[]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("трём одинаково названным объектам, у которых нет velocity, — три ошибки");
    let velocity: Vec<&str> = errors
        .iter()
        .filter(|e| e.message.contains("velocity"))
        .map(|e| e.message.as_str())
        .collect();
    assert_eq!(
        velocity.len(),
        3,
        "по сообщению на каждый сломанный объект, а не одно на все \"brick\": {errors:?}"
    );
    let unique: std::collections::HashSet<&str> = velocity.iter().copied().collect();
    assert_eq!(
        unique.len(),
        3,
        "три сообщения должны отличаться друг от друга, а не совпадать дословно: {errors:?}"
    );
    for i in 0..3 {
        let needle = format!("objects[{i}]");
        assert!(
            velocity.iter().any(|m| m.contains(&needle)),
            "сообщение об объекте {i} должно называть его место в scene.json: {errors:?}"
        );
    }
}

/// «Формат игры»: объявленное свойство, которым никто не пользуется, — предупреждение, игра
/// всё равно запускается.
#[test]
fn declared_property_used_nowhere_is_reported_as_a_warning() {
    let props = r#"{"properties":{"halo":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":[0,0]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("неиспользуемое свойство — предупреждение, а не ошибка");
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings
            .iter()
            .any(|w| w.file == "properties.json" && w.message.contains("halo")),
        "{warnings:?}"
    );
}

/// «Формат игры»: объект, стоящий за пределами сцены, — предупреждение, игра всё равно
/// запускается.
#[test]
fn object_outside_scene_is_reported_as_a_warning() {
    let scene = r#"{"objects":[{"position":[100,100],"size":[1,1]}]}"#;
    let rules = r#"{"rules":[]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect("объект вне сцены — предупреждение, а не ошибка");
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().any(|w| w.file == "scene.json"
            && w.path.contains("objects[0]")
            && w.message.contains("сцен")),
        "{warnings:?}"
    );
}

/// «Формат игры»: правило, чей отбор заведомо никого не находит, — предупреждение, игра всё
/// равно запускается. Отдельный сценарий от `selector_with_property_in_both_has_and_without_
/// matches_nobody_and_is_not_an_error`: тот тест смотрит, что ошибки нет, этот — что вместо неё
/// приходит предупреждение с адресом отбора.
#[test]
fn move_rule_selector_matching_nobody_is_reported_as_a_warning() {
    let props = r#"{"properties":{"tag":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":[0,0]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["tag"],"without":["tag"]}}]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("отбор, заведомо никого не находящий, — предупреждение, а не ошибка");
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().any(|w| w.file == "rules.json"
            && w.path.contains("rules[0]")
            && w.path.contains("for")),
        "{warnings:?}"
    );
}

/// Воспроизведённый баг: «мяч плюс кирпич», собственного `hit` ни у кого в `scene.json` нет и ни
/// в одном шаблоне «создать» — но `["give","hit"]` в `effects.b` правила столкновения раздаёт его
/// кирпичу во время игры, и отбор `for: {has:["hit"]}` правила удаления реально находит кирпич.
/// До фикса `possible_shapes` не знал о `give` и предупреждал «отбор заведомо никого не находит»
/// на исправных данных.
#[test]
fn selector_on_a_property_given_at_runtime_is_not_reported_as_matching_nobody() {
    let props = r#"{"properties":{"hit":"flag"}}"#;
    let scene = r#"{"objects":[
        {"name":"ball","position":[0,0],"size":[1,1],"velocity":[1,0],"collides":true},
        {"name":"brick","position":[2,0],"size":[1,1],"collides":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"b":[["give","hit"]]}},
        {"kind":"delete","for":{"has":["hit"]},"when":"outside_scene"}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("give во время игры раздаёт hit — отбор для delete подходит кирпичу");
    assert_eq!(game.world.alive_count(), 2);
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "give должен избавить отбор has:[\"hit\"] от ложного предупреждения: {warnings:?}"
    );
}

/// Тот же механизм, но свойство появляется не через `give`, а через `keys`: у объекта его нет ни
/// в `scene.json`, ни в шаблоне, но нажатие клавиши записывает его значением `press`. Отбор,
/// требующий это свойство, реально может кому-то подойти.
#[test]
fn selector_on_a_property_set_by_keys_is_not_reported_as_matching_nobody() {
    let props = r#"{"properties":{"boosted":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],
         "keys":{"Space":{"press":[["boosted",true]]}}}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["boosted"]}}]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("клавиша может дать boosted во время игры — отбор для move не заведомо пуст");
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "нажатие клавиши должно избавить отбор от ложного предупреждения: {warnings:?}"
    );
}

/// Все три вида предупреждений разом, в одних данных: проверка собирает их все за один заход,
/// не только первый попавшийся, и ни одно из них не мешает игре запуститься.
#[test]
fn all_three_warning_kinds_are_collected_together_and_the_game_still_starts() {
    let props = r#"{"properties":{"halo":"flag","mover":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"mover":true},
        {"position":[100,100],"size":[1,1]}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"move","for":{"has":["mover"]}},
        {"kind":"delete","for":{"has":["mover"],"without":["mover"]},"when":"outside_scene"}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("три предупреждения разом не должны помешать игре запуститься");
    assert_eq!(game.world.alive_count(), 2);

    assert!(
        warnings
            .iter()
            .any(|w| w.file == "properties.json" && w.message.contains("halo")),
        "нет предупреждения про неиспользуемое свойство: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.file == "scene.json" && w.path.contains("objects[1]")),
        "нет предупреждения про объект вне сцены: {warnings:?}"
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.file == "rules.json" && w.path.contains("rules[1]")),
        "нет предупреждения про отбор, который никого не находит: {warnings:?}"
    );
    assert_eq!(
        warnings.len(),
        3,
        "предупреждения должны собраться все разом: {warnings:?}"
    );
}

/// Игра при ошибках всё равно не запускается, значит предупреждение — «игра всё же идёт» — не
/// может относиться к такому заходу вообще; а сам список форм, по которому предупреждения
/// считаются, неполон именно из-за этих ошибок (объект `positon` выпал из `scene_objects`,
/// толком не разобравшись). Предстартовая проверка честнее не притворяется, что досчитала
/// предупреждения на таких данных, — раз игра не стартует, они не считаются вовсе.
#[test]
fn warnings_are_not_computed_when_the_game_fails_to_load() {
    let props = r#"{"properties":{"halo":"flag"}}"#;
    // "positon" — опечатка, безусловная ошибка; "halo" тем временем нигде не используется, что
    // само по себе — предупреждение, но не в этом заходе.
    let scene = r#"{"objects":[{"positon":[0,0],"size":[1,1]}]}"#;
    let rules = r#"{"rules":[]}"#;
    let LoadFailure { errors, warnings } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("опечатка в свойстве — игра не должна стартовать");
    assert!(
        errors.iter().any(|e| e.message.contains("positon")),
        "{errors:?}"
    );
    assert!(
        warnings.is_empty(),
        "при уже собранных ошибках предупреждения не считаются: {warnings:?}"
    );
}

/// Тот самый воспроизведённый баг: элементы `objects`, не оказавшиеся объектом JSON, — две
/// ошибки, — целиком выпадают из набора форм ещё до чтения `rules.json`. Без нового заслона
/// `rules[0]`, чей отбор в исправленных данных мог бы подойти любому из них, получал бы лишнее
/// предупреждение «отбор никого не находит» — предупреждение, порождённое той же неполнотой,
/// что и сама ошибка, а не свойствами самого правила.
#[test]
fn incomplete_scene_data_does_not_spawn_a_spurious_selector_warning() {
    let props = r#"{"properties":{"tag":"flag"}}"#;
    let scene = r#"{"objects":[42,"oops"]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["tag"]}}]}"#;
    let LoadFailure { errors, warnings } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("элементы objects не того вида — ошибки, игра не должна стартовать");
    assert!(errors.len() >= 2, "{errors:?}");
    assert!(
        warnings.is_empty(),
        "неполные из-за ошибок данные не должны порождать предупреждение про rules[0]: {warnings:?}"
    );
}

/// «Формат игры»: место внутри файла со строкой и столбцом, указывающими на само сломанное
/// значение — здесь `"oops"` вместо пары чисел.
#[test]
fn wrong_value_kind_error_points_at_the_value_with_line_and_column() {
    let scene = "{\n  \"objects\": [\n    {\"position\": \"oops\", \"size\": [1, 1]}\n  ]\n}\n";
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("строка вместо пары чисел — ошибка");
    let err = errors
        .iter()
        .find(|e| e.file == "scene.json" && e.path.contains("position"))
        .unwrap_or_else(|| panic!("должна быть ошибка про position: {errors:?}"));
    let (line, column) = expect_location(scene, "\"oops\"");
    assert_eq!(
        (err.line, err.column),
        (Some(line), Some(column)),
        "{err:?}"
    );
}

/// Ошибка о недостающей обязательной настройке правила указывает на само правило-объект, а не на
/// не существующее в тексте поле.
#[test]
fn missing_required_rule_field_error_points_at_the_rule() {
    let scene = r#"{"objects":[]}"#;
    let rules = "{\n  \"rules\": [\n    {\"kind\": \"move\"}\n  ]\n}\n";
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("нет \"for\" — ошибка");
    let err = errors
        .iter()
        .find(|e| e.file == "rules.json" && e.message.contains("for"))
        .unwrap_or_else(|| panic!("должна быть ошибка про for: {errors:?}"));
    let (line, column) = expect_location(rules, "{\"kind\": \"move\"}");
    assert_eq!(
        (err.line, err.column),
        (Some(line), Some(column)),
        "{err:?}"
    );
}

/// «Объекту не хватает свойства для правила» — сообщение о правиле, позиция указывает на само
/// правило в `rules.json`.
#[test]
fn object_missing_property_needed_by_rule_error_has_location() {
    let scene = r#"{"objects":[{"name":"o","position":[0,0],"size":[1,1]}]}"#;
    let rules = "{\"rules\": [\n  {\"kind\": \"move\", \"for\": {\"has\": [\"position\"]}}\n]}";
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("нет velocity — ошибка");
    let err = errors
        .iter()
        .find(|e| e.message.contains("velocity"))
        .unwrap_or_else(|| panic!("должна быть ошибка про velocity: {errors:?}"));
    let (line, column) = expect_location(
        rules,
        "{\"kind\": \"move\", \"for\": {\"has\": [\"position\"]}}",
    );
    assert_eq!(
        (err.line, err.column),
        (Some(line), Some(column)),
        "{err:?}"
    );
}

/// «Формат игры»: предупреждения получают позицию так же, как ошибки. Здесь — неиспользуемое
/// свойство в `properties.json`, позиция указывает на его значение (вид свойства).
#[test]
fn unused_property_warning_has_location() {
    let props = "{\n  \"properties\": {\n    \"halo\": \"flag\"\n  }\n}\n";
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":[0,0]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position","velocity"]}}]}"#;
    let (_game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("неиспользуемое свойство — предупреждение, а не ошибка");
    let warn = warnings
        .iter()
        .find(|w| w.file == "properties.json" && w.message.contains("halo"))
        .unwrap_or_else(|| panic!("должно быть предупреждение про halo: {warnings:?}"));
    let (line, column) = expect_location(props, "\"flag\"");
    assert_eq!(
        (warn.line, warn.column),
        (Some(line), Some(column)),
        "{warn:?}"
    );
}

/// «Формат игры»: объект вне сцены — предупреждение с позицией, указывающей на его `position`.
#[test]
fn object_outside_scene_warning_has_location() {
    let scene = "{\n  \"objects\": [\n    {\"position\": [100, 100], \"size\": [1, 1]}\n  ]\n}\n";
    let rules = r#"{"rules":[]}"#;
    let (_game, _screens, warnings) =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
            .expect("объект вне сцены — предупреждение, а не ошибка");
    let warn = warnings
        .iter()
        .find(|w| w.file == "scene.json" && w.path.contains("position"))
        .unwrap_or_else(|| panic!("должно быть предупреждение про сцену: {warnings:?}"));
    let (line, column) = expect_location(scene, "[100, 100]");
    assert_eq!(
        (warn.line, warn.column),
        (Some(line), Some(column)),
        "{warn:?}"
    );
}

/// `objects` отсутствует у `scene.json` целиком, значит называть путём ошибки сам недостающий
/// ключ бессмысленно — вторым проходом такой путь никогда не найти, он по определению не в
/// тексте. Тот же приём, что у `require_field`: путь называет объект-родителя (здесь — корень
/// файла, `""`). Позиции всё равно нет — у корня файла её и не бывает, — но выдуманных чисел нет,
/// это правильно.
#[test]
fn error_with_unresolvable_path_keeps_message_without_a_location() {
    let scene = r#"{}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("нет objects — ошибка");
    let err = errors
        .iter()
        .find(|e| e.file == "scene.json" && e.message.contains("список объектов"))
        .unwrap_or_else(|| panic!("должна быть ошибка про список объектов: {errors:?}"));
    assert_eq!(
        err.path, "",
        "путь называет объект-родителя, а не недостающий ключ"
    );
    assert_eq!((err.line, err.column), (None, None), "{err:?}");
}

/// Битый JSON и отсутствующий файл сохраняют своё прежнее поведение: без структурных `line`/
/// `column` — у битого JSON собственная позиция уже вписана словами в `message` через serde_json,
/// а у отсутствующего файла позиции нет вовсе, и вторым проходом искать нечего.
#[test]
fn broken_json_and_missing_file_get_no_structured_location() {
    let broken = "{ \"objects\": [ { \"position\": [0,0]";
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, broken, r#"{"rules":[]}"#, SCREENS)
            .expect_err("сломанный JSON — ошибка");
    let broken_err = errors
        .iter()
        .find(|e| e.file == "scene.json")
        .unwrap_or_else(|| panic!("должна быть ошибка про scene.json: {errors:?}"));
    assert_eq!((broken_err.line, broken_err.column), (None, None));
    assert!(broken_err.message.contains("строка"), "{broken_err:?}");

    let (config, _warnings) = read_entry(GAME).expect("game.json валиден");
    let missing_result = engine::data::load::load_rest(config, None, None, None, None, &[]);
    let LoadFailure { errors, .. } = missing_result.expect_err("отсутствующие файлы — ошибка");
    assert!(
        errors
            .iter()
            .all(|e| e.line.is_none() && e.column.is_none()),
        "{errors:?}"
    );
}

/// Баг 1, зеркальный к уже починенному `give`/`keys`: снятие признака во время игры (`take`) —
/// точно такое же расширение множества возможных объектов для `without`, как `give` даёт для
/// `has`. Единственный объект сцены несёт `shield` (certain), и только столкновение снимает его
/// через `take`; отбор `without:["shield"]` у «подвинуть» реально может подойти этому объекту
/// после снятия признака, значит предупреждения «отбор заведомо не подходит» быть не должно.
#[test]
fn selector_without_a_property_taken_at_runtime_is_not_reported_as_matching_nobody() {
    let props = r#"{"properties":{"shield":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"collides":true,"shield":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["shield"]},"b":{"has":[]},
         "effects":{"a":[["take","shield"]]}},
        {"kind":"move","for":{"has":["position","velocity"],"without":["shield"]}}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect(
            "take во время игры может снять shield — отбор without:[\"shield\"] не заведомо пуст",
        );
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "take должен избавить отбор without:[\"shield\"] от ложного предупреждения: {warnings:?}"
    );
}

/// Баг 1, тот же механизм через `["set", "<признак>", false]` вместо `take`.
#[test]
fn selector_without_a_property_set_to_false_at_runtime_is_not_reported_as_matching_nobody() {
    let props = r#"{"properties":{"shield":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"collides":true,"shield":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["shield"]},"b":{"has":[]},
         "effects":{"a":[["set","shield",false]]}},
        {"kind":"move","for":{"has":["position","velocity"],"without":["shield"]}}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS).expect(
        "set … false во время игры может снять shield — отбор without:[\"shield\"] не заведомо пуст",
    );
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "set … false должен избавить отбор without:[\"shield\"] от ложного предупреждения: {warnings:?}"
    );
}

/// Баг 2: `press`/`release` у клавиши — такой же документированный, закрытый список полей, как
/// прочие; опечатка `prss` вместо `press` не должна молча превращаться в пустой список правок.
#[test]
fn misspelled_key_binding_field_is_reported() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],
        "keys":{"Space":{"prss":[["position",[1,1]]]}}}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("опечатка prss вместо press — ошибка, а не молчаливый пропуск");
    assert!(
        errors.iter().any(|e| e.message.contains("prss")),
        "{errors:?}"
    );
}

/// Баг 2: набор полей у правила фиксирован его видом; опечатка `whn` вместо `when` у «удалить»
/// не должна молча пропасть — она тише всех, потому что выбрасывает целый отбор условия.
#[test]
fn misspelled_rule_field_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"delete","for":{"has":[]},"whn":"outside_scene"}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("опечатка whn вместо when — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("whn")),
        "{errors:?}"
    );
}

/// Баг 2: корень `scene.json` знает только `objects` — лишнее поле не должно молча загружаться.
#[test]
fn stray_field_in_scene_json_root_is_reported() {
    let scene = r#"{"objects":[],"cell_pixels":24}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("лишнее поле в корне scene.json — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "scene.json" && e.message.contains("cell_pixels")),
        "{errors:?}"
    );
}

/// Баг 2: отбор знает только `has` и `without`; опечатка `withut` тише всех прочих — она
/// выбрасывает целый фильтр, и игра идёт с другим поведением молча.
#[test]
fn misspelled_selector_field_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":[],"withut":["tag"]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("опечатка withut вместо without — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("withut")),
        "{errors:?}"
    );
}

/// Баг 3: одна ошибка данных должна давать одно сообщение. `velocity` не того вида уже даёт
/// ошибку про сам вид значения; вторая, «объекту не хватает свойства velocity», — производная от
/// той же неполноты и должна исчезнуть, раз объект, о котором она была, сам не разобрался.
#[test]
fn broken_value_on_an_object_does_not_also_report_it_missing_the_property() {
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":"oops"}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position"]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("velocity не того вида — ошибка");
    let velocity_errors: Vec<_> = errors
        .iter()
        .filter(|e| e.path.contains("velocity") || e.message.contains("velocity"))
        .collect();
    assert_eq!(
        velocity_errors.len(),
        1,
        "одна ошибка данных должна давать одно сообщение, а не два: {errors:?}"
    );
    assert!(
        velocity_errors[0].message.contains("ожидался массив"),
        "должно остаться сообщение про вид значения, а не про нехватку свойства: {velocity_errors:?}"
    );
}

/// Баг 3: настоящая нехватка свойства у исправного объекта не должна пропасть из-за того, что
/// рядом сломан другой объект того же файла — это отдельный объект от `broken_value_on_an_object_
/// does_not_also_report_it_missing_the_property`, где ломается тот же объект, о котором сообщение.
#[test]
fn missing_property_on_a_healthy_object_still_reported_next_to_a_broken_one() {
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":"oops"},
        {"position":[2,0],"size":[1,1]}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["position"]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("velocity не того вида и вторая нехватка — обе ошибки");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("velocity") && e.message.contains("objects[1]")),
        "исправному objects[1] по-прежнему не хватает velocity: {errors:?}"
    );
}

/// Баг 4: верхнеуровневое `name` в `game.json` документировано, но не проверялось — число вместо
/// строки должно быть ошибкой, а не молчаливо принятым значением.
#[test]
fn non_string_game_json_name_is_reported() {
    let game_json = r##"{"name":42,"scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{}}}"##;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("число вместо строки в name — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "game.json" && e.path == "name"),
        "{errors:?}"
    );
}

/// Баг 4: `files → images` не проверялось так же — число вместо строки не должно молча грузиться.
#[test]
fn non_string_files_images_is_reported() {
    let game_json = r##"{"name":"T","scene":{"width":4,"height":4,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json","screens":"screens.json","fonts":{},"images":42}}"##;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(game_json, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("число вместо строки в files → images — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "game.json" && e.path.contains("images")),
        "{errors:?}"
    );
}

/// Баг 6: `of` внутри `fewer_than` — такой же отбор, как всякий другой; заведомо пустой (`has` и
/// `without` на одном и том же свойстве) делает условие «меньше N» всегда истинным, и «создать»
/// сыпало бы объекты до `max_objects` — документ не делает исключения по виду отбора.
#[test]
fn spawn_rule_fewer_than_of_selector_matching_nobody_is_reported_as_a_warning() {
    let props = r#"{"properties":{"tag":"flag","food":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r##"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["tag"],"without":["tag"]}}},
         "where":"random_cell",
         "template":{"size":[1,1],"collides":true,"color":"#ffffff","food":true}}
    ]}"##;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("отбор fewer_than.of, заведомо никого не находящий, — предупреждение, а не ошибка");
    assert_eq!(game.world.alive_count(), 0, "загрузка ничего не создаёт");
    assert!(
        warnings.iter().any(|w| w.file == "rules.json"
            && w.path.contains("rules[0]")
            && w.path.contains("fewer_than")),
        "{warnings:?}"
    );
}

/// Баг 7, зеркальный к уже починенному случаю с нажатием клавиши: правка по клавише значением
/// `false` может только СНЯТЬ признак (`World::set_flag(id, prop, false)`), а не выдать его —
/// значит отбору `has:["boosted"]` подойти неоткуда, и предупреждение должно остаться.
#[test]
fn selector_on_a_property_only_ever_set_false_by_keys_is_reported_as_matching_nobody() {
    let props = r#"{"properties":{"boosted":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],
         "keys":{"Space":{"press":[["boosted",false]]}}}
    ]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["boosted"]}}]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect(
            "нажатие, всегда ставящее false, не может дать boosted — предупреждение, а не тишина",
        );
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().any(|w| w.file == "rules.json"
            && w.path.contains("rules[0]")
            && w.path.contains("for")),
        "ложноотрицательный keys-edit false не должен подавлять предупреждение: {warnings:?}"
    );
}

/// Баг 9: неудачный `read_entry` не сбрасывал конфиг, оставшийся от более раннего успешного
/// захода — второй `load()` тихо брал бы игру не из того захода. `PendingConfig` — сама эта
/// развязка вне wasm-обвязки, поэтому её можно проверить без браузера и канваса.
#[test]
fn pending_config_is_cleared_after_a_failed_read_entry() {
    use engine::data::load::PendingConfig;
    let mut pending = PendingConfig::default();

    pending.set(&read_entry(GAME));
    assert!(
        pending.take().is_some(),
        "успешный read_entry должен оставить конфиг ожидающим"
    );

    pending.set(&read_entry(GAME));
    let broken = r##"{"scene":{"width":4,"height":4,"background":"#000000"}}"##;
    pending.set(&read_entry(broken));
    assert!(
        pending.take().is_none(),
        "неудачный read_entry не должен оставлять конфиг от более раннего успешного захода"
    );
}

/// Регрессия этого круга ревью (пункт 1): `b` из пары столкновения не подходит ни одному
/// возможному объекту (`ghost` нет нигде), значит это правило столкновения никогда не сработает
/// и его `take` на стороне `a` не должен расширять `removable` для `shield`. Раньше расширение
/// смотрело только на сторону `a`, не спрашивая, может ли сработать сторона `b`, — и без реального
/// `take` отбор `without:["shield"]` у «подвинуть» ложно считался подходящим этому объекту, отчего
/// «подвинуть» требовало `velocity`, которого у объекта нет, — игра не запускалась вовсе. С фиксом
/// отбор «подвинуть» не подходит никому (shield остаётся certain), и игра загружается с
/// предупреждениями, а не с ошибкой.
#[test]
fn collide_effect_on_a_side_that_can_never_match_does_not_widen_the_other_side() {
    let props = r#"{"properties":{"shield":"flag","ghost":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"shield":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":["shield"]},"b":{"has":["ghost"]},
         "effects":{"a":[["take","shield"]]}},
        {"kind":"move","for":{"has":["position"],"without":["shield"]}}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect(
            "ghost нигде не встречается, столкновение никогда не сработает — take не должен ложно \
         снимать shield и требовать velocity",
        );
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().all(|w| !w.message.contains("velocity")),
        "{warnings:?}"
    );
}

/// Регрессия этого круга (пункт 2): значение `position` не того вида — отдельная, самостоятельная
/// ошибка данных на этом объекте, а нехватка `velocity` (её у объекта вовсе нет) — другая,
/// независимая. Раньше любая своя ошибка объекта гасила `require` для всех его свойств разом, и
/// вторая ошибка терялась в этом же заходе.
#[test]
fn broken_property_on_an_object_does_not_suppress_an_unrelated_missing_property_on_it() {
    let scene = r#"{"objects":[{"position":"oops","size":[1,1]}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"has":["size"]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("position не того вида и нет velocity — обе ошибки");
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains("position") && e.message.contains("ожидался массив")),
        "должна остаться ошибка про вид значения position: {errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.message.contains("velocity")),
        "нехватка velocity не должна пропасть из-за сломанного position на том же объекте: {errors:?}"
    );
}

/// Пункт 3: то же самое, только сломанное значение — поле шаблона «создать», а не поле объекта
/// сцены. `rules[0]`'s `template.size` не того вида — своя ошибка; `rules[1]` («столкнуть»)
/// безусловно требует `size` у формы `шаблон rules[0]` (она подходит под его отбор `mover`), и
/// раньше получал производную «не хватает size», указывая на невиновное правило `rules[1]`, хотя
/// единственная настоящая проблема — в `rules[0]`.
#[test]
fn broken_template_property_does_not_report_it_missing_on_a_different_rule() {
    let props = r#"{"properties":{"mover":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":[]}}},
         "where":"random_cell",
         "template":{"mover":true,"size":"oops"}},
        {"kind":"collide","a":{"has":["mover"]},"b":{"has":["mover"]}}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("size не того вида в шаблоне — ошибка");
    let size_errors: Vec<_> = errors.iter().filter(|e| e.path.contains("size")).collect();
    assert_eq!(
        size_errors.len(),
        1,
        "одна ошибка про size — про её собственный вид, без производной от rules[1]: {errors:?}"
    );
    assert!(
        size_errors[0].path.contains("template"),
        "оставшаяся ошибка должна указывать на template, а не на невиновное правило: {size_errors:?}"
    );
}

/// Пункт 4: стороны внутри `effects` — закрытый список `a`/`b`, как и всё остальное в формате
/// правил; опечатка `aa` не должна молча выбрасывать список действий стороны.
#[test]
fn misspelled_effects_side_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"aa":[["bounce"]]}}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("опечатка aa вместо a — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("aa")),
        "{errors:?}"
    );
}

/// Пункт 5: корень `rules.json` знает только `rules`, как и корни `game.json`/`scene.json`.
#[test]
fn stray_field_in_rules_json_root_is_reported() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[],"rlues":[]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("лишнее поле в корне rules.json — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "rules.json" && e.message.contains("rlues")),
        "{errors:?}"
    );
}

/// Пункт 6: `set` в `effects` пишет значение независимо от того, было ли свойство у объекта
/// раньше (`World::set_value`) — так же, как `keys`-правка тем же значением ничего не требует
/// (`keys_edited_properties`). Требовать наличие `hot` до столкновения — то же самое несоответствие
/// движку, которое когда-то уже правили для `give`.
#[test]
fn set_effect_does_not_require_the_property_to_already_be_present() {
    let props = r#"{"properties":{"hot":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"collides":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["set","hot",true]]}}
    ]}"#;
    let (game, _screens, _warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("set пишет hot независимо от прежнего наличия — требовать его не за что");
    assert_eq!(game.world.alive_count(), 1);
}

/// Пункт 6, продолжение: `set` расширяет множество возможных объектов так же, как `give` —
/// значение, которого нет ни у одного объекта сцены и ни в одном шаблоне, `set` во время игры
/// всё же может дать, значит отбор `has:["hot"]` не заведомо пуст.
#[test]
fn set_effect_widens_maybe_like_give() {
    let props = r#"{"properties":{"hot":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"velocity":[0,0],"collides":true}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["set","hot",true]]}},
        {"kind":"move","for":{"has":["hot"]}}
    ]}"#;
    let (game, _screens, warnings) = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect("set во время игры может дать hot — отбор has:[\"hot\"] у move не заведомо пуст");
    assert_eq!(game.world.alive_count(), 1);
    assert!(
        warnings.iter().all(|w| !w.message.contains("не подходит")),
        "set должен избавить отбор от ложного предупреждения: {warnings:?}"
    );
}

/// Пункт 7: `fewer_than` знает только `count` и `of`.
#[test]
fn fewer_than_rejects_unknown_key() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[
        {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":[]},"extra":true}},
         "where":"random_cell","template":{"size":[1,1]}}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("лишнее поле extra в fewer_than — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("extra")),
        "{errors:?}"
    );
}

/// Пункт 7: `grid` знает только `interval`.
#[test]
fn grid_rejects_unknown_key() {
    let scene =
        r#"{"objects":[{"position":[0,0],"size":[1,1],"grid":{"interval":1,"extra":true}}]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("лишнее поле extra в grid — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("extra")),
        "{errors:?}"
    );
}

/// Пункт 9: `kind` не того вида (число вместо строки) должен сообщать про вид значения, а не
/// маскироваться под «отсутствует».
#[test]
fn rule_kind_of_wrong_json_type_reports_wrong_kind_not_missing() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":5,"for":{"has":[]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("kind:5 — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.path.contains("kind") && e.message.contains("ожидалась строка")),
        "{errors:?}"
    );
    assert!(
        errors.iter().all(|e| !e.message.contains("отсутствует")),
        "число в kind — не то же самое, что отсутствующий kind: {errors:?}"
    );
}

/// Пункт 9, продолжение: опечатка в виде правила не должна прятать опечатку в остальных его полях
/// до следующего захода — обе должны прийти в одном списке.
#[test]
fn unknown_rule_kind_still_reports_a_typo_in_another_field_the_same_pass() {
    let scene = r#"{"objects":[]}"#;
    let rules = r#"{"rules":[{"kind":"mve","fpr":{"has":[]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("неизвестный kind — ошибка");
    assert!(
        errors.iter().any(|e| e.message.contains("mve")),
        "{errors:?}"
    );
    assert!(
        errors.iter().any(|e| e.message.contains("fpr")),
        "опечатка fpr должна быть видна в том же заходе, а не только после починки kind: {errors:?}"
    );
}

/// Пункт 1 этого круга: сломанное значение признака выбрасывает `ball` из `certain`, но кандидат
/// со сломанным свойством не должен считаться заведомо не имеющим его — иначе `without:["ball"]`
/// ложно подходит этому объекту, и «подвинуть» требует от него ещё и `velocity`, которого в этих
/// данных нет и требовать не за что: единственная настоящая проблема — сам сломанный `ball`.
#[test]
fn without_selector_does_not_match_a_candidate_whose_own_property_value_is_broken() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1],"ball":"yes"}]}"#;
    let rules = r#"{"rules":[{"kind":"move","for":{"without":["ball"]}}]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("ball не того вида — ошибка");
    assert_eq!(
        errors.len(),
        1,
        "сломанный ball не должен давать вторую, производную ошибку про without: {errors:?}"
    );
    assert!(errors[0].message.contains("ожидался признак"), "{errors:?}");
}

/// Пункт 2 этого круга: как и у `properties.json` (`require_field`), адрес ошибки об отсутствующем
/// `kind` называет само правило, а не несуществующий в тексте ключ — путём, вторым проходом
/// (`locate`) которого позицию найти неоткуда.
#[test]
fn missing_rule_kind_error_points_at_the_rule() {
    let scene = r#"{"objects":[]}"#;
    let rules = "{\n  \"rules\": [\n    {\"for\": {\"has\": []}}\n  ]\n}\n";
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, PROPS_EMPTY, scene, rules, SCREENS)
        .expect_err("нет kind — ошибка");
    let err = errors
        .iter()
        .find(|e| e.file == "rules.json" && e.message.contains("kind"))
        .unwrap_or_else(|| panic!("должна быть ошибка про kind: {errors:?}"));
    let (line, column) = expect_location(rules, "{\"for\": {\"has\": []}}");
    assert_eq!(
        (err.line, err.column),
        (Some(line), Some(column)),
        "{err:?}"
    );
}

/// Пункт 2 этого круга, тот же адрес для отсутствующего `where` у «создать».
#[test]
fn missing_spawn_where_error_points_at_the_rule() {
    let scene = r#"{"objects":[]}"#;
    let rule_text =
        "{\"kind\": \"spawn\", \"when\": {\"fewer_than\": {\"count\": 1, \"of\": {\"has\": []}}}}";
    let rules = format!("{{\n  \"rules\": [\n    {rule_text}\n  ]\n}}\n");
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, PROPS_EMPTY, scene, &rules, SCREENS)
            .expect_err("нет where — ошибка");
    let err = errors
        .iter()
        .find(|e| e.file == "rules.json" && e.message.contains("at_parent"))
        .unwrap_or_else(|| panic!("должна быть ошибка про where: {errors:?}"));
    let (line, column) = expect_location(&rules, rule_text);
    assert_eq!(
        (err.line, err.column),
        (Some(line), Some(column)),
        "{err:?}"
    );
}

/// Пункт 4 этого круга: корень `properties.json` знает только `properties`, как и корни
/// `game.json`/`scene.json`/`rules.json` — лишнее поле (здесь опечатка `propertes`) не должно
/// молча загружаться без единого сообщения.
#[test]
fn stray_field_in_properties_json_root_is_reported() {
    let props = r#"{"properties":{},"propertes":{"x":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let LoadFailure { errors, .. } =
        load_game_from_texts(GAME, props, scene, r#"{"rules":[]}"#, SCREENS)
            .expect_err("лишнее поле (опечатка propertes) в корне properties.json — ошибка");
    assert!(
        errors
            .iter()
            .any(|e| e.file == "properties.json" && e.message.contains("propertes")),
        "{errors:?}"
    );
}

/// Ревью, пункт 1: фикс «сломано бьёт снимаемо» закрыт только наполовину, если `removable`
/// проверяется без оглядки на `broken_properties` — свойство, снятое `take` в том же заходе
/// widening (и `a`, и `b` столкновения реально подходят кому-то), одновременно и сломано на этом
/// кандидате, и «снимаемо». Единственная настоящая проблема — сам сломанный `ball`; `without:
/// ["ball"]` не должен считать его заведомо отсутствующим только потому, что где-то есть `take`.
#[test]
fn without_selector_does_not_match_a_broken_property_widened_to_removable_by_take() {
    let props = r#"{"properties":{"ball":"flag"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"ball":"yes"}
    ]}"#;
    let rules = r#"{"rules":[
        {"kind":"collide","a":{"has":[]},"b":{"has":[]},"effects":{"a":[["take","ball"]]}},
        {"kind":"move","for":{"without":["ball"]}}
    ]}"#;
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, rules, SCREENS)
        .expect_err("ball не того вида — ошибка");
    assert_eq!(
        errors.len(),
        1,
        "снимаемость сломанного ball через take не должна давать вторую, производную ошибку про \
         velocity: {errors:?}"
    );
    assert!(errors[0].message.contains("ожидался признак"), "{errors:?}");
}

/// Ревью, пункт 2: правило, не разобравшееся из-за ошибки (`nonsense`), выпадает из `RuleSet::rules`
/// и сдвигает позиции всех следующих правил в этом списке — но сообщение обязано называть настоящий
/// номер правила в файле, а не номер в отфильтрованном списке. `rules[0]` — сломанный `nonsense`,
/// `rules[1]` — «создать» с шаблоном без `velocity`, `rules[2]` — «подвинуть», которому эта форма
/// не хватает `velocity`. И текст сообщения (`шаблон rules[1]`), и адрес (`rules[2]`), и позиция в
/// тексте должны указывать на настоящие, а не на соседние правила.
#[test]
fn message_about_a_rule_after_a_broken_one_addresses_its_real_file_position() {
    let props = r#"{"properties":{"mover":"flag"}}"#;
    let scene = r#"{"objects":[]}"#;
    let spawn_text = "{\"kind\": \"spawn\", \"when\": {\"fewer_than\": {\"count\": 1, \"of\": \
        {\"has\": []}}}, \"where\": \"random_cell\", \"template\": {\"mover\": true, \"size\": \
        [1, 1]}}";
    let move_text = "{\"kind\": \"move\", \"for\": {\"has\": [\"mover\"]}}";
    let rules = format!(
        "{{\n  \"rules\": [\n    {{\"kind\": \"nonsense\"}},\n    {spawn_text},\n    {move_text}\n  ]\n}}\n"
    );
    let LoadFailure { errors, .. } = load_game_from_texts(GAME, props, scene, &rules, SCREENS)
        .expect_err("nonsense и нехватка velocity — ошибки");
    let err = errors
        .iter()
        .find(|e| e.message.contains("velocity"))
        .unwrap_or_else(|| panic!("должна быть ошибка про velocity: {errors:?}"));
    assert!(
        err.message.contains("шаблон rules[1]"),
        "сообщение должно называть настоящий номер шаблона в файле, rules[1], а не rules[0]: \
         {err:?}"
    );
    assert_eq!(
        err.path, "rules[2]",
        "адрес должен указывать на настоящее виновное правило, rules[2] (move), а не на соседнее: \
         {err:?}"
    );
    let (line, column) = expect_location(&rules, move_text);
    assert_eq!(
        (err.line, err.column),
        (Some(line), Some(column)),
        "позиция должна указывать на настоящее правило-виновника в тексте файла: {err:?}"
    );
}

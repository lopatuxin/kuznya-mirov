//! Фаза 08 — редактор: скелет. «Редактор», требование 16 — `Game::show_scene` собирает мир из
//! сцены как `new_game`, но без начальных значений, без кода и без партии. Остальные требования
//! этого шага (`object_at`/`object_rect`) — чистые функции ядра, их тесты рядом с
//! `core::scene::letterbox` (`src/core/scene.rs`).

use engine::core::property;
use engine::data::load::load_game_from_texts_with_code;

const GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":7,"start_screen":"menu","max_objects":50,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
"screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
const SCREENS: &str = r#"{"screens":[
    {"name":"menu","world_runs":false,"keys":{"KeyP":["show_screen","game"]},"elements":[]},
    {"name":"game","world_runs":true,"elements":[]}
]}"#;
const RULES: &str = r#"{"rules":[]}"#;
const CODE: &str = r#"print("верхний уровень")"#;

/// Стартовый экран без `world_runs` — конструктор игры мир не строит и код не грузит, так что
/// любой след одного или другого после `load` доказуемо появляется только от `show_scene`.
#[test]
fn show_scene_numbers_objects_by_file_position_with_values_no_step_and_no_code() {
    let props = r#"{"properties":{"n":"number"}}"#;
    let scene = r#"{"objects":[
        {"position":[0,0],"size":[1,1],"name":"a","n":0},
        {"position":[1,0],"size":[1,1],"name":"b","n":1},
        {"position":[2,0],"size":[1,1],"name":"c","n":2}
    ]}"#;
    let (mut game, _screens, warnings) =
        load_game_from_texts_with_code(GAME, props, scene, RULES, SCREENS, Some(CODE))
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");
    assert_eq!(
        game.world.slot_count(),
        0,
        "экран меню не живой — мира ещё нет"
    );
    assert!(game.messages().is_empty());

    game.show_scene();

    let n = game.properties.resolve("n").unwrap();
    for (id, name) in [(0u32, "a"), (1, "b"), (2, "c")] {
        assert_eq!(
            game.world.text(id, property::NAME),
            Some(name),
            "объект {id} — номер по месту в scene.json"
        );
        assert_eq!(game.world.number_like(id, n), Some(id as f64));
    }
    assert_eq!(game.step_count(), 0);
    assert!(
        game.messages().is_empty(),
        "show_scene не выполняет файл кода: {:?}",
        game.messages()
    );
    assert!(game.code_error().is_none());
}

/// Повторный вызов заменяет мир целиком, а не добавляет к прежнему.
#[test]
fn show_scene_replaces_the_world_each_time_it_is_called() {
    let props = r#"{"properties":{}}"#;
    let scene = r#"{"objects":[{"position":[0,0],"size":[1,1]}]}"#;
    let (mut game, _screens, warnings) =
        load_game_from_texts_with_code(GAME, props, scene, RULES, SCREENS, Some(CODE))
            .expect("должно загрузиться");
    assert_eq!(warnings, Vec::new(), "{warnings:?}");

    game.show_scene();
    assert_eq!(game.world.slot_count(), 1);
    game.show_scene();
    assert_eq!(
        game.world.slot_count(),
        1,
        "второй вызов не копит объекты поверх первого"
    );
}

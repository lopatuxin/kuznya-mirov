//! «Тесты по записанному вводу», требования 30–33: runs every `tests/replays/<игра>-<случай>.json`
//! file against the real game folder it names (`games/<игра>`), through the same
//! `screens::engine_call` pipeline the wasm layer drives, and checks the recorded expectations.
//! Each file is its own `#[test]`-like check inside one test function, with its own clearly
//! labeled failure — `cargo test` doesn't support generating one `#[test]` per data file without a
//! build script, so this single test iterates the directory and panics with the failing file's own
//! name, step and expectation on the first mismatch, matching требование 32's "тест называет файл,
//! шаг, что ожидалось и что вышло".

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::property::PropertyId;
use engine::core::runner::{Runner, STEP_SECONDS};
use engine::core::screens::{self, ScreenState, ScreensConfig};
use engine::core::value::PropKind;
use engine::data::load::{self, parse_initial_values};
use serde_json::Value as Json;

fn replays_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("tests");
    path.push("replays");
    path
}

fn games_dir() -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path
}

fn read_optional(path: &Path) -> Option<String> {
    fs::read_to_string(path).ok()
}

/// Loads `games/<game>` end to end, JSON and Lua code read for real, no image/font/sound bytes at
/// all — «Тесты по записанному вводу», требование 33.
fn load_game(game: &str) -> (Game, ScreensConfig) {
    let dir = games_dir().join(game);
    let game_json = fs::read_to_string(dir.join("game.json"))
        .unwrap_or_else(|e| panic!("{game}: не смог прочитать game.json: {e}"));
    let (config, _entry_warnings) = load::read_entry(&game_json)
        .unwrap_or_else(|e| panic!("{game}: game.json не разобрался: {e:?}"));
    let properties_json = read_optional(&dir.join(&config.files.properties));
    let scene_json = read_optional(&dir.join(&config.files.scene));
    let rules_json = read_optional(&dir.join(&config.files.rules));
    let screens_json = read_optional(&dir.join(&config.files.screens));
    let code_json = config
        .files
        .code
        .as_ref()
        .and_then(|p| read_optional(&dir.join(p)));
    let (game_obj, screens_config, _warnings, _images) = load::load_rest(
        &game_json,
        config,
        properties_json.as_deref(),
        scene_json.as_deref(),
        rules_json.as_deref(),
        screens_json.as_deref(),
        &[],
        &[],
        &[],
        &[],
        code_json.as_deref(),
        true,
    )
    .unwrap_or_else(|e| panic!("{game}: не прошла предстартовая проверка: {e:?}"));
    (game_obj, screens_config)
}

#[derive(Debug)]
enum InputEvent {
    Press(String),
    Release(String),
    Cursor([f64; 2]),
}

struct ReplayCheck {
    step: u64,
    kind: CheckKind,
}

enum CheckKind {
    Property {
        object: String,
        property: String,
        equals: Json,
    },
    Count {
        has: Vec<String>,
        without: Vec<String>,
        equals: i64,
    },
    Screen {
        name: String,
    },
}

struct ReplaySpec {
    start: String,
    values: Json,
    steps: Option<u64>,
    input: Vec<(u64, InputEvent)>,
    checks: Vec<ReplayCheck>,
}

fn parse_replay(text: &str, file: &str) -> ReplaySpec {
    let root: Json = serde_json::from_str(text)
        .unwrap_or_else(|e| panic!("{file}: не разобрался как JSON: {e}"));
    let start = root["start"]
        .as_str()
        .unwrap_or_else(|| panic!("{file}: отсутствует строка \"start\""))
        .to_string();
    let values = root.get("values").cloned().unwrap_or(Json::Null);
    let steps = root.get("steps").and_then(Json::as_u64);

    let mut input = Vec::new();
    for entry in root["input"].as_array().cloned().unwrap_or_default() {
        let step = entry["step"]
            .as_u64()
            .unwrap_or_else(|| panic!("{file}: элемент input без \"step\""));
        if let Some(code) = entry.get("press").and_then(Json::as_str) {
            input.push((step, InputEvent::Press(code.to_string())));
        } else if let Some(code) = entry.get("release").and_then(Json::as_str) {
            input.push((step, InputEvent::Release(code.to_string())));
        } else if let Some(cursor) = entry.get("cursor").and_then(Json::as_array) {
            let x = cursor[0].as_f64().expect("cursor[0] число");
            let y = cursor[1].as_f64().expect("cursor[1] число");
            input.push((step, InputEvent::Cursor([x, y])));
        } else {
            panic!("{file}: элемент input на шаге {step} без press/release/cursor");
        }
    }

    let mut checks = Vec::new();
    for entry in root["checks"].as_array().cloned().unwrap_or_default() {
        let step = entry["step"]
            .as_u64()
            .unwrap_or_else(|| panic!("{file}: проверка без \"step\""));
        let kind = if let Some(object) = entry.get("object").and_then(Json::as_str) {
            CheckKind::Property {
                object: object.to_string(),
                property: entry["property"]
                    .as_str()
                    .unwrap_or_else(|| panic!("{file}: проверка объекта без \"property\""))
                    .to_string(),
                equals: entry["equals"].clone(),
            }
        } else if let Some(count) = entry.get("count") {
            let strings = |key: &str| -> Vec<String> {
                count[key]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|v| v.as_str().unwrap_or_default().to_string())
                            .collect()
                    })
                    .unwrap_or_default()
            };
            CheckKind::Count {
                has: strings("has"),
                without: strings("without"),
                equals: entry["equals"]
                    .as_i64()
                    .unwrap_or_else(|| panic!("{file}: проверка count без целого \"equals\"")),
            }
        } else if let Some(screen) = entry.get("screen").and_then(Json::as_str) {
            CheckKind::Screen {
                name: screen.to_string(),
            }
        } else {
            panic!("{file}: проверка на шаге {step} не object/count/screen");
        };
        checks.push(ReplayCheck { step, kind });
    }

    ReplaySpec {
        start,
        values,
        steps,
        input,
        checks,
    }
}

fn find_named_object(game: &Game, name: &str) -> Option<u32> {
    game.world
        .ids()
        .find(|&id| game.world.text(id, engine::core::property::NAME) == Some(name))
}

/// Reads `prop`'s current value off `id` in the same "как в scene.json" form a check's `equals`
/// is written in, formatted as JSON for a uniform comparison — «Тесты по записанному вводу»,
/// требование 32: numbers within 1e-6, time/timer in seconds to the step.
fn read_property_as_json(game: &Game, id: u32, prop: PropertyId) -> Option<Json> {
    match game.properties.kind(prop) {
        PropKind::Flag => Some(Json::Bool(game.world.flag(id, prop))),
        PropKind::Number => game.world.number_like(id, prop).map(json_number),
        PropKind::Time => game
            .world
            .time(id, prop)
            .map(|steps| json_number(steps as f64 / 60.0)),
        PropKind::Timer => game
            .world
            .timer(id, prop)
            .map(|steps| json_number(steps as f64 / 60.0)),
        PropKind::Rotation => game
            .world
            .rotation(id, prop)
            .map(|r| json_number(r.degrees() as f64)),
        PropKind::Layer => game.world.layer(id, prop).map(|l| json_number(l as f64)),
        PropKind::Text => game
            .world
            .text(id, prop)
            .map(|s| Json::String(s.to_string())),
        PropKind::Vec2 => game
            .world
            .vec2(id, prop)
            .map(|v| Json::Array(vec![json_number(v[0]), json_number(v[1])])),
        PropKind::FollowMouse => game
            .world
            .follow_mouse(id, prop)
            .map(|a| Json::String(a.as_str().to_string())),
        PropKind::Color | PropKind::Image | PropKind::Grid | PropKind::Keys => None,
    }
}

fn json_number(n: f64) -> Json {
    serde_json::Number::from_f64(n)
        .map(Json::Number)
        .unwrap_or(Json::Null)
}

fn values_match(actual: &Json, expected: &Json) -> bool {
    match (actual, expected) {
        (Json::Number(a), Json::Number(b)) => {
            (a.as_f64().unwrap() - b.as_f64().unwrap()).abs() < 1e-6
        }
        (Json::Array(a), Json::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| values_match(x, y))
        }
        _ => actual == expected,
    }
}

fn run_check(
    file: &str,
    game: &Game,
    screens_config: &ScreensConfig,
    state: &ScreenState,
    check: &ReplayCheck,
) {
    match &check.kind {
        CheckKind::Property {
            object,
            property,
            equals,
        } => {
            let id = find_named_object(game, object).unwrap_or_else(|| {
                panic!(
                    "{file}: шаг {}: объекта \"{object}\" нет в мире",
                    check.step
                )
            });
            let prop = game.properties.resolve(property).unwrap_or_else(|| {
                panic!(
                    "{file}: шаг {}: свойства \"{property}\" не существует",
                    check.step
                )
            });
            let actual = read_property_as_json(game, id, prop);
            let matches = actual.as_ref().is_some_and(|a| values_match(a, equals));
            if !matches {
                panic!(
                    "{file}: шаг {}: {object}.{property} — ожидалось {equals}, получено {actual:?}",
                    check.step
                );
            }
        }
        CheckKind::Count {
            has,
            without,
            equals,
        } => {
            let has_ids: Vec<PropertyId> = has
                .iter()
                .map(|n| {
                    game.properties
                        .resolve(n)
                        .unwrap_or_else(|| panic!("{file}: неизвестное свойство \"{n}\" в count"))
                })
                .collect();
            let without_ids: Vec<PropertyId> = without
                .iter()
                .map(|n| {
                    game.properties
                        .resolve(n)
                        .unwrap_or_else(|| panic!("{file}: неизвестное свойство \"{n}\" в count"))
                })
                .collect();
            let count = game
                .world
                .ids()
                .filter(|&id| has_ids.iter().all(|&p| game.world.has(id, p)))
                .filter(|&id| without_ids.iter().all(|&p| !game.world.has(id, p)))
                .count() as i64;
            if count != *equals {
                panic!(
                    "{file}: шаг {}: count has={has:?} without={without:?} — ожидалось {equals}, получено {count}",
                    check.step
                );
            }
        }
        CheckKind::Screen { name } => {
            let active = &screens_config.screens[state.active()].name;
            if active != name {
                panic!(
                    "{file}: шаг {}: экран — ожидался \"{name}\", получен \"{active}\"",
                    check.step
                );
            }
        }
    }
}

fn run_replay(path: &Path) {
    let file = path.file_name().unwrap().to_string_lossy().to_string();
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("{file}: не смог прочитать: {e}"));
    let spec = parse_replay(&text, &file);

    let game_name = file
        .split_once('-')
        .unwrap_or_else(|| panic!("{file}: имя файла должно быть \"<игра>-<случай>.json\""))
        .0
        .to_string();
    let (mut game, screens_config) = load_game(&game_name);

    let start_id = screens_config
        .screens
        .iter()
        .position(|s| s.name == spec.start)
        .unwrap_or_else(|| panic!("{file}: нет экрана \"{}\"", spec.start));
    if !screens_config.screens[start_id].world_runs {
        panic!("{file}: экран \"{}\" не помечен world_runs", spec.start);
    }

    let initial_values = if spec.values.is_null() {
        Vec::new()
    } else {
        parse_initial_values(&spec.values, &game.properties)
            .unwrap_or_else(|e| panic!("{file}: values: {e}"))
    };
    game.new_game_with_values(&initial_values);
    let mut state = ScreenState::new(start_id);

    let last_check_step = spec.checks.iter().map(|c| c.step).max().unwrap_or(0);
    let total_steps = spec.steps.unwrap_or(last_check_step);

    let mut input_by_step: HashMap<u64, Vec<&InputEvent>> = HashMap::new();
    for (step, event) in &spec.input {
        input_by_step.entry(*step).or_default().push(event);
    }
    let mut checks_by_step: HashMap<u64, Vec<&ReplayCheck>> = HashMap::new();
    for check in &spec.checks {
        checks_by_step.entry(check.step).or_default().push(check);
    }

    let mut ui_queue = UiQueue::new();
    let mut mouse = MouseState::default();
    let mut runner = Runner::new();
    let viewport = [800.0, 600.0];

    for step_n in 1..=total_steps {
        if game.is_running() {
            if let Some(events) = input_by_step.get(&step_n) {
                for event in events {
                    match event {
                        InputEvent::Press(code) => ui_queue.push_key_down(code),
                        InputEvent::Release(code) => ui_queue.push_key_up(code),
                        InputEvent::Cursor(cell) => game.set_cursor_cell(*cell),
                    }
                }
            }
            screens::engine_call(
                &mut ui_queue,
                &mut mouse,
                &mut runner,
                &mut game,
                &screens_config,
                &mut state,
                viewport,
                STEP_SECONDS,
            );
        }
        if let Some(checks) = checks_by_step.get(&step_n) {
            for check in checks {
                run_check(&file, &game, &screens_config, &state, check);
            }
        }
    }

    // Checks scheduled past the last step actually run (the game already ended) still look at
    // the frozen world and outcome screen — «Тесты по записанному вводу», требование 31.
    for check in &spec.checks {
        if check.step > total_steps {
            run_check(&file, &game, &screens_config, &state, check);
        }
    }
}

/// «Тесты по записанному вводу», требование 30: «каждый файл — отдельная проверка с понятным
/// именем в выводе» — `cargo test` has no way to register one `#[test]` per data file found at
/// run time, so this one test runs every file and catches each one's own panic instead of
/// stopping at the first, so a broken fixture never hides a later file's own result; the final
/// failure message lists every failing file by name, each with the failing check's own file/step/
/// expected/actual text already embedded (требование 32).
#[test]
fn replays_match_their_recorded_expectations() {
    let dir = replays_dir();
    let mut files: Vec<PathBuf> = fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("не смог прочитать {dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "в {dir:?} нет ни одного файла записанного ввода"
    );

    let mut failures = Vec::new();
    for path in &files {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let path = path.clone();
        let result = std::panic::catch_unwind(move || run_replay(&path));
        if let Err(payload) = result {
            let message = payload
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "неизвестная паника".to_string());
            failures.push(format!("{name}: {message}"));
        }
    }
    if !failures.is_empty() {
        panic!(
            "{} из {} файлов записанного ввода не сошлись:\n{}",
            failures.len(),
            files.len(),
            failures.join("\n")
        );
    }
}

//! «Редактор» → «Партия в редакторе», «Запись и повтор»: `data::session::PlaySession` driven the
//! way `wasm::mod` drives it — through `core::screens`' free functions, on a small hand-built game
//! whose collision, spawn and lifetime timing are exact by construction, so the report and replay
//! assertions below don't depend on any real demo game's own physics.

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::report::DeleteCause;
use engine::core::rules::Outcome;
use engine::core::runner::Runner;
use engine::core::screens::{ScreenState, ScreensConfig};
use engine::data::load::{load_rest, read_entry};
use engine::data::session::PlaySession;

const VIEWPORT: [f32; 2] = [800.0, 600.0];

fn game_json(start_screen: &str) -> String {
    format!(
        r##"{{"name":"T","scene":{{"width":20,"height":20,"background":"#000000"}},
"random_seed":1,"start_screen":"{start_screen}","win_screen":"win","max_objects":10,
"files":{{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{{}}}}}}"##
    )
}

const PROPS: &str = r#"{"properties":{"score":"number","mover":"flag","brick":"flag","spawned":"flag","marked":"flag"}}"#;

// mover moves exactly one cell right per step (velocity 60 = 1 cell / 1/60s step) and starts
// touching brick's own cell after step 1 — the collision, and its report, land on a known step.
// temp's lifetime (0.05s = 3 steps) expires on step 3.
const SCENE: &str = r##"{"objects":[
    {"name":"mover","position":[0,0],"size":[1,1],"velocity":[60,0],"collides":true,
     "mover":true,"score":0,
     "keys":{"Space":{"press":[["marked",true]],"release":[["marked",false]]}}},
    {"name":"brick","position":[1,0],"size":[1,1],"collides":true,"brick":true},
    {"name":"temp","position":[10,10],"size":[1,1],"lifetime":0.05}
]}"##;

const RULES: &str = r##"{"rules":[
    {"kind":"move","for":{"has":["position","velocity"]}},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["brick"]},
     "effects":{"a":[["bounce"]],"b":[["delete"]]},
     "do":[["add","score",10]]},
    {"kind":"spawn","when":{"fewer_than":{"count":1,"of":{"has":["spawned"]}}},
     "where":{"at":[8,8]},"template":{"spawned":true}},
    {"kind":"delete","for":{"has":["mover"]},"when":["score",">=",100],
     "do":[["end_game","win"]]}
]}"##;

const SCREENS_JSON: &str = r##"{"screens":[
    {"name":"menu","world_runs":false,"elements":[]},
    {"name":"game","world_runs":true,"elements":[]},
    {"name":"win","world_runs":false,"elements":[]}
]}"##;

/// «Редактор», требование 2: two LIVE screens, mirroring `tests/screens.rs`'s own
/// `MIRROR_TWO_LIVE_SCREENS` — `game` doesn't name `Space` at all (a press there reaches the
/// world's queue), `game2` absorbs it on release via `Enter`, itself absorbed by `game` on press.
const TWO_LIVE_SCREENS_JSON: &str = r##"{"screens":[
    {"name":"menu","world_runs":false,"elements":[]},
    {"name":"game","world_runs":true,"keys":{"Enter":["show_screen","game2"]},"elements":[]},
    {"name":"game2","world_runs":true,"keys":{"Space":["show_screen","game"]},"elements":[]},
    {"name":"win","world_runs":false,"elements":[]}
]}"##;

const MOVER: u32 = 0;
const BRICK: u32 = 1;
const TEMP: u32 = 2;

fn load_with_screens(start_screen: &str, screens_json: &str) -> (Game, ScreensConfig) {
    let game_json = game_json(start_screen);
    let (config, _entry_warnings) = read_entry(&game_json).expect("game.json должен разобраться");
    let (game, screens, _warnings, _images) = load_rest(
        &game_json,
        config,
        Some(PROPS),
        Some(SCENE),
        Some(RULES),
        Some(screens_json),
        &[],
        &[],
        &[],
        &[],
        None,
        false,
    )
    .expect("должно загрузиться");
    (game, screens)
}

fn load(start_screen: &str) -> (Game, ScreensConfig) {
    load_with_screens(start_screen, SCREENS_JSON)
}

fn load_two_live(start_screen: &str) -> (Game, ScreensConfig) {
    load_with_screens(start_screen, TWO_LIVE_SCREENS_JSON)
}

fn step_once(
    session: &mut PlaySession,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.step_once(&mut queue, &mut mouse, game, config, state, VIEWPORT, &[]);
}

#[test]
fn play_on_a_non_live_start_screen_leaves_the_world_empty_and_step_does_nothing() {
    let (mut game, config) = load("menu");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    assert_eq!(
        game.world.alive_count(),
        0,
        "экран без world_runs — мира нет"
    );
    assert_eq!(game.session_step_count(), 0);

    step_once(&mut session, &mut game, &config, &mut state);
    assert_eq!(
        game.session_step_count(),
        0,
        "шаг ничего не делает без world_runs"
    );
    assert!(game.last_report().is_none());
}

#[test]
fn play_on_a_live_start_screen_assembles_the_world_like_a_fresh_load() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let _session = PlaySession::begin_live(&mut game, &config, &mut state);

    assert_eq!(game.world.alive_count(), 3);
    assert_eq!(game.session_step_count(), 0);
    assert!(
        game.last_report().is_none(),
        "до первого шага отчёта ещё нет"
    );
}

#[test]
fn without_play_the_plain_page_path_builds_no_report_or_session_step() {
    let (mut game, _config) = load("game");
    assert!(!game.session_active());
    game.new_game();
    game.step(engine::core::input::StepInput::empty());
    assert_eq!(game.session_step_count(), 0);
    assert!(game.last_report().is_none());
}

#[test]
fn step_does_exactly_one_step_and_the_collision_reports_the_pair_and_the_deleted_brick() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    step_once(&mut session, &mut game, &config, &mut state);
    assert_eq!(game.session_step_count(), 1, "ровно один шаг");

    let report = game.last_report().expect("после шага отчёт есть");
    assert_eq!(report.step, 1);

    use engine::core::report::RuleFired;
    let collide = report
        .fired
        .iter()
        .find_map(|f| match f {
            RuleFired::Collide { rule, pairs } => Some((rule.clone(), pairs.clone())),
            _ => None,
        })
        .expect("collide должен был сработать на шаге 1");
    assert_eq!(collide.0, "rules[1]");
    assert_eq!(collide.1, vec![(MOVER, BRICK)]);

    assert_eq!(report.deleted.len(), 1);
    assert_eq!(report.deleted[0].id, BRICK);
    assert_eq!(
        report.deleted[0].cause,
        DeleteCause::Rule("rules[1]".to_string())
    );

    let score = game.properties.resolve("score").unwrap();
    assert_eq!(game.world.number_like(MOVER, score), Some(10.0));
}

#[test]
fn spawn_fires_once_and_reports_the_created_object_with_its_rule() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    step_once(&mut session, &mut game, &config, &mut state);
    use engine::core::report::RuleFired;
    let report = game.last_report().unwrap();
    let spawn = report
        .fired
        .iter()
        .find_map(|f| match f {
            RuleFired::Spawn { rule, objects } => Some((rule.clone(), objects.clone())),
            _ => None,
        })
        .expect("spawn должен был сработать на шаге 1");
    assert_eq!(spawn.0, "rules[2]");
    assert_eq!(spawn.1.len(), 1);
    let spawned_id = spawn.1[0];
    assert_eq!(report.created.len(), 1);
    assert_eq!(report.created[0].id, spawned_id);
    assert_eq!(report.created[0].rule, "rules[2]");
    // brick was deleted and the spawn created one object in this very step, so the count is
    // unchanged from the initial three (mover, temp, spawned) — «Номера объектов»: the freed slot
    // is reused, not appended.
    assert_eq!(game.world.alive_count(), 3);

    step_once(&mut session, &mut game, &config, &mut state);
    let report2 = game.last_report().unwrap();
    assert!(
        report2
            .fired
            .iter()
            .all(|f| !matches!(f, RuleFired::Spawn { .. })),
        "второй раз spawn не должен сработать — fewer_than уже не меньше 1"
    );
}

#[test]
fn expired_lifetime_is_reported_as_such_not_as_a_rule() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    for _ in 0..3 {
        step_once(&mut session, &mut game, &config, &mut state);
    }
    assert!(
        !game.world.is_alive(TEMP),
        "срок жизни истёк на третьем шаге"
    );
    let report = game.last_report().unwrap();
    let temp_deletion = report
        .deleted
        .iter()
        .find(|d| d.id == TEMP)
        .expect("temp должен быть в списке удалённых");
    assert_eq!(temp_deletion.cause, DeleteCause::LifetimeExpired);
}

#[test]
fn outcome_and_screen_change_land_in_the_same_step_report() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    let score = game.properties.resolve("score").unwrap();
    session
        .set_property(&mut game, &[], MOVER, "score", &serde_json::json!(100.0))
        .unwrap();
    assert_eq!(game.world.number_like(MOVER, score), Some(100.0));

    step_once(&mut session, &mut game, &config, &mut state);
    let report = game.last_report().unwrap();
    assert_eq!(report.outcome, Some(Outcome::Win));
    assert_eq!(
        report.screen_change,
        Some(("game".to_string(), "win".to_string()))
    );
    assert_eq!(config.screens[state.active()].name, "win");
}

#[test]
fn set_property_rejects_the_wrong_shape_and_leaves_the_world_unchanged() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    let score = game.properties.resolve("score").unwrap();
    let err = session
        .set_property(
            &mut game,
            &[],
            MOVER,
            "score",
            &serde_json::json!("не число"),
        )
        .unwrap_err();
    assert!(!err.is_empty());
    assert_eq!(game.world.number_like(MOVER, score), Some(0.0));
}

#[test]
fn add_object_takes_the_first_free_number_and_max_objects_is_russian() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    // max_objects is 10; the world already holds 3 (mover, brick, temp).
    for _ in 0..7 {
        session
            .add_object(&mut game, &[], &serde_json::json!({}))
            .unwrap();
    }
    let err = session
        .add_object(&mut game, &[], &serde_json::json!({}))
        .unwrap_err();
    assert_eq!(err, "Достигнут предел объектов");
}

#[test]
fn edits_and_deletes_are_not_available_during_replay() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_replay(
        r#"{"format": 1, "steps": 0, "events": []}"#,
        &mut game,
        &config,
        &mut state,
    )
    .unwrap();
    let err = session
        .set_property(&mut game, &[], MOVER, "score", &serde_json::json!(1))
        .unwrap_err();
    assert!(err.contains("партии"), "{err}");
}

#[test]
fn stop_keeps_the_recording_readable_but_freezes_further_stepping() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);

    session.end(&mut game);
    game.show_scene();
    assert_eq!(
        game.session_step_count(),
        2,
        "запись помнит, сколько шагов прошло"
    );
    let text = session.recording_text(&game);
    assert!(text.contains("\"steps\": 2"), "{text}");

    let alive_before = game.world.alive_count();
    step_once(&mut session, &mut game, &config, &mut state);
    assert_eq!(
        game.world.alive_count(),
        alive_before,
        "после «Стопа» «Шаг» не должен снова шагать статичный мир"
    );
    assert!(
        session
            .set_property(&mut game, &[], MOVER, "score", &serde_json::json!(1))
            .is_err(),
        "правка недоступна после «Стопа»"
    );
}

/// «Редактор», требования 31, 47: `reset_for_play` (used by `play()`, `replay()` and `seek()`)
/// must forget the previous partiya's cursor position, not just when it was last delivered to a
/// step — otherwise the first step of a new partiya/replay/seek sees a cursor the recording never
/// had.
#[test]
fn reset_for_play_forgets_the_previous_partiyas_cursor() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    game.set_cursor_cell([5.0, 5.0]);
    session.end(&mut game);

    let _session2 = PlaySession::begin_live(&mut game, &config, &mut state);
    let input = game.take_input_snapshot();
    assert_eq!(
        input.cursor, None,
        "первый шаг новой партии не должен видеть курсор, оставшийся от прошлой"
    );
}

#[test]
fn opening_something_that_is_not_a_recording_is_reported_with_the_required_prefix() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let err =
        PlaySession::begin_replay("не json вовсе", &mut game, &config, &mut state).unwrap_err();
    assert!(err.starts_with("Это не запись партии: "), "{err}");
}

/// End-to-end: a live session with a world key, a live edit, a collision and a spawn, saved and
/// replayed — «Исполнение игры» → «Повторяемость»: same steps, same object properties.
#[test]
fn a_recorded_partiya_replays_step_for_step() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    let marked = game.properties.resolve("marked").unwrap();
    let score = game.properties.resolve("score").unwrap();

    let mut snapshots: Vec<(bool, f64, bool, usize)> = Vec::new();
    let record_snapshot = |game: &Game| {
        (
            game.world.flag(MOVER, marked),
            game.world.number_like(MOVER, score).unwrap_or_default(),
            game.world.is_alive(BRICK),
            game.world.alive_count(),
        )
    };

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();

    // Step 1: mover collides with brick.
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    snapshots.push(record_snapshot(&game));

    // Before step 2: press Space (world key) and nudge score with a live edit.
    queue.push_key_down("Space");
    session
        .set_property(&mut game, &[], MOVER, "score", &serde_json::json!(50.0))
        .unwrap();
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    snapshots.push(record_snapshot(&game));

    // Before step 3: release Space; temp's lifetime also expires on this step.
    queue.push_key_up("Space");
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    snapshots.push(record_snapshot(&game));

    let text = session.recording_text(&game);
    assert!(text.contains("\"format\": 1"));

    // Fresh game, fresh session, replaying the saved text.
    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    assert!(replay.is_replay());
    assert_eq!(game2.session_step_count(), 0);

    for expected in &snapshots {
        let mut q = UiQueue::new();
        let mut m = MouseState::default();
        replay.step_once(
            &mut q,
            &mut m,
            &mut game2,
            &config2,
            &mut state2,
            VIEWPORT,
            &[],
        );
        assert_eq!(
            &record_snapshot(&game2),
            expected,
            "шаг {}",
            game2.session_step_count()
        );
    }

    // «Повтор с другим размером холста — то же»: seeking with a different viewport still lands on
    // the same recorded state, since replay never reads pixels.
    let mut q = UiQueue::new();
    let mut m = MouseState::default();
    replay.seek(
        2,
        &mut q,
        &mut m,
        &mut game2,
        &config2,
        &mut state2,
        [320.0, 240.0],
        &[],
    );
    assert_eq!(record_snapshot(&game2), snapshots[1]);
    assert_eq!(game2.session_step_count(), 2);

    // «Шаг назад».
    replay.step_back(
        &mut q,
        &mut m,
        &mut game2,
        &config2,
        &mut state2,
        VIEWPORT,
        &[],
    );
    assert_eq!(record_snapshot(&game2), snapshots[0]);
    assert_eq!(game2.session_step_count(), 1);
}

/// «Пауза», требование 40: `pause()` releases a held key straight into the world (`Engine::pause`
/// doesn't wait for a step), and that release has to reach the recording so a replay lets go of the
/// key at the same point rather than holding it forever.
#[test]
fn a_key_release_from_pause_is_recorded_and_replayed_at_the_same_step() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let marked = game.properties.resolve("marked").unwrap();

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    queue.push_key_down("Space");
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert!(
        game.world.flag(MOVER, marked),
        "Space держится после первого шага"
    );

    // «Пауза», как `Engine::pause`: отпускает зажатые клавиши прямо в мир и кладёт отпускание в
    // запись.
    let released = game.release_held_keys();
    assert_eq!(released, vec!["Space".to_string()]);
    session.record_key_releases(&game, &released);
    assert!(
        !game.world.flag(MOVER, marked),
        "пауза отпускает клавишу сразу, без ожидания следующего шага"
    );

    step_once(&mut session, &mut game, &config, &mut state);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let marked2 = game2.properties.resolve("marked").unwrap();
    step_once(&mut replay, &mut game2, &config2, &mut state2);
    step_once(&mut replay, &mut game2, &config2, &mut state2);

    assert!(
        !game2.world.flag(MOVER, marked2),
        "повтор должен отпустить клавишу, зажатую на паузе, а не держать её вечно"
    );
}

/// «Правка на ходу» + «Пауза», требование 43: an edit made right after a pause's own key release,
/// at the same recorded step, has to win in replay exactly as it did live — the release must not
/// re-apply after the edit just because a replay queues keys separately from edits.
#[test]
fn an_edit_made_right_after_a_pause_release_outlives_it_in_replay() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let marked = game.properties.resolve("marked").unwrap();

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    queue.push_key_down("Space");
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );

    let released = game.release_held_keys();
    session.record_key_releases(&game, &released);
    session
        .set_property(&mut game, &[], MOVER, "marked", &serde_json::json!(true))
        .unwrap();
    assert!(
        game.world.flag(MOVER, marked),
        "правка после отпускания должна пересилить его вживую"
    );

    step_once(&mut session, &mut game, &config, &mut state);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let marked2 = game2.properties.resolve("marked").unwrap();
    step_once(&mut replay, &mut game2, &config2, &mut state2);
    step_once(&mut replay, &mut game2, &config2, &mut state2);

    assert!(
        game2.world.flag(MOVER, marked2),
        "в повторе правка должна применяться после отпускания того же шага, а не наоборот"
    );
}

/// «Исполнение игры» → «Запись партии в редакторе», требование 32: an edit naming an object or a
/// property the current files no longer have is skipped, not a panic or a hard error.
#[test]
fn replay_skips_an_edit_that_no_longer_applies() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let text = r#"{"format": 1, "steps": 1, "events": [
        {"step": 0, "set": [999, "score", 5]},
        {"step": 0, "set": [0, "not_a_real_property", 5]}
    ]}"#;
    let mut session = PlaySession::begin_replay(text, &mut game, &config, &mut state).unwrap();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    // No panic, and the world is otherwise exactly what a plain step would have produced.
    let score = game.properties.resolve("score").unwrap();
    assert_eq!(game.world.number_like(MOVER, score), Some(10.0));
}

/// «Редактор», требование 1: a live session applies a world key immediately (`apply_now: true`
/// in `tick_live`), like a replay already applies its own recorded keys — not through the queue
/// that only the next `Game::step`'s own stage 1 drains. A frame that ends up doing zero world
/// steps (dt too small — a 120/144Hz monitor) followed by `pause()` must still see, and release,
/// the press it just made: before this fix the press sat queued, `release_held_keys` found the
/// world holding nothing, and the key stayed stuck down for the rest of the partiya.
#[test]
fn a_key_pressed_on_a_zero_step_frame_is_applied_immediately_so_pause_can_release_it() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let marked = game.properties.resolve("marked").unwrap();
    let mut runner = Runner::new();

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    queue.push_key_down("Space");
    session.tick_live(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        0.0,
        &[],
    );
    assert_eq!(
        game.session_step_count(),
        0,
        "dt=0 не должен был сделать ни одного шага"
    );
    assert!(
        game.world.flag(MOVER, marked),
        "нажатие должно применяться сразу, а не ждать шага"
    );

    let released = game.release_held_keys();
    assert_eq!(released, vec!["Space".to_string()]);
    session.record_key_releases(&game, &released);
    assert!(
        !game.world.flag(MOVER, marked),
        "пауза должна была отпустить клавишу сразу"
    );

    step_once(&mut session, &mut game, &config, &mut state);
    assert!(!game.world.flag(MOVER, marked));

    let text = session.recording_text(&game);

    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let marked2 = game2.properties.resolve("marked").unwrap();
    step_once(&mut replay, &mut game2, &config2, &mut state2);

    assert!(
        !game2.world.flag(MOVER, marked2),
        "повтор должен дать тот же мир: клавиша отпущена, а не зажата навсегда"
    );
}

/// «Редактор», требование 2: a release absorbed by the screen that's active *now* must still let
/// go of a key the world still holds from a press made on a *different* live screen — recording a
/// `WorldKeyUp` ahead of the `Command`, so a replay releases the key too, not just runs the
/// command.
#[test]
fn an_absorbed_release_records_the_worlds_own_key_up_before_the_command() {
    let (mut game, config) = load_two_live("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let marked = game.properties.resolve("marked").unwrap();
    let game2_id = config
        .screens
        .iter()
        .position(|s| s.name == "game2")
        .unwrap();

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();

    // Press Space on `game`, which doesn't name it — reaches the world.
    queue.push_key_down("Space");
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert!(
        game.world.flag(MOVER, marked),
        "Space держится после шага на game"
    );

    // Enter is absorbed by `game` and switches to `game2` — recorded as a plain `Command`, no
    // `WorldKeyUp`, since the world never held Enter.
    queue.push_key_down("Enter");
    queue.push_key_up("Enter");
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert_eq!(
        state.active(),
        game2_id,
        "Enter должен был переключить на game2"
    );

    // Now release Space on `game2`, which absorbs it — the world still holds it.
    queue.push_key_up("Space");
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert!(
        !game.world.flag(MOVER, marked),
        "поглощённое отпускание должно снять клавишу с мира сразу"
    );

    let text = session.recording_text(&game);

    let (mut game2, config2) = load_two_live("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let marked2 = game2.properties.resolve("marked").unwrap();
    step_once(&mut replay, &mut game2, &config2, &mut state2);
    step_once(&mut replay, &mut game2, &config2, &mut state2);
    step_once(&mut replay, &mut game2, &config2, &mut state2);

    assert!(
        !game2.world.flag(MOVER, marked2),
        "повтор должен отпустить клавишу, которую мир держал, а не только выполнить команду"
    );
}

/// «Редактор», требование 4: «Шаг» must not run a replay past the recording's own end.
#[test]
fn step_once_does_not_advance_past_the_end_of_a_replay() {
    let text = r#"{"format": 1, "steps": 1, "events": []}"#;
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut replay = PlaySession::begin_replay(text, &mut game, &config, &mut state).unwrap();

    step_once(&mut replay, &mut game, &config, &mut state);
    assert_eq!(game.session_step_count(), 1);

    let alive_before = game.world.alive_count();
    step_once(&mut replay, &mut game, &config, &mut state);
    assert_eq!(
        game.session_step_count(),
        1,
        "«Шаг» не должен идти дальше записи"
    );
    assert_eq!(game.world.alive_count(), alive_before);
}

/// «Редактор», требование 4: `seek` must not go past the recording's own length.
#[test]
fn seek_clamps_to_the_recordings_own_length() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    session.end(&mut game);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    replay.seek(
        1000,
        &mut queue,
        &mut mouse,
        &mut game2,
        &config2,
        &mut state2,
        VIEWPORT,
        &[],
    );
    assert_eq!(
        game2.session_step_count(),
        2,
        "seek не должен уходить за длину записи"
    );
}

/// «Редактор», требование 4: `seek`/`step_back` do nothing during a live partiya, only in a replay.
#[test]
fn seek_and_step_back_do_nothing_during_a_live_session() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.seek(
        0,
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert_eq!(
        game.session_step_count(),
        1,
        "seek не должен ничего делать в живой партии"
    );
    session.step_back(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert_eq!(game.session_step_count(), 1);
}

/// «Редактор», требование 5: repeated `step_once` calls stuck on the same non-live step must not
/// re-apply that step's own events — `toggle_sound` flips a visible flag every time it actually
/// runs, so a second flip would betray a duplicate application. Also covers `step_blocked_reason`'s
/// «Повтор разошёлся с записью» once the step has genuinely been tried and gone nowhere.
#[test]
fn step_once_applies_a_replayed_steps_events_exactly_once_when_stuck_on_a_non_live_screen() {
    let (mut game, config) = load("menu");
    let mut state = ScreenState::new(config.start_screen);
    let text = r#"{"format": 1, "steps": 5, "events": [
        {"step": 0, "command": ["toggle_sound"]}
    ]}"#;
    let mut replay = PlaySession::begin_replay(text, &mut game, &config, &mut state).unwrap();
    assert!(state.sound_enabled());
    assert!(
        replay.step_blocked_reason(&game, &config, &state).is_none(),
        "первая попытка ещё не пробовалась — «Шаг» должен быть доступен"
    );

    step_once(&mut replay, &mut game, &config, &mut state);
    assert!(!state.sound_enabled(), "toggle_sound сработал один раз");
    assert_eq!(
        game.session_step_count(),
        0,
        "menu без world_runs — мир стоит"
    );

    step_once(&mut replay, &mut game, &config, &mut state);
    assert!(
        !state.sound_enabled(),
        "повторный «Шаг» не должен снова применить событие того же шага"
    );

    assert_eq!(
        replay.step_blocked_reason(&game, &config, &state),
        Some("Повтор разошёлся с записью")
    );
}

/// «Редактор», требование 5: `seek` advances by the world's own step count, not the loop counter —
/// a screen stuck without `world_runs` must stop the loop there instead of applying every later
/// step's events onto a world that never actually reached them.
#[test]
fn seek_advances_by_the_games_own_step_count_and_stops_when_stuck() {
    let (mut game, config) = load("menu");
    let mut state = ScreenState::new(config.start_screen);
    let text = r#"{"format": 1, "steps": 3, "events": [
        {"step": 0, "command": ["toggle_sound"]},
        {"step": 1, "command": ["toggle_sound"]}
    ]}"#;
    let mut replay = PlaySession::begin_replay(text, &mut game, &config, &mut state).unwrap();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    replay.seek(
        2,
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert_eq!(
        game.session_step_count(),
        0,
        "menu без world_runs — мир застрял на шаге 0"
    );
    assert!(
        !state.sound_enabled(),
        "событие шага 1 не должно было примениться — мир до него не дошёл"
    );
}

/// «Редактор», требование 7: a code error raised while stepping surfaces through the session's own
/// `Game` state — the same state `wasm::Engine::step()` builds its return value from (checked only
/// at the wasm boundary, not testable natively).
#[test]
fn a_code_error_during_a_step_is_visible_through_the_sessions_own_game_state() {
    const CODE_GAME: &str = r##"{"name":"T","scene":{"width":10,"height":10,"background":"#000000"},
"random_seed":1,"start_screen":"game","max_objects":10,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},"code":"code.lua"}}"##;
    const CODE_PROPS: &str = r#"{"properties":{"marker":"flag"}}"#;
    const CODE_SCENE: &str =
        r#"{"objects":[{"position":[0,0],"size":[1,1],"velocity":[1,0],"marker":true}]}"#;
    const CODE_RULES: &str =
        r#"{"rules":[{"kind":"check","for":{"has":["marker"]},"do":[["run","boom"]]}]}"#;
    const CODE_SCREENS: &str = r#"{"screens":[{"name":"game","world_runs":true,"elements":[]}]}"#;
    const CODE_LUA: &str = "function boom(obj) obj.velocity.x = nil end";

    let (config, _w) = read_entry(CODE_GAME).expect("game.json должен разобраться");
    let (mut game, screens, _warnings, _images) = load_rest(
        CODE_GAME,
        config,
        Some(CODE_PROPS),
        Some(CODE_SCENE),
        Some(CODE_RULES),
        Some(CODE_SCREENS),
        &[],
        &[],
        &[],
        &[],
        Some(CODE_LUA),
        false,
    )
    .expect("должно загрузиться");
    let mut state = ScreenState::new(screens.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &screens, &mut state);

    assert!(game.code_error().is_none());
    step_once(&mut session, &mut game, &screens, &mut state);

    let err = game
        .code_error()
        .expect("ошибка кода должна остановить партию на этом шаге");
    assert!(!err.message.is_empty());
}

fn record_snapshot(game: &Game) -> (f64, bool, usize) {
    let score = game.properties.resolve("score").unwrap();
    (
        game.world.number_like(MOVER, score).unwrap_or_default(),
        game.world.is_alive(BRICK),
        game.world.alive_count(),
    )
}

/// «Редактор», требование 6: a replay driven by `tick_replay`'s own real-time accumulator must
/// give the same partiya as the live one, regardless of how the real time is sliced across calls —
/// one big tick, or several small ones, both bounded by `MAX_CATCHUP_STEPS` per call and neither
/// running past the recording's own end.
#[test]
fn a_replay_ticked_in_real_time_matches_the_live_partiya_regardless_of_frame_pacing() {
    let (mut live_game, live_config) = load("game");
    let mut live_state = ScreenState::new(live_config.start_screen);
    let mut live_session = PlaySession::begin_live(&mut live_game, &live_config, &mut live_state);
    let mut live_runner = Runner::new();
    let mut live_queue = UiQueue::new();
    let mut live_mouse = MouseState::default();
    let dt = 1.0 / 60.0;
    for _ in 0..3 {
        live_session.tick_live(
            &mut live_queue,
            &mut live_mouse,
            &mut live_runner,
            &mut live_game,
            &live_config,
            &mut live_state,
            VIEWPORT,
            dt,
            &[],
        );
    }
    assert_eq!(live_game.session_step_count(), 3);
    live_session.end(&mut live_game);
    let text = live_session.recording_text(&live_game);
    let expected = record_snapshot(&live_game);

    // One big tick covering all three steps in a single call.
    let (mut game_a, config_a) = load("game");
    let mut state_a = ScreenState::new(config_a.start_screen);
    let mut replay_a =
        PlaySession::begin_replay(&text, &mut game_a, &config_a, &mut state_a).unwrap();
    let mut runner_a = Runner::new();
    let mut queue_a = UiQueue::new();
    let mut mouse_a = MouseState::default();
    replay_a.tick_replay(
        &mut queue_a,
        &mut mouse_a,
        &mut runner_a,
        &mut game_a,
        &config_a,
        &mut state_a,
        VIEWPORT,
        3.0 * dt,
        &[],
    );
    assert_eq!(
        game_a.session_step_count(),
        3,
        "должно было сделать все три шага сразу"
    );
    assert_eq!(record_snapshot(&game_a), expected);

    // Several small ticks, none alone worth a whole step, summing to the same real time.
    let (mut game_b, config_b) = load("game");
    let mut state_b = ScreenState::new(config_b.start_screen);
    let mut replay_b =
        PlaySession::begin_replay(&text, &mut game_b, &config_b, &mut state_b).unwrap();
    let mut runner_b = Runner::new();
    let mut queue_b = UiQueue::new();
    let mut mouse_b = MouseState::default();
    for _ in 0..12 {
        replay_b.tick_replay(
            &mut queue_b,
            &mut mouse_b,
            &mut runner_b,
            &mut game_b,
            &config_b,
            &mut state_b,
            VIEWPORT,
            dt / 4.0,
            &[],
        );
    }
    assert_eq!(game_b.session_step_count(), 3);
    assert_eq!(record_snapshot(&game_b), expected);
}

/// «Редактор», требование 6: `tick_replay` must not run past the recording's own end even when a
/// huge `dt` owes far more than `MAX_CATCHUP_STEPS` steps.
#[test]
fn tick_replay_stops_at_the_recordings_end_even_with_a_huge_dt() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    step_once(&mut session, &mut game, &config, &mut state);
    session.end(&mut game);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let mut runner = Runner::new();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    replay.tick_replay(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game2,
        &config2,
        &mut state2,
        VIEWPORT,
        1.0,
        &[],
    );
    assert_eq!(game2.session_step_count(), 3);
}

/// «Редактор», второй круг ревью, пункт 1: `tick_replay` must apply a step's own recorded events
/// *before* deciding whether the world can advance — an outcome from the previous step must not
/// permanently block a `new_game` sitting right there in the next one. Reviewer's own probe:
/// `{step 0: set score 100}, {step 1: new_game game}, steps: 4`.
#[test]
fn tick_replay_applies_a_steps_own_events_before_deciding_it_is_stuck() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let text = r#"{"format": 1, "steps": 4, "events": [
        {"step": 0, "set": [0, "score", 100]},
        {"step": 1, "command": ["new_game", "game"]}
    ]}"#;
    let mut replay = PlaySession::begin_replay(text, &mut game, &config, &mut state).unwrap();
    let mut runner = Runner::new();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();

    // 20 real-time ticks, one step's worth each — the reviewer's own probe: the old code got stuck
    // on step 1 (the "win" screen) forever instead of ever trying the recorded `new_game`.
    for _ in 0..20 {
        replay.tick_replay(
            &mut queue,
            &mut mouse,
            &mut runner,
            &mut game,
            &config,
            &mut state,
            VIEWPORT,
            1.0 / 60.0,
            &[],
        );
    }
    assert_eq!(
        game.session_step_count(),
        4,
        "повтор должен был дойти до конца записи, а не застрять на шаге 1"
    );
    assert!(game.is_running(), "{:?}", game.code_error());
    let game_screen = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    assert_eq!(
        state.active(),
        game_screen,
        "new_game должен был вернуть на живой экран"
    );
}

/// «Редактор», второй круг ревью, пункт 2: `seek`'s own report must carry the same screen-change
/// its last step made in a live partiya — требования 23, 30.
#[test]
fn seek_reports_the_same_screen_change_a_live_step_made() {
    let (mut game, config) = load("game");
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    session
        .set_property(&mut game, &[], MOVER, "score", &serde_json::json!(100.0))
        .unwrap();
    step_once(&mut session, &mut game, &config, &mut state);
    let live_report = game.last_report().unwrap().clone();
    assert_eq!(
        live_report.screen_change,
        Some(("game".to_string(), "win".to_string()))
    );
    session.end(&mut game);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load("game");
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    replay.seek(
        1,
        &mut queue,
        &mut mouse,
        &mut game2,
        &config2,
        &mut state2,
        VIEWPORT,
        &[],
    );
    let seek_report = game2
        .last_report()
        .expect("seek должен был оставить отчёт о последнем шаге");
    assert_eq!(
        seek_report.screen_change,
        Some(("game".to_string(), "win".to_string())),
        "seek не позвал annotate_screen_change — вкладка «Шаг» разошлась бы с партией"
    );
}

/// «Редактор», второй круг ревью, пункт 9: mutation coverage for `self.applied_events_step = None`
/// in `seek` — without that reset, `step_once` after a `step_back` would think step 0's own events
/// were already applied and skip them, leaving the world stuck on `menu`.
#[test]
fn step_once_after_step_back_reapplies_that_steps_events() {
    let (mut game, config) = load("menu");
    let mut state = ScreenState::new(config.start_screen);
    let text = r#"{"format": 1, "steps": 2, "events": [
        {"step": 0, "command": ["new_game", "game"]}
    ]}"#;
    let mut replay = PlaySession::begin_replay(text, &mut game, &config, &mut state).unwrap();
    let game_screen = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    let menu_screen = config
        .screens
        .iter()
        .position(|s| s.name == "menu")
        .unwrap();

    step_once(&mut replay, &mut game, &config, &mut state);
    assert_eq!(game.session_step_count(), 1);
    assert_eq!(state.active(), game_screen);

    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    replay.step_back(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    assert_eq!(game.session_step_count(), 0);
    assert_eq!(
        state.active(),
        menu_screen,
        "шаг назад должен вернуть на menu"
    );

    step_once(&mut replay, &mut game, &config, &mut state);
    assert_eq!(game.session_step_count(), 1, "должен снова дойти до шага 1");
    assert_eq!(
        state.active(),
        game_screen,
        "new_game должен был примениться заново после отката"
    );
}

const BURST_GAME: &str = r##"{"name":"T","scene":{"width":20,"height":20,"background":"#000000"},
"random_seed":1,"start_screen":"main","max_objects":100,
"files":{"properties":"properties.json","scene":"scene.json","rules":"rules.json",
         "screens":"screens.json","fonts":{},
         "sounds":{"s1":"sounds/s1.wav","s2":"sounds/s2.wav","s3":"sounds/s3.wav",
                   "s4":"sounds/s4.wav","s5":"sounds/s5.wav"}}}"##;
const BURST_PROPS: &str = r#"{"properties":{
    "mover":"flag","zone1":"flag","zone2":"flag","zone3":"flag","zone4":"flag","zone5":"flag"
}}"#;
const BURST_SCENE: &str = r#"{"objects":[
    {"position":[6,0],"size":[1,1],"collides":true,"mover":true,"velocity":[1,0],
     "grid":{"interval":0.001}},
    {"position":[7,0],"size":[1,1],"collides":true,"zone1":true},
    {"position":[8,0],"size":[1,1],"collides":true,"zone2":true},
    {"position":[9,0],"size":[1,1],"collides":true,"zone3":true},
    {"position":[10,0],"size":[1,1],"collides":true,"zone4":true},
    {"position":[11,0],"size":[1,1],"collides":true,"zone5":true}
]}"#;
const BURST_RULES: &str = r#"{"rules":[
    {"kind":"move","for":{"has":["mover"]}},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone1"]},"do":[["play_sound","s1"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone2"]},"do":[["play_sound","s2"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone3"]},"do":[["play_sound","s3"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone4"]},"do":[["play_sound","s4"]]},
    {"kind":"collide","a":{"has":["mover"]},"b":{"has":["zone5"]},"do":[["play_sound","s5"]]}
]}"#;
const BURST_SCREENS: &str = r#"{"screens":[{"name":"main","world_runs":true,"elements":[]}]}"#;

fn load_burst() -> (Game, ScreensConfig) {
    let (config, _w) = read_entry(BURST_GAME).expect("game.json должен разобраться");
    let (game, screens, _warnings, _images) = load_rest(
        BURST_GAME,
        config,
        Some(BURST_PROPS),
        Some(BURST_SCENE),
        Some(BURST_RULES),
        Some(BURST_SCREENS),
        &[],
        &[],
        &[],
        &[],
        None,
        true,
    )
    .expect("должно загрузиться");
    (game, screens)
}

/// «Редактор», второй круг ревью, пункт 3: sound marks must accumulate over `tick_replay`'s whole
/// real-time burst, not get cleared between the steps inside it — mirrors `tests/sound_window.rs`'s
/// own `five_catchup_steps_in_one_call_collect_marks_from_every_step`, for the replay path.
#[test]
fn tick_replay_accumulates_sound_marks_across_a_multi_step_burst() {
    let (mut game, config) = load_burst();
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);
    let mut runner = Runner::new();
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    session.tick_live(
        &mut queue,
        &mut mouse,
        &mut runner,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        5.0 / 60.0,
        &[],
    );
    assert_eq!(
        game.session_step_count(),
        5,
        "весь остаток копилки — пять шагов"
    );
    session.end(&mut game);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load_burst();
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let mut runner2 = Runner::new();
    let mut q2 = UiQueue::new();
    let mut m2 = MouseState::default();
    replay.tick_replay(
        &mut q2,
        &mut m2,
        &mut runner2,
        &mut game2,
        &config2,
        &mut state2,
        VIEWPORT,
        5.0 / 60.0,
        &[],
    );
    assert_eq!(game2.session_step_count(), 5);
    for i in 0..5 {
        assert!(
            game2.sound_window().mark(i),
            "s{} должен быть отмечен — отметки должны копиться за весь вызов tick_replay",
            i + 1
        );
    }
}

/// «Редактор», второй круг ревью, пункт 7: `Game::has_world` is an explicit flag, not
/// `alive_count() > 0` — a live world every rule has emptied out must still read as "мир есть".
#[test]
fn has_world_tracks_new_game_quit_show_scene_and_reset_for_play_not_alive_count() {
    let (mut game, _config) = load("menu");
    assert!(
        !game.has_world(),
        "стартовый экран без world_runs — мира ещё нет"
    );

    game.new_game();
    assert!(game.has_world(), "new_game должен был собрать мир");

    // Emptying every object by hand (not through quit/reset_for_play) must not flip the flag —
    // this is exactly the case `alive_count() > 0` would get wrong.
    let ids: Vec<u32> = game.world.ids().collect();
    for id in ids {
        game.world.delete(id);
    }
    assert_eq!(game.world.alive_count(), 0);
    assert!(
        game.has_world(),
        "живой мир, который правила опустошили, — всё ещё мир, не «Мира нет»"
    );

    game.quit();
    assert!(!game.has_world(), "quit должен был убрать мир");

    game.show_scene();
    assert!(game.has_world(), "show_scene всегда собирает мир");

    game.reset_for_play(false);
    assert!(!game.has_world(), "reset_for_play(false) — мира нет");

    game.reset_for_play(true);
    assert!(game.has_world(), "reset_for_play(true) — мир есть");
}

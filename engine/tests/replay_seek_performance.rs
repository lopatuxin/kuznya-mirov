//! «Редактор», нефункциональное требование: переход по шкале в пятиминутной партии тетриса
//! (18 000 шагов) не дольше секунды в Chrome. `PlaySession::seek` recomputes the whole replay from
//! scratch, so its own cost is dominated by how cheaply it can find each step's recorded events —
//! `apply_events_before` looks them up by binary search (`events_for_step`), not a linear scan of
//! the whole recording, which is what this test guards against regressing back to O(шаги × события).
//!
//! Runs with real demo-tetris rules and code (`games/tetris`), so it doubles as a smoke test that
//! a long, real partiya seeks correctly, not just quickly. Print the timing with
//! `cargo test --release -- --nocapture`.

use std::fs;
use std::path::PathBuf;
use std::time::Instant;

use engine::core::game::Game;
use engine::core::input::{MouseState, UiQueue};
use engine::core::property;
use engine::core::screens::ScreenState;
use engine::data::load::{load_rest, read_entry};
use engine::data::session::PlaySession;

const VIEWPORT: [f32; 2] = [800.0, 600.0];
const TOTAL_STEPS: u64 = 18_000;

fn game_path(name: &str) -> PathBuf {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.push("..");
    path.push("games");
    path.push("tetris");
    path.push(name);
    path
}

fn read(name: &str) -> String {
    let path = game_path(name);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("не смог прочитать {path:?}: {e}"))
}

fn load() -> (
    engine::core::game::Game,
    engine::core::screens::ScreensConfig,
) {
    let game_json = read("game.json");
    let (config, _entry_warnings) =
        read_entry(&game_json).expect("game.json демо-тетриса должен разбираться");
    let (game, screens, _warnings, _images) = load_rest(
        &game_json,
        config,
        Some(&read("properties.json")),
        Some(&read("scene.json")),
        Some(&read("rules.json")),
        Some(&read("screens.json")),
        &[],
        &[],
        &[],
        &[],
        Some(&read("code.lua")),
        true,
    )
    .expect("демо-тетрис должен проходить предстартовую проверку");
    (game, screens)
}

/// `(alive objects, "game" object's own score)` — «Редактор», второй круг ревью, пункт 1: `seek`
/// has to land on the *same* world a live partiya reached at that step, not merely the same step
/// number.
fn snapshot(game: &Game) -> (usize, f64) {
    let score_prop = game.properties.resolve("score").unwrap();
    let score = game
        .world
        .ids()
        .find(|&id| game.world.text(id, property::NAME) == Some("game"))
        .and_then(|id| game.world.number_like(id, score_prop))
        .unwrap_or(0.0);
    (game.world.alive_count(), score)
}

/// Records an 18 000-step tetris partiya: a menu click to start (level 0), then `ArrowLeft`
/// pressed and released roughly every 20 steps, the cursor nudged roughly every 15, exercising the
/// same event kinds `seek` has to look up at every step.
#[test]
fn seeking_to_the_end_of_an_eighteen_thousand_step_tetris_replay_is_fast_and_correct() {
    let (mut game, config) = load();
    let mut state = ScreenState::new(config.start_screen);
    let mut session = PlaySession::begin_live(&mut game, &config, &mut state);

    // Level-0 button: `anchor: center, offset: [-136, -40], size: [56, 56]` — its center at this
    // viewport.
    let mut queue = UiQueue::new();
    let mut mouse = MouseState::default();
    queue.push_mouse_move(264.0, 260.0);
    queue.push_mouse_down();
    queue.push_mouse_up();
    session.step_once(
        &mut queue,
        &mut mouse,
        &mut game,
        &config,
        &mut state,
        VIEWPORT,
        &[],
    );
    let game_screen = config
        .screens
        .iter()
        .position(|s| s.name == "game")
        .unwrap();
    assert_eq!(state.active(), game_screen, "клик должен был начать партию");

    // The board tops out well before 18 000 steps with no real play — restart, same as a human
    // playing for five minutes across several games would, so the recording keeps growing towards
    // the target length instead of stopping dead on the first "Игра окончена".
    let mut left_held = false;
    let mut restarts = 0;
    while game.session_step_count() < TOTAL_STEPS {
        if !game.is_running() {
            assert!(game.code_error().is_none(), "{:?}", game.code_error());
            restarts += 1;
            assert!(
                restarts < 500,
                "слишком много перезапусков — что-то не так с миром"
            );
            // "В меню": anchor center, offset [0, 60], size [220, 50].
            queue.push_mouse_move(400.0, 360.0);
            queue.push_mouse_down();
            queue.push_mouse_up();
            session.step_once(
                &mut queue,
                &mut mouse,
                &mut game,
                &config,
                &mut state,
                VIEWPORT,
                &[],
            );
            queue.push_mouse_move(264.0, 260.0);
            queue.push_mouse_down();
            queue.push_mouse_up();
            session.step_once(
                &mut queue,
                &mut mouse,
                &mut game,
                &config,
                &mut state,
                VIEWPORT,
                &[],
            );
            left_held = false;
            continue;
        }
        let step = game.session_step_count();
        if step % 20 == 0 {
            if left_held {
                queue.push_key_up("ArrowLeft");
            } else {
                queue.push_key_down("ArrowLeft");
            }
            left_held = !left_held;
        }
        if step % 15 == 0 {
            let cell = [(step % 17) as f64, (step % 27) as f64];
            game.set_cursor_cell(cell);
            session.record_cursor(&game, cell);
        }
        session.step_once(
            &mut queue,
            &mut mouse,
            &mut game,
            &config,
            &mut state,
            VIEWPORT,
            &[],
        );
    }
    assert_eq!(game.session_step_count(), TOTAL_STEPS);
    let live_snapshot = snapshot(&game);
    session.end(&mut game);
    let text = session.recording_text(&game);

    let (mut game2, config2) = load();
    let mut state2 = ScreenState::new(config2.start_screen);
    let mut replay = PlaySession::begin_replay(&text, &mut game2, &config2, &mut state2).unwrap();
    let mut seek_queue = UiQueue::new();
    let mut seek_mouse = MouseState::default();

    let started = Instant::now();
    replay.seek(
        TOTAL_STEPS,
        &mut seek_queue,
        &mut seek_mouse,
        &mut game2,
        &config2,
        &mut state2,
        VIEWPORT,
        &[],
    );
    let elapsed = started.elapsed();
    println!("seek({TOTAL_STEPS}) заняло {elapsed:?}, restarts={restarts}");

    assert_eq!(
        game2.session_step_count(),
        TOTAL_STEPS,
        "seek должен дойти до конца записи"
    );
    assert_eq!(
        snapshot(&game2),
        live_snapshot,
        "seek должен был дать тот же мир, что и живая партия, а не только тот же номер шага"
    );
    // The 1-second bound is «Редактор»'s own release/browser figure — an unoptimized Lua
    // interpreter in a debug build routinely blows past it on the same 18 000 real steps this test
    // also needs just to build the recording, so only `--release` enforces it (`cargo test
    // --release -- --nocapture` per the task).
    #[cfg(not(debug_assertions))]
    assert!(
        elapsed.as_secs_f64() < 1.0,
        "seek({TOTAL_STEPS}) занял {elapsed:?} — дольше секунды"
    );
}

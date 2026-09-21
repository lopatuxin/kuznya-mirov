use super::game::Game;
use super::input::StepInput;

pub const STEP_SECONDS: f64 = 1.0 / 60.0;
pub const MAX_CATCHUP_STEPS: u32 = 5;

/// The real-time "копилка" from Исполнение игры: measures wall-clock time between calls and
/// turns it into a bounded burst of fixed steps. Lives outside `Game::step`, on purpose.
#[derive(Debug, Clone, Copy, Default)]
pub struct Runner {
    accumulator: f64,
    last_tick_ms: Option<f64>,
}

impl Runner {
    pub fn new() -> Self {
        Runner {
            accumulator: 0.0,
            last_tick_ms: None,
        }
    }

    /// Turns an absolute `performance.now()` timestamp into elapsed real time since the previous
    /// call — the way `wasm::Engine::tick` feeds `advance_or_reset`. `None` (never called yet, or
    /// forgotten by `forget_last_tick`) gives zero elapsed time instead of comparing `now_ms`
    /// against a stale timestamp from before a gap.
    pub fn dt_since_last_tick(&mut self, now_ms: f64) -> f64 {
        let dt = match self.last_tick_ms {
            Some(prev) => ((now_ms - prev) / 1000.0).max(0.0),
            None => 0.0,
        };
        self.last_tick_ms = Some(now_ms);
        dt
    }

    /// Call on `visibilitychange` going to hidden, alongside `reset`: without this, the next
    /// `dt_since_last_tick` would still compare the resumed `now_ms` against the timestamp from
    /// right before the tab was hidden and hand that whole gap to `advance` — `reset` alone only
    /// clears real time already banked in the accumulator, not a dt about to be added to it.
    pub fn forget_last_tick(&mut self) {
        self.last_tick_ms = None;
    }

    /// Advances `game` by `dt_seconds` of real time. Runs at most `MAX_CATCHUP_STEPS` fixed
    /// steps; a real ArrowUp/ArrowDown queued beforehand is delivered only to the first of them.
    pub fn advance(&mut self, game: &mut Game, dt_seconds: f64) {
        self.accumulator += dt_seconds.max(0.0);
        let mut ran = 0;
        while self.accumulator >= STEP_SECONDS && ran < MAX_CATCHUP_STEPS && game.is_running() {
            let input = if ran == 0 {
                game.take_input_snapshot()
            } else {
                StepInput::empty()
            };
            game.step(input);
            self.accumulator -= STEP_SECONDS;
            ran += 1;
        }
        if ran == MAX_CATCHUP_STEPS || !game.is_running() {
            self.accumulator = 0.0;
        }
    }

    /// The tab was hidden: the gap is not caught up, the bucket is dropped.
    pub fn reset(&mut self) {
        self.accumulator = 0.0;
    }

    /// «Экраны и состояние» → «Жизнь партии»: on the screen the active screen
    /// has `world_runs`, steps as `advance` always did; otherwise the step is skipped
    /// entirely and the accumulator is dropped every call, so a minute spent paused does not
    /// arrive as a burst of catch-up steps on return.
    ///
    /// «Звук» → «Один вызов движка»: this is the one call every tick
    /// makes exactly once, live screen or not, so the sound marks the page already read last
    /// time are cleared right here, before any of this call's steps get a chance to raise new
    /// ones — never at the end, and never per step inside the catch-up burst below.
    pub fn advance_or_reset(&mut self, game: &mut Game, dt_seconds: f64, screen_live: bool) {
        game.sound_window_mut().clear_marks();
        if !screen_live {
            self.reset();
            return;
        }
        self.advance(game, dt_seconds);
    }
}

/// «Картинки» → «Кадры»: window time for the interface's own image frames, in steps (60/second) —
/// counted from an absolute `now_ms` timestamp rather than accumulated frame-to-frame deltas, so
/// neither a paused screen nor a hidden tab affects it: the browser's own clock keeps running
/// through both, and the interface's frame simply reflects however much real time has actually
/// elapsed, jumping forward on return from a hidden tab instead of standing still.
#[derive(Debug, Clone, Copy, Default)]
pub struct UiClock {
    first_tick_ms: Option<f64>,
}

impl UiClock {
    pub fn new() -> Self {
        UiClock {
            first_tick_ms: None,
        }
    }

    /// Starts the count over from the next `elapsed_steps` call — a fresh game load starts the
    /// interface's own ribbon at frame 0 too, same as the world's.
    pub fn reset(&mut self) {
        self.first_tick_ms = None;
    }

    /// Steps elapsed since the first call after creation or the last `reset`.
    pub fn elapsed_steps(&mut self, now_ms: f64) -> f64 {
        let first = *self.first_tick_ms.get_or_insert(now_ms);
        (now_ms - first) / 1000.0 * 60.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::property::PropertyTable;
    use crate::core::rules::RuleSet;
    use crate::core::scene::SceneConfig;
    use crate::core::world::World;

    fn empty_game() -> Game {
        let properties = PropertyTable::new();
        let world = World::new(&properties);
        let scene = SceneConfig {
            width: 10,
            height: 10,
            background: [0.0; 4],
        };
        Game::new(
            properties,
            world,
            RuleSet::default(),
            scene,
            100,
            1,
            Vec::new(),
            0,
            None,
            String::new(),
            Vec::new(),
            Vec::new(),
            true,
        )
    }

    #[test]
    fn catches_up_at_most_five_steps_and_drops_the_rest() {
        let mut game = empty_game();
        let mut runner = Runner::new();
        runner.advance(&mut game, 1.0); // 60 steps owed, way over the cap
        assert_eq!(game.step_count(), 5);
        // the leftover bucket was thrown away, not carried into the next call
        runner.advance(&mut game, 0.0);
        assert_eq!(game.step_count(), 5);
    }

    #[test]
    fn small_delta_does_not_step_yet() {
        let mut game = empty_game();
        let mut runner = Runner::new();
        runner.advance(&mut game, STEP_SECONDS * 0.5);
        assert_eq!(game.step_count(), 0);
        runner.advance(&mut game, STEP_SECONDS * 0.5);
        assert_eq!(game.step_count(), 1);
    }

    #[test]
    fn dt_since_last_tick_is_zero_on_the_very_first_call() {
        let mut runner = Runner::new();
        assert_eq!(runner.dt_since_last_tick(12_345.0), 0.0);
    }

    #[test]
    fn dt_since_last_tick_measures_the_gap_between_two_calls() {
        let mut runner = Runner::new();
        runner.dt_since_last_tick(1_000.0);
        assert_eq!(runner.dt_since_last_tick(1_016.0), 0.016);
    }

    /// «Исполнение игры»: «вкладку спрятали — пропущенное время не догоняется». Without
    /// `forget_last_tick`, the first `dt_since_last_tick` after a multi-minute gap would hand that
    /// whole gap to `advance` as a real `dt_seconds` — `Runner::reset` alone only clears time
    /// already banked in the accumulator, not a dt about to be added to it next.
    #[test]
    fn tab_hidden_then_resumed_gives_zero_dt_on_the_first_tick_back() {
        let mut game = empty_game();
        let mut runner = Runner::new();
        runner.dt_since_last_tick(1_000.0);

        // The tab was hidden for five real minutes.
        runner.reset();
        runner.forget_last_tick();

        let dt = runner.dt_since_last_tick(1_000.0 + 5.0 * 60_000.0);
        assert_eq!(
            dt, 0.0,
            "первый кадр после возврата должен дать нулевое прошедшее время"
        );
        runner.advance(&mut game, dt);
        assert_eq!(
            game.step_count(),
            0,
            "без пропущенного времени шагов быть не должно"
        );
    }

    #[test]
    fn ui_clock_starts_at_zero_on_the_first_call() {
        let mut clock = UiClock::new();
        assert_eq!(clock.elapsed_steps(1_000.0), 0.0);
    }

    #[test]
    fn ui_clock_advances_with_real_time_between_calls() {
        let mut clock = UiClock::new();
        clock.elapsed_steps(1_000.0);
        assert_eq!(clock.elapsed_steps(1_016.0), 0.96); // 16ms * 60/1000
    }

    /// «Картинки» → «Прочее»: «лента интерфейса перескакивает вперёд, потому что часы окна
    /// шли» — a five-minute real-world gap between two `tick()` calls (the tab was hidden and
    /// resumed) must show up as five minutes' worth of steps, not as zero the way the world's own
    /// step accumulator sees it.
    #[test]
    fn ui_clock_jumps_forward_across_a_gap_with_no_calls_in_between() {
        let mut clock = UiClock::new();
        clock.elapsed_steps(1_000.0);
        let gap_ms = 5.0 * 60_000.0;
        let steps = clock.elapsed_steps(1_000.0 + gap_ms);
        assert_eq!(steps, gap_ms / 1000.0 * 60.0);
    }

    #[test]
    fn ui_clock_reset_starts_a_new_count_from_the_next_call() {
        let mut clock = UiClock::new();
        clock.elapsed_steps(1_000.0);
        clock.elapsed_steps(2_000.0);
        clock.reset();
        assert_eq!(clock.elapsed_steps(5_000.0), 0.0);
    }
}

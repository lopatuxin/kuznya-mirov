use super::game::Game;
use super::input::StepInput;

pub const STEP_SECONDS: f64 = 1.0 / 60.0;
pub const MAX_CATCHUP_STEPS: u32 = 5;

/// The real-time "копилка" from Исполнение игры: measures wall-clock time between calls and
/// turns it into a bounded burst of fixed steps. Lives outside `Game::step`, on purpose.
#[derive(Debug, Clone, Copy, Default)]
pub struct Runner {
    accumulator: f64,
}

impl Runner {
    pub fn new() -> Self {
        Runner { accumulator: 0.0 }
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

    /// «Экраны и состояние» → «Как это ложится в круг движка»: on the screen the active screen
    /// has `world_runs`, steps as `advance` always did; otherwise the step is skipped
    /// entirely and the accumulator is dropped every call, so a minute spent paused does not
    /// arrive as a burst of catch-up steps on return.
    ///
    /// «Звук в шаге и кадре» → «Порядок работ за один вызов»: this is the one call every tick
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
}

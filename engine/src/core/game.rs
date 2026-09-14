use super::grid::SpatialGrid;
use super::input::{InputQueue, StepInput};
use super::property::{self, PropertyTable};
use super::rng::Rng;
use super::rules::{Outcome, RuleSet};
use super::scene::{ObjectSpec, SceneConfig};
use super::sound::SoundWindow;
use super::step;
use super::value::Vec2;
use super::world::World;

#[derive(Debug)]
pub struct Game {
    pub properties: PropertyTable,
    pub world: World,
    pub rules: RuleSet,
    pub scene: SceneConfig,
    pub max_objects: usize,

    scene_objects: Vec<ObjectSpec>,
    random_seed: u64,
    rng: Rng,
    step_count: u64,
    input_queue: InputQueue,
    grid: SpatialGrid,
    outcome: Option<(Outcome, u64)>,
    max_objects_warned: bool,
    random_cell_warned: bool,
    messages: Vec<String>,
    sound_window: SoundWindow,

    grid_hop_ready: Vec<bool>,
    moved: Vec<bool>,
    bounced: Vec<bool>,
}

impl Game {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        properties: PropertyTable,
        world: World,
        rules: RuleSet,
        scene: SceneConfig,
        max_objects: usize,
        random_seed: u64,
        scene_objects: Vec<ObjectSpec>,
        sound_count: usize,
    ) -> Self {
        Game {
            properties,
            world,
            rules,
            scene,
            max_objects,
            scene_objects,
            random_seed,
            rng: Rng::new(random_seed),
            step_count: 0,
            input_queue: InputQueue::new(),
            grid: SpatialGrid::new(),
            outcome: None,
            max_objects_warned: false,
            random_cell_warned: false,
            messages: Vec::new(),
            sound_window: SoundWindow::new(sound_count),
            grid_hop_ready: Vec::new(),
            moved: Vec::new(),
            bounced: Vec::new(),
        }
    }

    pub fn key_down(&mut self, code: &str) {
        self.input_queue.press(code);
    }

    pub fn key_up(&mut self, code: &str) {
        self.input_queue.release(code);
    }

    /// Whether `code` is held right now — a key the world already treats as pressed, whether or
    /// not the currently active screen's own `keys` table names it.
    pub fn is_key_held(&self, code: &str) -> bool {
        self.input_queue.is_held(code)
    }

    /// «Экраны и состояние»: applied by the screen layer's `switch_to` when a transition leaves
    /// a live screen — synthesizes a release for every key held right now, then drops both the
    /// pending queue and the held set, so a returning player has to press the key again.
    pub fn release_held_keys(&mut self) {
        let held = self.input_queue.held_keys();
        step::apply_input(&mut self.world, &[], &held);
        self.input_queue.clear();
    }

    /// «Экраны и состояние» → «Клавиша экрана»: releases one held key immediately, the same way
    /// `release_held_keys` releases all of them — used when the screen that declares `code`
    /// absorbs its release, so the world binding a *different* screen set up for it doesn't stay
    /// stuck. Applied right away rather than queued: a queued release would sit in the input
    /// queue until the next step, and a screen switch landing before that step clears the queue
    /// out from under it.
    pub fn release_key(&mut self, code: &str) {
        step::apply_input(
            &mut self.world,
            &[],
            std::slice::from_ref(&code.to_string()),
        );
        self.input_queue.forget(code);
    }

    /// «Экраны и состояние» → «Что происходит при создании»: rebuilds the world from the
    /// parsed copy of `scene.json` — the file itself is never reopened — resets the step
    /// counter, the random-number generator (same seed, so the second playthrough replays the
    /// same way the first one would), the input queue, and the sticky win/loss mark.
    pub fn new_game(&mut self) {
        self.world = World::new(&self.properties);
        for spec in &self.scene_objects {
            let id = self.world.create();
            for (prop, value) in &spec.values {
                self.world.set_value(id, *prop, value);
            }
            if let Some(grid) = &spec.grid {
                self.world.set_grid(id, property::GRID, *grid);
                self.world.set_grid_counter(id, grid.interval_steps);
            }
            if let Some(keys) = &spec.keys {
                self.world.set_keys(id, property::KEYS, keys.clone());
            }
        }
        self.step_count = 0;
        self.rng = Rng::new(self.random_seed);
        self.input_queue = InputQueue::new();
        self.outcome = None;
    }

    /// «Экраны и состояние»: `quit` throws the run away entirely — the world empties and there
    /// is no way back to it, a fresh `new_game` starts from a clean scene. Clearing the sticky
    /// win/loss mark here matters as much as emptying the world: `handle_outcome` runs every
    /// tick regardless of the active screen, so a mark left standing would drag the player
    /// straight back to the outcome screen the next tick after `quit` returns them to the menu.
    pub fn quit(&mut self) {
        self.world = World::new(&self.properties);
        self.outcome = None;
    }

    pub fn is_running(&self) -> bool {
        self.outcome.is_none()
    }

    pub fn outcome(&self) -> Option<(Outcome, u64)> {
        self.outcome
    }

    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// «Звук снаружи движка»: read side, for the circle (clearing/writing the header) and for
    /// the wasm layer's `sound_window_ptr()`/`sound_window_len()` — never for a step.
    pub fn sound_window(&self) -> &SoundWindow {
        &self.sound_window
    }

    pub fn sound_window_mut(&mut self) -> &mut SoundWindow {
        &mut self.sound_window
    }

    pub fn take_input_snapshot(&mut self) -> StepInput {
        self.input_queue.take_snapshot()
    }

    fn ensure_scratch_capacity(&mut self) {
        let n = self.world.slot_count();
        self.grid_hop_ready.resize(n, false);
        self.moved.resize(n, false);
        self.bounced.resize(n, false);
    }

    /// Runs the nine stages of "Исполнение игры" for one fixed step. `input` is the picture
    /// of key presses/releases for this step (stage 1 happened in `take_input_snapshot`).
    pub fn step(&mut self, input: StepInput) {
        if self.outcome.is_some() {
            return;
        }
        self.step_count += 1;
        self.ensure_scratch_capacity();

        step::apply_input(&mut self.world, &input.pressed, &input.released);

        let expired = step::tick_counters(&mut self.world, &mut self.grid_hop_ready);

        let pre_move_positions: Vec<Option<Vec2>> = (0..self.world.slot_count() as u32)
            .map(|id| self.world.vec2(id, property::POSITION))
            .collect();

        for moved in self.moved.iter_mut() {
            *moved = false;
        }
        step::apply_move_rules(
            &self.rules.rules,
            &mut self.world,
            &mut self.grid_hop_ready,
            &mut self.moved,
        );

        let pairs = step::find_collision_pairs(&self.world, &mut self.grid);

        for bounced in self.bounced.iter_mut() {
            *bounced = false;
        }
        let mut outcome_flag = None;
        let mut deletes_from_collide = Vec::new();
        // «Звук в шаге и кадре»: этому и только этому обёртка `SoundMarks` даётся — поднять
        // отметку и никогда её не прочитать; окно целиком (`self.sound_window`) шаг не видит.
        let mut marks = self.sound_window.marks();
        step::apply_collide_rules(
            &self.rules.rules,
            &mut self.world,
            &pairs,
            &mut self.bounced,
            &mut deletes_from_collide,
            &mut outcome_flag,
            &mut marks,
        );

        let already_deleted: Vec<u32> = expired.into_iter().chain(deletes_from_collide).collect();
        let mut random_cell_exhausted = false;
        let (mut all_deleted, creates) = step::queue_create_and_delete_rules(
            &self.rules.rules,
            &mut self.world,
            &self.properties,
            &self.scene,
            &already_deleted,
            &self.moved,
            &pre_move_positions,
            &mut self.rng,
            &mut outcome_flag,
            &mut random_cell_exhausted,
            &mut marks,
        );
        if random_cell_exhausted && !self.random_cell_warned {
            self.random_cell_warned = true;
            self.messages.push(
                "random_cell: свободной клетки нет, заявка на создание отброшена".to_string(),
            );
        }

        all_deleted.sort_unstable();
        all_deleted.dedup();
        for id in all_deleted {
            self.world.delete(id);
        }
        for create in creates {
            if self.world.alive_count() >= self.max_objects {
                if !self.max_objects_warned {
                    self.max_objects_warned = true;
                    self.messages.push(format!(
                        "достигнут потолок объектов ({}): заявка на создание отброшена",
                        self.max_objects
                    ));
                }
                continue;
            }
            let id = self.world.create();
            self.world.set_vec2(id, property::POSITION, create.position);
            for (prop, value) in &create.props {
                self.world.set_value(id, *prop, value);
            }
            if let Some(spec) = self.world.grid(id, property::GRID) {
                self.world.set_grid_counter(id, spec.interval_steps);
            }
        }

        if let Some(outcome) = outcome_flag {
            self.outcome = Some((outcome, self.step_count));
        }
    }
}

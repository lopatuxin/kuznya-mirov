use super::grid::SpatialGrid;
use super::input::{InputQueue, StepInput};
use super::property::{self, PropertyTable};
use super::rng::Rng;
use super::rules::{Outcome, RuleSet};
use super::scene::SceneConfig;
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

    rng: Rng,
    step_count: u64,
    input_queue: InputQueue,
    grid: SpatialGrid,
    outcome: Option<(Outcome, u64)>,
    max_objects_warned: bool,
    random_cell_warned: bool,
    messages: Vec<String>,

    grid_hop_ready: Vec<bool>,
    moved: Vec<bool>,
    bounced: Vec<bool>,
}

impl Game {
    pub fn new(
        properties: PropertyTable,
        world: World,
        rules: RuleSet,
        scene: SceneConfig,
        max_objects: usize,
        random_seed: u64,
    ) -> Self {
        Game {
            properties,
            world,
            rules,
            scene,
            max_objects,
            rng: Rng::new(random_seed),
            step_count: 0,
            input_queue: InputQueue::new(),
            grid: SpatialGrid::new(),
            outcome: None,
            max_objects_warned: false,
            random_cell_warned: false,
            messages: Vec::new(),
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
        step::apply_collide_rules(
            &self.rules.rules,
            &mut self.world,
            &pairs,
            &mut self.bounced,
            &mut deletes_from_collide,
            &mut outcome_flag,
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

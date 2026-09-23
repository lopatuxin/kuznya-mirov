use super::code::{self, CodeError};
use super::grid::SpatialGrid;
use super::input::{InputQueue, KeyAction, KeyEvent, StepInput};
use super::property::{self, PropertyId, PropertyTable};
use super::rng::Rng;
use super::rules::{Outcome, RuleSet};
use super::scene::{ObjectSpec, SceneConfig};
use super::sound::SoundWindow;
use super::step;
use super::value::{Value, Vec2};
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
    /// «Код игры»: текст файла кода и его путь (для сообщений об ошибке) — хранятся, чтобы
    /// `new_game` могла каждый раз строить свежий исполнитель. `None`, когда у игры нет
    /// `files.code`.
    code_source: Option<String>,
    code_path: String,
    /// «Код игры»: имя картинки/звука по их `ImageId`/`SoundId` — код обращается к ним по имени,
    /// а не по номеру, так что рантайму нужна обратная таблица, которой правилам не требовалось.
    image_names: Vec<String>,
    sound_names: Vec<String>,
    code: Option<code::Runner>,
    /// «Код игры»: ошибка кода во время партии — раз поднятая, остаётся до `new_game`/`quit`;
    /// `is_running` смотрит и сюда, и на `outcome`.
    code_error: Option<CodeError>,
    /// Codes the world currently holds — updated only where `apply_to_world` runs `step::
    /// apply_input`, in lockstep with it: inserted when a `Press` is actually applied, removed
    /// when a `Release` is. The one source of truth for "does the world hold this key right now",
    /// deliberately not `InputQueue`'s own held set, which only says the *page* still thinks a key
    /// is down — true whether or not any step ever applied its press. Browser auto-repeat, or a
    /// press still queued when a screen absorbs the matching release, can both leave that
    /// page-level set stale without this one moving.
    world_held_keys: std::collections::HashSet<String>,
    grid: SpatialGrid,
    outcome: Option<(Outcome, u64)>,
    max_objects_warned: bool,
    random_cell_warned: bool,
    messages: Vec<String>,
    sound_window: SoundWindow,

    grid_hop_ready: Vec<bool>,
    moved: Vec<bool>,
    bounced: Vec<bool>,

    /// «Курсор в мире»: the world cursor's current scene-coordinate position, updated every time
    /// the page reports a mouse move (live screen or not — требование 26: сдвиги на паузе
    /// копятся). `None` until the cursor has moved at least once.
    cursor_current: Option<Vec2>,
    /// The position the most recent step actually took — compared against `cursor_current` at
    /// `take_input_snapshot` time to decide whether *this* step gets a cursor at all.
    cursor_last_step: Option<Vec2>,
}

/// «Экраны и состояние» / «Редактор», требование 16: builds a fresh world from `scene_objects` —
/// one object per entry, in file order (`World::create` hands out slots in call order and nothing
/// here ever deletes one, so `scene_objects[n]` always lands in slot `n`), its `values`/`grid`/
/// `keys` written — with no other new-game side effect (no rng, step counter, input, code).
/// Shared by `new_game_with_values`, `show_scene` and `data::load::load_rest`'s own initial world.
pub(crate) fn world_from_scene(properties: &PropertyTable, scene_objects: &[ObjectSpec]) -> World {
    let mut world = World::new(properties);
    for spec in scene_objects {
        let id = world.create();
        for (prop, value) in &spec.values {
            world.set_value(id, *prop, value);
        }
        if let Some(grid) = &spec.grid {
            world.set_grid(id, property::GRID, *grid);
            world.set_grid_counter(id, grid.interval_steps);
        }
        if let Some(keys) = &spec.keys {
            world.set_keys(id, property::KEYS, keys.clone());
        }
    }
    world
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
        code_source: Option<String>,
        code_path: String,
        image_names: Vec<String>,
        sound_names: Vec<String>,
        start_is_live: bool,
    ) -> Self {
        let mut game = Game {
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
            code_source,
            code_path,
            image_names,
            sound_names,
            code: None,
            code_error: None,
            world_held_keys: std::collections::HashSet::new(),
            grid: SpatialGrid::new(),
            outcome: None,
            max_objects_warned: false,
            random_cell_warned: false,
            messages: Vec::new(),
            sound_window: SoundWindow::new(sound_count),
            grid_hop_ready: Vec::new(),
            moved: Vec::new(),
            bounced: Vec::new(),
            cursor_current: None,
            cursor_last_step: None,
        };
        // «Код игры» → «Экраны и состояние»: код грузится ровно один раз на партию. Партия
        // начинается либо прямо здесь (стартовый экран без меню — `world_runs` поднят сразу), и
        // тогда код выполняется здесь и только здесь, либо позже, кнопкой `new_game` (стартовый
        // экран — меню): тогда партии здесь ещё нет, и код грузится позже, в `new_game`, а не
        // дважды — здесь и там.
        if start_is_live {
            game.load_code();
        }
        game
    }

    /// «Код игры»: строит исполнитель лениво, в начале шага, если у игры есть код, но исполнителя
    /// ещё нет — покрывает оба пути в обход `new_game`, единственного места, которое иначе его бы
    /// строило: стартовый экран — меню, `show_screen` уводит сразу на живой экран (движок этого не
    /// запрещает — «Экраны и состояние»: `show_screen` не трогает мир); и `quit`,
    /// выбросивший исполнитель, а не начавший партию заново сам, за которым тоже может идти живой
    /// экран без `new_game`. Счётчик случайности сбрасывается тут же, перед загрузкой, точно как
    /// в `new_game` — партия, начатая так, идёт так же, как если бы её начал `new_game`. Пустая
    /// операция, когда исполнитель уже есть (обычный путь — `Game::new`/`new_game` уже собрали
    /// его) или уже стоит ошибка кода (`is_running` не даст шагам идти дальше).
    fn ensure_code_loaded(&mut self) {
        if self.code.is_some() || self.code_error.is_some() || self.code_source.is_none() {
            return;
        }
        self.rng = Rng::new(self.random_seed);
        self.load_code();
    }

    /// «Код игры»: (пере)строит исполнитель из хранимого текста файла — общая часть `new`,
    /// `new_game` и `quit`. Ошибка компиляции здесь означала бы, что файл прошёл проверку при
    /// загрузке (`data::load`), но на этот раз выполнился иначе — крайне маловероятно (тот же
    /// текст, тот же бюджет), но не паникует: партия просто стартует уже остановленной ошибкой
    /// кода, как и любая другая ошибка кода во время игры.
    fn load_code(&mut self) {
        self.code = None;
        self.code_error = None;
        let Some(source) = self.code_source.clone() else {
            return;
        };
        match code::Runner::compile(
            &source,
            &self.code_path,
            &self.properties,
            &self.image_names,
            &self.sound_names,
            &mut self.rng,
            &mut self.messages,
        ) {
            Ok(runner) => self.code = Some(runner),
            Err(err) => self.code_error = Some(err),
        }
    }

    pub fn key_down(&mut self, code: &str) {
        self.input_queue.press(code);
    }

    pub fn key_up(&mut self, code: &str) {
        self.input_queue.release(code);
    }

    /// Whether the world currently holds `code` — a step actually applied its `Press` and no step
    /// (nor `release_key`) has applied a matching `Release` since. Deliberately not a question
    /// about `InputQueue`'s own bookkeeping: what the page has queued can disagree with what the
    /// world has seen, and this answers about the world. See `release_key`.
    fn is_key_held(&self, code: &str) -> bool {
        self.world_held_keys.contains(code)
    }

    /// Runs `events` against the world and keeps `world_held_keys` in lockstep — the only place
    /// either happens, so the two can never drift apart. `step`, `release_held_keys` and
    /// `release_key` all go through this rather than calling `step::apply_input` on their own.
    fn apply_to_world(&mut self, events: &[KeyEvent]) {
        step::apply_input(&mut self.world, events);
        for event in events {
            match event.action {
                KeyAction::Press => {
                    self.world_held_keys.insert(event.code.clone());
                }
                KeyAction::Release => {
                    self.world_held_keys.remove(&event.code);
                }
            }
        }
    }

    /// «Экраны и состояние»: applied by the screen layer's `switch_to` when a transition leaves
    /// a live screen — synthesizes a release for every key the *world* currently holds, then
    /// drops both the page's pending queue and its own held set, so a returning player has to
    /// press the key again. Releasing `world_held_keys` rather than `InputQueue`'s own held set
    /// matters for the same reason `release_key` checks it: the page can think a key is still
    /// down (browser auto-repeat, or a press still sitting unconsumed in the queue) when the
    /// world never actually saw its press, and manufacturing a release for that key would fire a
    /// `release` binding with no matching `press` ever having run.
    pub fn release_held_keys(&mut self) {
        // «Исполнение игры» → «Повторяемость»: the same input must give the same run on any
        // machine — a `HashSet`'s own iteration order isn't that, so the codes are sorted before
        // two releases that write the same property differently can disagree on which wins.
        let mut codes: Vec<&String> = self.world_held_keys.iter().collect();
        codes.sort();
        let events: Vec<KeyEvent> = codes
            .into_iter()
            .map(|code| KeyEvent {
                code: code.clone(),
                action: KeyAction::Release,
            })
            .collect();
        self.apply_to_world(&events);
        self.input_queue.clear();
    }

    /// «Экраны и состояние» → «Клавиши экрана»: releases one key immediately, applying its world
    /// effect right now instead of going through the input queue — *if and only if* the world
    /// currently holds it (`is_key_held`). Needed for exactly one case: a key's press reached the
    /// world on a live screen that did not declare it, the player switches to a *different* live
    /// screen that does declare it, and releases it there — that release is absorbed (the
    /// screen's own command runs, the key never reaches the world through the normal `key_up`
    /// path), but the press already happened and was applied by an earlier step, so without this
    /// the property it set (e.g. a paddle's velocity) stays on until the player leaves a live
    /// screen entirely. Applied immediately rather than queued through `key_up`: the absorbed
    /// release's own command can switch straight to a non-live screen, whose `switch_to` calls
    /// `release_held_keys` and clears the whole input queue — a release sitting in that queue
    /// would be thrown out right along with it, leaving the property stuck on.
    ///
    /// The guard is `is_key_held` (world state), never "is a `Press` for `code` still sitting in
    /// the queue" — that queue-shaped question has two different wrong answers. A `Press` can
    /// still be pending *after* the world already holds the key: browser auto-repeat (suppressed
    /// at the front edge by `InputQueue::press` now, but the engine must not depend on that), or a
    /// `Release`-then-`Press` of the same key landing in one gap before the absorbing switch.
    /// Either would make "no pending `Press`" wrongly say "world doesn't hold it" and silently
    /// drop a release the world is owed. Conversely a `Press` can still be pending while the world
    /// has *never* stepped it at all — the scenario this method exists for in the first place —
    /// which "a pending `Press`" alone can't distinguish from the auto-repeat case above. Only the
    /// world's own bookkeeping answers both correctly. Whatever the page still has queued for
    /// `code` is dropped either way: a screen has just decided this key's fate, so nothing left
    /// over should surface later and re-trigger a binding on its own.
    pub fn release_key(&mut self, code: &str) {
        if self.is_key_held(code) {
            self.apply_to_world(std::slice::from_ref(&KeyEvent {
                code: code.to_string(),
                action: KeyAction::Release,
            }));
        }
        self.input_queue.forget(code);
    }

    /// «Экраны и состояние» → «Жизнь партии»: same as `new_game_with_values`, with no initial
    /// values — the common case, and the one every native test still calls directly.
    pub fn new_game(&mut self) {
        self.new_game_with_values(&[]);
    }

    /// «Экраны и состояние» → «Начальные значения у new_game», требование 29: rebuilds the world
    /// from the parsed copy of `scene.json` — the file itself is never reopened — resets the
    /// step counter, the random-number generator (same seed, so the second playthrough replays
    /// the same way the first one would), the input queue and the sticky win/loss mark, then
    /// writes `initial_values` — each `(name, prop, value)` addresses the scene object of that
    /// `name` with the smallest id — over the freshly built world, before the first step.
    pub fn new_game_with_values(&mut self, initial_values: &[(String, PropertyId, Value)]) {
        self.world = world_from_scene(&self.properties, &self.scene_objects);
        for (name, prop, value) in initial_values {
            if let Some(id) = self
                .world
                .ids()
                .filter(|&id| self.world.text(id, property::NAME) == Some(name.as_str()))
                .min()
            {
                self.world.set_value(id, *prop, value);
            }
        }
        self.step_count = 0;
        self.rng = Rng::new(self.random_seed);
        self.input_queue = InputQueue::new();
        self.world_held_keys.clear();
        self.outcome = None;
        // «Курсор в мире»: не сбрасывает саму позицию (мышь физически не двигалась), только
        // «доставлена ли она уже шагу» — первый шаг новой партии видит текущее положение курсора,
        // как если бы оно только что сдвинулось.
        self.cursor_last_step = None;
        // «Код игры»: свежий исполнитель на каждую партию — счётчик случайности уже сброшен
        // строкой выше, так что вторая партия идёт как первая.
        self.load_code();
    }

    /// «Редактор», требование 16: rebuilds `world` from the scene exactly like `new_game`, but
    /// with no initial values, no code (compiled or run) and none of `new_game`'s other resets —
    /// no partiya at all, just the scene's own objects for the editor to look at. Idempotent: the
    /// editor calls it once after every successful `load()`.
    pub fn show_scene(&mut self) {
        self.world = world_from_scene(&self.properties, &self.scene_objects);
    }

    /// «Экраны и состояние»: `quit` throws the run away entirely — the world empties and there
    /// is no way back to it, a fresh `new_game` starts from a clean scene. Clearing the sticky
    /// win/loss mark here matters as much as emptying the world: `handle_outcome` runs every
    /// tick regardless of the active screen, so a mark left standing would drag the player
    /// straight back to the outcome screen the next tick after `quit` returns them to the menu.
    /// The input queue and `world_held_keys` are dropped the same way `new_game` drops them: if
    /// `start_screen` is itself live, `switch_to` never calls `release_held_keys` on the way there
    /// (leaving one live screen for another doesn't), so anything left standing here would sit on
    /// top of the just-emptied world — `is_key_held` lying about what a destroyed world holds, a
    /// held key's repeat swallowed by a page-level `held` that quit never touched.
    pub fn quit(&mut self) {
        self.world = World::new(&self.properties);
        self.input_queue = InputQueue::new();
        self.world_held_keys.clear();
        self.outcome = None;
        // «Код игры»: «quit выбрасывает исполнитель вместе с миром» — never rebuilt here; the
        // next `new_game` (not `quit`) is the one that loads a fresh one.
        self.code = None;
        self.code_error = None;
    }

    pub fn is_running(&self) -> bool {
        self.outcome.is_none() && self.code_error.is_none()
    }

    pub fn outcome(&self) -> Option<(Outcome, u64)> {
        self.outcome
    }

    /// «Код игры»: ошибка кода во время партии, если она уже остановила игру — `tick` показывает
    /// её так же, как ошибку загрузки, а `is_running` учитывает её наравне с `outcome`.
    pub fn code_error(&self) -> Option<&CodeError> {
        self.code_error.as_ref()
    }

    /// «Код игры»: путь файла кода из `files.code` — для показа ошибки во время партии в том же
    /// виде, что ошибку загрузки. Пустая строка, когда у игры нет кода.
    pub fn code_path(&self) -> &str {
        &self.code_path
    }

    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    pub fn messages(&self) -> &[String] {
        &self.messages
    }

    /// «Звук»: read side, for the circle (clearing/writing the header) and for
    /// the wasm layer's `sound_window_ptr()`/`sound_window_len()` — never for a step.
    pub fn sound_window(&self) -> &SoundWindow {
        &self.sound_window
    }

    pub fn sound_window_mut(&mut self) -> &mut SoundWindow {
        &mut self.sound_window
    }

    /// «Курсор в мире»: translates a window-pixel cursor position through `self.scene`'s own
    /// letterbox (`SceneConfig::window_to_scene`) and remembers it — called on every mouse move,
    /// whether or not the active screen is live, so a move made during a pause is still known once
    /// the world starts stepping again (требование 26).
    pub fn update_cursor(&mut self, window_pos: [f32; 2], viewport: [f32; 2]) {
        self.set_cursor_cell(self.scene.window_to_scene(window_pos, viewport));
    }

    /// The same remembering `update_cursor` does, already in scene coordinates — «Тесты по
    /// записанному вводу», требование 31: a replay's own `cursor` is given in scene cells
    /// directly, bypassing the window-pixel translation that has its own, separate tests.
    pub fn set_cursor_cell(&mut self, cell: Vec2) {
        self.cursor_current = Some(cell);
    }

    pub fn take_input_snapshot(&mut self) -> StepInput {
        let mut snapshot = self.input_queue.take_snapshot();
        if self.cursor_current.is_some() && self.cursor_current != self.cursor_last_step {
            snapshot.cursor = self.cursor_current;
            self.cursor_last_step = self.cursor_current;
        }
        snapshot
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
        self.ensure_code_loaded();
        self.step_count += 1;
        self.ensure_scratch_capacity();

        self.apply_to_world(&input.events);
        step::apply_follow_mouse(&mut self.world, input.cursor, &self.scene);

        let expired =
            step::tick_counters(&mut self.world, &mut self.grid_hop_ready, &self.properties);

        let pre_move_positions: Vec<Option<Vec2>> = (0..self.world.slot_count() as u32)
            .map(|id| self.world.vec2(id, property::POSITION))
            .collect();

        for moved in self.moved.iter_mut() {
            *moved = false;
        }
        let mut outcome_flag = None;
        let mut stage4_deleted: Vec<u32> = expired.clone();
        // «Звук»: этому и только этому обёртка `SoundMarks` даётся — поднять
        // отметку и никогда её не прочитать; окно целиком (`self.sound_window`) шаг не видит.
        let mut marks = self.sound_window.marks();
        let mut code_env = match self.code.as_mut() {
            Some(runner) => {
                // «Код игры»: предел операций — на все вызовы `run` этого шага вместе, не на
                // каждый по отдельности (see `code::Runner::reset_step_budget`).
                runner.reset_step_budget();
                Some(step::CodeEnv {
                    runner,
                    messages: &mut self.messages,
                })
            }
            None => None,
        };
        if let Err(err) = step::apply_stage4(
            &self.rules.rules,
            &mut self.world,
            &self.scene,
            &mut self.grid_hop_ready,
            &mut self.moved,
            &mut stage4_deleted,
            &mut self.grid,
            &mut marks,
            &mut code_env,
            &mut self.rng,
            &mut outcome_flag,
        ) {
            let mut err = err;
            err.step = Some(self.step_count);
            self.code_error = Some(err);
            return;
        }

        let pairs = step::find_collision_pairs(&self.world, &mut self.grid);

        for bounced in self.bounced.iter_mut() {
            *bounced = false;
        }
        let mut deletes_from_collide = Vec::new();
        if let Err(err) = step::apply_collide_rules(
            &self.rules.rules,
            &mut self.world,
            &pairs,
            &mut self.bounced,
            &mut deletes_from_collide,
            &mut self.moved,
            &mut outcome_flag,
            &mut marks,
            &mut code_env,
            &mut self.rng,
            &stage4_deleted,
            &mut self.grid,
        ) {
            // «Код игры»: ошибка во время партии останавливает игру на месте — остаток шага не
            // выполняется, `is_running` теперь тоже видит `code_error`. `step` — единственное
            // поле «Код игры» → требование 20 не заполняет ниже: только `Game` знает номер шага.
            let mut err = err;
            err.step = Some(self.step_count);
            self.code_error = Some(err);
            return;
        }

        let already_deleted: Vec<u32> = stage4_deleted
            .into_iter()
            .chain(deletes_from_collide)
            .collect();
        let mut random_cell_exhausted = false;
        let (mut all_deleted, creates) = match step::queue_create_and_delete_rules(
            &self.rules.rules,
            &mut self.world,
            &self.properties,
            &self.scene,
            &already_deleted,
            &mut self.moved,
            &pre_move_positions,
            &mut self.rng,
            &mut outcome_flag,
            &mut random_cell_exhausted,
            &mut marks,
            &mut code_env,
            &mut self.grid,
        ) {
            Ok(result) => result,
            Err(err) => {
                let mut err = err;
                err.step = Some(self.step_count);
                self.code_error = Some(err);
                return;
            }
        };
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

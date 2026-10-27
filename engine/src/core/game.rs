use std::cell::{Ref, RefCell};

use super::camera::{
    self, Camera3d, CameraHeight, EditorCamera, FlatEditorCamera, FreeView, PlayerCamera,
};
use super::code::{self, CodeCreate, CodeError, ObjectParser};
use super::grid::SpatialGrid;
use super::input::{InputQueue, KeyAction, KeyEvent, StepInput};
use super::math3::Vec3;
use super::motion;
use super::pathfind::WalkCaches;
use super::property::{self, PropertyId, PropertyTable};
use super::report::{CreatedObject, DeleteCause, StepReport, StepReportBuilder};
use super::rng::Rng;
use super::road_kinds::RoadKindLayout;
use super::roads::{RoadLinks, RoadState};
use super::rules::{Outcome, RuleSet};
use super::scene::{self, GroundLayer, ObjectSpec, SceneConfig, letterbox};
use super::sound::SoundWindow;
use super::step;
use super::surface;
use super::terrain::{Cover, Terrain};
use super::value::{Value, Vec2};
use super::world::World;

/// «Мир на экране» → «Камера игрока», требование 20: вид камеры игрока, как он идёт в запись партии.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlayerView {
    pub center: Vec2,
    /// Высота точки взгляда; только в трёхмерной сцене. Без неё повтор ставит камеру на землю под точкой.
    pub z: Option<f64>,
    pub view_height: f64,
    pub follow: bool,
}

#[derive(Debug)]
pub struct Game {
    pub properties: PropertyTable,
    pub world: World,
    pub rules: RuleSet,
    pub scene: SceneConfig,
    pub max_objects: usize,
    /// «Мир на экране» → «Земля»: `scene.json → ground`, in list order — set once at load and
    /// never touched again (требование 19: земля не объект, правила и код её не видят, по ходу
    /// партии она не меняется).
    pub ground: Vec<GroundLayer>,

    /// «Ветер и частицы» → «Ветер»: `scene.json → wind` — ветер, с которым собирается каждый мир.
    scene_wind: Vec2,
    /// Ровный ветер живой игры: ветер файла, пока его не поменяли (`set_wind`); сборка мира берёт
    /// его из сцены заново.
    wind: Vec2,

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
    /// «Таблицы данных», требования 27, 29: разобранный JSON `files.tables`, в порядке объявления
    /// — хранится тут, а не только внутри `code`, чтобы каждая партия (`load_code`) строила
    /// глобальную `tables` заново из исходных данных, а не из того, что успел переписать код
    /// прошлой партии.
    tables: Vec<(String, serde_json::Value)>,
    /// «Создание объектов кодом»: как запись, что код передал в `create`, становится объектом.
    object_parser: ObjectParser,
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
    cursor_current: Option<Vec3>,
    /// «Трёхмерная сцена», требование 23: где стояла камера, когда курсор пришёл в `cursor_current` —
    /// щелчок идёт лучом от неё через эту точку земли. `None` — плоская сцена или курсор без камеры.
    cursor_eye: Option<Vec3>,
    /// The position the most recent step actually took — compared against `cursor_current` at
    /// `take_input_snapshot` time to decide whether *this* step gets a cursor at all.
    cursor_last_step: Option<Vec3>,

    /// «Редактор»: whether a play/replay session is open — `Game::begin_session` sets it,
    /// `Game::end_session` clears it. Gates the report/message-log bookkeeping below so the plain
    /// page (which never opens a session) pays for none of it.
    session_active: bool,
    /// «Редактор», требование 6: steps done since `begin_session`, across every `new_game` in this
    /// session — unlike `step_count`, `new_game_with_values` never resets this.
    session_step: u64,
    /// «Редактор», требование 26, 45: every `print`/warning message that appeared during this
    /// session, each tagged with the session step it appeared on.
    session_messages: Vec<(u64, String)>,
    /// «Редактор», требование 23, 44: the last step's report — `None` outside a session, or before
    /// the session's first step.
    last_report: Option<StepReport>,
    /// «Редактор», требования 8, 13: whether a world currently exists — explicit rather than
    /// `world.alive_count() > 0`, so a live world every rule has emptied out (score run away and
    /// deleted everything) doesn't read as "Мира нет" alongside a genuinely unstarted one. Set by
    /// `reset_for_play`, `new_game_with_values`, `show_scene` and `quit`, the only places that
    /// rebuild or empty `world`.
    world_exists: bool,

    /// «Камера», требования 5–6: последняя известная середина объекта камеры — держит её, пока
    /// такого объекта нет; сбрасывается при сборке мира (`new_game_with_values`, `show_scene`,
    /// `reset_for_play`), а не при каждом шаге.
    camera_last: Option<Vec2>,
    /// «Рельеф» → «Камера», требование 24: высота, на которой камера игры держит взгляд.
    camera_height: CameraHeight,
    /// «Ходьба», требование 28: запомненные пути идущих объектов — вне мира, сброшены там же, где
    /// `camera_last`.
    walk_paths: WalkCaches,
    /// «Езда по дорогам»: сеть дорог и пути машин — вне мира, сброшены там же, где пути идущих.
    roads: RoadState,
    /// «Вид дороги»: раскладка кусков и перекрёстков по дорогам мира — кэш рисования, он пересобирается при показе
    /// кадра (`road_kind_layout`), а не на шаге, потому что редактор правит дороги и вне партии.
    road_view: RefCell<RoadKindLayout>,
    /// «Мир на экране» → «Камера игрока»: вид камеры, которой водит игрок; `None`, пока у сцены нет
    /// `player_camera`. Сброшен там же, где `camera_last`.
    player: Option<PlayerCamera>,
    /// Сколько записей `camera_follows` мир насчитал к тому моменту, когда камера в последний раз
    /// решала, следить ли ей: новая запись — слежение возвращается.
    follow_writes_seen: u64,
    /// Требование 11: клавиши, названные в `keys` объектов `scene.json`, — камера игрока их не берёт.
    taken_keys: std::collections::HashSet<String>,
    /// «Редактор», «Сцена»: камера редактора трёхмерной сцены — последняя, что прислал редактор;
    /// `None`, пока не присылал. Сборка мира её не сбрасывает.
    editor_camera: Option<EditorCamera>,
    /// «Редактор», «Сцена»: то же для плоской сцены.
    flat_editor_camera: Option<FlatEditorCamera>,
}

/// «Экраны и состояние» / «Редактор», требование 16: builds a fresh world from `scene_objects` —
/// one object per entry, in file order (`World::create` hands out slots in call order and nothing
/// here ever deletes one, so `scene_objects[n]` always lands in slot `n`), its `values`/`grid`/
/// `keys` written — with no other new-game side effect (no rng, step counter, input, code).
/// Shared by `new_game_with_values`, `show_scene` and `data::load::load_rest`'s own initial world.
/// «Рельеф», требование 8: сначала встают настилы, потом остальные объекты; тот, у кого в данных
/// есть третье число `position`, стоит на нём ровно.
pub(crate) fn world_from_scene(properties: &PropertyTable, scene_objects: &[ObjectSpec]) -> World {
    let mut world = World::new(properties);
    let mut seated_by_data = Vec::with_capacity(scene_objects.len());
    world.suspend_roads();
    for spec in scene_objects {
        let id = world.create();
        seated_by_data.push(fill_object(&mut world, id, spec));
    }
    world.settle_roads();
    if world.three_d() {
        seat_scene(&mut world, &seated_by_data);
    }
    world
}

/// Пишет в свежий объект `id` значения, `grid`, `keys` и `on_click` записи `spec` — общее у объекта
/// сцены и объекта, созданного кодом. `true`, если высоту основания задали данные (третье число
/// `position`).
fn fill_object(world: &mut World, id: u32, spec: &ObjectSpec) -> bool {
    let seated_by_data = spec
        .values
        .iter()
        .any(|(prop, value)| *prop == property::POSITION && matches!(value, Value::Vec3(_)));
    for (prop, value) in placed_last(&spec.values) {
        world.set_value(id, prop, value);
    }
    if let Some(grid) = &spec.grid {
        world.set_grid(id, property::GRID, *grid);
        world.set_grid_counter(id, grid.interval_steps);
    }
    if let Some(keys) = &spec.keys {
        world.set_keys(id, property::KEYS, keys.clone());
    }
    if let Some(on_click) = &spec.on_click {
        world.set_on_click(id, property::ON_CLICK, on_click.clone());
    }
    seated_by_data
}

/// Значения объекта, `position` последним: пока места нет, ни поворот, ни размер объект не пересаживают.
pub(crate) fn placed_last(
    values: &[(PropertyId, Value)],
) -> impl Iterator<Item = (PropertyId, &Value)> {
    values
        .iter()
        .filter(|(prop, _)| *prop != property::POSITION)
        .chain(
            values
                .iter()
                .filter(|(prop, _)| *prop == property::POSITION),
        )
        .map(|(prop, value)| (*prop, value))
}

/// «Рельеф», требование 8: настилы без `z` в данных — низом на самую высокую точку рельефа под ними,
/// потом остальные объекты без `z` — на верх самого высокого настила под ними или на рельеф.
fn seat_scene(world: &mut World, seated_by_data: &[bool]) {
    let ids: Vec<u32> = world.ids().collect();
    for deck in [true, false] {
        for &id in &ids {
            if world.flag(id, property::DECK) == deck && !seated_by_data[id as usize] {
                surface::seat_fresh(world, id);
            }
        }
    }
}

impl Game {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        properties: PropertyTable,
        world: World,
        rules: RuleSet,
        scene: SceneConfig,
        max_objects: usize,
        ground: Vec<GroundLayer>,
        scene_wind: Vec2,
        random_seed: u64,
        scene_objects: Vec<ObjectSpec>,
        sound_count: usize,
        code_source: Option<String>,
        code_path: String,
        image_names: Vec<String>,
        sound_names: Vec<String>,
        start_is_live: bool,
        tables: Vec<(String, serde_json::Value)>,
        object_parser: ObjectParser,
    ) -> Self {
        let taken_keys = scene_objects
            .iter()
            .filter_map(|spec| spec.keys.as_ref())
            .flat_map(|keys| keys.keys().cloned())
            .collect();
        let mut game = Game {
            properties,
            world,
            rules,
            scene,
            max_objects,
            ground,
            scene_wind,
            wind: scene_wind,
            scene_objects,
            random_seed,
            rng: Rng::new(random_seed),
            step_count: 0,
            input_queue: InputQueue::new(),
            code_source,
            code_path,
            image_names,
            sound_names,
            tables,
            object_parser,
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
            cursor_eye: None,
            cursor_last_step: None,
            session_active: false,
            session_step: 0,
            session_messages: Vec::new(),
            last_report: None,
            world_exists: start_is_live,
            camera_last: None,
            camera_height: CameraHeight::default(),
            walk_paths: WalkCaches::new(),
            roads: RoadState::new(),
            road_view: RefCell::default(),
            player: None,
            follow_writes_seen: 0,
            taken_keys,
            editor_camera: None,
            flat_editor_camera: None,
        };
        // «Код игры» → «Экраны и состояние»: код грузится ровно один раз на партию. Партия
        // начинается либо прямо здесь (стартовый экран без меню — `world_runs` поднят сразу), и
        // тогда код выполняется здесь и только здесь, либо позже, кнопкой `new_game` (стартовый
        // экран — меню): тогда партии здесь ещё нет, и код грузится позже, в `new_game`, а не
        // дважды — здесь и там.
        if start_is_live {
            game.load_code();
        }
        game.update_camera();
        game.reset_player_camera();
        game.prepare_walking();
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
            &self.tables,
            self.roads.network_handle(),
            self.object_parser.clone(),
        ) {
            Ok(runner) => self.code = Some(runner),
            Err(err) => self.code_error = Some(err),
        }
    }

    pub fn key_down(&mut self, code: &str) {
        self.input_queue.press(code);
    }

    /// «Надписи, полоски и картинки в мире», требование 6: то же для нажатия, нацеленного на объект
    /// `target`, — щелчок по его картинке в мире.
    pub fn key_down_on(&mut self, code: &str, target: u32) {
        self.input_queue.press_on(code, Some(target));
    }

    pub fn key_up(&mut self, code: &str) {
        self.input_queue.release(code);
    }

    /// Whether the world currently holds `code` — a step actually applied its `Press` and no step
    /// (nor `release_key`) has applied a matching `Release` since. Deliberately not a question
    /// about `InputQueue`'s own bookkeeping: what the page has queued can disagree with what the
    /// world has seen, and this answers about the world. See `release_key`.
    pub(crate) fn is_key_held(&self, code: &str) -> bool {
        self.world_held_keys.contains(code)
    }

    /// Runs `events` against the world and keeps `world_held_keys` in lockstep — the only place
    /// either happens, so the two can never drift apart. `step`, `release_held_keys` and
    /// `release_key` all go through this rather than calling `step::apply_input` on their own.
    fn apply_to_world(&mut self, events: &[KeyEvent]) {
        step::apply_input(
            &mut self.world,
            events,
            self.cursor_current,
            self.cursor_eye,
            &self.scene,
        );
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
    /// Returns the codes actually released (sorted), so a caller that records input — «Пауза»,
    /// требование 40 — can put each release into the recording the same way a screen-absorbed one
    /// already does; a caller that doesn't (a live screen switch) simply ignores it.
    pub fn release_held_keys(&mut self) -> Vec<String> {
        // «Исполнение игры» → «Повторяемость»: the same input must give the same run on any
        // machine — a `HashSet`'s own iteration order isn't that, so the codes are sorted before
        // two releases that write the same property differently can disagree on which wins.
        let mut codes: Vec<String> = self.world_held_keys.iter().cloned().collect();
        codes.sort();
        let events: Vec<KeyEvent> = codes
            .iter()
            .map(|code| KeyEvent {
                code: code.clone(),
                action: KeyAction::Release,
                target: None,
            })
            .collect();
        self.apply_to_world(&events);
        self.input_queue.clear();
        codes
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
                target: None,
            }));
        }
        self.input_queue.forget(code);
    }

    /// Symmetric with `release_key`, for «Повтор»: a replay applies its own recorded press/release
    /// events one at a time, in the order they were recorded, interleaved with edits and commands
    /// that touch the world synchronously — going through `key_down`'s own queue would apply this
    /// step's presses only once `step` runs, after every edit/command of the same step already has,
    /// regardless of where the press actually fell among them when it was recorded.
    pub fn press_key(&mut self, code: &str) {
        self.press_key_aimed(code, None);
    }

    /// «Надписи, полоски и картинки в мире», требование 7: то же для нажатия, нацеленного на объект
    /// `target` — повтор щелчка по картинке попадает в тот же объект.
    pub fn press_key_on(&mut self, code: &str, target: u32) {
        self.press_key_aimed(code, Some(target));
    }

    fn press_key_aimed(&mut self, code: &str, target: Option<u32>) {
        if !self.is_key_held(code) {
            self.apply_to_world(std::slice::from_ref(&KeyEvent {
                code: code.to_string(),
                action: KeyAction::Press,
                target,
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
        self.world_exists = true;
        self.wind = self.scene_wind;
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
        self.reset_camera_and_walk();
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
        self.world_exists = true;
        self.wind = self.scene_wind;
        self.reset_camera_and_walk();
    }

    /// «Редактор», «Вызовы движка», `set_terrain`: ставит рельеф так, будто он прочитан из файла, и собирает
    /// мир из сцены заново — объекты без `z` в данных садятся по нему. Сети поиска пути прежнего рельефа
    /// отбрасываются и не строятся заново: их строит «Запуск». Камера редактора не меняется.
    pub fn set_terrain(&mut self, terrain: Terrain) {
        self.properties.set_terrain(terrain);
        self.world = world_from_scene(&self.properties, &self.scene_objects);
        self.world_exists = true;
        self.forget_camera_and_paths();
        self.walk_paths.clear_navigation();
    }

    /// «Редактор», «Вызовы движка», `set_covers`: ставит слои покрытий `covers` на нынешний рельеф; высоты,
    /// отпечатки, вода, карта цвета и мир остаются, камера не двигается. Без рельефа сцены не делает ничего
    /// и говорит `false`.
    pub fn set_covers(&mut self, covers: Vec<Cover>) -> bool {
        let Some(current) = self.properties.terrain() else {
            return false;
        };
        let terrain = current
            .as_ref()
            .clone()
            .with_covers(covers, current.has_tint());
        self.properties.set_terrain(terrain);
        self.world
            .replace_terrain(self.properties.terrain().cloned());
        true
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
        self.world_exists = false;
        self.input_queue = InputQueue::new();
        self.world_held_keys.clear();
        self.outcome = None;
        // «Код игры»: «quit выбрасывает исполнитель вместе с миром» — never rebuilt here; the
        // next `new_game` (not `quit`) is the one that loads a fresh one.
        self.code = None;
        self.code_error = None;
        self.camera_release_input();
    }

    /// «Редактор», требование 39: rebuilds the world exactly the way `data::load::load_rest`
    /// built it for a fresh `load()` — live if `start_is_live` (the caller's own start screen),
    /// empty otherwise — and resets everything a brand-new engine load would start with: step
    /// count, rng, input, the sticky outcome, the once-only warnings, and the code runner. Unlike
    /// `new_game_with_values`, which always assumes a live target screen (checked at load time),
    /// this is the one place that also handles a non-live start screen, for «Запуск» itself.
    pub fn reset_for_play(&mut self, start_is_live: bool) {
        self.world = if start_is_live {
            world_from_scene(&self.properties, &self.scene_objects)
        } else {
            World::new(&self.properties)
        };
        self.world_exists = start_is_live;
        self.wind = self.scene_wind;
        self.step_count = 0;
        self.rng = Rng::new(self.random_seed);
        self.input_queue = InputQueue::new();
        self.world_held_keys.clear();
        self.outcome = None;
        // «Курсор в мире»: в отличие от `new_game_with_values` (партия продолжается, мышь не
        // «телепортировалась»), здесь начинается новая партия/повтор/переход по шкале — курсор от
        // прошлой партии не должен попасть в первый шаг новой записи, которая его не видела.
        self.cursor_current = None;
        self.cursor_eye = None;
        self.cursor_last_step = None;
        self.reset_camera_and_walk();
        self.max_objects_warned = false;
        self.random_cell_warned = false;
        self.code = None;
        self.code_error = None;
        if start_is_live {
            self.load_code();
        }
    }

    /// «Камера», требование 5; «Ходьба», требование 28: shared by every place that rebuilds
    /// `world` from the scene — the camera forgets the point it was following and every
    /// remembered walk path goes with it, since both describe a world that no longer exists.
    fn reset_camera_and_walk(&mut self) {
        self.forget_camera_and_paths();
        if self.scene.is_3d() {
            self.prepare_walking();
        }
    }

    fn forget_camera_and_paths(&mut self) {
        self.camera_last = None;
        self.camera_height = CameraHeight::default();
        self.walk_paths.clear_paths();
        self.roads.clear();
        if self.scene.is_3d() || self.scene.player_camera.is_some() {
            self.update_camera();
        }
        self.reset_player_camera();
    }

    /// «Рельеф» → «Ходьба», нефункциональные требования: идущим по рельефу и настилам готовится всё, что
    /// не зависит от цели, при сборке мира, а не на первом пути.
    fn prepare_walking(&mut self) {
        step::prepare_walkers(
            &self.world,
            &self.rules.rules,
            &self.scene,
            &mut self.walk_paths,
        );
    }

    /// «Камера», требования 5–7: recomputes the camera's own followed point from the world right
    /// now — sticky when no `camera_follows` object exists. Called after the editor's own live-world
    /// edits, «без отставания»: камера сразу стоит на объекте, без плавного прохода по высоте.
    pub fn update_camera(&mut self) {
        if let Some((center, z)) = camera::followed_point(&self.world) {
            self.camera_last = Some(center);
            self.camera_height.snap(z);
        }
        self.follow_again_if_written();
    }

    /// «Рельеф» → «Камера», требование 24: то же после шага, но скачок основания на ступеньке камера
    /// проходит за 12 шагов, а не сразу.
    fn step_camera(&mut self) {
        if let Some((center, z)) = camera::followed_point(&self.world) {
            let moved = self.camera_last.map_or(0.0, |previous| {
                ((center[0] - previous[0]).powi(2) + (center[1] - previous[1]).powi(2)).sqrt()
            });
            self.camera_last = Some(center);
            self.camera_height.step(z, moved);
        }
        self.follow_again_if_written();
    }

    fn scene_middle(&self) -> Vec2 {
        [
            self.scene.width as f64 / 2.0,
            self.scene.height as f64 / 2.0,
        ]
    }

    /// «Мир на экране» → «Камера игрока», требования 3–4: камера при сборке мира — на объекте слежения,
    /// а без него стоит на `start` или в середине сцены, вид — `view_height` из `game.json`.
    fn reset_player_camera(&mut self) {
        self.follow_writes_seen = self.world.camera_follow_writes();
        self.player = self.scene.player_camera.map(|config| {
            let free = camera::followed_point(&self.world).is_none().then(|| {
                let center = config.start.unwrap_or_else(|| self.scene_middle());
                FreeView {
                    center,
                    z: self.world.terrain().height_at(center[0], center[1]),
                }
            });
            PlayerCamera::new(self.scene.view_height.unwrap_or(config.zoom[0]), free)
        });
    }

    /// Требование 16: объекту записали `camera_follows` — камера снова следит за объектом, если он
    /// есть, с этого шага.
    fn follow_again_if_written(&mut self) {
        let writes = self.world.camera_follow_writes();
        if writes == self.follow_writes_seen {
            return;
        }
        self.follow_writes_seen = writes;
        if camera::followed_point(&self.world).is_some()
            && let Some(player) = self.player.as_mut()
        {
            player.set_free(None);
        }
    }

    /// Вид и место, на которые камера смотрит сейчас, до прижатия к краю сцены: у камеры игрока,
    /// которую сдвинули, — её место, иначе последняя точка слежения или середина сцены.
    fn camera_state(&self) -> (Option<f64>, Vec2, f64) {
        let followed = (
            self.camera_last.unwrap_or_else(|| self.scene_middle()),
            self.camera_height.z(),
        );
        match &self.player {
            Some(player) => {
                let (center, z) = player.free().map_or(followed, |free| (free.center, free.z));
                (Some(player.view_height()), center, z)
            }
            None => (self.scene.view_height, followed.0, followed.1),
        }
    }

    /// Середина камеры, не заглядывающей за край сцены при виде `view_height`.
    fn clamp_center(&self, view_height: f64, viewport: [f64; 2], center: Vec2) -> Vec2 {
        match self.scene.camera {
            Some(camera) => {
                camera::clamp_center_3d(&self.scene, camera.pitch, view_height, viewport, center)
            }
            None => camera::clamp_center_flat(&self.scene, view_height, viewport, center),
        }
    }

    /// «Трёхмерная сцена», требования 2–5: камера сверху под углом на середину followed-объекта,
    /// остановленная у края сцены; `None` в плоской сцене. «Мир на экране» → «Камера игрока»: камера
    /// игрока смотрит с её видом и места, а клетка надписей остаётся по `view_height` из `game.json`.
    pub fn camera_3d(&self, viewport: [f32; 2]) -> Option<Camera3d> {
        let pitch = self.scene.camera?.pitch;
        let base_view_height = self.scene.view_height?;
        let viewport = [viewport[0] as f64, viewport[1] as f64];
        let (view_height, center, z) = self.camera_state();
        let view_height = view_height.unwrap_or(base_view_height);
        let center = self.clamp_center(view_height, viewport, center);
        Some(
            Camera3d::looking_at(center, pitch, view_height, viewport)
                .raised(z)
                .with_label_view_height(base_view_height),
        )
    }

    /// «Камера», требование 4: the scale/offset «Мир на экране» draws and picks the world by
    /// right now — the followed point if one was ever known, the scene's own middle otherwise.
    pub fn camera_frame(&self, viewport: [f32; 2]) -> (f32, [f32; 2]) {
        let (view_height, center, _) = self.camera_state();
        match view_height {
            Some(view_height) => camera::frame_at(&self.scene, view_height, center, viewport),
            None => camera::frame(&self.scene, center, viewport),
        }
    }

    /// «Камера игрока», требование 13: камерой водит игрок на любом экране, где мир есть.
    pub fn player_camera_works(&self) -> bool {
        self.player.is_some() && self.world_exists
    }

    /// Нынешний вид камеры игрока; `None`, пока у сцены нет `player_camera`.
    pub fn player_view_height(&self) -> Option<f64> {
        self.player.as_ref().map(PlayerCamera::view_height)
    }

    /// Следит ли камера игрока за объектом; `None` без камеры игрока.
    pub fn player_camera_follows(&self) -> Option<bool> {
        self.player.as_ref().map(|player| player.free().is_none())
    }

    /// Место на сцене под точкой окна `pointer`: в плоской сцене сама точка, в трёхмерной — место на
    /// земле, воде или настиле под ней на его высоте.
    fn scene_point_under(&self, pointer: [f32; 2], viewport: [f32; 2]) -> Vec3 {
        match self.camera_3d(viewport) {
            Some(camera) => scene::pointer_hit(
                &self.world,
                &self.scene,
                &camera,
                [pointer[0] as f64, pointer[1] as f64],
            )
            .unwrap_or([camera.target[0], camera.target[1], camera.target_z]),
            None => {
                let (scale, offset) = self.camera_frame(viewport);
                let point = self.scene.window_to_scene_frame(pointer, scale, offset);
                [point[0], point[1], 0.0]
            }
        }
    }

    /// Ставит камеру игрока на место `center` с высотой `z`: слежение отпущено.
    fn set_player_free(&mut self, center: Vec2, z: f64) {
        if let Some(player) = self.player.as_mut() {
            player.set_free(Some(FreeView { center, z }));
        }
    }

    /// Требование 5–7: колесо на `delta_y` точек у точки окна `pointer`. Вид меняется, место под
    /// указателем остаётся под ним; пока камера следит, объект остаётся в середине окна. `true` —
    /// вид изменился.
    pub fn camera_wheel(&mut self, delta_y: f64, pointer: [f32; 2], viewport: [f32; 2]) -> bool {
        let (Some(config), Some(player), true) = (
            self.scene.player_camera,
            self.player.as_ref(),
            self.world_exists,
        ) else {
            return false;
        };
        let current = player.view_height();
        let next = camera::zoomed_height(current, delta_y, config.zoom);
        if !delta_y.is_finite() || next == current {
            return false;
        }
        let Some(free) = player.free() else {
            if let Some(player) = self.player.as_mut() {
                player.set_view_height(next);
            }
            return true;
        };
        let viewport64 = [viewport[0] as f64, viewport[1] as f64];
        let anchor = self.scene_point_under(pointer, viewport);
        let pointer64 = [pointer[0] as f64, pointer[1] as f64];
        let (center, z) = match self.camera_3d(viewport) {
            Some(camera) => {
                let target = [camera.target[0], camera.target[1], camera.target_z];
                let moved = camera::dolly_target(target, anchor, next / current);
                ([moved[0], moved[1]], moved[2])
            }
            None => (
                camera::flat_center_under([anchor[0], anchor[1]], pointer64, next, viewport64),
                free.z,
            ),
        };
        let center = self.clamp_center(next, viewport64, center);
        self.set_player_free(center, z);
        if let Some(player) = self.player.as_mut() {
            player.set_view_height(next);
        }
        true
    }

    /// Требование 9: правая или средняя кнопка нажата над миром у точки окна `pointer` — схватить место
    /// сцены под указателем.
    pub fn camera_grab(&mut self, pointer: [f32; 2], viewport: [f32; 2]) -> bool {
        if !self.player_camera_works() {
            return false;
        }
        let anchor = self.scene_point_under(pointer, viewport);
        if let Some(player) = self.player.as_mut() {
            player.set_grab(Some(anchor));
        }
        true
    }

    /// Указатель сдвинулся, пока карту держат: схваченное место остаётся под ним, слежение отпущено.
    /// `true` — камера сдвинулась.
    pub fn camera_drag(&mut self, pointer: [f32; 2], viewport: [f32; 2]) -> bool {
        let Some((anchor, view_height)) = self
            .player
            .as_ref()
            .and_then(|player| Some((player.grab()?, player.view_height())))
        else {
            return false;
        };
        let viewport64 = [viewport[0] as f64, viewport[1] as f64];
        let pointer64 = [pointer[0] as f64, pointer[1] as f64];
        let (center, z) = match self.camera_3d(viewport) {
            Some(camera) => match camera::dragged_target(&camera, anchor, pointer64) {
                Some(center) => (center, camera.target_z),
                None => return false,
            },
            None => (
                camera::flat_center_under(
                    [anchor[0], anchor[1]],
                    pointer64,
                    view_height,
                    viewport64,
                ),
                0.0,
            ),
        };
        let center = self.clamp_center(view_height, viewport64, center);
        self.set_player_free(center, z);
        true
    }

    /// Кнопку отпустили: карту больше не держат.
    pub fn camera_release(&mut self) {
        if let Some(player) = self.player.as_mut() {
            player.set_grab(None);
        }
    }

    /// Требование 11: берёт ли камера игрока клавишу `code` — это её клавиша, мир есть и ни у одного
    /// объекта `scene.json` она не названа в `keys`. Клавишу активного экрана проверяет вызывающий.
    pub fn camera_takes_key(&self, code: &str) -> bool {
        self.player_camera_works() && camera::is_pan_key(code) && !self.taken_keys.contains(code)
    }

    /// Камера берёт нажатую клавишу, если может; `true` — взяла, и в мир клавиша не идёт.
    pub fn camera_key_down(&mut self, code: &str) -> bool {
        self.camera_takes_key(code)
            && self
                .player
                .as_mut()
                .is_some_and(|player| player.key_down(code))
    }

    /// Камера отпускает клавишу, которую держала; `true` — держала, и в мир отпускание не идёт.
    pub fn camera_key_up(&mut self, code: &str) -> bool {
        self.player
            .as_mut()
            .is_some_and(|player| player.key_up(code))
    }

    /// Камера забывает зажатые клавиши и схваченное место: пауза, спрятанная вкладка.
    pub fn camera_release_input(&mut self) {
        if let Some(player) = self.player.as_mut() {
            player.release_keys();
            player.set_grab(None);
        }
    }

    /// Требования 10, 12: зажатые клавиши камеры сдвигают её за `seconds` настоящего времени кадра на
    /// 0,6 видимой высоты в секунду; слежение отпущено. `true` — камера сдвинулась.
    pub fn camera_pan(&mut self, seconds: f64, viewport: [f32; 2]) -> bool {
        let Some(player) = self.player.as_ref().filter(|_| self.world_exists) else {
            return false;
        };
        let direction = player.pan_direction();
        if direction == [0.0, 0.0] || !seconds.is_finite() || seconds <= 0.0 {
            return false;
        }
        let view_height = player.view_height();
        let viewport64 = [viewport[0] as f64, viewport[1] as f64];
        let (_, center, z) = self.camera_state();
        let center = self.clamp_center(view_height, viewport64, center);
        let step = camera::PAN_VIEW_SHARE_PER_SECOND * view_height * seconds;
        let moved = [
            center[0] + direction[0] * step,
            center[1] + direction[1] * step,
        ];
        let moved = self.clamp_center(view_height, viewport64, moved);
        self.set_player_free(moved, z);
        true
    }

    /// Требование 20: вид камеры игрока для записи партии — место (в трёхмерной сцене с высотой), вид и
    /// следит ли она; `None` без камеры игрока.
    pub fn player_view(&self) -> Option<PlayerView> {
        let player = self.player.as_ref()?;
        let (_, center, z) = self.camera_state();
        Some(PlayerView {
            center,
            z: self.scene.is_3d().then_some(z),
            view_height: player.view_height(),
            follow: player.free().is_none(),
        })
    }

    /// Повтор записи ставит камеру игрока по записанному виду; слежение — как в записи.
    pub fn set_player_view(&mut self, view: PlayerView) {
        let z = view.z.unwrap_or_else(|| {
            self.world
                .terrain()
                .height_at(view.center[0], view.center[1])
        });
        if let Some(player) = self.player.as_mut() {
            player.set_view_height(view.view_height);
            player.set_free((!view.follow).then_some(FreeView {
                center: view.center,
                z,
            }));
        }
    }

    /// «Редактор», «Правка сцены», требование 18: ставит объект `id` на `position` в живом мире.
    /// Камера игры встаёт на объект без отставания только в плоской сцене: в трёхмерной ручку или
    /// объект тянут под указателем, и камера, ушедшая вслед за объектом, увела бы землю из-под него.
    pub fn move_object(&mut self, id: u32, position: Vec2, z: Option<f64>) {
        super::scene::move_object(&mut self.world, id, position, z);
        if !self.scene.is_3d() {
            self.update_camera();
        }
    }

    /// «Езда по дорогам»: у скольких машин запомнен путь.
    pub fn road_path_count(&self) -> usize {
        self.roads.path_count()
    }

    /// «Вид дороги»: раскладка кусков и перекрёстков по нынешним дорогам мира — собирается заново, только когда дороги
    /// сменились (требование 17), иначе отдаётся готовая. Без видов в игре дороги не просматриваются. Пока
    /// кто-то держит раскладку, она уже свежа с его вызова, и повторный вызов её не сверяет.
    pub fn road_kind_layout(&self) -> Ref<'_, RoadKindLayout> {
        let kinds = self.world.road_kinds();
        if !kinds.is_empty()
            && let Ok(mut layout) = self.road_view.try_borrow_mut()
        {
            layout.sync_with(kinds, self.world.models(), |roads| {
                self.world.collect_layout_roads(roads)
            });
        }
        self.road_view.borrow()
    }

    /// «Редактор», «Вызовы движка», `road_links`: связи дороги `id` по нынешнему миру — сеть сверяется
    /// с дорогами мира и собирается заново, если их правили. `None`, если `id` не дорога.
    pub fn road_links(&mut self, id: u32) -> Option<RoadLinks> {
        self.roads
            .sync_with(|shapes, speeds| self.world.collect_roads(shapes, speeds));
        self.roads.links_of(id)
    }

    /// «Редактор», «Сцена»: запоминает камеру редактора; наклон и расстояние приводятся к
    /// допустимым.
    pub fn set_editor_camera(&mut self, camera: EditorCamera) {
        self.editor_camera = Some(camera.clamped());
    }

    /// «Редактор», «Сцена»: камера редактора — присланная или та, что видит всю землю; `None` в
    /// плоской сцене.
    fn current_editor_camera(&self, viewport: [f64; 2]) -> Option<EditorCamera> {
        self.editor_camera
            .or_else(|| camera::fit_ground(&self.scene, self.world.terrain(), viewport))
    }

    /// «Редактор», «Сцена», требование 1: камера, которой трёхмерная сцена видна вне партии. Она
    /// вращается вокруг точки `target` на высоте `target_z`, которую прислал редактор.
    pub fn editor_camera_3d(&self, viewport: [f32; 2]) -> Option<Camera3d> {
        let viewport = [viewport[0] as f64, viewport[1] as f64];
        let editor = self.current_editor_camera(viewport)?;
        Some(editor.camera(viewport))
    }

    /// «Редактор», «Вызовы движка», `fit_camera`: без номера — камера, что видит всю землю под
    /// углом камеры игры; с номером — что видит объём объекта целиком при нынешних повороте и
    /// наклоне. `None` в плоской сцене и без объекта, его `position` и `size`.
    pub fn fit_camera(&self, id: Option<u32>, viewport: [f32; 2]) -> Option<EditorCamera> {
        let viewport = [viewport[0] as f64, viewport[1] as f64];
        let Some(id) = id else {
            return camera::fit_ground(&self.scene, self.world.terrain(), viewport);
        };
        let current = self.current_editor_camera(viewport)?;
        camera::fit_object(&self.world, id, current.yaw, current.pitch, viewport)
    }

    /// «Редактор», «Сцена»: запоминает камеру редактора плоской сцены.
    pub fn set_flat_editor_camera(&mut self, camera: FlatEditorCamera) {
        self.flat_editor_camera = Some(camera);
    }

    /// «Редактор», «Сцена»: камера редактора плоской сцены — присланная или та, что видит всю сцену.
    fn current_flat_editor_camera(&self, viewport: [f64; 2]) -> FlatEditorCamera {
        self.flat_editor_camera
            .unwrap_or_else(|| camera::fit_flat_scene(&self.scene, viewport))
    }

    /// «Редактор», «Сцена», требования 1, 13: масштаб и сдвиг, которыми плоская сцена видна вне
    /// партии, — камера редактора, а пока её не прислали — вся сцена по её пропорциям.
    pub fn editor_flat_frame(&self, viewport: [f32; 2]) -> (f32, [f32; 2]) {
        match self.flat_editor_camera {
            Some(editor) => editor.frame(viewport),
            None => letterbox(
                viewport,
                [self.scene.width as f32, self.scene.height as f32],
            ),
        }
    }

    /// «Редактор», «Вызовы движка», `fit_camera`: в плоской сцене без номера — камера, что видит
    /// всю сцену, с номером — что видит место, где нарисован объект. `None` в трёхмерной сцене и
    /// без объекта, его `position` и `size`.
    pub fn fit_flat_camera(&self, id: Option<u32>, viewport: [f32; 2]) -> Option<FlatEditorCamera> {
        if self.scene.is_3d() {
            return None;
        }
        let viewport = [viewport[0] as f64, viewport[1] as f64];
        let Some(id) = id else {
            return Some(camera::fit_flat_scene(&self.scene, viewport));
        };
        let current = self.current_flat_editor_camera(viewport).center;
        camera::fit_flat_object(&self.world, &self.scene, id, current, viewport)
    }

    /// «Ветер и частицы» → «Ветер»: ровный ветер сейчас — `[0, 0]` без `wind` и в трёхмерной сцене.
    pub fn wind(&self) -> Vec2 {
        self.wind
    }

    /// «Редактор», «Вызовы движка», `set_wind`: ставит ровный ветер этому миру; наклоны и
    /// кадры качающихся объектов не сбрасываются. Ошибка по-русски — сцена трёхмерная.
    pub fn set_wind(&mut self, wind: Vec2) -> Result<(), String> {
        if self.scene.is_3d() {
            return Err("ветер есть только в плоской сцене".to_string());
        }
        self.wind = wind;
        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.outcome.is_none() && self.code_error.is_none()
    }

    /// «Исполнение игры» → «Шаг и кадр»: скорость времени мира — `time_scale` объекта с меньшим
    /// номером, у которого оно есть, от 0 до `MAX_TIME_SCALE`; ни у кого нет — 1.
    pub fn time_scale(&self) -> f64 {
        self.world
            .ids()
            .find_map(|id| self.world.number_like(id, property::TIME_SCALE))
            .map_or(1.0, |scale| {
                if scale.is_nan() {
                    1.0
                } else {
                    scale.clamp(0.0, property::MAX_TIME_SCALE)
                }
            })
    }

    /// Мир стоит, потому что скорость времени 0, а не потому что экран без `world_runs`.
    pub fn time_is_stopped(&self) -> bool {
        self.world_exists && self.time_scale() <= 0.0
    }

    /// «Экраны и состояние» → «Команды», `run`: вызывает функцию кода игры `function` между шагами —
    /// с объектом кнопки `object`, если он есть, — и сразу исполняет её заявки: удаления, потом создания.
    /// Работает на живом мире и на замершем; без мира, без кода и после ошибки кода не делает ничего.
    /// Предел операций и памяти — свой на этот вызов. Ошибка останавливает игру, как ошибка кода в шаге;
    /// `origin` называет экран и кнопку или клавишу, что вызвали функцию.
    pub fn run_screen_function(&mut self, function: &str, object: Option<u32>, origin: &str) {
        if !self.world_exists || self.code_error.is_some() {
            return;
        }
        self.ensure_code_loaded();
        self.ensure_scratch_capacity();
        let Some(runner) = self.code.as_mut() else {
            return;
        };
        runner.reset_step_budget();
        runner.set_wind(self.wind);
        let args: Vec<u32> = object
            .into_iter()
            .filter(|&id| self.world.is_alive(id))
            .collect();
        let messages_before = self.messages.len();
        let mut deletes = Vec::new();
        let result = runner.run(
            function,
            &args,
            &mut self.world,
            &mut self.rng,
            &mut deletes,
            &mut self.moved,
            self.sound_window.marks(),
            &mut self.messages,
        );
        let creates = runner.take_creates();
        let failure = result.err();
        if failure.is_none() {
            deletes.sort_unstable();
            deletes.dedup();
            for id in deletes {
                self.world.delete(id);
            }
            self.spawn_code_creates(creates, &mut None);
        }
        if self.session_active {
            for message in &self.messages[messages_before..] {
                self.session_messages
                    .push((self.session_step, message.clone()));
            }
        }
        if let Some(mut err) = failure {
            err.button = Some(origin.to_string());
            err.step = Some(self.step_count);
            self.code_error = Some(err);
            return;
        }
        self.update_camera();
    }

    /// Стоит ли мир на потолке `max_objects`: тогда новая заявка отбрасывается, а сообщение об этом —
    /// одно за партию.
    fn object_ceiling_reached(&mut self) -> bool {
        if self.world.alive_count() < self.max_objects {
            return false;
        }
        if !self.max_objects_warned {
            self.max_objects_warned = true;
            self.messages.push(format!(
                "достигнут потолок объектов ({}): заявка на создание отброшена",
                self.max_objects
            ));
        }
        true
    }

    /// «Создание объектов кодом»: исполняет заявки `create` — этап 8 после создания правил или сразу
    /// после функции кнопки. Объект без `position` — данные, с `position` в трёхмерной сцене без `z` —
    /// встаёт на поверхность, как объект `scene.json`.
    fn spawn_code_creates(
        &mut self,
        creates: Vec<CodeCreate>,
        report: &mut Option<StepReportBuilder>,
    ) {
        for create in creates {
            if self.object_ceiling_reached() {
                continue;
            }
            let id = self.world.create();
            if !fill_object(&mut self.world, id, &create.spec) {
                surface::seat_fresh(&mut self.world, id);
            }
            if let Some(r) = report.as_mut() {
                let name = create
                    .spec
                    .values
                    .iter()
                    .find_map(|(prop, value)| match value {
                        Value::Text(text) if *prop == property::NAME => Some(text.clone()),
                        _ => None,
                    });
                r.created.push(CreatedObject {
                    id,
                    name,
                    rule: format!("функция \"{}\"", create.function),
                });
            }
        }
    }

    /// «Редактор», требования 8, 13: whether a world exists right now — see `world_exists`'s own
    /// doc comment for exactly which calls flip it.
    pub fn has_world(&self) -> bool {
        self.world_exists
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

    /// «Редактор», требование 39: opens a session — `step` starts counting `session_step` from
    /// zero and building a report; `session_messages` starts empty. Does not touch the world or
    /// the partiya itself; the caller (`play()`/`replay()`) does that separately.
    pub fn begin_session(&mut self) {
        self.session_active = true;
        self.session_step = 0;
        self.session_messages.clear();
        self.last_report = None;
    }

    /// «Редактор», требование 40: closes the session — `step` stops counting and reporting.
    pub fn end_session(&mut self) {
        self.session_active = false;
        self.last_report = None;
    }

    pub fn session_active(&self) -> bool {
        self.session_active
    }

    /// «Редактор», требование 6: steps done since `begin_session`, unaffected by `new_game`.
    pub fn session_step_count(&self) -> u64 {
        self.session_step
    }

    /// «Редактор», требования 26, 45: every message so far this session, each with the session
    /// step it appeared on — a replay `seek(N)` naturally holds only messages up to `N`, since it
    /// recomputes the whole session from scratch.
    pub fn session_messages(&self) -> &[(u64, String)] {
        &self.session_messages
    }

    /// «Редактор», требование 23, 44: the last step's rule report — `None` outside a session.
    pub fn last_report(&self) -> Option<&StepReport> {
        self.last_report.as_ref()
    }

    /// «Редактор», требование 23: only the caller (which owns `ScreensConfig`/`ScreenState`) knows
    /// screen names, so it annotates the just-built report with the transition it saw, if any.
    pub fn annotate_screen_change(&mut self, from: &str, to: &str) {
        if let Some(report) = self.last_report.as_mut() {
            report.screen_change = Some((from.to_string(), to.to_string()));
        }
    }

    /// «Звук»: read side, for the circle (clearing/writing the header) and for
    /// the wasm layer's `sound_window_ptr()`/`sound_window_len()` — never for a step.
    pub fn sound_window(&self) -> &SoundWindow {
        &self.sound_window
    }

    pub fn sound_window_mut(&mut self) -> &mut SoundWindow {
        &mut self.sound_window
    }

    /// The same remembering a window-pixel cursor move does, already in scene coordinates — «Тесты по
    /// записанному вводу», требование 31: a replay's own `cursor` is given in scene cells
    /// directly, bypassing the window-pixel translation that has its own, separate tests.
    pub fn set_cursor_cell(&mut self, cell: Vec2) {
        self.set_cursor_point([cell[0], cell[1], 0.0], None);
    }

    /// «Трёхмерная сцена», требование 23: то же, с местом камеры в клетках сцены и высоте — щелчок
    /// идёт лучом от неё через `cell` на высоте 0.
    pub fn set_cursor_ray(&mut self, cell: Vec2, eye: Vec3) {
        self.set_cursor_point([cell[0], cell[1], 0.0], Some(eye));
    }

    /// «Рельеф», требования 27, 29: точка под курсором с высотой и, в трёхмерной сцене, место камеры,
    /// от которого щелчок идёт лучом через эту точку.
    pub fn set_cursor_point(&mut self, point: Vec3, eye: Option<Vec3>) {
        self.cursor_current = Some(point);
        self.cursor_eye = eye;
    }

    pub fn cursor_eye(&self) -> Option<Vec3> {
        self.cursor_eye
    }

    /// «Редактор», требование 27: the world cursor's current scene-cell position, for the session
    /// layer's own recording — `None` before the cursor has ever moved.
    pub fn cursor_current(&self) -> Option<Vec2> {
        self.cursor_current.map(|point| [point[0], point[1]])
    }

    /// «Рельеф», требование 29: то же с высотой точки.
    pub fn cursor_point(&self) -> Option<Vec3> {
        self.cursor_current
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
    ///
    /// «Редактор»: outside `begin_session` this is exactly the step the page always ran; a session
    /// additionally counts `session_step`, tags any new `messages` with it, and builds the rule
    /// report `step_body` collects — all in `step_body`, so an early return there (a code error)
    /// still leaves whatever partial report/messages that step produced in place.
    pub fn step(&mut self, input: StepInput) {
        if self.outcome.is_some() {
            return;
        }
        self.ensure_code_loaded();
        self.step_count += 1;
        if self.session_active {
            self.session_step += 1;
        }
        let messages_before = self.messages.len();
        self.step_body(input);
        if self.session_active {
            for m in &self.messages[messages_before..] {
                self.session_messages.push((self.session_step, m.clone()));
            }
        }
    }

    fn step_body(&mut self, input: StepInput) {
        self.ensure_scratch_capacity();

        self.apply_to_world(&input.events);
        step::apply_follow_mouse(&mut self.world, input.cursor, &self.scene);

        let expired =
            step::tick_counters(&mut self.world, &mut self.grid_hop_ready, &self.properties);

        let pre_move_positions: Vec<Option<Vec2>> = (0..self.world.slot_count() as u32)
            .map(|id| self.world.vec2(id, property::POSITION))
            .collect();
        let pre_move_z: Vec<f64> = (0..self.world.slot_count() as u32)
            .map(|id| self.world.base_z(id))
            .collect();

        for moved in self.moved.iter_mut() {
            *moved = false;
        }
        let mut outcome_flag = None;
        let mut stage4_deleted: Vec<u32> = expired.clone();
        let mut report: Option<StepReportBuilder> =
            self.session_active.then(StepReportBuilder::new);
        if let Some(r) = report.as_mut() {
            for &id in &expired {
                let name = self.world.text(id, property::NAME).map(str::to_string);
                r.deletes.push((id, name, DeleteCause::LifetimeExpired));
            }
        }
        // «Звук»: этому и только этому обёртка `SoundMarks` даётся — поднять
        // отметку и никогда её не прочитать; окно целиком (`self.sound_window`) шаг не видит.
        let mut marks = self.sound_window.marks();
        let mut code_env = match self.code.as_mut() {
            Some(runner) => {
                // «Код игры»: предел операций — на все вызовы `run` этого шага вместе, не на
                // каждый по отдельности (see `code::Runner::reset_step_budget`).
                runner.reset_step_budget();
                runner.set_wind(self.wind);
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
            &mut report,
            &mut code_env,
            &mut self.rng,
            &mut outcome_flag,
            &mut self.walk_paths,
            &mut self.roads,
        ) {
            let mut err = err;
            err.step = Some(self.step_count);
            self.code_error = Some(err);
            self.last_report = report.map(|b| b.finish(self.session_step));
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
            &mut report,
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
            self.last_report = report.map(|b| b.finish(self.session_step));
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
            &pre_move_z,
            &mut self.rng,
            &mut outcome_flag,
            &mut random_cell_exhausted,
            &mut marks,
            &mut report,
            &mut code_env,
            &mut self.grid,
        ) {
            Ok(result) => result,
            Err(err) => {
                let mut err = err;
                err.step = Some(self.step_count);
                self.code_error = Some(err);
                self.last_report = report.map(|b| b.finish(self.session_step));
                return;
            }
        };
        if random_cell_exhausted && !self.random_cell_warned {
            self.random_cell_warned = true;
            self.messages.push(
                "random_cell: свободной клетки нет, заявка на создание отброшена".to_string(),
            );
        }

        if let Some(r) = report.as_mut() {
            r.outcome = outcome_flag;
        }

        all_deleted.sort_unstable();
        all_deleted.dedup();
        for id in all_deleted {
            self.world.delete(id);
        }
        for create in creates {
            if self.object_ceiling_reached() {
                continue;
            }
            let id = self.world.create();
            let mut seated_by_data = false;
            for (prop, value) in placed_last(&create.props) {
                seated_by_data |= prop == property::POSITION && matches!(value, Value::Vec3(_));
                self.world.set_value(id, prop, value);
            }
            if create
                .props
                .iter()
                .all(|(prop, _)| *prop != property::POSITION)
            {
                self.world.set_vec2(id, property::POSITION, create.position);
            }
            match create.seat {
                _ if seated_by_data => {}
                step::SeatRule::Fresh => surface::seat_fresh(&mut self.world, id),
                step::SeatRule::From(z) => surface::seat_from(&mut self.world, id, z),
            }
            if let Some(spec) = self.world.grid(id, property::GRID) {
                self.world.set_grid_counter(id, spec.interval_steps);
            }
            if let Some(r) = report.as_mut() {
                let name = create.props.iter().find_map(|(p, v)| match (*p, v) {
                    (prop, Value::Text(s)) if prop == property::NAME => Some(s.clone()),
                    _ => None,
                });
                r.created.push(super::report::CreatedObject {
                    id,
                    name,
                    rule: create.rule.clone(),
                });
            }
        }

        let code_creates = self
            .code
            .as_mut()
            .map(code::Runner::take_creates)
            .unwrap_or_default();
        self.spawn_code_creates(code_creates, &mut report);

        if let Some(outcome) = outcome_flag {
            self.outcome = Some((outcome, self.step_count));
        }
        motion::advance(&mut self.world);
        // «Камера», требование 6: без отставания — сразу после последнего шага, здесь, а не при
        // следующей отрисовке.
        self.step_camera();
        self.last_report = report.map(|b| b.finish(self.session_step));
    }
}

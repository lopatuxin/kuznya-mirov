//! «Редактор»: the play/pause/step/stop/replay state machine — everything `wasm::mod`'s
//! `play`/`pause`/`step`/`stop`/`recording`/`replay`/`seek`/`step_back`/live-edit calls delegate
//! to. Lives in `data`, not `core`, because it has to turn a replay's own JSON-shaped values back
//! into `Value`s the same way `set_property` does — but it touches no browser API and runs under
//! plain `cargo test`.

use serde_json::Value as Json;

use crate::core::game::Game;
use crate::core::input::{MouseState, UiQueue};
use crate::core::property::PropertyId;
use crate::core::runner::{MAX_CATCHUP_STEPS, Runner};
use crate::core::screens::{self, RecordedEvent, ScreenId, ScreenState, ScreensConfig};
use crate::core::value::Value;

use super::edit;
use super::load::ImageDecl;
use super::recording::{self, Recording, ReplayCommand, ReplayEdit, ReplayEvent, ReplayEventKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Live,
    Replay,
    /// «Стоп», требование 27: the recording itself outlives the partiya — "живёт до следующего
    /// «Запуска», открытия другой записи или закрытия проекта" — so `stop()` does not drop the
    /// session, only freezes it: `recording()` still answers, everything that would advance the
    /// world or touch it becomes a no-op.
    Ended,
}

/// One open play/replay session — «Редактор» → «Партия в редакторе», «Запись и повтор». `wasm::
/// Engine` holds at most one of these, `None` outside a partiya or a replay.
#[derive(Debug)]
pub struct PlaySession {
    mode: Mode,
    recording: Recording,
    /// «Редактор», требование 27: dedupes consecutive identical cursor positions so a paddle
    /// tracking the mouse for ten minutes doesn't write one event per `mouse_move` call.
    last_cursor: Option<[f64; 2]>,
    /// «Редактор», требование 5: the replay step whose own recorded events `step_once` last fed
    /// into the world — `None` before the first one. A repeated `step_once` call landing on the
    /// same session step (the active screen still isn't live, so the world never actually
    /// advances) must not re-apply that step's events a second time.
    applied_events_step: Option<u64>,
}

impl PlaySession {
    /// «Запуск», требование 39: resets `game`/`state` to the same state a fresh `load()` would
    /// leave them in and opens a new, empty recording.
    pub fn begin_live(game: &mut Game, config: &ScreensConfig, state: &mut ScreenState) -> Self {
        let start_is_live = config.screens[config.start_screen].world_runs;
        game.reset_for_play(start_is_live);
        *state = ScreenState::new(config.start_screen);
        game.begin_session();
        PlaySession {
            mode: Mode::Live,
            recording: Recording::default(),
            last_cursor: None,
            applied_events_step: None,
        }
    }

    /// «Открыть запись»/«Повтор», требования 35, 39: parses `text`, and on success resets `game`/
    /// `state` and opens a replay of it, paused on step 0 — требование 28.
    pub fn begin_replay(
        text: &str,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
    ) -> Result<Self, String> {
        let recording =
            recording::parse(text).map_err(|reason| format!("Это не запись партии: {reason}"))?;
        let start_is_live = config.screens[config.start_screen].world_runs;
        game.reset_for_play(start_is_live);
        *state = ScreenState::new(config.start_screen);
        game.begin_session();
        Ok(PlaySession {
            mode: Mode::Replay,
            recording,
            last_cursor: None,
            applied_events_step: None,
        })
    }

    pub fn is_replay(&self) -> bool {
        self.mode == Mode::Replay
    }

    /// `true` for an open, still-running partiya — `false` for a replay and for a session `end()`
    /// already froze.
    pub fn is_live(&self) -> bool {
        self.mode == Mode::Live
    }

    fn is_active(&self) -> bool {
        matches!(self.mode, Mode::Live | Mode::Replay)
    }

    /// «Стоп», требование 40: ends the partiya/повтор but keeps the session (and its recording)
    /// around — требование 27. The caller still has to rebuild the editor's own static scene
    /// (`Game::show_scene`) itself, same as it does outside any session.
    pub fn end(&mut self, game: &mut Game) {
        let was_live = self.mode == Mode::Live;
        self.mode = Mode::Ended;
        game.end_session();
        if was_live {
            // «Технические детали»: a live session's own length is `game.session_step_count()`
            // while it runs; once ended that counter resets on the next partiya, so the total is
            // frozen into the recording itself right here, before that can happen.
            self.recording.steps = game.session_step_count();
        }
    }

    /// «Технические детали»: the recording's own length — steps actually simulated while live
    /// (frozen by `end()` once the partiya stops), the fixed total of a loaded replay.
    pub fn length(&self, game: &Game) -> u64 {
        if self.mode == Mode::Live {
            game.session_step_count()
        } else {
            self.recording.steps
        }
    }

    /// «Сохранить запись», требование 46: the current recording as replay-file text.
    pub fn recording_text(&self, game: &Game) -> String {
        recording::serialize(&Recording {
            steps: self.length(game),
            events: self.recording.events.clone(),
        })
    }

    fn push_screen_events(
        &mut self,
        step: u64,
        events: Vec<RecordedEvent>,
        game: &Game,
        config: &ScreensConfig,
        images: &[ImageDecl],
    ) {
        for event in events {
            let kind = match event {
                RecordedEvent::WorldKeyDown(code) => ReplayEventKind::KeyDown(code),
                RecordedEvent::WorldKeyUp(code) => ReplayEventKind::KeyUp(code),
                RecordedEvent::Command(cmd) => ReplayEventKind::Command(replay_command_of(
                    cmd,
                    config,
                    &game.properties,
                    images,
                )),
            };
            self.recording.events.push(ReplayEvent { step, kind });
        }
    }

    fn annotate_screen_change(
        game: &mut Game,
        config: &ScreensConfig,
        before: ScreenId,
        after: ScreenId,
    ) {
        if before != after {
            game.annotate_screen_change(&config.screens[before].name, &config.screens[after].name);
        }
    }

    /// «Курсор в мире», требование 27: records the world cursor's scene-cell position, deduped
    /// against the last one recorded — a no-op in replay (требование 28: правка мира и ввод
    /// хозяина недоступны там, so there is nothing of the host's own to record).
    pub fn record_cursor(&mut self, game: &Game, cell: [f64; 2]) {
        if !self.is_live() || self.last_cursor == Some(cell) {
            return;
        }
        self.last_cursor = Some(cell);
        self.recording.events.push(ReplayEvent {
            step: game.session_step_count(),
            kind: ReplayEventKind::Cursor(cell),
        });
    }

    /// «Пауза», требование 40: records the releases `Game::release_held_keys` just applied to the
    /// world, the same `KeyUp` shape a screen-absorbed release already gets — a no-op in replay,
    /// same reason `record_cursor` is (`pause()` there would replay input the recording never had).
    pub fn record_key_releases(&mut self, game: &Game, codes: &[String]) {
        if !self.is_live() {
            return;
        }
        let step = game.session_step_count();
        for code in codes {
            self.recording.events.push(ReplayEvent {
                step,
                kind: ReplayEventKind::KeyUp(code.clone()),
            });
        }
    }

    /// One real-time tick of a live session — «Партия в редакторе»: mirrors `wasm::Engine::tick`'s
    /// own call to `screens::engine_call`, additionally recording world keys/commands this call
    /// resolved and tagging the resulting rule report with any screen change it made.
    #[allow(clippy::too_many_arguments)]
    pub fn tick_live(
        &mut self,
        queue: &mut UiQueue,
        mouse: &mut MouseState,
        runner: &mut Runner,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
        viewport: [f32; 2],
        dt_seconds: f64,
        images: &[ImageDecl],
    ) {
        let step_before = game.session_step_count();
        let before_screen = state.active();
        let mut events = Vec::new();
        screens::engine_call_recording(
            queue,
            mouse,
            runner,
            game,
            config,
            state,
            viewport,
            dt_seconds,
            Some(&mut events),
            true,
        );
        self.push_screen_events(step_before, events, game, config, images);
        Self::annotate_screen_change(game, config, before_screen, state.active());
    }

    /// «Повтор», требование 6, 28: the same bounded real-time burst `Runner::advance` gives a live
    /// partiya (at most `MAX_CATCHUP_STEPS` fixed steps per call), driving `step_once` — recorded
    /// input, then one world step — instead of `Game::step` directly, so a replay runs at the
    /// partiya's own pace rather than one step per animation frame. Sound marks accumulate over
    /// the whole call, cleared once here up front, same as the page's own `Runner::advance_or_reset`
    /// clears once before its own multi-step burst. Stops early at the recording's own end.
    ///
    /// Deliberately does *not* gate the loop on `game.is_running()` the way `Runner::advance` gates
    /// on it for a live partiya — an outcome or a code error from the *previous* step can be a
    /// dead end only until *this* step's own recorded events run (a `new_game` sitting right there
    /// is exactly how a real partiya continues past its own outcome screen), and `step_once` always
    /// applies those before deciding whether the world can actually advance. Checking `is_running`
    /// first here would block `step_once` from ever getting the chance to apply that unblocking
    /// event, freezing the replay on the outcome step forever. Stops the burst early only once a
    /// step has genuinely gone nowhere (session step unchanged) with the game still not running —
    /// a real dead end, not one more recorded event away from continuing.
    #[allow(clippy::too_many_arguments)]
    pub fn tick_replay(
        &mut self,
        queue: &mut UiQueue,
        mouse: &mut MouseState,
        runner: &mut Runner,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
        viewport: [f32; 2],
        dt_seconds: f64,
        images: &[ImageDecl],
    ) {
        game.sound_window_mut().clear_marks();
        runner.accumulate(dt_seconds);
        let mut ran = 0;
        while ran < MAX_CATCHUP_STEPS
            && game.session_step_count() < self.recording.steps
            && runner.take_step()
        {
            let before = game.session_step_count();
            self.step_once(queue, mouse, game, config, state, viewport, images);
            ran += 1;
            if game.session_step_count() == before && !game.is_running() {
                break;
            }
        }
        if ran == MAX_CATCHUP_STEPS
            || !game.is_running()
            || game.session_step_count() >= self.recording.steps
        {
            runner.reset();
        }
    }

    /// «Шаг», требования 9, 23, 30, 40, 47: exactly one world step, live or replay, paused or not —
    /// bypasses the real-time accumulator entirely. In replay, applies this step's own recorded
    /// events first (требование 30: «сначала применяет ввод... потом делает шаг») — *before*
    /// anything below reads `state`/`game`'s outcome or error, so a recorded event that clears one
    /// (a `new_game` right there) gets the chance to — unless they were already applied by an
    /// earlier call that landed on this same step without the world actually advancing —
    /// «Редактор», требование 5: a screen without `world_runs` must not replay its own commands and
    /// key presses again on every repeated call. Does nothing once the replay has already reached
    /// the recording's own end — требование 4, 29: «Шаг» stops there instead of running the world
    /// past what was recorded. `before_screen` is captured before any of that, so the screen-change
    /// this step's own report ends up with (требование 23) is this step's own transition, not
    /// whatever the previous call already left `state` in. Does not clear the sound window's marks
    /// itself — the caller does, once, at whatever granularity it needs (a single manual «Шаг», or
    /// `tick_replay`'s whole real-time burst). The page's own mouse/keyboard, if any is queued, is
    /// not this call's concern — the caller (`wasm::mod`) simply never queues it in replay.
    #[allow(clippy::too_many_arguments)]
    pub fn step_once(
        &mut self,
        queue: &mut UiQueue,
        mouse: &mut MouseState,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
        viewport: [f32; 2],
        images: &[ImageDecl],
    ) {
        if !self.is_active() {
            return;
        }
        let step_before = game.session_step_count();
        if self.is_replay() && step_before >= self.recording.steps {
            return;
        }
        let before_screen = state.active();
        if self.is_replay() && self.applied_events_step != Some(step_before) {
            self.apply_events_before(step_before, game, config, state, images);
            self.applied_events_step = Some(step_before);
        }
        let mut events = Vec::new();
        screens::advance_one_step(
            queue,
            mouse,
            game,
            config,
            state,
            viewport,
            Some(&mut events),
            true,
        );
        if !self.is_replay() {
            self.push_screen_events(step_before, events, game, config, images);
        }
        Self::annotate_screen_change(game, config, before_screen, state.active());
    }

    /// «Повтор», требования 23, 29, 30, 47: recomputes the whole replay from scratch up to
    /// `target`, clamped to the recording's own length (требование 4), no drawing or sound anyone
    /// reads — «Исполнение игры» → «Запись партии в редакторе». Only in a replay — требование 4: a
    /// no-op during a live partiya, whose length isn't fixed yet. Advances by the world's own step
    /// count, not the loop counter — требование 5: a screen without `world_runs` that a divergent
    /// replay (требование 32) never leaves live stops the loop right there instead of applying
    /// every later step's events onto a world stuck one step behind. Annotates the resulting report
    /// with the *last* step's own screen change, same as a live `step_once` landing on `target`
    /// would have — требование 23: the step tab has to match a live partiya's, not go blank because
    /// nothing here called `Game::annotate_screen_change` the way `step_once`/`tick_live` do.
    #[allow(clippy::too_many_arguments)]
    pub fn seek(
        &mut self,
        target: u64,
        queue: &mut UiQueue,
        mouse: &mut MouseState,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
        viewport: [f32; 2],
        images: &[ImageDecl],
    ) {
        if !self.is_replay() {
            return;
        }
        let target = target.min(self.recording.steps);
        let start_is_live = config.screens[config.start_screen].world_runs;
        game.reset_for_play(start_is_live);
        *state = ScreenState::new(config.start_screen);
        game.begin_session();
        self.applied_events_step = None;
        let mut before_screen = state.active();
        loop {
            let current = game.session_step_count();
            if current >= target {
                break;
            }
            before_screen = state.active();
            self.apply_events_before(current, game, config, state, images);
            self.applied_events_step = Some(current);
            let stepped =
                screens::advance_one_step(queue, mouse, game, config, state, viewport, None, true);
            if !stepped {
                break;
            }
        }
        Self::annotate_screen_change(game, config, before_screen, state.active());
    }

    /// `seek(N - 1)` — требование 47; a no-op at step 0, or outside a replay (требование 4).
    #[allow(clippy::too_many_arguments)]
    pub fn step_back(
        &mut self,
        queue: &mut UiQueue,
        mouse: &mut MouseState,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
        viewport: [f32; 2],
        images: &[ImageDecl],
    ) {
        if !self.is_replay() {
            return;
        }
        let current = game.session_step_count();
        if current == 0 {
            return;
        }
        self.seek(
            current - 1,
            queue,
            mouse,
            game,
            config,
            state,
            viewport,
            images,
        );
    }

    /// «Редактор», требование 12: why «Шаг» would do nothing right now, or `None` when it would
    /// actually advance the world — a frozen (post-«Стоп») session counts as none at all.
    ///
    /// Live: the screen simply not being live comes *first* — requirement 12's own wording is
    /// unconditional ("на экране без world_runs «Шаг» неактивен"), and an outcome screen is a
    /// non-live screen like any other, so it reads "На этом экране мир стоит" too, not "Партия
    /// закончилась" (reserved for the rarer case of a live screen whose own outcome is set).
    ///
    /// Replay: the same ordering bug `tick_replay` had — `outcome`/`code_error`/`is_live` must not
    /// be judged until *this* step's own recorded events have actually been tried (a `new_game`
    /// sitting right there can clear any of the three), so an untried step always answers `None`
    /// regardless of what it inherited from the step before. Once tried, a replay adds two of its
    /// own on top of `outcome`/`code_error`: the recording already exhausted, or stuck on a
    /// non-live screen after that one genuine attempt — a divergence («Исполнение игры», требование
    /// 32) can cause the latter.
    pub fn step_blocked_reason(
        &self,
        game: &Game,
        config: &ScreensConfig,
        state: &ScreenState,
    ) -> Option<&'static str> {
        if !self.is_active() {
            return Some("Нет партии");
        }
        if self.is_replay() {
            let current = game.session_step_count();
            if current >= self.recording.steps {
                return Some("Запись кончилась");
            }
            if self.applied_events_step != Some(current) {
                return None;
            }
            if game.outcome().is_some() {
                return Some("Партия закончилась");
            }
            if game.code_error().is_some() {
                return Some("Ошибка кода игры");
            }
            if !state.is_live(config) {
                return Some("Повтор разошёлся с записью");
            }
            return None;
        }
        if !state.is_live(config) {
            return Some("На этом экране мир стоит");
        }
        if game.outcome().is_some() {
            return Some("Партия закончилась");
        }
        if game.code_error().is_some() {
            return Some("Ошибка кода игры");
        }
        None
    }

    /// Routes `KeyDown`/`KeyUp` straight to the world — «Редактор», требование 1: the recorded
    /// event is already a world-bound key (a screen-absorbed press/release never becomes one — see
    /// `RecordedEvent`), already resolved by whichever screen was active when it was recorded, so
    /// replaying it needs no screen lookup of its own, unlike a command. Applied in recorded order
    /// alongside commands and edits, rather than queued for the next `Game::step`'s own stage 1 to
    /// apply once every edit/command of this step already has, regardless of where the press or
    /// release actually fell among them when it was recorded. Looks its events up by binary search
    /// (`events_for_step`) rather than scanning the whole recording — «Редактор», нефункциональное
    /// требование: `seek` on an 18 000-step recording must not cost O(шаги × события).
    fn apply_events_before(
        &self,
        step: u64,
        game: &mut Game,
        config: &ScreensConfig,
        state: &mut ScreenState,
        images: &[ImageDecl],
    ) {
        for event in self.events_for_step(step) {
            match &event.kind {
                ReplayEventKind::KeyDown(code) => game.press_key(code),
                ReplayEventKind::KeyUp(code) => game.release_key(code),
                ReplayEventKind::Cursor(cell) => game.set_cursor_cell(*cell),
                ReplayEventKind::Command(cmd) => {
                    apply_replay_command(cmd, game, config, state, images)
                }
                ReplayEventKind::Edit(e) => apply_replay_edit(e, game, images),
            }
        }
    }

    /// The slice of `self.recording.events` whose `step` equals `step` — «Технические детали»:
    /// events are stored in non-decreasing `step` order, so both ends of the range are a binary
    /// search (`partition_point`) rather than a linear scan.
    fn events_for_step(&self, step: u64) -> &[ReplayEvent] {
        let events = &self.recording.events;
        let start = events.partition_point(|e| e.step < step);
        let end = start + events[start..].partition_point(|e| e.step == step);
        &events[start..end]
    }

    /// «Правка на ходу», требование 43. `Err` outside a live session (в повторе правка
    /// недоступна — требование 28) or on a validation failure; the world is unchanged either way.
    pub fn set_property(
        &mut self,
        game: &mut Game,
        images: &[ImageDecl],
        id: u32,
        name: &str,
        value: &Json,
    ) -> Result<(), String> {
        if !self.is_live() {
            return Err("правка мира недоступна вне партии".to_string());
        }
        edit::set_property(&mut game.world, &game.properties, images, id, name, value)?;
        self.recording.events.push(ReplayEvent {
            step: game.session_step_count(),
            kind: ReplayEventKind::Edit(ReplayEdit::Set(id, name.to_string(), value.clone())),
        });
        Ok(())
    }

    pub fn remove_property(&mut self, game: &mut Game, id: u32, name: &str) -> Result<(), String> {
        if !self.is_live() {
            return Err("правка мира недоступна вне партии".to_string());
        }
        edit::remove_property(&mut game.world, &game.properties, id, name)?;
        self.recording.events.push(ReplayEvent {
            step: game.session_step_count(),
            kind: ReplayEventKind::Edit(ReplayEdit::Remove(id, name.to_string())),
        });
        Ok(())
    }

    pub fn add_object(
        &mut self,
        game: &mut Game,
        images: &[ImageDecl],
        props: &Json,
    ) -> Result<u32, String> {
        if !self.is_live() {
            return Err("правка мира недоступна вне партии".to_string());
        }
        let id = edit::add_object(
            &mut game.world,
            &game.properties,
            images,
            game.max_objects,
            props,
        )?;
        self.recording.events.push(ReplayEvent {
            step: game.session_step_count(),
            kind: ReplayEventKind::Edit(ReplayEdit::Add(props.clone())),
        });
        Ok(id)
    }

    pub fn delete_object(&mut self, game: &mut Game, id: u32) -> Result<(), String> {
        if !self.is_live() {
            return Err("правка мира недоступна вне партии".to_string());
        }
        edit::delete_object(&mut game.world, id);
        self.recording.events.push(ReplayEvent {
            step: game.session_step_count(),
            kind: ReplayEventKind::Edit(ReplayEdit::Delete(id)),
        });
        Ok(())
    }
}

fn replay_command_of(
    cmd: screens::ButtonCommand,
    config: &ScreensConfig,
    properties: &crate::core::property::PropertyTable,
    images: &[ImageDecl],
) -> ReplayCommand {
    use crate::core::screens::ButtonCommand;
    match cmd {
        ButtonCommand::ShowScreen(id) => ReplayCommand::ShowScreen(config.screens[id].name.clone()),
        ButtonCommand::NewGame(id, values_id) => {
            let values = values_id
                .and_then(|vid| config.initial_values.get(vid))
                .map(|list| {
                    list.iter()
                        .map(|(name, prop, value)| {
                            (
                                format!("{name}.{}", properties.name(*prop)),
                                edit::value_to_json(value, images),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            ReplayCommand::NewGame(config.screens[id].name.clone(), values)
        }
        ButtonCommand::Resume => ReplayCommand::Resume,
        ButtonCommand::Quit => ReplayCommand::Quit,
        ButtonCommand::ToggleSound => ReplayCommand::ToggleSound,
    }
}

fn resolve_named_value(
    key: &str,
    json: &Json,
    game: &Game,
    images: &[ImageDecl],
) -> Option<(String, PropertyId, Value)> {
    let dot = key.find('.')?;
    let (object_name, prop_name) = (&key[..dot], &key[dot + 1..]);
    if object_name.is_empty() || prop_name.is_empty() {
        return None;
    }
    let prop = game.properties.resolve(prop_name)?;
    let value = edit::parse_edit_value(json, prop, &game.properties, images).ok()?;
    Some((object_name.to_string(), prop, value))
}

fn resolve_screen(config: &ScreensConfig, name: &str) -> Option<ScreenId> {
    config.screens.iter().position(|s| s.name == name)
}

/// «Исполнение игры» → «Запись партии в редакторе», требование 32: a screen or a property the
/// current files no longer have simply drops that one event — the rest of the replay carries on.
fn apply_replay_command(
    cmd: &ReplayCommand,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    images: &[ImageDecl],
) {
    match cmd {
        ReplayCommand::ShowScreen(name) => {
            if let Some(id) = resolve_screen(config, name) {
                screens::apply_command(screens::ButtonCommand::ShowScreen(id), game, config, state);
            }
        }
        ReplayCommand::NewGame(name, raw_values) => {
            let Some(id) = resolve_screen(config, name) else {
                return;
            };
            let values: Vec<(String, PropertyId, Value)> = raw_values
                .iter()
                .filter_map(|(key, json)| resolve_named_value(key, json, game, images))
                .collect();
            screens::apply_replay_new_game(id, &values, game, config, state);
        }
        ReplayCommand::Resume => {
            screens::apply_command(screens::ButtonCommand::Resume, game, config, state);
        }
        ReplayCommand::Quit => {
            screens::apply_command(screens::ButtonCommand::Quit, game, config, state);
        }
        ReplayCommand::ToggleSound => {
            screens::apply_command(screens::ButtonCommand::ToggleSound, game, config, state);
        }
    }
}

/// требование 32: an object the current files no longer produce at that number is simply skipped —
/// `data::edit`'s own `Err` (object gone, unknown property, bad value) is exactly that signal.
fn apply_replay_edit(edit_event: &ReplayEdit, game: &mut Game, images: &[ImageDecl]) {
    let _ = match edit_event {
        ReplayEdit::Set(id, name, value) => {
            edit::set_property(&mut game.world, &game.properties, images, *id, name, value)
        }
        ReplayEdit::Remove(id, name) => {
            edit::remove_property(&mut game.world, &game.properties, *id, name)
        }
        ReplayEdit::Add(props) => edit::add_object(
            &mut game.world,
            &game.properties,
            images,
            game.max_objects,
            props,
        )
        .map(|_| ()),
        ReplayEdit::Delete(id) => {
            edit::delete_object(&mut game.world, *id);
            Ok(())
        }
    };
}

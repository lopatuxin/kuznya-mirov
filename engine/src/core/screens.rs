//! «Экраны и состояние» + «Интерфейс игры»: screens, their elements, and the mouse-driven state
//! machine that moves between them. The engine itself knows nothing about "menu" or "pause" —
//! only a screen's name, whether the world runs on it, and its list of elements.

use std::collections::HashMap;

use super::game::Game;
use super::input::{MouseEvent, MouseState, UiEvent, UiQueue};
use super::property::{self, PropertyId, PropertyTable};
use super::rules::Outcome;
use super::value::PropKind;
use super::world::World;

pub type ScreenId = usize;
pub type FontId = usize;
/// Index into `files.music`, in declaration order — «Звук»: a screen's `music`
/// field resolves its name to one of these at load time, the same way `font` resolves to `FontId`.
pub type MusicId = usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Anchor {
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

impl Anchor {
    /// `(fx, fy)`: the fractional point on the window this anchor names — 0 for an edge on the
    /// near side, 1 for the far side, 0.5 for the midline. «Интерфейс игры» → «Раскладка».
    fn fractions(self) -> (f32, f32) {
        match self {
            Anchor::TopLeft => (0.0, 0.0),
            Anchor::Top => (0.5, 0.0),
            Anchor::TopRight => (1.0, 0.0),
            Anchor::Left => (0.0, 0.5),
            Anchor::Center => (0.5, 0.5),
            Anchor::Right => (1.0, 0.5),
            Anchor::BottomLeft => (0.0, 1.0),
            Anchor::Bottom => (0.5, 1.0),
            Anchor::BottomRight => (1.0, 1.0),
        }
    }

    pub fn parse(s: &str) -> Option<Anchor> {
        Some(match s {
            "top_left" => Anchor::TopLeft,
            "top" => Anchor::Top,
            "top_right" => Anchor::TopRight,
            "left" => Anchor::Left,
            "center" => Anchor::Center,
            "right" => Anchor::Right,
            "bottom_left" => Anchor::BottomLeft,
            "bottom" => Anchor::Bottom,
            "bottom_right" => Anchor::BottomRight,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Left,
    Center,
    Right,
}

impl Align {
    pub fn parse(s: &str) -> Option<Align> {
        Some(match s {
            "left" => Align::Left,
            "center" => Align::Center,
            "right" => Align::Right,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub anchor: Anchor,
    pub offset: [f32; 2],
    pub size: [f32; 2],
}

impl Placement {
    /// The element's top-left corner in window pixels, given the window's current size.
    /// «Интерфейс игры» → «Раскладка».
    pub fn top_left(&self, viewport: [f32; 2]) -> [f32; 2] {
        let (fx, fy) = self.anchor.fractions();
        let x = if fx == 0.0 {
            self.offset[0]
        } else if fx == 1.0 {
            viewport[0] - self.size[0] - self.offset[0]
        } else {
            (viewport[0] - self.size[0]) / 2.0 + self.offset[0]
        };
        let y = if fy == 0.0 {
            self.offset[1]
        } else if fy == 1.0 {
            viewport[1] - self.size[1] - self.offset[1]
        } else {
            (viewport[1] - self.size[1]) / 2.0 + self.offset[1]
        };
        [x, y]
    }

    pub fn contains(&self, viewport: [f32; 2], point: [f32; 2]) -> bool {
        let [x, y] = self.top_left(viewport);
        point[0] >= x && point[0] < x + self.size[0] && point[1] >= y && point[1] < y + self.size[1]
    }
}

/// One piece of a `text` field after parsing: literal characters, or a `{object.prop}`
/// substitution naming an object by its `name` property and one of its properties.
#[derive(Debug, Clone)]
pub enum TextPart {
    Literal(String),
    Value {
        object_name: String,
        prop: PropertyId,
    },
}

/// «Интерфейс игры» → «Текст»: resolves every `{object.prop}` substitution against `world`,
/// finding the object by its `name` property (the one with the lowest id wins if several share
/// a name) — literal text passes through unchanged. A named object that doesn't currently exist
/// substitutes empty, not an error: it can legitimately have been deleted mid-partie.
pub fn format_text(parts: &[TextPart], world: &World, properties: &PropertyTable) -> String {
    let mut out = String::new();
    for part in parts {
        match part {
            TextPart::Literal(s) => out.push_str(s),
            TextPart::Value { object_name, prop } => {
                if let Some(id) = find_named_object(world, object_name) {
                    out.push_str(&format_property(world, id, *prop, properties));
                }
            }
        }
    }
    out
}

fn find_named_object(world: &World, name: &str) -> Option<u32> {
    world
        .ids()
        .find(|&id| world.text(id, property::NAME) == Some(name))
}

/// «Числа печатаются как есть» — целые без дробной части, дробные со своей. `{}` на `f64`
/// already omits a trailing `.0`, so no special-casing is needed here.
fn format_property(world: &World, id: u32, prop: PropertyId, properties: &PropertyTable) -> String {
    match properties.kind(prop) {
        PropKind::Number => world
            .number_like(id, prop)
            .map(|n| format!("{n}"))
            .unwrap_or_default(),
        // «Значение свойства вида "время" подставляется в секундах» — переведено обратно из
        // шагов, в которых оно хранится внутри движка.
        PropKind::Time => world
            .time(id, prop)
            .map(|steps| format!("{}", steps as f64 / 60.0))
            .unwrap_or_default(),
        PropKind::Flag => {
            if world.flag(id, prop) {
                "да".to_string()
            } else {
                String::new()
            }
        }
        PropKind::Text => world.text(id, prop).unwrap_or_default().to_string(),
        PropKind::Vec2 | PropKind::Color | PropKind::Layer | PropKind::Grid | PropKind::Keys => {
            String::new()
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum ButtonCommand {
    ShowScreen(ScreenId),
    NewGame(ScreenId),
    Resume,
    Quit,
    /// «Звук» → «Два вида звука»: touches no world property, so it needs
    /// none of `apply_command`'s screen-switching machinery — just flips `ScreenState`'s own
    /// `sound_enabled`.
    ToggleSound,
}

#[derive(Debug, Clone)]
pub enum Element {
    Panel {
        placement: Placement,
        color: [f32; 4],
    },
    Label {
        placement: Placement,
        text: Vec<TextPart>,
        font: FontId,
        font_size: f32,
        color: [f32; 4],
        align: Align,
    },
    Button {
        placement: Placement,
        text: Vec<TextPart>,
        font: FontId,
        font_size: f32,
        text_color: [f32; 4],
        color: [f32; 4],
        color_hover: [f32; 4],
        color_pressed: [f32; 4],
        on_click: ButtonCommand,
    },
}

/// «Экраны и состояние» → «Клавиши экрана»: name-to-command, the same five commands as
/// `on_click`. A key named here never reaches the world, on either press or release.
pub type ScreenKeyTable = HashMap<String, ButtonCommand>;

#[derive(Debug, Clone)]
pub struct Screen {
    pub name: String,
    pub world_runs: bool,
    pub elements: Vec<Element>,
    pub keys: ScreenKeyTable,
    /// «Звук» → «Два вида звука»: `None` is silence, same as the pause screen.
    pub music: Option<MusicId>,
}

#[derive(Debug, Clone, Default)]
pub struct ScreensConfig {
    pub screens: Vec<Screen>,
    pub start_screen: ScreenId,
    pub win_screen: Option<ScreenId>,
    pub loss_screen: Option<ScreenId>,
}

/// The active screen and the one remembered screen `resume` returns to — «Экраны и состояние» → «Команды»: at most one level deep, no further history.
#[derive(Debug, Clone, Copy)]
pub struct ScreenState {
    active: ScreenId,
    previous: Option<ScreenId>,
    /// «Звук» → «Два вида звука»: belongs to the circle, next
    /// to the active screen, not to the world — `new_game`/`quit` don't touch it, and it starts
    /// `true` on every load, since the engine keeps no storage for it at all.
    sound_enabled: bool,
}

impl ScreenState {
    pub fn new(start_screen: ScreenId) -> Self {
        ScreenState {
            active: start_screen,
            previous: None,
            sound_enabled: true,
        }
    }

    pub fn active(&self) -> ScreenId {
        self.active
    }

    pub fn sound_enabled(&self) -> bool {
        self.sound_enabled
    }

    pub fn is_live(&self, config: &ScreensConfig) -> bool {
        config.screens[self.active].world_runs
    }

    fn switch(&mut self, target: ScreenId, remember: bool) {
        self.previous = if remember { Some(self.active) } else { None };
        self.active = target;
    }
}

/// Moves to `target`, releasing held keys through `game` first when the transition leaves a
/// live screen for one that isn't — «Экраны и состояние» → «Жизнь партии».
fn switch_to(
    state: &mut ScreenState,
    config: &ScreensConfig,
    game: &mut Game,
    target: ScreenId,
    remember: bool,
) {
    let was_live = config.screens[state.active].world_runs;
    state.switch(target, remember);
    if was_live && !config.screens[target].world_runs {
        game.release_held_keys();
    }
}

/// Executes one button command. `config` resolves `show_screen`/`new_game` targets, already
/// checked to exist by the prestart validation, so no error path is needed here.
pub fn apply_command(
    cmd: ButtonCommand,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    match cmd {
        ButtonCommand::ShowScreen(target) => switch_to(state, config, game, target, true),
        ButtonCommand::NewGame(target) => {
            game.new_game();
            switch_to(state, config, game, target, false);
        }
        ButtonCommand::Resume => {
            if let Some(previous) = state.previous {
                switch_to(state, config, game, previous, true);
            }
        }
        ButtonCommand::Quit => {
            game.quit();
            switch_to(state, config, game, config.start_screen, false);
        }
        // «Звук» → «Два вида звука»: touches no world property, no
        // screen, no queued anything — the whole command is this one flip.
        ButtonCommand::ToggleSound => state.sound_enabled = !state.sound_enabled,
    }
}

/// Stage-9 equivalent: switches to the win/loss screen the first time `game.outcome()` reports
/// one, and does nothing once already there. «Формат игры» / «Исполнение игры»: `end_game`
/// finishes the step, the screen switch happens here, on the step's boundary.
fn handle_outcome(game: &mut Game, config: &ScreensConfig, state: &mut ScreenState) {
    let Some((outcome, _step)) = game.outcome() else {
        return;
    };
    let target = match outcome {
        Outcome::Win => config.win_screen,
        Outcome::Loss => config.loss_screen,
    };
    if let Some(target) = target
        && state.active() != target
    {
        switch_to(state, config, game, target, true);
    }
}

/// One fixed-step tick: steps the world when the active screen is live (or resets the runner's
/// accumulator when it isn't), then applies the win/loss screen switch if a step just ended the
/// game. «Экраны и состояние» → «Жизнь партии».
pub fn tick(
    runner: &mut super::runner::Runner,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    dt_seconds: f64,
) {
    runner.advance_or_reset(game, dt_seconds, state.is_live(config));
    handle_outcome(game, config, state);
}

/// «Звук» → «Один вызов движка», пункт 5: writes the active screen's
/// music (or silence, if it names none) and the sound-enabled flag into the window — called once
/// per call, after both the shared mouse/screen-key queue has drained and this call's own steps
/// have run, so a click that just switched screens writes the screen it landed on, never the one
/// it left. «Экраны и состояние» → «Кому принадлежит состояние звука»: `game`'s own window
/// carries the result, `config`/`state` only supply what's currently true.
pub fn write_sound_frame(game: &mut Game, config: &ScreensConfig, state: &ScreenState) {
    let music = config.screens[state.active()].music;
    game.sound_window_mut()
        .write_header(music, state.sound_enabled());
}

/// «Экраны и состояние» → «Клавиши экрана»: a key named in the active screen's own `keys` table
/// never reaches the world, on press or release — judged here against whichever screen is
/// active *right now*, at drain time, not when the raw event first arrived, so a key queued
/// before a click that switches screens still sees the screen the click lands on. Dropped
/// outright on a screen without `world_runs`, except a key the active screen names.
fn handle_screen_key_down(
    game: &mut Game,
    config: &ScreensConfig,
    state: &ScreenState,
    code: &str,
) {
    if config.screens[state.active()].keys.contains_key(code) {
        return;
    }
    if state.is_live(config) {
        game.key_down(code);
    }
}

/// Mirrors `handle_screen_key_down`: an absorbed key's command runs right here, in this same
/// drain pass, instead of through a second queue of its own — drain order already puts it after
/// every mouse event queued ahead of it. `Game::release_key` runs unconditionally first: it
/// releases the key from the world only if the world actually holds it, and either way drops
/// whatever the page still has queued for it — see its own doc comment for exactly which case
/// that first part covers. A release the world never held on a screen that never absorbs it (its
/// press was itself absorbed, by this screen or an earlier one) is silently dropped by
/// `Game::key_up` itself in the non-absorbed branch below, so this function does not need to
/// track that case separately.
fn handle_screen_key_up(
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    code: &str,
) {
    if let Some(cmd) = config.screens[state.active()].keys.get(code).copied() {
        game.release_key(code);
        apply_command(cmd, game, config, state);
        return;
    }
    if state.is_live(config) {
        game.key_up(code);
    }
}

/// «Интерфейс игры» → «Мышь»: only buttons participate, checked in
/// reverse drawing order so the topmost one wins.
fn topmost_button_at(screen: &Screen, viewport: [f32; 2], point: [f32; 2]) -> Option<usize> {
    screen
        .elements
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, el)| {
            if let Element::Button { placement, .. } = el {
                placement.contains(viewport, point).then_some(i)
            } else {
                None
            }
        })
}

/// Applies one queued mouse event to `mouse`'s hover/capture state against `screen`'s buttons.
/// Returns the command to run once a press releases inside the button that captured it —
/// «Интерфейс игры» → «Мышь».
fn handle_mouse_event(
    mouse: &mut MouseState,
    screen: &Screen,
    viewport: [f32; 2],
    event: MouseEvent,
) -> Option<ButtonCommand> {
    match event {
        MouseEvent::Move(pos) => {
            mouse.position = pos;
            if mouse.captured.is_none() {
                mouse.hover = topmost_button_at(screen, viewport, pos);
            }
            None
        }
        MouseEvent::Down => {
            mouse.captured = mouse.hover;
            None
        }
        MouseEvent::Up => {
            let captured = mouse.captured.take()?;
            let Element::Button {
                placement,
                on_click,
                ..
            } = screen.elements.get(captured)?
            else {
                return None;
            };
            placement
                .contains(viewport, mouse.position)
                .then_some(*on_click)
        }
    }
}

/// Drains the shared mouse/screen-key queue, judging every event — mouse or keyboard — against
/// whichever screen is active at the moment it is *this* event's turn, not when it arrived. A
/// click that switches the screen mid-drain leaves later events in the same batch, key or mouse,
/// to land on the new one, same as the browser delivering them one at a time would — «Экраны и
/// состояние» → «Клавиши экрана».
pub fn process_ui_queue(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
) {
    for event in queue.drain() {
        match event {
            UiEvent::Mouse(mouse_event) => {
                let screen = &config.screens[state.active()];
                if let Some(cmd) = handle_mouse_event(mouse, screen, viewport, mouse_event) {
                    apply_command(cmd, game, config, state);
                }
            }
            UiEvent::KeyDown(code) => handle_screen_key_down(game, config, state, &code),
            UiEvent::KeyUp(code) => handle_screen_key_up(game, config, state, &code),
        }
    }
}

/// The one call every `tick` makes, once per `requestAnimationFrame` — «Исполнение игры» →
/// «Шаг и кадр», «Звук» → «Один вызов движка»: drain the shared mouse/screen-key queue first (a
/// click's own screen switch, and a key the new screen doesn't claim, are both visible to this
/// same call's steps below, not only a later call's), advance the fixed-step simulation, then
/// write what the sound window should say — after both, so a click that just changed screen
/// writes the screen it landed on. `wasm::Engine::tick` calls this and then draws a frame; native
/// code (tests included) calls it directly instead of repeating the three calls by hand, so a
/// change to this order is exercised the same way in both.
#[allow(clippy::too_many_arguments)]
pub fn engine_call(
    queue: &mut UiQueue,
    mouse: &mut MouseState,
    runner: &mut super::runner::Runner,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
    dt_seconds: f64,
) {
    process_ui_queue(queue, mouse, game, config, state, viewport);
    tick(runner, game, config, state, dt_seconds);
    write_sound_frame(game, config, state);
}

/// Which of the three fill colors a button currently shows — pressed wins over hover, and no
/// button hovers while another one holds the capture. «Интерфейс игры» → «Мышь».
pub fn button_fill<'a>(
    color: &'a [f32; 4],
    color_hover: &'a [f32; 4],
    color_pressed: &'a [f32; 4],
    index: usize,
    mouse: &MouseState,
) -> &'a [f32; 4] {
    if mouse.captured == Some(index) {
        color_pressed
    } else if mouse.captured.is_none() && mouse.hover == Some(index) {
        color_hover
    } else {
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placement(anchor: Anchor, offset: [f32; 2], size: [f32; 2]) -> Placement {
        Placement {
            anchor,
            offset,
            size,
        }
    }

    #[test]
    fn top_left_anchor_ignores_window_size() {
        let p = placement(Anchor::TopLeft, [10.0, 20.0], [50.0, 30.0]);
        assert_eq!(p.top_left([800.0, 600.0]), [10.0, 20.0]);
        assert_eq!(p.top_left([320.0, 200.0]), [10.0, 20.0]);
    }

    #[test]
    fn bottom_right_anchor_hugs_the_far_corner_inward() {
        let p = placement(Anchor::BottomRight, [16.0, 16.0], [100.0, 40.0]);
        // right edge sits 16px from the window's right edge, bottom 16px from the bottom.
        let [x, y] = p.top_left([800.0, 600.0]);
        assert_eq!(x, 800.0 - 100.0 - 16.0);
        assert_eq!(y, 600.0 - 40.0 - 16.0);
    }

    #[test]
    fn center_anchor_offsets_from_the_middle_at_three_window_sizes() {
        let p = placement(Anchor::Center, [0.0, -60.0], [400.0, 48.0]);
        for &(w, h) in &[(800.0, 600.0), (1920.0, 1080.0), (360.0, 640.0)] {
            let [x, y] = p.top_left([w, h]);
            assert_eq!(x, (w - 400.0) / 2.0);
            assert_eq!(y, (h - 48.0) / 2.0 - 60.0);
        }
    }

    #[test]
    fn edge_midpoint_anchors_center_along_the_tangential_axis() {
        let top = placement(Anchor::Top, [0.0, 10.0], [200.0, 20.0]);
        let [x, y] = top.top_left([800.0, 600.0]);
        assert_eq!(x, (800.0 - 200.0) / 2.0);
        assert_eq!(y, 10.0);

        let left = placement(Anchor::Left, [5.0, 0.0], [30.0, 200.0]);
        let [x, y] = left.top_left([800.0, 600.0]);
        assert_eq!(x, 5.0);
        assert_eq!(y, (600.0 - 200.0) / 2.0);
    }

    #[test]
    fn contains_matches_the_resolved_rectangle() {
        let p = placement(Anchor::TopLeft, [0.0, 0.0], [100.0, 50.0]);
        let viewport = [800.0, 600.0];
        assert!(p.contains(viewport, [50.0, 25.0]));
        assert!(!p.contains(viewport, [150.0, 25.0]));
        assert!(!p.contains(viewport, [50.0, 60.0]));
    }

    /// All nine anchors, at three window sizes — «Тесты»: «Раскладка: девять якорей при трёх
    /// размерах окна».
    #[test]
    fn all_nine_anchors_resolve_at_three_window_sizes() {
        let anchors = [
            Anchor::TopLeft,
            Anchor::Top,
            Anchor::TopRight,
            Anchor::Left,
            Anchor::Center,
            Anchor::Right,
            Anchor::BottomLeft,
            Anchor::Bottom,
            Anchor::BottomRight,
        ];
        for &(w, h) in &[(320.0, 240.0), (800.0, 600.0), (1920.0, 1080.0)] {
            for &anchor in &anchors {
                let p = placement(anchor, [4.0, 6.0], [40.0, 20.0]);
                let [x, y] = p.top_left([w, h]);
                // Every resolved rectangle must fit inside the window — the anchor names are
                // there precisely so an element never has to be placed off-window on purpose.
                assert!(x >= -1e-6 && x + 40.0 <= w + 1e-6, "{anchor:?} x={x} w={w}");
                assert!(y >= -1e-6 && y + 20.0 <= h + 1e-6, "{anchor:?} y={y} h={h}");
            }
        }
    }

    fn table_with(names_and_kinds: &[(&str, PropKind)]) -> PropertyTable {
        let mut table = PropertyTable::new();
        for &(name, kind) in names_and_kinds {
            table.declare_author(name, kind).unwrap();
        }
        table
    }

    #[test]
    fn format_text_substitutes_number_time_flag_and_literal() {
        let properties = table_with(&[
            ("score", PropKind::Number),
            ("elapsed", PropKind::Time),
            ("on", PropKind::Flag),
        ]);
        let mut world = World::new(&properties);
        let id = world.create();
        world.set_text(id, property::NAME, "head".to_string());
        world.set_number(id, properties.resolve("score").unwrap(), 12.0);
        world.set_time(id, properties.resolve("elapsed").unwrap(), 90); // 1.5s at 60 steps/s
        world.set_flag(id, properties.resolve("on").unwrap(), true);

        let parts = vec![
            TextPart::Literal("Счёт: ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("score").unwrap(),
            },
            TextPart::Literal(" за ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("elapsed").unwrap(),
            },
            TextPart::Literal("с, вкл: ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("on").unwrap(),
            },
        ];
        assert_eq!(
            format_text(&parts, &world, &properties),
            "Счёт: 12 за 1.5с, вкл: да"
        );
    }

    #[test]
    fn format_text_substitutes_empty_when_the_named_object_is_gone() {
        let properties = table_with(&[("score", PropKind::Number)]);
        let world = World::new(&properties);
        let parts = vec![
            TextPart::Literal("Счёт: ".to_string()),
            TextPart::Value {
                object_name: "head".to_string(),
                prop: properties.resolve("score").unwrap(),
            },
        ];
        assert_eq!(format_text(&parts, &world, &properties), "Счёт: ");
    }
}

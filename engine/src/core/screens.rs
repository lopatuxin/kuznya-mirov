//! «Экраны и состояние» + «Интерфейс игры»: screens, their elements, and the mouse-driven state
//! machine that moves between them. The engine itself knows nothing about "menu" or "pause" —
//! only a screen's name, whether the world runs on it, and its list of elements.

use std::collections::HashMap;

use super::game::Game;
use super::input::{KeyQueue, MouseEvent, MouseState};
use super::property::{self, PropertyId, PropertyTable};
use super::rules::Outcome;
use super::value::PropKind;
use super::world::World;

pub type ScreenId = usize;
pub type FontId = usize;

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
    /// «Интерфейс игры» → «Раскладка: якорь и отступ».
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

/// «Экраны и состояние» → «Клавиша экрана»: name-to-command, the same four commands as
/// `on_click`. A key named here never reaches the world, on either press or release.
pub type ScreenKeyTable = HashMap<String, ButtonCommand>;

#[derive(Debug, Clone)]
pub struct Screen {
    pub name: String,
    pub world_runs: bool,
    pub elements: Vec<Element>,
    pub keys: ScreenKeyTable,
}

#[derive(Debug, Clone, Default)]
pub struct ScreensConfig {
    pub screens: Vec<Screen>,
    pub start_screen: ScreenId,
    pub win_screen: Option<ScreenId>,
    pub loss_screen: Option<ScreenId>,
}

/// The active screen and the one remembered screen `resume` returns to — «Экраны и состояние» →
/// «Четыре команды кнопки»: at most one level deep, no further history.
#[derive(Debug, Clone, Copy)]
pub struct ScreenState {
    active: ScreenId,
    previous: Option<ScreenId>,
}

impl ScreenState {
    pub fn new(start_screen: ScreenId) -> Self {
        ScreenState {
            active: start_screen,
            previous: None,
        }
    }

    pub fn active(&self) -> ScreenId {
        self.active
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
/// live screen for one that isn't — «Экраны и состояние» → «Как это ложится в круг движка».
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
/// game. «Экраны и состояние» → «Как это ложится в круг движка».
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

/// «Экраны и состояние»: keyboard events are dropped outright on a screen with no `world_runs`.
/// A key the active screen names in its own `keys` table is absorbed here instead — it never
/// reaches the world, live screen or not, and never joins the engine's held-keys set.
pub fn key_down(game: &mut Game, config: &ScreensConfig, state: &ScreenState, code: &str) {
    if config.screens[state.active()].keys.contains_key(code) {
        return;
    }
    if state.is_live(config) {
        game.key_down(code);
    }
}

/// Queues the release for `process_key_queue` when the active screen names `code` in its own
/// `keys` table — «Экраны и состояние» → «Клавиша экрана»: fires on release, not on press, and
/// the command runs against whichever screen is active once the queue drains, same as a mouse
/// click. Otherwise forwarded to the world exactly as before, and only if the key actually
/// reached the world on press — a release absorbed by press (by this screen's own table, by a
/// different screen's, or already synthesized by `release_held_keys`) must not reach the world a
/// second time, or a binding fires for a key the player never pressed on this screen.
///
/// A key can be held from a *different* live screen that didn't declare it, then get absorbed
/// here once the player switches to a live screen that does. Without also releasing it in the
/// world, it would stay held forever — the same servicing action `release_held_keys` performs on
/// a live-to-non-live transition, just for one key instead of all of them. That release is
/// applied immediately, the same way `release_held_keys` applies its own — queuing it through
/// `game.key_up` instead would leave it sitting in `Game`'s input queue until the next step, and
/// a `new_game`/`switch_to` landing before that step throws the whole queue away, taking the
/// release with it while `held` no longer names the key to retry.
pub fn key_up(
    game: &mut Game,
    config: &ScreensConfig,
    state: &ScreenState,
    keys: &mut KeyQueue,
    code: &str,
) {
    if config.screens[state.active()].keys.contains_key(code) {
        if game.is_key_held(code) {
            game.release_key(code);
        }
        keys.push_release(code);
        return;
    }
    if state.is_live(config) && game.is_key_held(code) {
        game.key_up(code);
    }
}

/// Drains queued screen-key releases, running each one's command against whichever screen is
/// active at the moment it is processed — a release that lands after a mouse-driven switch in
/// the same batch uses the new screen's table, same as `process_mouse_queue`.
pub fn process_key_queue(
    queue: &mut KeyQueue,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
) {
    for code in queue.drain() {
        let cmd = config.screens[state.active()].keys.get(&code).copied();
        if let Some(cmd) = cmd {
            apply_command(cmd, game, config, state);
        }
    }
}

/// «Интерфейс игры» → «Над каким элементом курсор»: only buttons participate, checked in
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
/// «Интерфейс игры» → «Срабатывает по отпусканию внутри границ».
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

/// Drains the mouse queue, processing every event against whichever screen is active at the
/// time — a click that switches the screen mid-drain leaves later events in the same batch to
/// land on the new one, same as the browser delivering them one at a time would.
pub fn process_mouse_queue(
    queue: &mut super::input::MouseQueue,
    mouse: &mut MouseState,
    game: &mut Game,
    config: &ScreensConfig,
    state: &mut ScreenState,
    viewport: [f32; 2],
) {
    for event in queue.drain() {
        let screen = &config.screens[state.active()];
        if let Some(cmd) = handle_mouse_event(mouse, screen, viewport, event) {
            apply_command(cmd, game, config, state);
        }
    }
}

/// Which of the three fill colors a button currently shows — pressed wins over hover, and no
/// button hovers while another one holds the capture. «Интерфейс игры» → «Три состояния кнопки».
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

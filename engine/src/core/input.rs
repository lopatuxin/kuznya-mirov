#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Release,
}

/// One press or release, in the order it was queued — «Исполнение игры»: a release and a press
/// of the same key landing in the same real-time gap must reach `step::apply_input` in that same
/// order, or whichever of the two a fixed press-then-release pass applies last wins regardless of
/// which the player actually did last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyEvent {
    pub code: String,
    pub action: KeyAction,
}

/// What a single step sees: every press/release queued since the previous step, still in arrival
/// order — frozen for the duration of that step. During a burst of catch-up steps only the first
/// one gets this; the rest see `StepInput::empty()`, so a stuck spacebar does not fire five times.
#[derive(Debug, Clone, Default)]
pub struct StepInput {
    pub events: Vec<KeyEvent>,
}

impl StepInput {
    pub fn empty() -> Self {
        StepInput::default()
    }

    /// Test convenience: whether any key event at all is queued for this step — asserting on this
    /// reads better than reaching into `events` directly.
    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }
}

#[derive(Debug, Clone, Default)]
pub struct InputQueue {
    pending: Vec<KeyEvent>,
    /// Codes the *page* still thinks are down — set on `press`, cleared on `release`/`forget`.
    /// Only for this queue's own bookkeeping: suppressing a repeat of an already-pressed key here,
    /// and refusing to queue a release with no matching queued press. Not what the world holds —
    /// `Game::world_held_keys` is that, and `Game::release_held_keys` releases *that* set when
    /// leaving a live screen, not this one; this set is simply dropped along with the rest of the
    /// queue at that point.
    held: std::collections::HashSet<String>,
}

impl InputQueue {
    pub fn new() -> Self {
        InputQueue::default()
    }

    /// A press that repeats an already-held key queues nothing more: it's the front edge that
    /// matters, and browser auto-repeat otherwise keeps queuing `Press` events for a key nothing
    /// ever released, some of which can still be sitting unconsumed long after the world first
    /// took the original press. Suppressed here, in the engine, rather than relying on the page to
    /// filter `event.repeat` — the engine has to stay correct however the page behaves.
    pub fn press(&mut self, code: &str) {
        if !self.held.insert(code.to_string()) {
            return;
        }
        self.pending.push(KeyEvent {
            code: code.to_string(),
            action: KeyAction::Press,
        });
    }

    /// A no-op when `code` isn't currently held — «Исполнение игры»: a release whose press never
    /// reached the world must not reach it either.
    pub fn release(&mut self, code: &str) {
        if !self.held.remove(code) {
            return;
        }
        self.pending.push(KeyEvent {
            code: code.to_string(),
            action: KeyAction::Release,
        });
    }

    /// Stage 1: folds everything queued since the previous step into a fixed picture, in the
    /// order it arrived, and empties the queue.
    pub fn take_snapshot(&mut self) -> StepInput {
        StepInput {
            events: std::mem::take(&mut self.pending),
        }
    }

    /// Drops every queued event for `code` and forgets the page ever held it — used when a screen
    /// absorbs `code`'s release (see `Game::release_key`): whatever the page still has queued for
    /// it (a stray `Press` from before the switch, browser auto-repeat, …) must not surface later
    /// and re-trigger a binding on its own, now that a screen has already decided this key's fate.
    /// Says nothing about whether the *world* held `code` — that question belongs to
    /// `Game::is_key_held`, backed by the world's own bookkeeping, not this queue's.
    pub fn forget(&mut self, code: &str) {
        self.pending.retain(|event| event.code != code);
        self.held.remove(code);
    }

    /// Drops both the pending queue and the held set — «Исполнение игры»: на входе в паузу
    /// очередь чистится, а на неигровом экране набор не пополняется.
    pub fn clear(&mut self) {
        self.pending.clear();
        self.held.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MouseEvent {
    Move([f32; 2]),
    Down,
    Up,
}

/// One raw event queued for the screen layer to judge at drain time — «Экраны и состояние» →
/// «Клавиши экрана»: a key press/release is judged by whichever screen is active *when this
/// queue is drained*, not when the event arrived, so it shares one FIFO with mouse events instead
/// of a queue of its own. That keeps a key arriving between two frames in the same relative order
/// as a click queued alongside it — the click's own screen switch, if any, is applied first
/// whenever it was queued first.
#[derive(Debug, Clone, PartialEq)]
pub enum UiEvent {
    Mouse(MouseEvent),
    KeyDown(String),
    KeyUp(String),
}

/// Mouse and screen-key events share this one FIFO: the page just appends as they happen, the
/// engine drains it in order once per `tick`. See «Интерфейс игры» → «Мышь» and «Экраны и
/// состояние» → «Клавиши экрана».
#[derive(Debug, Clone, Default)]
pub struct UiQueue {
    pending: Vec<UiEvent>,
}

impl UiQueue {
    pub fn new() -> Self {
        UiQueue::default()
    }

    pub fn push_mouse_move(&mut self, x: f32, y: f32) {
        self.pending.push(UiEvent::Mouse(MouseEvent::Move([x, y])));
    }

    pub fn push_mouse_down(&mut self) {
        self.pending.push(UiEvent::Mouse(MouseEvent::Down));
    }

    pub fn push_mouse_up(&mut self) {
        self.pending.push(UiEvent::Mouse(MouseEvent::Up));
    }

    pub fn push_key_down(&mut self, code: &str) {
        self.pending.push(UiEvent::KeyDown(code.to_string()));
    }

    pub fn push_key_up(&mut self, code: &str) {
        self.pending.push(UiEvent::KeyUp(code.to_string()));
    }

    pub fn drain(&mut self) -> Vec<UiEvent> {
        std::mem::take(&mut self.pending)
    }
}

/// Cursor position plus which button (by index into the active screen's `elements`) is
/// hovered or has captured the press — «Интерфейс игры» → «Мышь».
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseState {
    pub position: [f32; 2],
    pub hover: Option<usize>,
    pub captured: Option<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_keeps_press_and_release_in_arrival_order_and_empties_queue() {
        let mut queue = InputQueue::new();
        queue.press("ArrowUp");
        queue.press("ArrowDown");
        queue.release("ArrowDown");
        let snap = queue.take_snapshot();
        assert_eq!(
            snap.events,
            vec![
                KeyEvent {
                    code: "ArrowUp".to_string(),
                    action: KeyAction::Press
                },
                KeyEvent {
                    code: "ArrowDown".to_string(),
                    action: KeyAction::Press
                },
                KeyEvent {
                    code: "ArrowDown".to_string(),
                    action: KeyAction::Release
                },
            ]
        );
        assert!(queue.take_snapshot().is_empty());
    }

    /// «Исполнение игры»: a release whose press never reached the queue must not reach it either
    /// — enforced here, not by every caller checking `held` first.
    #[test]
    fn release_of_a_key_never_pressed_is_dropped() {
        let mut queue = InputQueue::new();
        queue.release("ArrowDown");
        let snap = queue.take_snapshot();
        assert!(snap.is_empty());
    }

    /// «Исполнение игры»: a release and a press of the same key landing in the same gap must
    /// reach the step in that same order — the regression this guards against applied every
    /// release after every press, regardless of which actually came last.
    #[test]
    fn a_release_then_a_press_of_the_same_key_keeps_that_order() {
        let mut queue = InputQueue::new();
        queue.press("KeyA");
        queue.take_snapshot();
        queue.release("KeyA");
        queue.press("KeyA");
        let snap = queue.take_snapshot();
        assert_eq!(
            snap.events,
            vec![
                KeyEvent {
                    code: "KeyA".to_string(),
                    action: KeyAction::Release
                },
                KeyEvent {
                    code: "KeyA".to_string(),
                    action: KeyAction::Press
                },
            ]
        );
    }

    /// «Исполнение игры»: a press that repeats an already-held key queues nothing — the front
    /// edge already queued a `Press`, and browser auto-repeat should add no more of them.
    #[test]
    fn a_repeated_press_of_an_already_held_key_queues_nothing() {
        let mut queue = InputQueue::new();
        queue.press("KeyA");
        queue.take_snapshot();
        queue.press("KeyA");
        assert!(queue.take_snapshot().is_empty());
    }
}

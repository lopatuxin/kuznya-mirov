#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyAction {
    Press,
    Release,
}

#[derive(Debug, Clone)]
struct KeyEvent {
    code: String,
    action: KeyAction,
}

/// What a single step sees: which keys were pressed and which were released, frozen for the
/// duration of that step. During a burst of catch-up steps only the first one gets this;
/// the rest see `StepInput::empty()`, so a stuck spacebar does not fire five times.
#[derive(Debug, Clone, Default)]
pub struct StepInput {
    pub pressed: Vec<String>,
    pub released: Vec<String>,
}

impl StepInput {
    pub fn empty() -> Self {
        StepInput::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct InputQueue {
    pending: Vec<KeyEvent>,
    /// «Исполнение игры»: клавиши, нажатые прямо сейчас — служебный набор движка, не видимый
    /// правилам. Уходя с живого экрана, движок отпускает каждую из них, а не только ждёт
    /// естественного `release` от браузера.
    held: std::collections::HashSet<String>,
}

impl InputQueue {
    pub fn new() -> Self {
        InputQueue::default()
    }

    pub fn press(&mut self, code: &str) {
        self.pending.push(KeyEvent {
            code: code.to_string(),
            action: KeyAction::Press,
        });
        self.held.insert(code.to_string());
    }

    pub fn release(&mut self, code: &str) {
        self.pending.push(KeyEvent {
            code: code.to_string(),
            action: KeyAction::Release,
        });
        self.held.remove(code);
    }

    /// Stage 1: folds everything queued since the previous step into a fixed picture and
    /// empties the queue.
    pub fn take_snapshot(&mut self) -> StepInput {
        let mut snapshot = StepInput::default();
        for event in self.pending.drain(..) {
            match event.action {
                KeyAction::Press => snapshot.pressed.push(event.code),
                KeyAction::Release => snapshot.released.push(event.code),
            }
        }
        snapshot
    }

    /// Keys held right now, in no particular order — used only to synthesize the "release
    /// everything" step when the game leaves a live screen.
    pub fn held_keys(&self) -> Vec<String> {
        self.held.iter().cloned().collect()
    }

    /// Whether `code` is in the held set right now.
    pub fn is_held(&self, code: &str) -> bool {
        self.held.contains(code)
    }

    /// Forgets `code` entirely — used when its release is applied to the world immediately
    /// instead of going through the queue. The pending events go too: a press still waiting in
    /// the queue would otherwise be folded into the world a step *after* that release, switching
    /// the binding on with nothing left in `held` to ever switch it off again.
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

/// Mouse events are queued the same short way keys are: the page just appends, the engine
/// drains between steps. See «Интерфейс игры» → «Мышь».
#[derive(Debug, Clone, Default)]
pub struct MouseQueue {
    pending: Vec<MouseEvent>,
}

impl MouseQueue {
    pub fn new() -> Self {
        MouseQueue::default()
    }

    pub fn push_move(&mut self, x: f32, y: f32) {
        self.pending.push(MouseEvent::Move([x, y]));
    }

    pub fn push_down(&mut self) {
        self.pending.push(MouseEvent::Down);
    }

    pub fn push_up(&mut self) {
        self.pending.push(MouseEvent::Up);
    }

    pub fn drain(&mut self) -> Vec<MouseEvent> {
        std::mem::take(&mut self.pending)
    }
}

/// Cursor position plus which button (by index into the active screen's `elements`) is
/// hovered or has captured the press — «Интерфейс игры» → «Три состояния кнопки».
#[derive(Debug, Clone, Copy, Default)]
pub struct MouseState {
    pub position: [f32; 2],
    pub hover: Option<usize>,
    pub captured: Option<usize>,
}

/// Releases of a key the active screen declared in its own `keys` table — queued the same short
/// way mouse events are, and drained at the same step boundary, never through the world's input
/// pipeline. «Экраны и состояние» → «Клавиша экрана».
#[derive(Debug, Clone, Default)]
pub struct KeyQueue {
    pending: Vec<String>,
}

impl KeyQueue {
    pub fn new() -> Self {
        KeyQueue::default()
    }

    pub fn push_release(&mut self, code: &str) {
        self.pending.push(code.to_string());
    }

    pub fn drain(&mut self) -> Vec<String> {
        std::mem::take(&mut self.pending)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_splits_press_and_release_and_empties_queue() {
        let mut queue = InputQueue::new();
        queue.press("ArrowUp");
        queue.release("ArrowDown");
        let snap = queue.take_snapshot();
        assert_eq!(snap.pressed, vec!["ArrowUp".to_string()]);
        assert_eq!(snap.released, vec!["ArrowDown".to_string()]);
        assert!(queue.take_snapshot().pressed.is_empty());
    }
}

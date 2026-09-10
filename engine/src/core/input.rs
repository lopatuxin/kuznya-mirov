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
    }

    pub fn release(&mut self, code: &str) {
        self.pending.push(KeyEvent {
            code: code.to_string(),
            action: KeyAction::Release,
        });
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

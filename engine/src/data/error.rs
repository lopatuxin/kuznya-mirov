#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameError {
    pub file: String,
    pub path: String,
    pub message: String,
    /// 1-based, filled in by `ErrorSink::fill_locations` for a message whose `path` names a node
    /// this run's own text still holds — `None` when the path is empty (the file itself is
    /// missing or broken as JSON, cases with their own position already in `message`) or when the
    /// second-pass walk can't resolve it, rather than a wrong number.
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl GameError {
    pub fn new(
        file: impl Into<String>,
        path: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        GameError {
            file: file.into(),
            path: path.into(),
            message: message.into(),
            line: None,
            column: None,
        }
    }
}

/// A failed load: the game did not start, but the warnings collected up to that point are still
/// worth showing next to the errors that stopped it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoadFailure {
    pub errors: Vec<GameError>,
    pub warnings: Vec<GameError>,
}

/// Collects every prestart-check failure in one pass instead of stopping at the first one:
/// an outside model rewriting the game's files needs the whole list in a single round trip.
/// Warnings travel next to errors, collected just as completely, but never stop the game from
/// starting.
#[derive(Debug, Default)]
pub struct ErrorSink {
    errors: Vec<GameError>,
    warnings: Vec<GameError>,
}

impl ErrorSink {
    pub fn new() -> Self {
        ErrorSink::default()
    }

    pub fn push(&mut self, file: &str, path: &str, message: impl Into<String>) {
        self.errors.push(GameError::new(file, path, message));
    }

    /// Same as `push`, but with `line` already known rather than left for `fill_locations` to
    /// find by walking JSON — «Код игры»: a code error's line comes straight from the Lua error
    /// message, and `fill_locations`'s JSON-path walk has no path into a Lua file to find it with.
    pub fn push_at(
        &mut self,
        file: &str,
        path: &str,
        message: impl Into<String>,
        line: Option<usize>,
    ) {
        let mut error = GameError::new(file, path, message);
        error.line = line;
        self.errors.push(error);
    }

    pub fn push_warning(&mut self, file: &str, path: &str, message: impl Into<String>) {
        self.warnings.push(GameError::new(file, path, message));
    }

    pub fn has_no_errors(&self) -> bool {
        self.errors.is_empty()
    }

    /// Second pass over `file`'s own already-parsed source `text`: fills `line`/`column` on every
    /// collected message (error or warning) that names `file`, by walking `text` along that
    /// message's `path`. Safe to call once per file after every push for it is done.
    pub fn fill_locations(&mut self, file: &str, text: &str) {
        for error in self.errors.iter_mut().chain(self.warnings.iter_mut()) {
            if error.file != file {
                continue;
            }
            if let Some((line, column)) = super::locate::locate(text, &error.path) {
                error.line = Some(line);
                error.column = Some(column);
            }
        }
    }

    /// `(errors, warnings)`, for callers that need both channels regardless of whether the
    /// errors are empty.
    pub fn into_parts(self) -> (Vec<GameError>, Vec<GameError>) {
        (self.errors, self.warnings)
    }
}

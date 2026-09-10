//! Finds where a node named by a prestart-check path (`objects[3] → velocity`) sits inside the
//! file's own source text. `serde_json::Value` throws away node positions once parsed, so this
//! walks the raw bytes a second time along the same path instead of building a position-aware
//! tree. Only called on text that already parsed as JSON without error, so the grammar here never
//! has to handle malformed input — just skip past what the path doesn't name.

struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    line: usize,
    line_start: usize,
}

impl<'a> Cursor<'a> {
    fn new(text: &'a str) -> Self {
        Cursor {
            bytes: text.as_bytes(),
            pos: 0,
            line: 1,
            line_start: 0,
        }
    }

    /// 1-based line and column, the column counted in bytes since the last newline — the same
    /// convention `serde_json` uses for its own syntax-error positions, so the two kinds of
    /// messages never disagree.
    fn location(&self) -> (usize, usize) {
        (self.line, self.pos - self.line_start + 1)
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.pos).copied()
    }

    fn advance(&mut self, n: usize) {
        for _ in 0..n {
            if self.bytes.get(self.pos) == Some(&b'\n') {
                self.line += 1;
                self.line_start = self.pos + 1;
            }
            self.pos += 1;
        }
    }

    fn skip_ws(&mut self) {
        while matches!(self.peek(), Some(b' ' | b'\t' | b'\r' | b'\n')) {
            self.advance(1);
        }
    }

    fn skip_string(&mut self) -> Option<()> {
        self.advance(1); // opening quote
        loop {
            match self.peek()? {
                b'\\' => self.advance(2),
                b'"' => {
                    self.advance(1);
                    return Some(());
                }
                _ => self.advance(1),
            }
        }
    }

    /// Decodes an object key at the cursor via `serde_json` itself, rather than re-implementing
    /// JSON's escape rules here for a comparison that almost never needs them.
    fn read_key(&mut self) -> Option<String> {
        let start = self.pos;
        self.skip_string()?;
        let raw = std::str::from_utf8(&self.bytes[start..self.pos]).ok()?;
        serde_json::from_str::<String>(raw).ok()
    }

    fn skip_value(&mut self) -> Option<()> {
        self.skip_ws();
        match self.peek()? {
            b'"' => self.skip_string(),
            b'{' => self.skip_container(b'{', b'}'),
            b'[' => self.skip_container(b'[', b']'),
            _ => {
                while !matches!(
                    self.peek(),
                    None | Some(b',' | b'}' | b']' | b' ' | b'\t' | b'\r' | b'\n')
                ) {
                    self.advance(1);
                }
                Some(())
            }
        }
    }

    fn skip_container(&mut self, open: u8, close: u8) -> Option<()> {
        self.advance(1);
        self.skip_ws();
        if self.peek() == Some(close) {
            self.advance(1);
            return Some(());
        }
        loop {
            if open == b'{' {
                self.skip_ws();
                self.skip_string()?; // key, content unused here
                self.skip_ws();
                self.advance(1); // ':'
            }
            self.skip_value()?;
            self.skip_ws();
            match self.peek()? {
                b',' => self.advance(1),
                b if b == close => {
                    self.advance(1);
                    return Some(());
                }
                _ => return None,
            }
        }
    }
}

enum Segment<'a> {
    Key(&'a str),
    Index(usize),
}

/// Splits a path like `objects[3] → velocity` into `[Key("objects"), Index(3), Key("velocity")]`.
/// `None` on a shape the prestart check never actually produces (an index that isn't a number).
fn tokenize(path: &str) -> Option<Vec<Segment<'_>>> {
    let mut segments = Vec::new();
    for token in path.split(" → ") {
        let (key_part, mut rest) = match token.find('[') {
            Some(i) => (&token[..i], &token[i..]),
            None => (token, ""),
        };
        if !key_part.is_empty() {
            segments.push(Segment::Key(key_part));
        }
        while let Some(stripped) = rest.strip_prefix('[') {
            let end = stripped.find(']')?;
            segments.push(Segment::Index(stripped[..end].parse().ok()?));
            rest = &stripped[end + 1..];
        }
    }
    Some(segments)
}

fn walk(cursor: &mut Cursor, segments: &[Segment]) -> Option<(usize, usize)> {
    cursor.skip_ws();
    let Some((head, tail)) = segments.split_first() else {
        return Some(cursor.location());
    };
    match head {
        Segment::Key(key) => {
            if cursor.peek() != Some(b'{') {
                return None;
            }
            cursor.advance(1);
            cursor.skip_ws();
            if cursor.peek() == Some(b'}') {
                return None;
            }
            loop {
                cursor.skip_ws();
                let found_key = cursor.read_key()?;
                cursor.skip_ws();
                if cursor.peek() != Some(b':') {
                    return None;
                }
                cursor.advance(1);
                cursor.skip_ws();
                if found_key == *key {
                    return walk(cursor, tail);
                }
                cursor.skip_value()?;
                cursor.skip_ws();
                match cursor.peek()? {
                    b',' => cursor.advance(1),
                    _ => return None,
                }
            }
        }
        Segment::Index(target) => {
            if cursor.peek() != Some(b'[') {
                return None;
            }
            cursor.advance(1);
            cursor.skip_ws();
            if cursor.peek() == Some(b']') {
                return None;
            }
            let mut i = 0usize;
            loop {
                cursor.skip_ws();
                if i == *target {
                    return walk(cursor, tail);
                }
                cursor.skip_value()?;
                cursor.skip_ws();
                i += 1;
                match cursor.peek()? {
                    b',' => cursor.advance(1),
                    _ => return None,
                }
            }
        }
    }
}

/// The 1-based `(line, column)` of the node `path` names inside `text`, in the same notation
/// `join()` in `load.rs` builds (`" → "` between segments, `[N]` for an array index). `None` for
/// the empty path (the root — used by the two message kinds this locator never touches: a missing
/// file and a file broken as JSON, which already carries its own position in the message text)
/// and for any path this walk can't resolve, rather than an invented position.
pub(crate) fn locate(text: &str, path: &str) -> Option<(usize, usize)> {
    if path.is_empty() {
        return None;
    }
    let segments = tokenize(path)?;
    let mut cursor = Cursor::new(text);
    walk(&mut cursor, &segments)
}

#[cfg(test)]
mod tests {
    use super::locate;

    #[test]
    fn locates_a_top_level_key() {
        let text = r#"{"a": 1, "b": true}"#;
        let expected_col = text.find("true").unwrap() + 1;
        assert_eq!(locate(text, "b"), Some((1, expected_col)));
    }

    #[test]
    fn locates_inside_nested_arrays_and_objects() {
        let text = "{\"objects\":[{\"position\":[1,2]},{\"velocity\":[3,4]}]}";
        let expected_col = text.find("[3,4]").unwrap() + 1;
        assert_eq!(
            locate(text, "objects[1] → velocity"),
            Some((1, expected_col))
        );
    }

    #[test]
    fn counts_lines_across_newlines() {
        let text = "{\n  \"a\": {\n    \"b\": 5\n  }\n}\n";
        let expected_col = "    \"b\": ".len() + 1;
        assert_eq!(locate(text, "a → b"), Some((3, expected_col)));
    }

    #[test]
    fn missing_key_returns_none() {
        assert_eq!(locate(r#"{"a": 1}"#, "missing"), None);
    }

    #[test]
    fn out_of_range_index_returns_none() {
        assert_eq!(locate(r#"{"items": [1, 2]}"#, "items[5]"), None);
    }

    #[test]
    fn empty_path_returns_none() {
        assert_eq!(locate(r#"{"a": 1}"#, ""), None);
    }

    #[test]
    fn key_with_escaped_characters_still_matches() {
        let text = r#"{"a\"b": 42}"#;
        let expected_col = text.find("42").unwrap() + 1;
        assert_eq!(locate(text, "a\"b"), Some((1, expected_col)));
    }

    /// The column is a byte count, the same convention `serde_json`'s own syntax errors use (see
    /// `Cursor::location`) — a Cyrillic key or value before the target has multi-byte characters,
    /// so a column counted in `char`s instead would disagree with `serde_json` and land short.
    #[test]
    fn cyrillic_key_and_value_are_located_by_byte_column() {
        let text = r##"{"имя": "Кузня Миров", "цвет": "#12141a"}"##;
        let expected_col = text.find("\"#12141a\"").unwrap() + 1;
        assert_eq!(locate(text, "цвет"), Some((1, expected_col)));
    }
}

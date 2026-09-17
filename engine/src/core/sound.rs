//! «Звук» → «Окно чисел»: the mark window shared with the page — a
//! small, fixed-size block of numbers a step can only write into, cleared once per call and
//! read only by the page, after the call has already returned control to it.

use super::rules::SoundId;
use super::screens::MusicId;

const ENABLED_INDEX: usize = 0;
const MUSIC_INDEX: usize = 1;
const HEADER_LEN: usize = 2;

/// The only thing a step may do with sound: raise a mark, never read one back — not even one it
/// just raised itself. «Звук» → «Звук ничего не решает»: this type is
/// the guard, not a convention that a rule could get around.
pub struct SoundMarks<'a> {
    marks: &'a mut [i32],
}

impl SoundMarks<'_> {
    /// Idempotent — «Одно исполнение или одна отметка: звук не счётчик»: raising the same id ten
    /// times in one call leaves exactly the mark the first raise already left.
    pub fn raise(&mut self, id: SoundId) {
        if let Some(slot) = self.marks.get_mut(id) {
            *slot = 1;
        }
    }
}

/// The window of numbers the page reads after every call — «Звук» → «Окно чисел»: `[0]` whether sound is enabled, `[1]` the music id to play or `-1` for
/// silence, `[2..]` one mark per declared sound, indexed by `SoundId`. Sized once, from the
/// game's `files.sounds` table, and never reallocated after that — «память под набор не
/// выделяется никогда» вне загрузки.
#[derive(Debug)]
pub struct SoundWindow {
    cells: Vec<i32>,
}

impl SoundWindow {
    pub fn new(sound_count: usize) -> Self {
        let mut cells = vec![0; HEADER_LEN + sound_count];
        cells[ENABLED_INDEX] = 1;
        cells[MUSIC_INDEX] = -1;
        SoundWindow { cells }
    }

    /// «Порядок работ за один вызов», пункт 1: clears every mark at the start of a call, before
    /// its first step — never at the end, or the page would lose marks it hasn't read yet.
    /// Leaves the header untouched; `write_header` overwrites it later in the same call.
    pub fn clear_marks(&mut self) {
        for slot in &mut self.cells[HEADER_LEN..] {
            *slot = 0;
        }
    }

    /// Hands out the write-only handle a step gets — never the window itself.
    pub fn marks(&mut self) -> SoundMarks<'_> {
        SoundMarks {
            marks: &mut self.cells[HEADER_LEN..],
        }
    }

    /// «Порядок работ за один вызов», пункт 5: written once per call, after the mouse and screen
    /// keys are processed, so a click that just switched screens writes the screen it landed on,
    /// not the one it left.
    pub fn write_header(&mut self, music: Option<MusicId>, enabled: bool) {
        self.cells[ENABLED_INDEX] = enabled as i32;
        self.cells[MUSIC_INDEX] = music.map_or(-1, |id| id as i32);
    }

    /// Read side, for the circle and for tests — a step never sees this, only `SoundMarks`.
    pub fn mark(&self, id: SoundId) -> bool {
        self.cells.get(HEADER_LEN + id).is_some_and(|&v| v != 0)
    }

    pub fn music(&self) -> Option<MusicId> {
        let raw = self.cells[MUSIC_INDEX];
        (raw >= 0).then_some(raw as MusicId)
    }

    pub fn sound_enabled(&self) -> bool {
        self.cells[ENABLED_INDEX] != 0
    }

    /// «Звук»: the wasm layer's `sound_window_ptr()`/`sound_window_len()` hand
    /// this straight to the page — one contiguous block of `i32`s, header first.
    pub fn as_ptr(&self) -> *const i32 {
        self.cells.as_ptr()
    }

    pub fn cell_count(&self) -> usize {
        self.cells.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_window_starts_sound_enabled_with_no_music_before_any_write_header() {
        let window = SoundWindow::new(3);
        assert!(window.sound_enabled());
        assert_eq!(window.music(), None);
    }

    #[test]
    fn raising_the_same_id_ten_times_leaves_one_mark() {
        let mut window = SoundWindow::new(3);
        {
            let mut marks = window.marks();
            for _ in 0..10 {
                marks.raise(1);
            }
        }
        assert!(!window.mark(0));
        assert!(window.mark(1));
        assert!(!window.mark(2));
    }

    #[test]
    fn clear_marks_resets_marks_but_not_the_header() {
        let mut window = SoundWindow::new(2);
        window.write_header(Some(4), true);
        window.marks().raise(0);
        window.clear_marks();
        assert!(!window.mark(0));
        assert_eq!(window.music(), Some(4));
        assert!(window.sound_enabled());
    }

    #[test]
    fn write_header_encodes_silence_as_no_music() {
        let mut window = SoundWindow::new(1);
        window.write_header(None, false);
        assert_eq!(window.music(), None);
        assert!(!window.sound_enabled());
    }

    #[test]
    fn raising_an_out_of_range_id_does_not_panic() {
        let mut window = SoundWindow::new(1);
        window.marks().raise(5);
    }
}

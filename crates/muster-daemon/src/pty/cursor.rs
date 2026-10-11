//! The cursor position, for programs that ask for it (`CSI 6 n`).
//!
//! A program asking where its cursor is (crossterm's `cursor::position()`,
//! which inline viewports and line editors call at startup) waits for the
//! answer and gives up after about two seconds. The answer is screen state,
//! and the daemon keeps bytes, not a screen. So a session that asks gets a
//! screen of its own, built from its output so far at the first question
//! and kept up to date after that; sessions that never ask parse nothing.
//!
//! Adapted from Nebula's `pty/cursor.rs` (MIT, Copyright (c) 2026 Cody
//! Seibert; see NOTICE).

/// vt100 underflows below two cells a side, and a client can ask for a
/// terminal squeezed to nothing.
const MIN_SIDE: u16 = 2;

pub(crate) struct CursorTracker {
    /// No scrollback: only the cursor is ever read.
    parser: vt100::Parser,
}

impl std::fmt::Debug for CursorTracker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CursorTracker")
            .field("cursor", &self.parser.screen().cursor_position())
            .finish()
    }
}

impl CursorTracker {
    /// A screen at the terminal's size with `history` already drawn on it.
    pub(crate) fn new(cols: u16, rows: u16, history: &[u8]) -> Self {
        let (cols, rows) = floor(cols, rows);
        let mut parser = vt100::Parser::new(rows, cols, 0);
        parser.process(history);
        Self { parser }
    }

    pub(crate) fn feed(&mut self, data: &[u8]) {
        self.parser.process(data);
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        let (cols, rows) = floor(cols, rows);
        self.parser.screen_mut().set_size(rows, cols);
    }

    /// The answer to `CSI 6 n`: `CSI row ; col R`, counting from 1.
    pub(crate) fn report(&self) -> Vec<u8> {
        let screen = self.parser.screen();
        let (row, col) = screen.cursor_position();
        // After writing the last column, vt100 parks the cursor one past it
        // until the wrap happens; a terminal reports the last column.
        let col = col.min(screen.size().1 - 1);
        format!("\x1b[{};{}R", row + 1, col + 1).into_bytes()
    }
}

fn floor(cols: u16, rows: u16) -> (u16, u16) {
    (cols.max(MIN_SIDE), rows.max(MIN_SIDE))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_places_the_cursor() {
        assert_eq!(
            CursorTracker::new(80, 24, b"$ muse\r\nhello").report(),
            b"\x1b[2;6R"
        );
    }

    #[test]
    fn output_moves_it() {
        let mut tracker = CursorTracker::new(80, 24, b"");
        assert_eq!(tracker.report(), b"\x1b[1;1R");
        tracker.feed(b"\x1b[10;20H");
        assert_eq!(tracker.report(), b"\x1b[10;20R");
    }

    #[test]
    fn a_pending_wrap_reports_the_last_column() {
        let mut tracker = CursorTracker::new(5, 3, b"abcde");
        assert_eq!(tracker.report(), b"\x1b[1;5R");
        tracker.feed(b"f");
        assert_eq!(tracker.report(), b"\x1b[2;2R");
    }

    #[test]
    fn a_resize_clamps_the_cursor() {
        let mut tracker = CursorTracker::new(80, 24, b"\x1b[20;70H");
        tracker.resize(40, 10);
        assert_eq!(tracker.report(), b"\x1b[10;40R");
    }

    #[test]
    fn a_squeezed_terminal_does_not_panic() {
        let nasty = "wrap wrap\r\n漢字😀\x1b[999;999Hx\n\n".as_bytes();
        for (cols, rows) in [(0, 0), (1, 1), (1, 5), (5, 1), (2, 2)] {
            let mut tracker = CursorTracker::new(cols, rows, nasty);
            tracker.resize(cols, rows);
            tracker.feed(nasty);
            let corner = format!("\x1b[{};{}R", rows.max(2), cols.max(2));
            assert_eq!(tracker.report(), corner.as_bytes(), "{cols}x{rows}");
        }
    }
}

//! The question a session asking the person shows in *Needs you*: the
//! last lines of its screen, read from its terminal stream as the terminal
//! UI reads a session's (vt100), so the person can decide without opening
//! it (`PRODUCT.md` §6a; the design system's `QueueItem`).

use muster_core::protocol::TermSize;

use super::streams::{Cursor, Piece};

/// The lines a question shows, at most (`QueueItem`: "at most four").
pub(crate) const LINES: usize = 4;

/// The size a screen is read at until the webview sizes the session's
/// terminal: as tall and as wide as a large window's, so a program that
/// draws by place rather than line by line lands within it, as it would on
/// the person's screen.
const ROWS: u16 = 100;
const COLS: u16 = 250;

/// A session's screen, read for its question.
pub(super) struct Screen {
    parser: vt100::Parser,
    cursor: Cursor,
    /// The question as last read, while nothing new came.
    lines: Vec<String>,
    /// Bytes came since `lines` was read.
    dirty: bool,
}

impl Screen {
    /// An empty screen of the size the session's terminal has, if known.
    pub(super) fn new(size: Option<TermSize>) -> Self {
        let (rows, cols) = size.map_or((ROWS, COLS), |size| (size.rows, size.cols));
        Self {
            parser: vt100::Parser::new(rows.max(1), cols.max(1), 0),
            cursor: Cursor::default(),
            lines: Vec::new(),
            dirty: false,
        }
    }

    pub(super) fn cursor(&self) -> Cursor {
        self.cursor
    }

    /// The connection went: the next replay starts it over.
    pub(super) fn go_stale(&mut self) {
        self.cursor.go_stale();
    }

    /// The stream (re)starts; `true` if the screen changed.
    pub(super) fn replay(&mut self, seq: u64, data: &[u8], reset: bool, asked: bool) -> bool {
        let piece = self.cursor.replay(seq, data, reset, asked);
        self.take(&piece)
    }

    /// More output; `true` if the screen changed.
    pub(super) fn output(&mut self, seq: u64, data: &[u8]) -> bool {
        let piece = self.cursor.output(seq, data);
        self.take(&piece)
    }

    fn take(&mut self, piece: &Piece<'_>) -> bool {
        match *piece {
            Piece::Reset { data, .. } => {
                let (rows, cols) = self.parser.screen().size();
                self.parser = vt100::Parser::new(rows, cols, 0);
                self.parser.process(data);
            }
            Piece::More { data, .. } => self.parser.process(data),
            Piece::Nothing => return false,
        }
        self.dirty = true;
        true
    }

    /// The session's program now draws for `size`.
    pub(super) fn resize(&mut self, size: TermSize) {
        self.parser
            .screen_mut()
            .set_size(size.rows.max(1), size.cols.max(1));
        self.dirty = true;
    }

    /// Its question: the last [`LINES`] lines with anything on them.
    pub(super) fn lines(&mut self) -> &[String] {
        if std::mem::take(&mut self.dirty) {
            self.lines = last_lines(self.parser.screen(), LINES);
        }
        &self.lines
    }
}

/// The last `count` lines of `screen` with anything on them, oldest first,
/// each without its trailing blanks or any control character, which drawn
/// would reach the person's screen as a command.
pub(crate) fn last_lines(screen: &vt100::Screen, count: usize) -> Vec<String> {
    let (_, cols) = screen.size();
    let mut lines: Vec<String> = screen
        .rows(0, cols)
        .map(|row| {
            row.chars()
                .filter(|c| !c.is_control())
                .collect::<String>()
                .trim_end()
                .to_owned()
        })
        .filter(|line| !line.trim().is_empty())
        .collect();
    let keep = lines.len().saturating_sub(count);
    lines.drain(..keep);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn screen(text: &str) -> Screen {
        let mut screen = Screen::new(Some(TermSize { cols: 40, rows: 10 }));
        screen.replay(0, text.as_bytes(), false, false);
        screen
    }

    #[test]
    fn a_question_is_the_last_four_lines_with_anything_on_them() {
        let mut screen = screen(
            "Building\r\n\r\nmuster-desktop\r\n  \r\nAllow Codex to run this command?\r\n\r\n$ cargo build -p muster-desktop\r\n› Yes\r\n  No\r\n",
        );
        assert_eq!(
            screen.lines(),
            [
                "Allow Codex to run this command?",
                "$ cargo build -p muster-desktop",
                "› Yes",
                "  No",
            ]
        );
    }

    #[test]
    fn a_screen_with_little_on_it_shows_what_it_has() {
        assert_eq!(screen("Where?\r\n").lines(), ["Where?"]);
        assert!(screen("\r\n\r\n   \r\n").lines().is_empty());
    }

    #[test]
    fn colours_and_moves_are_read_as_the_screen_shows_them() {
        // A line drawn in colour, then a line rewritten in place.
        let mut screen = screen("\x1b[33mWhich one?\x1b[0m\r\nfirst draft\r\x1b[2Ksecond\r\n");
        assert_eq!(screen.lines(), ["Which one?", "second"]);
    }

    #[test]
    fn more_output_changes_the_question_and_what_came_before_is_not_read_twice() {
        let mut screen = screen("one\r\n");
        assert!(screen.output(5, b"two\r\n"));
        assert!(!screen.output(5, b"two\r\n"), "bytes it has already");
        assert!(screen.output(7, b"o\r\nthree\r\n"), "partly new");
        assert_eq!(screen.lines(), ["one", "two", "three"]);
    }

    #[test]
    fn a_reset_starts_the_screen_over() {
        let mut screen = screen("old\r\n");
        assert!(screen.replay(0, b"new\r\n", true, false));
        assert_eq!(screen.lines(), ["new"]);
    }
}

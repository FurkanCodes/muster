//! A session's screen as this client draws it: a terminal emulator fed by
//! the daemon's replay and output, and the program's input modes.

use muster_core::protocol::{DetachReason, TerminalModes};

/// Lines kept above the visible screen.
const SCROLLBACK: usize = 2_000;

pub(crate) struct Screen {
    parser: vt100::Parser,
    /// The position of the next byte expected, once a replay has arrived.
    next_seq: Option<u64>,
    pub(crate) modes: TerminalModes,
    /// Why the stream ended, once it has.
    pub(crate) ended: Option<DetachReason>,
}

impl std::fmt::Debug for Screen {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Screen")
            .field("size", &self.parser.screen().size())
            .field("next_seq", &self.next_seq)
            .field("ended", &self.ended)
            .finish_non_exhaustive()
    }
}

impl Screen {
    pub(crate) fn new(cols: u16, rows: u16) -> Self {
        Self {
            parser: vt100::Parser::new(rows.max(2), cols.max(2), SCROLLBACK),
            next_seq: None,
            modes: TerminalModes::default(),
            ended: None,
        }
    }

    pub(crate) fn screen(&self) -> &vt100::Screen {
        self.parser.screen()
    }

    /// Where to ask the daemon to resume from.
    pub(crate) fn next_seq(&self) -> Option<u64> {
        self.next_seq
    }

    /// The start of a stream. With `reset`, or when there is a gap between
    /// what this screen shows and the replay, the screen starts over; a
    /// replay that overlaps it (frames already on their way when the client
    /// attached again) adds only what is new.
    pub(crate) fn replay(&mut self, seq: u64, data: &[u8], reset: bool) {
        self.ended = None;
        let end = seq + data.len() as u64;
        match self.next_seq {
            Some(next) if !reset && (seq..=end).contains(&next) => {
                let skip = usize::try_from(next - seq).unwrap_or(usize::MAX);
                self.parser.process(&data[skip.min(data.len())..]);
            }
            // Wholly behind what is shown: nothing new.
            Some(next) if !reset && end < next => return,
            _ => {
                let (rows, cols) = self.parser.screen().size();
                self.parser = vt100::Parser::new(rows, cols, SCROLLBACK);
                self.parser.process(data);
            }
        }
        self.next_seq = Some(end);
    }

    /// Output that follows the replay. Bytes already shown are skipped.
    pub(crate) fn output(&mut self, seq: u64, data: &[u8]) {
        let skip = self.next_seq.map_or(0, |next| {
            usize::try_from(next.saturating_sub(seq)).unwrap_or(usize::MAX)
        });
        if let Some(fresh) = data.get(skip..) {
            self.parser.process(fresh);
        }
        self.next_seq = Some(seq + data.len() as u64);
    }

    pub(crate) fn resize(&mut self, cols: u16, rows: u16) {
        self.parser.screen_mut().set_size(rows.max(2), cols.max(2));
    }

    /// Moves the view `lines` back into the scrollback, or forward with a
    /// negative count; never past either end. The view stays on the same
    /// lines as output arrives, until it returns to the bottom.
    pub(crate) fn scroll(&mut self, lines: isize) {
        let at = self.scrolled_back();
        let to = at.saturating_add_signed(lines);
        self.parser.screen_mut().set_scrollback(to);
    }

    /// How many lines back the view is; 0 at the bottom.
    pub(crate) fn scrolled_back(&self) -> usize {
        self.parser.screen().scrollback()
    }

    pub(crate) fn scroll_to_bottom(&mut self) {
        self.parser.screen_mut().set_scrollback(0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(screen: &Screen) -> String {
        screen.screen().contents()
    }

    #[test]
    fn output_follows_the_replay_and_overlap_is_skipped() {
        let mut screen = Screen::new(20, 4);
        screen.replay(0, b"hello ", true);
        screen.output(6, b"world");
        screen.output(8, b"rld!");
        assert_eq!(text(&screen).trim(), "hello world!");
        assert_eq!(screen.next_seq(), Some(12));
    }

    #[test]
    fn the_view_scrolls_back_within_what_was_kept_and_holds_still() {
        let mut screen = Screen::new(10, 3);
        let lines: String = (1..=10).map(|n| format!("line{n}\r\n")).collect();
        screen.replay(0, lines.as_bytes(), true);
        assert_eq!(screen.scrolled_back(), 0);
        screen.scroll(2);
        assert_eq!(screen.scrolled_back(), 2);
        assert!(text(&screen).starts_with("line7"), "{}", text(&screen));
        // New output does not move what is being read.
        screen.output(70, b"line11\r\n");
        assert!(text(&screen).starts_with("line7"), "{}", text(&screen));
        // Not past the oldest line kept, nor below the bottom.
        screen.scroll(1000);
        assert!(text(&screen).starts_with("line1\n"), "{}", text(&screen));
        screen.scroll(-2000);
        assert_eq!(screen.scrolled_back(), 0);
        screen.scroll(4);
        screen.scroll_to_bottom();
        assert_eq!(screen.scrolled_back(), 0);
    }

    #[test]
    fn a_replay_overlapping_the_screen_adds_only_what_is_new() {
        let mut screen = Screen::new(20, 4);
        screen.replay(0, b"start ", true);
        screen.output(6, b"middle ");
        // Attached again from 6 while the output above was on its way.
        screen.replay(6, b"middle end", false);
        assert_eq!(text(&screen).trim(), "start middle end");
        assert_eq!(screen.next_seq(), Some(16));
        // Entirely old: changes nothing.
        screen.replay(0, b"start", false);
        assert_eq!(text(&screen).trim(), "start middle end");
        assert_eq!(screen.next_seq(), Some(16));
    }

    #[test]
    fn a_reset_or_a_gap_starts_the_screen_over() {
        let mut screen = Screen::new(20, 4);
        screen.replay(0, b"old", true);
        screen.replay(100, b"new", false);
        assert_eq!(text(&screen).trim(), "new");
        screen.replay(103, b"er", false);
        assert_eq!(text(&screen).trim(), "newer");
        screen.replay(105, b"fresh", true);
        assert_eq!(text(&screen).trim(), "fresh");
    }
}

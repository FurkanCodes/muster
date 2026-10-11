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

    /// The start of a stream. With `reset`, or when it does not follow on
    /// from what this screen already shows, the screen starts over.
    pub(crate) fn replay(&mut self, seq: u64, data: &[u8], reset: bool) {
        if reset || self.next_seq != Some(seq) {
            let (rows, cols) = self.parser.screen().size();
            self.parser = vt100::Parser::new(rows, cols, SCROLLBACK);
        }
        self.ended = None;
        self.parser.process(data);
        self.next_seq = Some(seq + data.len() as u64);
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

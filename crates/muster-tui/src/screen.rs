//! A session's screen as this client draws it: a terminal emulator fed by
//! the daemon's replay and output, and the program's input modes. It also
//! keeps what the program copied (OSC 52) in its live output, for the
//! person's clipboard (see `clipboard`).

use muster_core::protocol::{DetachReason, TerminalModes};

/// Lines kept above the visible screen.
const SCROLLBACK: usize = 2_000;
/// The start and end of a synchronized update (DEC mode 2026): a program
/// draws a frame between them, to be shown once it is whole.
const FRAME_START: &[u8] = b"\x1b[?2026h";
const FRAME_END: &[u8] = b"\x1b[?2026l";

/// What the program copied while its output was live: what it writes as
/// the person looks on, never its history replayed on attaching.
#[derive(Debug, Default)]
struct Copies {
    live: bool,
    /// The last copy, once read as text.
    copied: Option<String>,
}

impl vt100::Callbacks for Copies {
    fn copy_to_clipboard(&mut self, _: &mut vt100::Screen, _selection: &[u8], data: &[u8]) {
        // Any selection: the person has the one clipboard.
        if self.live
            && let Some(text) = crate::clipboard::from_program(data)
        {
            self.copied = Some(text);
        }
    }

    // A program asking to read the clipboard (`52;c;?`) is never answered:
    // the default does nothing.
}

pub(crate) struct Screen {
    parser: vt100::Parser<Copies>,
    /// The position of the next byte expected, once a replay has arrived.
    next_seq: Option<u64>,
    pub(crate) modes: TerminalModes,
    /// Why the stream ended, once it has.
    pub(crate) ended: Option<DetachReason>,
    /// The program is drawing a frame (a synchronized update), not yet
    /// whole.
    in_frame: bool,
    /// The last bytes of output, for a frame's start or end split across
    /// two chunks.
    tail: Vec<u8>,
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
            parser: parser(rows.max(2), cols.max(2)),
            next_seq: None,
            modes: TerminalModes::default(),
            ended: None,
            in_frame: false,
            tail: Vec::new(),
        }
    }

    /// Whether the program is drawing a frame it has not finished: one
    /// shown now would be half of it.
    pub(crate) fn in_frame(&self) -> bool {
        self.in_frame
    }

    /// Follows the program's synchronized updates through `data`: the last
    /// start or end in it says where the program is.
    fn track_frames(&mut self, data: &[u8]) {
        let mut seen = std::mem::take(&mut self.tail);
        seen.extend_from_slice(data);
        let last = |marker: &[u8]| {
            seen.windows(marker.len())
                .rposition(|window| window == marker)
        };
        match (last(FRAME_START), last(FRAME_END)) {
            (Some(start), Some(end)) => self.in_frame = start > end,
            (Some(_), None) => self.in_frame = true,
            (None, Some(_)) => self.in_frame = false,
            (None, None) => {}
        }
        let keep = seen.len().saturating_sub(FRAME_START.len() - 1);
        self.tail = seen.split_off(keep);
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
                let fresh = &data[skip.min(data.len())..];
                self.process(fresh, false);
                self.track_frames(fresh);
            }
            // Wholly behind what is shown: nothing new.
            Some(next) if !reset && end < next => return,
            _ => {
                let (rows, cols) = self.parser.screen().size();
                self.parser = parser(rows, cols);
                self.process(data, false);
                (self.in_frame, self.tail) = (false, Vec::new());
                self.track_frames(data);
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
            self.process(fresh, true);
            self.track_frames(fresh);
        }
        self.next_seq = Some(seq + data.len() as u64);
    }

    /// Feeds the emulator; copies count only from `live` output.
    fn process(&mut self, data: &[u8], live: bool) {
        self.parser.callbacks_mut().live = live;
        self.parser.process(data);
        self.parser.callbacks_mut().live = false;
    }

    /// What the program last copied in its live output since this was last
    /// asked, if anything.
    pub(crate) fn take_copy(&mut self) -> Option<String> {
        self.parser.callbacks_mut().copied.take()
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

fn parser(rows: u16, cols: u16) -> vt100::Parser<Copies> {
    vt100::Parser::new_with_callbacks(rows, cols, SCROLLBACK, Copies::default())
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
        #[expect(clippy::format_collect, reason = "test input")]
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

    #[test]
    fn a_frame_in_progress_is_known_until_it_ends_also_when_split() {
        let mut screen = Screen::new(20, 4);
        screen.replay(0, b"old", true);
        let feed = |screen: &mut Screen, data: &[u8]| {
            let seq = screen.next_seq().unwrap();
            screen.output(seq, data);
            screen.in_frame()
        };
        assert!(!screen.in_frame());
        assert!(feed(&mut screen, b"\x1b[?2026h\x1b[Hnew"));
        assert!(
            feed(&mut screen, b" frame\x1b[?20"),
            "its end has not all arrived"
        );
        assert!(!feed(&mut screen, b"26l"));
        // A frame that starts across two chunks, and one that starts and
        // ends in one.
        assert!(!feed(&mut screen, b"\x1b[?2"));
        assert!(feed(&mut screen, b"026hx"));
        assert!(!feed(&mut screen, b"\x1b[?2026l\x1b[?2026hy\x1b[?2026l"));
        assert_eq!(text(&screen).trim(), "new framexy");
        // A replay that starts over forgets a frame cut off.
        assert!(feed(&mut screen, b"\x1b[?2026h"));
        screen.replay(0, b"fresh", true);
        assert!(!screen.in_frame());
    }
}

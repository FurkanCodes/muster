//! Answers the questions a program asks its terminal.
//!
//! A session's program talks to a terminal nobody is sitting at: the daemon
//! holds it, and a client draws a copy. Programs ask the terminal things at
//! startup and wait for the answer, so the daemon answers. Claude Code
//! 2.1.291 asks three, captured on a bare terminal: keyboard-protocol support
//! (`CSI ? u`), device attributes (`CSI c`) and the terminal's version
//! (`CSI > 0 q`). Claude Code 2.1.296, once the version is answered, also
//! asks which private modes the terminal has (DECRQM, `CSI ? Ps $ p`), for
//! synchronized output (2026) among them: each mode tracked here is answered
//! as set or reset, any other as not recognized, so nothing waits out a
//! timeout. Clients honour synchronized output: they draw a frame once the
//! program has finished it. Questions about the terminal's colours (OSC 10,
//! 11 and 4) are found here too, and answered from the person's terminal's
//! colours (see `colours`).
//!
//! Questions about the terminal's size in pixels (XTWINOPS 14 and 16) are
//! answered from the size a client gave, where it gave one.
//!
//! Also tracked: whether the program asked to be told as its terminal
//! gains and loses focus (`CSI ? 1004 h`), which the actor tells it as
//! the person's keys come and go. And, for clients: the kitty keyboard
//! flags the program pushes,
//! whether it turned bracketed paste on, whether cursor keys are in
//! application mode, which mouse events it wants and how encoded, and
//! whether it is on the alternate screen; these decide how a client encodes
//! keys, pastes and the mouse, and whether it can scroll back. A client
//! parsing the output could see the modes itself, but not once the ring has
//! dropped the bytes that set them. Mouse modes follow xterm as the `vt100`
//! crate reads it: setting one replaces the last, and resetting one turns
//! tracking off only if it is the one in effect.
//!
//! Adapted from Nebula's `pty/kitty.rs` (MIT, Copyright (c) 2026 Cody
//! Seibert; see NOTICE). Changed: the version query is answered too (Nebula
//! leaves it unanswered, so a program waits out its timeout), application
//! cursor mode is tracked, and the scanner is renamed for what it does.

use muster_core::protocol::{MouseEncoding, MouseTracking};

use super::colours::{self, Colour, Ending};

const ESC: u8 = 0x1b;
/// The deepest kitty flag stack the spec asks implementations to keep.
const MAX_STACK: usize = 32;
/// A real query has short parameters; anything longer is not for us.
const MAX_PARAMS: usize = 16;
/// The longest OSC body read: a question about all sixteen palette colours
/// at once fits; a longer one (a copy, a picture) is passed over.
const MAX_OSC: usize = 128;

/// Answers owed to the program, in the order it asked: programs that probe
/// for a feature key on which answer arrives first.
#[derive(Debug, Default, PartialEq, Eq)]
pub(crate) struct Answers {
    pub(crate) replies: Vec<Reply>,
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Reply {
    /// Known from the scanner alone.
    Bytes(Vec<u8>),
    /// The cursor's position on the screen as it stood `at` bytes into the
    /// scanned chunk, just past the question. Needs a screen; see `cursor`.
    CursorPosition { at: usize },
    /// One of the terminal's colours, ended as the question was. Needs the
    /// person's terminal's colours; see `colours`.
    Colour { colour: Colour, ending: Ending },
    /// The terminal's size in pixels (XTWINOPS 14), or with `cell` one
    /// cell's (16). Needs the terminal's size as a client gave it.
    PixelSize { cell: bool },
}

impl Answers {
    fn reply(&mut self, bytes: &[u8]) {
        match self.replies.last_mut() {
            Some(Reply::Bytes(pending)) => pending.extend_from_slice(bytes),
            _ => self.replies.push(Reply::Bytes(bytes.to_vec())),
        }
    }
}

#[derive(Clone, Copy, Debug)]
enum State {
    Ground,
    Escape,
    /// Inside a CSI. Parameters collect, then at most one intermediate
    /// byte, as DECRQM has (`$`); more than that, or too many parameters,
    /// poison it, since no question we answer has them.
    Csi {
        poisoned: bool,
        intermediate: Option<u8>,
    },
    /// Inside an OSC, its body collecting; one too long for a question is
    /// poisoned, and read on to its end.
    Osc {
        poisoned: bool,
    },
    /// Saw ESC inside an OSC: `\` ends it, anything else starts another
    /// sequence.
    OscEscape {
        poisoned: bool,
    },
}

#[derive(Debug)]
pub(crate) struct QueryScanner {
    state: State,
    params: Vec<u8>,
    /// The body of the OSC being read.
    osc: Vec<u8>,
    kitty_flags: Vec<u8>,
    bracketed_paste: bool,
    /// Whether bracketed paste was ever turned on, even within one chunk.
    paste_used: bool,
    application_cursor: bool,
    mouse: MouseTracking,
    mouse_encoding: MouseEncoding,
    alternate_screen: bool,
    /// Inside a synchronized update (`CSI ? 2026 h`): the program is
    /// drawing a frame.
    synchronized: bool,
    /// The program asked to be told of focus (`CSI ? 1004 h`).
    focus_events: bool,
}

/// The input modes a program set, carried across the daemon's restart in
/// place: the bytes that set them may long have left the ring, and a client
/// encodes keys, pastes and the mouse by them (Shift+Enter in Claude Code
/// needs the kitty flags).
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct CarriedModes {
    kitty_flags: Vec<u8>,
    bracketed_paste: bool,
    paste_used: bool,
    application_cursor: bool,
    mouse: MouseTracking,
    mouse_encoding: MouseEncoding,
    alternate_screen: bool,
    /// Missing from a daemon before focus events, which had none.
    #[serde(default)]
    focus_events: bool,
}

impl QueryScanner {
    /// The modes to carry.
    pub(crate) fn carry(&self) -> CarriedModes {
        CarriedModes {
            kitty_flags: self.kitty_flags.clone(),
            bracketed_paste: self.bracketed_paste,
            paste_used: self.paste_used,
            application_cursor: self.application_cursor,
            mouse: self.mouse,
            mouse_encoding: self.mouse_encoding,
            alternate_screen: self.alternate_screen,
            focus_events: self.focus_events,
        }
    }

    /// A scanner with carried modes, between sequences.
    pub(crate) fn adopt(modes: CarriedModes) -> Self {
        Self {
            kitty_flags: modes.kitty_flags,
            bracketed_paste: modes.bracketed_paste,
            paste_used: modes.paste_used,
            application_cursor: modes.application_cursor,
            mouse: modes.mouse,
            mouse_encoding: modes.mouse_encoding,
            alternate_screen: modes.alternate_screen,
            focus_events: modes.focus_events,
            ..Self::default()
        }
    }
}

impl Default for QueryScanner {
    fn default() -> Self {
        Self {
            state: State::Ground,
            params: Vec::new(),
            osc: Vec::new(),
            kitty_flags: Vec::new(),
            bracketed_paste: false,
            paste_used: false,
            application_cursor: false,
            mouse: MouseTracking::Off,
            mouse_encoding: MouseEncoding::Default,
            alternate_screen: false,
            synchronized: false,
            focus_events: false,
        }
    }
}

impl QueryScanner {
    /// The kitty keyboard flags in effect: the top of the stack, or 0.
    pub(crate) fn kitty_flags(&self) -> u8 {
        self.kitty_flags.last().copied().unwrap_or(0)
    }

    /// Whether the program turned bracketed paste on (`CSI ? 2004 h`).
    #[cfg(test)]
    pub(crate) fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
    }

    /// Whether the program asked to be told as its terminal gains and
    /// loses focus.
    pub(crate) fn focus_events(&self) -> bool {
        self.focus_events
    }

    /// Whether the program has ever turned bracketed paste on. A program
    /// that did and has turned it off while still running has handed the
    /// terminal back, as Claude Code does when it suspends itself.
    pub(crate) fn paste_used(&self) -> bool {
        self.paste_used
    }

    /// The input modes in effect, for clients.
    pub(crate) fn modes(&self) -> muster_core::protocol::TerminalModes {
        muster_core::protocol::TerminalModes {
            kitty_flags: self.kitty_flags(),
            bracketed_paste: self.bracketed_paste,
            application_cursor: self.application_cursor,
            mouse: self.mouse,
            mouse_encoding: self.mouse_encoding,
            alternate_screen: self.alternate_screen,
        }
    }

    /// Reads program output and returns the answers it is owed. A question
    /// split across chunks is still answered.
    pub(crate) fn scan(&mut self, data: &[u8]) -> Answers {
        let mut answers = Answers::default();
        for (index, &byte) in data.iter().enumerate() {
            self.step(byte, index + 1, &mut answers);
        }
        answers
    }

    /// `end` is the offset in the scanned chunk just past `byte`.
    fn step(&mut self, byte: u8, end: usize, answers: &mut Answers) {
        if let State::OscEscape { poisoned } = self.state {
            if byte == b'\\' {
                if !poisoned {
                    self.dispatch_osc(Ending::St, answers);
                }
                self.state = State::Ground;
                return;
            }
            // The ESC cut the OSC off and starts a sequence of its own,
            // which this byte goes on.
            self.state = State::Escape;
        }
        self.state = match self.state {
            State::Ground if byte == ESC => State::Escape,
            State::Ground => State::Ground,
            State::Escape if byte == b'[' => {
                self.params.clear();
                State::Csi {
                    poisoned: false,
                    intermediate: None,
                }
            }
            State::Escape if byte == b']' => {
                self.osc.clear();
                State::Osc { poisoned: false }
            }
            State::Escape => {
                // RIS (`ESC c`) puts every mode back to its default.
                if byte == b'c' {
                    self.bracketed_paste = false;
                    self.application_cursor = false;
                    self.mouse = MouseTracking::Off;
                    self.mouse_encoding = MouseEncoding::Default;
                    self.alternate_screen = false;
                    self.synchronized = false;
                    self.focus_events = false;
                }
                if byte == ESC {
                    State::Escape
                } else {
                    State::Ground
                }
            }
            State::Csi {
                poisoned,
                intermediate,
            } => match byte {
                0x30..=0x3f if intermediate.is_none() && self.params.len() < MAX_PARAMS => {
                    self.params.push(byte);
                    State::Csi {
                        poisoned,
                        intermediate,
                    }
                }
                0x20..=0x2f if intermediate.is_none() => State::Csi {
                    poisoned,
                    intermediate: Some(byte),
                },
                0x20..=0x3f => State::Csi {
                    poisoned: true,
                    intermediate,
                },
                0x40..=0x7e => {
                    if !poisoned {
                        self.dispatch(byte, intermediate, end, answers);
                    }
                    State::Ground
                }
                // A cancelled or malformed sequence.
                _ => State::Ground,
            },
            State::Osc { poisoned } => match byte {
                0x07 => {
                    if !poisoned {
                        self.dispatch_osc(Ending::Bel, answers);
                    }
                    State::Ground
                }
                ESC => State::OscEscape { poisoned },
                // CAN and SUB cancel it.
                0x18 | 0x1a => State::Ground,
                _ if poisoned || self.osc.len() == MAX_OSC => State::Osc { poisoned: true },
                _ => {
                    self.osc.push(byte);
                    State::Osc { poisoned }
                }
            },
            // Handled above.
            State::OscEscape { .. } => State::Ground,
        };
    }

    /// An OSC ended: the colours it asks for are owed, in order.
    fn dispatch_osc(&mut self, ending: Ending, answers: &mut Answers) {
        for colour in colours::questions(&self.osc) {
            answers.replies.push(Reply::Colour { colour, ending });
        }
        self.osc.clear();
    }

    fn dispatch(
        &mut self,
        final_byte: u8,
        intermediate: Option<u8>,
        end: usize,
        answers: &mut Answers,
    ) {
        let params = std::mem::take(&mut self.params);
        if let Some(intermediate) = intermediate {
            // DECRQM for a private mode: answered with DECRPM, 1 for set, 2
            // for reset, 0 for a mode this terminal does not know.
            if let ((b'$', b'p'), [b'?', mode @ ..]) =
                ((intermediate, final_byte), params.as_slice())
                && let Some(number) = number(mode)
            {
                let status = match self.private_mode(number) {
                    Some(true) => 1,
                    Some(false) => 2,
                    None => 0,
                };
                answers.reply(format!("\x1b[?{number};{status}$y").as_bytes());
            }
            return;
        }
        match (final_byte, params.as_slice()) {
            // Kitty keyboard protocol: query, push, pop, and set.
            (b'u', [b'?']) => {
                answers.reply(format!("\x1b[?{}u", self.kitty_flags()).as_bytes());
            }
            (b'u', [b'>', rest @ ..]) => {
                if self.kitty_flags.len() == MAX_STACK {
                    // The spec: at the limit, the oldest entry goes.
                    self.kitty_flags.remove(0);
                }
                self.kitty_flags.push(kitty_bits(number(rest).unwrap_or(0)));
            }
            (b'u', [b'<', rest @ ..]) => {
                let count = number(rest).unwrap_or(1).max(1) as usize;
                let keep = self.kitty_flags.len().saturating_sub(count);
                self.kitty_flags.truncate(keep);
            }
            (b'u', [b'=', rest @ ..]) => {
                let mut fields = rest.split(|&byte| byte == b';');
                let flags = kitty_bits(fields.next().and_then(number).unwrap_or(0));
                let mode = fields.next().and_then(number).unwrap_or(1);
                if self.kitty_flags.is_empty() {
                    self.kitty_flags.push(0);
                }
                if let Some(top) = self.kitty_flags.last_mut() {
                    match mode {
                        1 => *top = flags,
                        2 => *top |= flags,
                        3 => *top &= !flags,
                        _ => {}
                    }
                }
            }
            // Private mode set and reset, several at once
            // (`CSI ? 1 ; 2004 h`).
            (b'h' | b'l', [b'?', modes @ ..]) => {
                let on = final_byte == b'h';
                for mode in modes.split(|&byte| byte == b';') {
                    match mode {
                        b"2004" => {
                            self.bracketed_paste = on;
                            self.paste_used |= on;
                        }
                        b"1" => self.application_cursor = on,
                        b"9" => self.track_mouse(MouseTracking::Press, on),
                        b"1000" => self.track_mouse(MouseTracking::PressRelease, on),
                        b"1002" => self.track_mouse(MouseTracking::ButtonMotion, on),
                        b"1003" => self.track_mouse(MouseTracking::AnyMotion, on),
                        b"1005" => self.encode_mouse(MouseEncoding::Utf8, on),
                        b"1006" => self.encode_mouse(MouseEncoding::Sgr, on),
                        b"47" | b"1047" | b"1049" => self.alternate_screen = on,
                        b"2026" => self.synchronized = on,
                        b"1004" => self.focus_events = on,
                        _ => {}
                    }
                }
            }
            // Device attributes: claim a VT102 so probing loops end.
            (b'c', [] | [b'0']) => answers.reply(b"\x1b[?6c"),
            // Terminal version (XTVERSION).
            (b'q', [b'>'] | [b'>', b'0']) => answers
                .reply(concat!("\x1bP>|muster(", env!("CARGO_PKG_VERSION"), ")\x1b\\").as_bytes()),
            // Device status: fine.
            (b'n', [b'5']) => answers.reply(b"\x1b[0n"),
            // Cursor position: needs the screen.
            (b'n', [b'6']) => answers.replies.push(Reply::CursorPosition { at: end }),
            // The text area's size in pixels, and a cell's (XTWINOPS):
            // need the size a client gave.
            (b't', b"14" | b"14;2") => answers.replies.push(Reply::PixelSize { cell: false }),
            (b't', b"16") => answers.replies.push(Reply::PixelSize { cell: true }),
            _ => {}
        }
    }
}

impl QueryScanner {
    /// Whether private mode `number` is set, for the modes tracked here.
    fn private_mode(&self, number: u32) -> Option<bool> {
        Some(match number {
            1 => self.application_cursor,
            9 => self.mouse == MouseTracking::Press,
            1000 => self.mouse == MouseTracking::PressRelease,
            1002 => self.mouse == MouseTracking::ButtonMotion,
            1003 => self.mouse == MouseTracking::AnyMotion,
            1005 => self.mouse_encoding == MouseEncoding::Utf8,
            1006 => self.mouse_encoding == MouseEncoding::Sgr,
            47 | 1047 | 1049 => self.alternate_screen,
            2004 => self.bracketed_paste,
            2026 => self.synchronized,
            1004 => self.focus_events,
            _ => return None,
        })
    }

    /// As xterm and iTerm2 do: the one set last is in effect, and resetting
    /// any stops the reports, so a program that resets another than it set
    /// is never sent clicks it no longer reads.
    fn track_mouse(&mut self, mode: MouseTracking, on: bool) {
        self.mouse = if on { mode } else { MouseTracking::Off };
    }

    /// An encoding, though, is reset only by its own number (xterm).
    fn encode_mouse(&mut self, encoding: MouseEncoding, on: bool) {
        if on {
            self.mouse_encoding = encoding;
        } else if self.mouse_encoding == encoding {
            self.mouse_encoding = MouseEncoding::Default;
        }
    }
}

/// The kitty keyboard flags in a number a program sent: the five the
/// protocol defines, and no others.
fn kitty_bits(number: u32) -> u8 {
    u8::try_from(number & 0b1_1111).unwrap_or(0)
}

pub(super) fn number(digits: &[u8]) -> Option<u32> {
    if digits.is_empty() || digits.len() > 9 || !digits.iter().all(u8::is_ascii_digit) {
        return None;
    }
    std::str::from_utf8(digits).ok()?.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bytes(reply: &[u8]) -> Vec<Reply> {
        vec![Reply::Bytes(reply.to_vec())]
    }

    #[test]
    fn claude_codes_startup_questions_are_answered_in_order() {
        let mut scanner = QueryScanner::default();
        let answers = scanner.scan(b"\x1b[?u\x1b[c\x1b[>0q");
        let version = concat!("\x1bP>|muster(", env!("CARGO_PKG_VERSION"), ")\x1b\\");
        let expected = [b"\x1b[?0u".as_slice(), b"\x1b[?6c", version.as_bytes()].concat();
        assert_eq!(answers.replies, bytes(&expected));
    }

    #[test]
    fn kitty_flags_follow_push_pop_and_set() {
        let mut scanner = QueryScanner::default();
        scanner.scan(b"\x1b[>1u\x1b[>15u");
        assert_eq!(scanner.kitty_flags(), 15);
        assert_eq!(scanner.scan(b"\x1b[?u").replies, bytes(b"\x1b[?15u"));
        scanner.scan(b"\x1b[<u");
        assert_eq!(scanner.kitty_flags(), 1);
        scanner.scan(b"\x1b[<5u");
        assert_eq!(scanner.kitty_flags(), 0);
        scanner.scan(b"\x1b[=5;1u\x1b[=2;2u\x1b[=1;3u");
        assert_eq!(scanner.kitty_flags(), 6);
    }

    #[test]
    fn the_flag_stack_is_capped_by_dropping_the_oldest() {
        let mut scanner = QueryScanner::default();
        for flags in 1..=40 {
            scanner.scan(format!("\x1b[>{flags}u").as_bytes());
        }
        assert_eq!(scanner.kitty_flags.len(), MAX_STACK);
        assert_eq!(scanner.kitty_flags[0], 9);
    }

    #[test]
    fn questions_split_across_chunks_are_answered() {
        let mut scanner = QueryScanner::default();
        assert_eq!(scanner.scan(b"hello \x1b"), Answers::default());
        assert_eq!(scanner.scan(b"[>"), Answers::default());
        assert_eq!(scanner.scan(b"0q world").replies.len(), 1);
    }

    #[test]
    fn other_sequences_are_left_alone() {
        let mut scanner = QueryScanner::default();
        // Colour, cursor move, SCO restore-cursor, a long parameter, DA2,
        // DEC's private device status, and an intermediate byte.
        let noise = b"\x1b[31mred\x1b[H\x1b[u\x1b[12345678901234567890u\x1b[>c\x1b[?6n\x1b[ q";
        assert_eq!(scanner.scan(noise), Answers::default());
        assert_eq!(scanner.kitty_flags(), 0);
    }

    #[test]
    fn bracketed_paste_follows_its_mode() {
        let mut scanner = QueryScanner::default();
        assert!(!scanner.bracketed_paste());
        scanner.scan(b"\x1b[?25l\x1b[?10");
        scanner.scan(b"04;2004h prompt");
        assert!(scanner.bracketed_paste());
        // Another mode, or the non-private form, changes nothing.
        scanner.scan(b"\x1b[?1004l\x1b[2004l\x1b[?20045l");
        assert!(scanner.bracketed_paste());
        scanner.scan(b"\x1b[?2004l");
        assert!(!scanner.bracketed_paste());
        scanner.scan(b"\x1b[?2004h\x1bc");
        assert!(!scanner.bracketed_paste(), "a full reset turns it off");
    }

    #[test]
    fn paste_counts_as_used_even_when_turned_off_in_the_same_chunk() {
        let mut scanner = QueryScanner::default();
        assert!(!scanner.paste_used());
        scanner.scan(b"\x1b[?2004h\x1b[?2004l");
        assert!(scanner.paste_used());
        assert!(!scanner.modes().bracketed_paste);
    }

    #[test]
    fn cursor_key_mode_is_tracked_and_reset() {
        let mut scanner = QueryScanner::default();
        scanner.scan(b"\x1b[?1;2004h");
        let modes = scanner.modes();
        assert!(modes.application_cursor && modes.bracketed_paste);
        scanner.scan(b"\x1b[?1l");
        assert!(!scanner.modes().application_cursor);
        scanner.scan(b"\x1b[?1h\x1bc");
        assert_eq!(
            scanner.modes(),
            muster_core::protocol::TerminalModes::default()
        );
    }

    #[test]
    fn mouse_modes_follow_the_program_as_xterm_does() {
        let mut scanner = QueryScanner::default();
        assert_eq!(scanner.modes().mouse, MouseTracking::Off);
        // vim's choice: button motion, SGR encoding.
        scanner.scan(b"\x1b[?1002h\x1b[?1006h");
        let modes = scanner.modes();
        assert_eq!(modes.mouse, MouseTracking::ButtonMotion);
        assert_eq!(modes.mouse_encoding, MouseEncoding::Sgr);
        // Setting another replaces it; resetting an encoding not in effect
        // changes nothing.
        scanner.scan(b"\x1b[?1000;1005h\x1b[?1006l");
        let modes = scanner.modes();
        assert_eq!(modes.mouse, MouseTracking::PressRelease);
        assert_eq!(modes.mouse_encoding, MouseEncoding::Utf8);
        // But resetting any way of tracking stops it: the program set 1000
        // after 1002 and resets 1002.
        scanner.scan(b"\x1b[?1002l");
        assert_eq!(scanner.modes().mouse, MouseTracking::Off);
        scanner.scan(b"\x1b[?1005l");
        assert_eq!(scanner.modes().mouse_encoding, MouseEncoding::Default);
        // Claude Code's, on and off around its full-screen views.
        scanner.scan(b"\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h");
        assert_eq!(scanner.modes().mouse, MouseTracking::AnyMotion);
        scanner.scan(b"\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l");
        assert_eq!(scanner.modes().mouse, MouseTracking::Off);
        assert_eq!(scanner.modes().mouse_encoding, MouseEncoding::Default);
        scanner.scan(b"\x1b[?9h");
        assert_eq!(scanner.modes().mouse, MouseTracking::Press);
        scanner.scan(b"\x1b[?1003h\x1bc");
        assert_eq!(
            scanner.modes(),
            muster_core::protocol::TerminalModes::default()
        );
    }

    #[test]
    fn the_alternate_screen_is_tracked_in_each_form() {
        let mut scanner = QueryScanner::default();
        for (on, off) in [
            (&b"\x1b[?1049h"[..], &b"\x1b[?1049l"[..]),
            (b"\x1b[?1047h", b"\x1b[?1047l"),
            (b"\x1b[?47h", b"\x1b[?47l"),
        ] {
            scanner.scan(on);
            assert!(scanner.modes().alternate_screen);
            scanner.scan(off);
            assert!(!scanner.modes().alternate_screen);
        }
    }

    #[test]
    fn device_status_is_ok_and_cursor_position_keeps_its_place() {
        let mut scanner = QueryScanner::default();
        assert_eq!(scanner.scan(b"\x1b[5n").replies, bytes(b"\x1b[0n"));
        let answers = scanner.scan(b"\x1b[?u\x1b[cab\x1b[6ncd\x1b[c");
        assert_eq!(
            answers.replies,
            [
                Reply::Bytes(b"\x1b[?0u\x1b[?6c".to_vec()),
                Reply::CursorPosition { at: 13 },
                Reply::Bytes(b"\x1b[?6c".to_vec()),
            ]
        );
    }

    #[test]
    fn decrqm_is_answered_for_tracked_modes_and_not_recognized_otherwise() {
        let mut scanner = QueryScanner::default();
        // Claude Code's probe: synchronized output, then pixel mouse.
        assert_eq!(
            scanner.scan(b"\x1b[?2026$p\x1b[?1016$p").replies,
            bytes(b"\x1b[?2026;2$y\x1b[?1016;0$y")
        );
        scanner.scan(b"\x1b[?2026h\x1b[?2004h");
        assert_eq!(
            scanner
                .scan(b"\x1b[?2026$p\x1b[?2004$p\x1b[?1049$p")
                .replies,
            bytes(b"\x1b[?2026;1$y\x1b[?2004;1$y\x1b[?1049;2$y")
        );
        scanner.scan(b"\x1b[?2026l");
        assert_eq!(
            scanner.scan(b"\x1b[?2026$p").replies,
            bytes(b"\x1b[?2026;2$y")
        );
        // Split across chunks, too.
        assert_eq!(scanner.scan(b"\x1b[?20"), Answers::default());
        assert_eq!(scanner.scan(b"26$").replies, Vec::new());
        assert_eq!(scanner.scan(b"p").replies, bytes(b"\x1b[?2026;2$y"));
    }

    #[test]
    fn pixel_sizes_are_asked_in_order_and_other_window_operations_left_alone() {
        let mut scanner = QueryScanner::default();
        assert_eq!(
            scanner.scan(b"\x1b[14t\x1b[c\x1b[16t\x1b[14;2t").replies,
            [
                Reply::PixelSize { cell: false },
                Reply::Bytes(b"\x1b[?6c".to_vec()),
                Reply::PixelSize { cell: true },
                Reply::PixelSize { cell: false },
            ]
        );
        // The window's place, a resize, iconifying, and the title stack.
        let noise = b"\x1b[13t\x1b[8;24;80t\x1b[2t\x1b[22;0t\x1b[?14t";
        assert_eq!(scanner.scan(noise), Answers::default());
    }

    #[test]
    fn focus_events_follow_their_mode_and_are_carried() {
        let mut scanner = QueryScanner::default();
        assert!(!scanner.focus_events());
        assert_eq!(
            scanner.scan(b"\x1b[?1004$p").replies,
            bytes(b"\x1b[?1004;2$y")
        );
        scanner.scan(b"\x1b[?1004;2004h");
        assert!(scanner.focus_events());
        assert_eq!(
            scanner.scan(b"\x1b[?1004$p").replies,
            bytes(b"\x1b[?1004;1$y")
        );
        let adopted = QueryScanner::adopt(scanner.carry());
        assert!(adopted.focus_events(), "carried to the next daemon");
        scanner.scan(b"\x1b[?1004l");
        assert!(!scanner.focus_events());
        scanner.scan(b"\x1b[?1004h\x1bc");
        assert!(!scanner.focus_events(), "a full reset turns it off");
    }

    #[test]
    fn modes_carried_by_a_daemon_before_focus_events_read_as_none() {
        // Named fields, as the handoff writes them, without the new one.
        #[derive(serde::Serialize)]
        struct Before {
            kitty_flags: Vec<u8>,
            bracketed_paste: bool,
            paste_used: bool,
            application_cursor: bool,
            mouse: MouseTracking,
            mouse_encoding: MouseEncoding,
            alternate_screen: bool,
        }
        let bytes = rmp_serde::to_vec_named(&Before {
            kitty_flags: vec![1],
            bracketed_paste: true,
            paste_used: true,
            application_cursor: false,
            mouse: MouseTracking::Off,
            mouse_encoding: MouseEncoding::Default,
            alternate_screen: false,
        })
        .unwrap();
        let modes: CarriedModes = rmp_serde::from_slice(&bytes).unwrap();
        let scanner = QueryScanner::adopt(modes);
        assert!(scanner.modes().bracketed_paste && !scanner.focus_events());
    }

    #[test]
    fn other_sequences_with_an_intermediate_byte_are_left_alone() {
        let mut scanner = QueryScanner::default();
        // The cursor's shape, ANSI-mode DECRQM, two intermediates, and a
        // parameter after the intermediate.
        let noise = b"\x1b[2 q\x1b[4$p\x1b[?2026$$p\x1b[?$2026p";
        assert_eq!(scanner.scan(noise), Answers::default());
    }
}

//! Answers the questions a program asks its terminal.
//!
//! A session's program talks to a terminal nobody is sitting at: the daemon
//! holds it, and a client draws a copy. Programs ask the terminal things at
//! startup and wait for the answer, so the daemon answers. Claude Code
//! 2.1.291 asks three, captured on a bare terminal: keyboard-protocol support
//! (`CSI ? u`), device attributes (`CSI c`) and the terminal's version
//! (`CSI > 0 q`).
//!
//! Also tracked, for clients to use later: the kitty keyboard flags the
//! program pushes (so keys can be encoded the way it asked) and whether it
//! turned bracketed paste on.
//!
//! Adapted from Nebula's `pty/kitty.rs` (MIT, Copyright (c) 2026 Cody
//! Seibert; see NOTICE). Changed: the version query is answered too (Nebula
//! leaves it unanswered, so a program waits out its timeout), and the
//! scanner is renamed for what it does.

const ESC: u8 = 0x1b;
/// The deepest kitty flag stack the spec asks implementations to keep.
const MAX_STACK: usize = 32;
/// A real query has short parameters; anything longer is not for us.
const MAX_PARAMS: usize = 16;

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
    /// Inside a CSI. Parameters collect; an intermediate byte or too many
    /// parameters poison it, since no question we answer has either.
    Csi {
        poisoned: bool,
    },
}

#[derive(Debug)]
pub(crate) struct QueryScanner {
    state: State,
    params: Vec<u8>,
    kitty_flags: Vec<u8>,
    bracketed_paste: bool,
}

impl Default for QueryScanner {
    fn default() -> Self {
        Self {
            state: State::Ground,
            params: Vec::new(),
            kitty_flags: Vec::new(),
            bracketed_paste: false,
        }
    }
}

impl QueryScanner {
    /// The kitty keyboard flags in effect: the top of the stack, or 0.
    pub(crate) fn kitty_flags(&self) -> u8 {
        self.kitty_flags.last().copied().unwrap_or(0)
    }

    /// Whether the program turned bracketed paste on (`CSI ? 2004 h`).
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "the TUI reads it to paste correctly (M1 step 9)")
    )]
    pub(crate) fn bracketed_paste(&self) -> bool {
        self.bracketed_paste
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
        self.state = match self.state {
            State::Ground if byte == ESC => State::Escape,
            State::Ground => State::Ground,
            State::Escape if byte == b'[' => {
                self.params.clear();
                State::Csi { poisoned: false }
            }
            State::Escape => {
                // RIS (`ESC c`) puts every mode back to its default.
                if byte == b'c' {
                    self.bracketed_paste = false;
                }
                if byte == ESC {
                    State::Escape
                } else {
                    State::Ground
                }
            }
            State::Csi { poisoned } => match byte {
                0x30..=0x3f if self.params.len() < MAX_PARAMS => {
                    self.params.push(byte);
                    State::Csi { poisoned }
                }
                0x20..=0x3f => State::Csi { poisoned: true },
                0x40..=0x7e => {
                    if !poisoned {
                        self.dispatch(byte, end, answers);
                    }
                    State::Ground
                }
                // A cancelled or malformed sequence.
                _ => State::Ground,
            },
        };
    }

    fn dispatch(&mut self, final_byte: u8, end: usize, answers: &mut Answers) {
        let params = std::mem::take(&mut self.params);
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
                self.kitty_flags.push(number(rest).unwrap_or(0) as u8);
            }
            (b'u', [b'<', rest @ ..]) => {
                let count = number(rest).unwrap_or(1).max(1) as usize;
                let keep = self.kitty_flags.len().saturating_sub(count);
                self.kitty_flags.truncate(keep);
            }
            (b'u', [b'=', rest @ ..]) => {
                let mut fields = rest.split(|&byte| byte == b';');
                let flags = fields.next().and_then(number).unwrap_or(0) as u8;
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
            // (`CSI ? 1004 ; 2004 h`): only bracketed paste matters.
            (b'h' | b'l', [b'?', modes @ ..]) => {
                if modes
                    .split(|&byte| byte == b';')
                    .any(|mode| mode == b"2004")
                {
                    self.bracketed_paste = final_byte == b'h';
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
            _ => {}
        }
    }
}

fn number(digits: &[u8]) -> Option<u32> {
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
}

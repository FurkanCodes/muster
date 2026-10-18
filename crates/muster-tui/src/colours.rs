//! The colours of the person's terminal, asked once as the UI starts, so
//! the daemon can tell programs in sessions that ask theirs
//! (`muster_core::colours`).
//!
//! The terminal is asked for its foreground (OSC 10), background (OSC 11)
//! and palette's sixteen colours (OSC 4), then for its device attributes
//! (`CSI c`), which every terminal answers, and answers in order: once that
//! answer is in, every colour it was going to say has been said, and those
//! it did not say are none. A terminal that says nothing in time leaves
//! them all none, as if it had none.

use std::fs::File;
use std::io::{Read, Write};
use std::os::fd::AsFd;
use std::time::{Duration, Instant};

use muster_core::colours::{PALETTE, Rgb, TerminalColours};

/// How long the terminal has to answer, as for the keyboard question the UI
/// asks before: it answers within a round trip, and only one that answers
/// nothing makes the UI wait this long.
pub(crate) const WAIT: Duration = Duration::from_secs(2);

/// The longest answer read; a colour's is under 40 bytes.
const MAX_ANSWER: usize = 64;

/// The questions, the device attributes last.
pub(crate) fn questions() -> Vec<u8> {
    let mut asked = b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\".to_vec();
    for index in 0..PALETTE {
        asked.extend_from_slice(format!("\x1b]4;{index};?\x1b\\").as_bytes());
    }
    asked.extend_from_slice(b"\x1b[c");
    asked
}

/// Asks the person's terminal for its colours, waiting until `deadline`
/// at most; `now` reads the UI's clock. Reads the answers one byte at a
/// time, so nothing after them, such as a key typed meanwhile, is taken
/// from the UI.
pub(crate) fn ask(deadline: Instant, now: impl Fn() -> Instant) -> TerminalColours {
    let Ok(mut terminal) = File::options().read(true).write(true).open("/dev/tty") else {
        return TerminalColours::default();
    };
    if terminal
        .write_all(&questions())
        .and_then(|()| terminal.flush())
        .is_err()
    {
        return TerminalColours::default();
    }
    let mut answers = Answers::default();
    let mut byte = [0];
    while !answers.done() {
        let left = deadline.saturating_duration_since(now());
        if left.is_zero() || !readable(&terminal, left) {
            break;
        }
        match terminal.read(&mut byte) {
            Ok(1) => answers.feed(byte[0]),
            _ => break,
        }
    }
    answers.colours
}

/// Whether the terminal has a byte to read within `wait`. With `select`,
/// as macOS's `poll` does not work on terminals.
fn readable(terminal: &File, wait: Duration) -> bool {
    use nix::sys::select::{FdSet, select};
    use nix::sys::time::{TimeVal, TimeValLike};
    let mut fds = FdSet::new();
    fds.insert(terminal.as_fd());
    let micros = i64::try_from(wait.as_micros()).unwrap_or(i64::MAX);
    let mut timeout = TimeVal::microseconds(micros);
    select(None, &mut fds, None, None, &mut timeout).is_ok_and(|ready| ready > 0)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Ground,
    Escape,
    Osc,
    OscEscape,
    Csi,
}

/// The terminal's answers, read as they come.
#[derive(Debug, Default)]
pub(crate) struct Answers {
    state: State,
    body: Vec<u8>,
    colours: TerminalColours,
    /// The device attributes have been answered: nothing more is coming.
    done: bool,
}

impl Answers {
    /// Whether the last answer is in.
    pub(crate) fn done(&self) -> bool {
        self.done
    }

    /// The colours said so far.
    #[cfg(test)]
    pub(crate) fn colours(&self) -> &TerminalColours {
        &self.colours
    }

    /// Reads one byte of the answers.
    pub(crate) fn feed(&mut self, byte: u8) {
        if self.state == State::OscEscape && byte != b'\\' {
            // The ESC cut the answer off and starts another.
            self.state = State::Escape;
        }
        self.state = match (self.state, byte) {
            (_, 0x1b) if self.state != State::Osc => State::Escape,
            (State::Escape, b']') => self.start(State::Osc),
            (State::Escape, b'[') => self.start(State::Csi),
            (State::Osc, 0x07) | (State::OscEscape, b'\\') => {
                self.said();
                State::Ground
            }
            (State::Osc, 0x1b) => State::OscEscape,
            (State::Osc | State::Csi, _) => {
                if self.body.len() < MAX_ANSWER {
                    self.body.push(byte);
                }
                if self.state == State::Csi && (0x40..=0x7e).contains(&byte) {
                    // Device attributes, as `?62;22c` says them.
                    self.done |= byte == b'c' && self.body.first() == Some(&b'?');
                    State::Ground
                } else {
                    self.state
                }
            }
            _ => State::Ground,
        };
    }

    fn start(&mut self, state: State) -> State {
        self.body.clear();
        state
    }

    /// An OSC answer ended: a colour, if it says one.
    fn said(&mut self) {
        let mut fields = self.body.splitn(3, |&byte| byte == b';');
        match (fields.next(), fields.next(), fields.next()) {
            (Some(b"10"), Some(spec), None) => self.colours.foreground = Rgb::parse(spec),
            (Some(b"11"), Some(spec), None) => self.colours.background = Rgb::parse(spec),
            (Some(b"4"), Some(index), Some(spec)) => {
                let index = std::str::from_utf8(index)
                    .ok()
                    .and_then(|index| index.parse::<usize>().ok())
                    .filter(|index| *index < PALETTE);
                if let Some(index) = index {
                    if self.colours.palette.len() <= index {
                        self.colours.palette.resize(index + 1, None);
                    }
                    self.colours.palette[index] = Rgb::parse(spec);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(bytes: &[u8]) -> Answers {
        let mut answers = Answers::default();
        for &byte in bytes {
            answers.feed(byte);
        }
        answers
    }

    #[test]
    fn the_questions_ask_every_colour_and_then_the_device_attributes() {
        let asked = questions();
        assert!(asked.starts_with(b"\x1b]10;?\x1b\\\x1b]11;?\x1b\\\x1b]4;0;?\x1b\\"));
        assert!(asked.ends_with(b"\x1b]4;15;?\x1b\\\x1b[c"));
    }

    #[test]
    fn the_colours_said_are_kept_in_either_ending_until_the_last_answer() {
        let answers = read(
            b"\x1b]10;rgb:d0d0/d0d0/d0d0\x1b\\\x1b]11;rgb:1e/1e/2e\x07\
              \x1b]4;1;rgb:cdcd/0000/0000\x1b\\\x1b]4;15;rgb:ff/ff/ff\x07",
        );
        assert!(!answers.done());
        let colours = answers.colours();
        let grey = |level| {
            Some(Rgb {
                red: level,
                green: level,
                blue: level,
            })
        };
        assert_eq!(colours.foreground, grey(0xd0d0));
        assert_eq!(
            colours.background,
            Some(Rgb {
                red: 0x1e1e,
                green: 0x1e1e,
                blue: 0x2e2e
            })
        );
        assert_eq!(colours.palette(0), None);
        assert_eq!(
            colours.palette(1),
            Some(Rgb {
                red: 0xcdcd,
                green: 0,
                blue: 0
            })
        );
        assert_eq!(colours.palette(15), grey(0xffff));
        let mut answers = answers;
        for &byte in b"\x1b[?62;22c" {
            answers.feed(byte);
        }
        assert!(answers.done());
    }

    #[test]
    fn a_terminal_that_knows_no_colours_only_answers_the_last_question() {
        let answers = read(b"\x1b[?6c");
        assert!(answers.done());
        assert!(answers.colours().is_empty());
    }

    #[test]
    fn other_answers_and_bad_colours_say_nothing() {
        let answers = read(
            b"\x1b[?0u\x1b]11;#1e1e2e\x07\x1b]4;16;rgb:ff/ff/ff\x07\
              \x1b]4;x;rgb:ff/ff/ff\x07\x1b]12;rgb:ff/ff/ff\x07\x1b[0n",
        );
        assert!(!answers.done(), "only device attributes end it");
        assert!(answers.colours().is_empty(), "{:?}", answers.colours());
        // An answer cut off by the next is not read, and the next is.
        let answers = read(b"\x1b]11;rgb:ff/ff/ff\x1b[?6c");
        assert!(answers.done());
        assert!(answers.colours().is_empty());
    }
}

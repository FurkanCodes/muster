//! Finds the terminal progress sequence in a session's output.
//!
//! Agents such as Claude Code report busy and idle with OSC 9;4:
//! `ESC ] 9 ; 4 ; <state> [; <percent>] (BEL | ESC \)`. State 0 means idle;
//! any other state means busy. Only changes are reported. The scanner keeps a
//! few bytes between chunks, so a sequence split across reads is still found,
//! and never buffers more than a short OSC body.

/// The longest OSC body worth reading; longer ones are not progress reports.
const MAX_BODY: usize = 24;

#[derive(Debug, Default)]
pub(crate) struct ProgressScanner {
    state: State,
    body: Vec<u8>,
    /// The current OSC is longer than [`MAX_BODY`], so it is ignored.
    overflowed: bool,
    busy: Option<bool>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum State {
    #[default]
    Text,
    /// Saw ESC.
    Escape,
    /// Inside `ESC ]`, collecting the body.
    Osc,
    /// Saw ESC inside an OSC: a `\` ends it.
    OscEscape,
}

impl ProgressScanner {
    /// Reads output and returns the progress changes it contains, in order.
    pub(crate) fn scan(&mut self, bytes: &[u8]) -> Vec<bool> {
        let mut changes = Vec::new();
        for &byte in bytes {
            self.state = match (self.state, byte) {
                (State::Text, 0x1b) | (State::Escape, 0x1b) => State::Escape,
                (State::Escape | State::OscEscape, b']') => self.start_osc(),
                (State::Text | State::Escape, _) => State::Text,
                (State::Osc, 0x07) | (State::OscEscape, b'\\') => {
                    self.finish(&mut changes);
                    State::Text
                }
                (State::Osc, 0x1b) => State::OscEscape,
                (State::Osc, _) => {
                    if self.body.len() < MAX_BODY {
                        self.body.push(byte);
                    } else {
                        self.overflowed = true;
                    }
                    State::Osc
                }
                // Some other escape sequence interrupted the OSC.
                (State::OscEscape, _) => State::Text,
            };
        }
        changes
    }

    fn start_osc(&mut self) -> State {
        self.body.clear();
        self.overflowed = false;
        State::Osc
    }

    /// Ends an OSC: if it was a progress report that changes the state,
    /// records the change.
    fn finish(&mut self, changes: &mut Vec<bool>) {
        if self.overflowed {
            return;
        }
        let Some(rest) = self.body.strip_prefix(b"9;4;") else {
            return;
        };
        let state = rest.split(|&byte| byte == b';').next().unwrap_or_default();
        let busy = match state {
            b"0" => false,
            _ if !state.is_empty() && state.iter().all(u8::is_ascii_digit) => true,
            _ => return,
        };
        if self.busy != Some(busy) {
            self.busy = Some(busy);
            changes.push(busy);
        }
    }
}

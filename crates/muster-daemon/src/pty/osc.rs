//! Finds what a session's program reports about itself in its output.
//!
//! - Progress: agents such as Claude Code and Copilot report busy and idle
//!   with OSC 9;4: `ESC ] 9 ; 4 ; <state> [; <percent>] (BEL | ESC \)`. State
//!   0 means idle; any other state means busy.
//! - The window title, OSC 0 or 2: `ESC ] 0 ; <title> (BEL | ESC \)`. Codex
//!   and Gemini CLI say there whether they work, wait or are ready.
//!
//! Only changes are reported. The scanner keeps a few bytes between chunks,
//! so a sequence split across reads is still found, and never buffers more
//! than a short OSC body.

/// The longest OSC body worth reading; longer ones are neither progress
/// reports nor titles worth reading.
const MAX_BODY: usize = 512;

/// What a program reported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Report {
    /// The progress sequence turned busy or idle.
    Progress { busy: bool },
    /// The window title changed.
    Title(String),
}

#[derive(Debug, Default)]
pub(crate) struct OscScanner {
    state: State,
    body: Vec<u8>,
    /// The current OSC is longer than [`MAX_BODY`], so it is ignored.
    overflowed: bool,
    busy: Option<bool>,
    title: Option<String>,
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

impl OscScanner {
    /// Reads output and returns the changes it reports, in order.
    pub(crate) fn scan(&mut self, bytes: &[u8]) -> Vec<Report> {
        let mut reports = Vec::new();
        for &byte in bytes {
            self.state = match (self.state, byte) {
                (State::Text, 0x1b) | (State::Escape, 0x1b) => State::Escape,
                (State::Escape | State::OscEscape, b']') => self.start_osc(),
                (State::Text | State::Escape, _) => State::Text,
                (State::Osc, 0x07) | (State::OscEscape, b'\\') => {
                    self.finish(&mut reports);
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
        reports
    }

    fn start_osc(&mut self) -> State {
        self.body.clear();
        self.overflowed = false;
        State::Osc
    }

    /// Ends an OSC: records a progress report or a title that changes what
    /// was last reported.
    fn finish(&mut self, reports: &mut Vec<Report>) {
        if self.overflowed {
            return;
        }
        if let Some(rest) = self.body.strip_prefix(b"9;4;") {
            let state = rest.split(|&byte| byte == b';').next().unwrap_or_default();
            let busy = match state {
                b"0" => false,
                _ if !state.is_empty() && state.iter().all(u8::is_ascii_digit) => true,
                _ => return,
            };
            if self.busy != Some(busy) {
                self.busy = Some(busy);
                reports.push(Report::Progress { busy });
            }
        } else if let Some(title) = self
            .body
            .strip_prefix(b"0;")
            .or_else(|| self.body.strip_prefix(b"2;"))
        {
            let title = String::from_utf8_lossy(title).into_owned();
            if self.title.as_ref() != Some(&title) {
                self.title = Some(title.clone());
                reports.push(Report::Title(title));
            }
        }
    }
}

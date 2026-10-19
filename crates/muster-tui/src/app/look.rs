//! The UI's look, among Muster's own settings on the Settings screen:
//! whose colours it draws in (`Preferences::palette`) and how many lines a
//! session's line takes (`Preferences::session_lines`), each turned to the
//! other by ⏎ or space. Kept by a daemon of `LOOK_PROTOCOL` or later; an
//! older one is never sent the change (`ARC-09`), and shows no rows for it.

use muster_core::protocol::{Command, ErrorBody};
use muster_core::settings::{LOOK_PROTOCOL, Palette, PreferenceChange, SessionLines};

use super::{Action, App, Then};

/// A change to the look, as the answer to it is said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LookChange {
    Palette(Palette),
    SessionLines(SessionLines),
}

impl App {
    /// Whether the daemon keeps the look.
    pub(crate) fn look_known(&self) -> bool {
        self.daemon_protocol >= LOOK_PROTOCOL
    }

    /// Turns the colours to the others: Muster's own, or the terminal's.
    pub(super) fn toggle_palette(&mut self) -> Vec<Action> {
        let next = match self.model.preferences().palette {
            Palette::Muster => Palette::Terminal,
            Palette::Terminal => Palette::Muster,
        };
        self.change_look(LookChange::Palette(next))
    }

    /// Turns a session's lines to the other count: two, or one.
    pub(super) fn toggle_session_lines(&mut self) -> Vec<Action> {
        let next = match self.model.preferences().session_lines {
            SessionLines::Two => SessionLines::One,
            SessionLines::One => SessionLines::Two,
        };
        self.change_look(LookChange::SessionLines(next))
    }

    fn change_look(&mut self, change: LookChange) -> Vec<Action> {
        if !self.look_known() || self.look_changing {
            return Vec::new();
        }
        self.look_changing = true;
        let asked = match change {
            LookChange::Palette(palette) => PreferenceChange::Palette(palette),
            LookChange::SessionLines(lines) => PreferenceChange::SessionLines(lines),
        };
        vec![self.command(Command::ChangePreference(asked), Then::LookChanged(change))]
    }

    /// The look was changed, or could not be. The settings as they are now
    /// come as a change to the state, which draws in them.
    pub(super) fn look_changed(&mut self, change: LookChange, outcome: Result<(), ErrorBody>) {
        self.look_changing = false;
        match outcome {
            Ok(()) => self.say(match change {
                LookChange::Palette(Palette::Muster) => "Muster draws in its own colours.",
                LookChange::Palette(Palette::Terminal) => {
                    "Muster draws in your terminal's colours."
                }
                LookChange::SessionLines(SessionLines::Two) => "Each session takes two lines.",
                LookChange::SessionLines(SessionLines::One) => "Each session takes one line.",
            }),
            Err(error) => self.complain(error.message),
        }
    }
}

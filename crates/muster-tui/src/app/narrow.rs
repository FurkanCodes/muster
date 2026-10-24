//! A window too narrow for both shows the list or a session, not both: a
//! session squeezed beside the list would wrap all its program draws. How
//! narrow is Muster's own setting (`Preferences::side_by_side_from`, on
//! the Settings screen). There, the list takes the whole width; ⏎ opens a
//! session over it, as full screen does, and Shift-Q goes back; the
//! changes take the whole width while they have the keys. A session the
//! list does not show is not streamed, so nothing it does counts as seen.

use muster_core::protocol::Command;
use muster_core::settings::{PreferenceChange, SIDE_BY_SIDE_CHOICES};

use super::{Action, App, Dialog, Help, Mode, Then};

impl App {
    /// Whether the window is too narrow for a session beside the list.
    pub(crate) fn narrow(&self) -> bool {
        let from = self.model.preferences().side_by_side_from;
        from > 0 && self.terminal_size.cols < from
    }

    /// Whether the session with the keys fills the window below the top
    /// bar: full screen, or in a window too narrow for the list beside it.
    pub(crate) fn fills_window(&self) -> bool {
        matches!(self.mode, Mode::Terminal(_)) && (self.full_screen || self.narrow())
    }

    /// Whether the pane beside the list takes the whole width instead of
    /// the list: a workspace's changes with the keys, in a narrow window.
    pub(crate) fn pane_fills_width(&self) -> bool {
        self.narrow()
            && matches!(
                self.mode,
                Mode::Changes
                    | Mode::Dialog(Dialog::Undo { .. })
                    | Mode::Help(Help {
                        over_changes: true,
                        ..
                    })
            )
    }

    /// Whether the session selected is drawn: beside the list, or filling
    /// the window; a narrow window's list draws none, nor does a window
    /// whose panels leave the session no room.
    pub(crate) fn session_drawn(&self) -> bool {
        if matches!(self.mode, Mode::Terminal(_)) {
            return true;
        }
        let window =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        !crate::view::browse_areas(self, window).preview.is_empty()
    }

    /// The width a session sits beside the list from, `by` choices on
    /// (`SIDE_BY_SIDE_CHOICES`), round from the widest to any width with
    /// `round`, else no further than either end.
    pub(super) fn step_side_by_side(&mut self, by: isize, round: bool) -> Vec<Action> {
        if self.side_by_side_changing() {
            return Vec::new();
        }
        let now = self.model.preferences().side_by_side_from;
        // A width written by hand goes to the choice at or above it.
        let at = SIDE_BY_SIDE_CHOICES
            .iter()
            .position(|choice| *choice >= now)
            .unwrap_or(SIDE_BY_SIDE_CHOICES.len() - 1);
        let last = SIDE_BY_SIDE_CHOICES.len() - 1;
        let next = if round {
            at.checked_add_signed(by)
                .filter(|next| *next <= last)
                .unwrap_or(if by > 0 { 0 } else { last })
        } else {
            at.saturating_add_signed(by).min(last)
        };
        let columns = SIDE_BY_SIDE_CHOICES[next];
        if columns == now {
            return Vec::new();
        }
        vec![self.command(
            Command::ChangePreference(PreferenceChange::SideBySideFrom(columns)),
            Then::SideBySideChanged(columns),
        )]
    }

    /// The width was changed to `columns`, or could not be. The settings
    /// as they are now come as a change to the state.
    pub(super) fn side_by_side_changed(
        &mut self,
        columns: u16,
        outcome: Result<(), muster_core::protocol::ErrorBody>,
    ) {
        match outcome {
            Ok(()) if columns == 0 => {
                self.say("A session sits beside the list at any width.");
            }
            Ok(()) => self.say(format!(
                "A session sits beside the list in a window of {columns} columns or more."
            )),
            Err(error) => self.complain(error.message),
        }
    }
}

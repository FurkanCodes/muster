//! Renaming a chat (`R`, `PRODUCT.md` §6): the selected session's name in
//! a field, as it is, for the person to change; ⏎ asks the daemon, which
//! cleans it (`muster_core::names::given`) and tells every client.

use crossterm::event::KeyEvent;
use muster_core::SessionId;
use muster_core::names::NAMES_PROTOCOL;
use muster_core::protocol::Command;

use super::{Action, App, Dialog, Mode, Then, View};
use crate::input::{Edit, LineInput};

impl App {
    /// `R`: asks for the selected session's new name.
    pub(super) fn ask_rename(&mut self) -> Vec<Action> {
        let selected = match self.view {
            View::Sessions => match self.selected {
                Some(crate::model::Row::Session(session)) => Some(session),
                _ => None,
            },
            View::NeedsYou => self.needs_selected,
            View::Tickets | View::Settings => None,
        };
        let Some(session) = selected.and_then(|session| self.model.session(session)) else {
            self.complain("Select a session to rename it.");
            return Vec::new();
        };
        if self.daemon_protocol < NAMES_PROTOCOL {
            self.complain(
                "The daemon runs an older muster that cannot rename sessions: `muster reload` \
                 moves it onto this one, every session running on.",
            );
            return Vec::new();
        }
        self.mode = Mode::Dialog(Dialog::Rename {
            session: session.id,
            name: LineInput::with(&session.title),
        });
        Vec::new()
    }

    /// A key in the dialog renaming a session.
    pub(super) fn rename_key(
        &mut self,
        session: SessionId,
        mut name: LineInput,
        key: &KeyEvent,
    ) -> Vec<Action> {
        match name.handle(key) {
            Edit::Submit if name.text().trim().is_empty() => {
                // Nothing to call it: the field stays for a name.
                self.mode = Mode::Dialog(Dialog::Rename { session, name });
                Vec::new()
            }
            Edit::Submit => {
                self.mode = Mode::Browse;
                let title = name.text().trim().to_owned();
                vec![self.command(
                    Command::RenameSession { session, title },
                    Then::Say("Renamed."),
                )]
            }
            edit => self.keep_editing(edit, Dialog::Rename { session, name }),
        }
    }
}

#[cfg(test)]
#[path = "rename_tests.rs"]
mod tests;

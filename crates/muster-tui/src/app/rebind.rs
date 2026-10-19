//! Setting the keys (`PRODUCT.md` §6): *Keys* on the Settings screen
//! lists every action by what it does, under the screen it works on first,
//! with its keys as set; ⏎ waits for the next key and makes it the
//! action's, refused where another action has it on a screen they share
//! or the key cannot be one (`muster_core::keys`); ⌫ puts an action's keys
//! back as Muster has them, and Shift-R every key. The daemon keeps them
//! in Muster's own settings (`[keys]`).

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::keys::Action as Does;
use muster_core::protocol::Command;
use muster_core::settings::{KEYS_PROTOCOL, PreferenceChange};

use super::{Action, App, Dialog, Mode, Then};

/// The keys being set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rebinding {
    /// The action chosen, among [`rebind_rows`]'s.
    pub(crate) chosen: usize,
    /// The first line shown.
    pub(crate) offset: usize,
    /// Waiting for the chosen action's new key.
    pub(crate) waiting: bool,
}

/// One line of the list: a heading, or an action.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RebindRow {
    Heading(&'static str),
    Does(Does),
}

/// Every action, under where it works (`Action::group`).
pub(crate) fn rebind_rows() -> Vec<RebindRow> {
    let mut rows = Vec::new();
    let mut heading = "";
    for does in Does::ALL {
        if does.group() != heading {
            heading = does.group();
            rows.push(RebindRow::Heading(heading));
        }
        rows.push(RebindRow::Does(*does));
    }
    rows
}

impl Rebinding {
    /// The action chosen.
    pub(crate) fn does(&self) -> Does {
        Does::ALL[self.chosen.min(Does::ALL.len() - 1)]
    }
}

impl App {
    /// Whether the daemon keeps the person's keys.
    pub(crate) fn keys_known(&self) -> bool {
        self.daemon_protocol >= KEYS_PROTOCOL
    }

    /// ⏎ on *Keys*: the list of every action and its keys.
    pub(super) fn open_rebinding(&mut self) -> Vec<Action> {
        self.mode = Mode::Dialog(Dialog::Keys(Rebinding::default()));
        Vec::new()
    }

    /// A key while the keys are listed.
    pub(super) fn rebind_key(&mut self, mut rebinding: Rebinding, key: &KeyEvent) -> Vec<Action> {
        let does = rebinding.does();
        if rebinding.waiting {
            rebinding.waiting = false;
            let keep = |app: &mut Self, rebinding| {
                app.mode = Mode::Dialog(Dialog::Keys(rebinding));
            };
            if key.code == KeyCode::Esc {
                keep(self, rebinding);
                return Vec::new();
            }
            let Some(chord) = crate::keys::chord(key) else {
                self.complain("That key cannot be one of Muster's.");
                keep(self, rebinding);
                return Vec::new();
            };
            if let Err(error) = self.keymap.check(does, chord) {
                self.complain(error.to_string());
                keep(self, rebinding);
                return Vec::new();
            }
            keep(self, rebinding);
            return self.set_keys(does, Some(vec![chord.to_string()]));
        }
        let last = Does::ALL.len() - 1;
        match key.code {
            KeyCode::Up => rebinding.chosen = rebinding.chosen.saturating_sub(1),
            KeyCode::Down => rebinding.chosen = (rebinding.chosen + 1).min(last),
            KeyCode::PageUp => rebinding.chosen = rebinding.chosen.saturating_sub(10),
            KeyCode::PageDown => rebinding.chosen = (rebinding.chosen + 10).min(last),
            KeyCode::Home => rebinding.chosen = 0,
            KeyCode::End => rebinding.chosen = last,
            KeyCode::Enter => {
                if self.keys_known() {
                    rebinding.waiting = true;
                } else {
                    self.complain(
                        "The daemon runs an older muster that keeps no keys of yours: \
                         `muster reload` moves it onto this one, every session running on.",
                    );
                }
            }
            KeyCode::Backspace | KeyCode::Delete if !self.keymap.is_default(does) => {
                self.mode = Mode::Dialog(Dialog::Keys(rebinding));
                return self.set_keys(does, None);
            }
            KeyCode::Char('R') if self.keys_known() => {
                self.mode = Mode::Dialog(Dialog::Keys(rebinding));
                return vec![self.command(
                    Command::ChangePreference(PreferenceChange::AllKeysBack),
                    Then::Say("Every key is back as Muster has it."),
                )];
            }
            KeyCode::Esc => {
                self.mode = Mode::Browse;
                return Vec::new();
            }
            _ => {}
        }
        self.mode = Mode::Dialog(Dialog::Keys(rebinding));
        Vec::new()
    }

    /// Has the daemon keep `keys` as `does`'s, or Muster's where `None`.
    fn set_keys(&mut self, does: Does, keys: Option<Vec<String>>) -> Vec<Action> {
        if !self.keys_known() {
            return Vec::new();
        }
        let said = if keys.is_some() {
            "The key is set."
        } else {
            "Its keys are back as Muster has them."
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::Keys {
                action: does.name().to_owned(),
                keys,
            }),
            Then::Say(said),
        )]
    }
}

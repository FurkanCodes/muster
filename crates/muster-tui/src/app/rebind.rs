//! Setting the keys (`PRODUCT.md` §6): *Keys* on the Settings screen
//! lists every action by what it does, under the screen it works on first,
//! then the panels of the plugins that run, under each plugin's name, each
//! with its keys as set; ⏎ waits for the next key and makes it the
//! action's, refused where another action has it on a screen they share
//! or the key cannot be one (`muster_core::keys`); ⌫ puts an action's keys
//! back as Muster has them (a panel's as its plugin suggests), and Shift-R
//! every key. The daemon keeps them in Muster's own settings (`[keys]`).

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::keys::Action as Does;
use muster_core::protocol::Command;
use muster_core::settings::PreferenceChange;

use super::{Action, App, Dialog, Mode, Then};

/// The keys being set.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Rebinding {
    /// The action chosen, among [`App::rebind_rows`]'s.
    pub(crate) chosen: usize,
    /// The first line shown.
    pub(crate) offset: usize,
    /// Waiting for the chosen action's new key.
    pub(crate) waiting: bool,
}

/// What a key is set for: one of Muster's actions, or a plugin's panel by
/// its name in `[keys]`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Rebound {
    Does(Does),
    Panel(String),
}

/// One line of the list: a heading, or what a key is set for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RebindRow {
    Heading(String),
    Key(Rebound),
}

impl App {
    /// Every action, under where it works (`Action::group`), then the
    /// panels of the plugins that run, under each plugin's name.
    pub(crate) fn rebind_rows(&self) -> Vec<RebindRow> {
        let mut rows = Vec::new();
        let mut heading = String::new();
        let panels = self
            .keymap
            .panels()
            .map(|(action, _)| (action.plugin_name.clone(), Rebound::Panel(action.name())));
        let all = Does::ALL
            .iter()
            .map(|does| (does.group().to_owned(), Rebound::Does(*does)))
            .chain(panels);
        for (group, rebound) in all {
            if group != heading {
                heading.clone_from(&group);
                rows.push(RebindRow::Heading(group));
            }
            rows.push(RebindRow::Key(rebound));
        }
        rows
    }

    /// What can be chosen, in order: each row but the headings.
    fn rebound(&self) -> Vec<Rebound> {
        self.rebind_rows()
            .into_iter()
            .filter_map(|row| match row {
                RebindRow::Key(rebound) => Some(rebound),
                RebindRow::Heading(_) => None,
            })
            .collect()
    }

    /// What `rebinding` has chosen.
    pub(crate) fn rebinding_chosen(&self, rebinding: &Rebinding) -> Rebound {
        let all = self.rebound();
        let last = all.len().saturating_sub(1);
        all.into_iter()
            .nth(rebinding.chosen.min(last))
            .unwrap_or(Rebound::Does(Does::Quit))
    }

    /// What `rebound`'s keys do, as the list says it.
    pub(crate) fn rebound_what(&self, rebound: &Rebound) -> String {
        match rebound {
            Rebound::Does(does) => does.what().to_owned(),
            Rebound::Panel(name) => self
                .keymap
                .panel_named(name)
                .map(muster_core::keys::PanelAction::what)
                .unwrap_or_default(),
        }
    }

    /// `rebound`'s keys as people read them, spaced.
    pub(crate) fn rebound_labels(&self, rebound: &Rebound) -> String {
        match rebound {
            Rebound::Does(does) => self.keymap.labels(*does),
            Rebound::Panel(name) => self.keymap.panel_labels(name),
        }
    }

    /// Whether `rebound` has keys the person set.
    pub(crate) fn rebound_is_set(&self, rebound: &Rebound) -> bool {
        match rebound {
            Rebound::Does(does) => !self.keymap.is_default(*does),
            Rebound::Panel(name) => self.keymap.panel_is_set(name),
        }
    }

    /// ⏎ on *Keys*: the list of every action and its keys.
    pub(super) fn open_rebinding(&mut self) -> Vec<Action> {
        self.mode = Mode::Dialog(Dialog::Keys(Rebinding::default()));
        Vec::new()
    }

    /// A key while the keys are listed.
    pub(super) fn rebind_key(&mut self, mut rebinding: Rebinding, key: &KeyEvent) -> Vec<Action> {
        let rebound = self.rebinding_chosen(&rebinding);
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
            let checked = match &rebound {
                Rebound::Does(does) => self.keymap.check(*does, chord),
                Rebound::Panel(name) => self.keymap.check_panel(name, chord),
            };
            if let Err(error) = checked {
                self.complain(error.to_string());
                keep(self, rebinding);
                return Vec::new();
            }
            keep(self, rebinding);
            return self.set_keys(&rebound, Some(vec![chord.to_string()]));
        }
        let last = self.rebound().len().saturating_sub(1);
        match key.code {
            KeyCode::Up => rebinding.chosen = rebinding.chosen.saturating_sub(1),
            KeyCode::Down => rebinding.chosen = (rebinding.chosen + 1).min(last),
            KeyCode::PageUp => rebinding.chosen = rebinding.chosen.saturating_sub(10),
            KeyCode::PageDown => rebinding.chosen = (rebinding.chosen + 10).min(last),
            KeyCode::Home => rebinding.chosen = 0,
            KeyCode::End => rebinding.chosen = last,
            KeyCode::Enter => {
                rebinding.waiting = true;
            }
            KeyCode::Backspace | KeyCode::Delete if self.rebound_is_set(&rebound) => {
                self.mode = Mode::Dialog(Dialog::Keys(rebinding));
                return self.set_keys(&rebound, None);
            }
            KeyCode::Char('R') => {
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

    /// Has the daemon keep `keys` as `rebound`'s, or Muster's (a panel's
    /// suggested) where `None`.
    fn set_keys(&mut self, rebound: &Rebound, keys: Option<Vec<String>>) -> Vec<Action> {
        let said = if keys.is_some() {
            "The key is set."
        } else {
            "Its keys are back as Muster has them."
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::Keys {
                action: match rebound {
                    Rebound::Does(does) => does.name().to_owned(),
                    Rebound::Panel(name) => name.clone(),
                },
                keys,
            }),
            Then::Say(said),
        )]
    }
}

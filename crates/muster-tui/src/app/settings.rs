//! The Settings screen: each project's settings that the person can
//! change from here, read from the daemon as the screen is shown: whether
//! the Tickets screen lists only the tickets assigned to the person, in
//! whatever tracker the project's tickets are, and where they come from
//! (`tickets_from`).
//!
//! ⏎ or space turns the selected setting on or off, or opens its dialog.
//! The daemon writes the change into the person's own settings for the
//! project, never into its repository (`ARC-05`), and the project's
//! tickets are fetched again.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::ProjectId;
use muster_core::protocol::Command;
use muster_core::settings::{ProjectSettings, SettingChange};

use super::{Action, App, Mode, Then, View};

/// One line of the settings list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SettingRow {
    Project(ProjectId),
    /// Only the tickets assigned to the person.
    OnlyMine(ProjectId),
    /// Where its tickets come from.
    TicketsFrom(ProjectId),
}

impl SettingRow {
    pub(crate) fn project(self) -> ProjectId {
        match self {
            Self::Project(project) | Self::OnlyMine(project) | Self::TicketsFrom(project) => {
                project
            }
        }
    }
}

/// A project's settings, as last read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Settings {
    /// Being read, or being changed.
    Loading,
    Loaded(ProjectSettings),
    /// Why they could not be read: the reason names the file.
    Failed(String),
}

impl App {
    /// The settings list: each project, then its settings.
    pub(crate) fn setting_rows(&self) -> Vec<SettingRow> {
        self.model
            .projects()
            .iter()
            .flat_map(|project| {
                [
                    SettingRow::Project(project.id),
                    SettingRow::OnlyMine(project.id),
                    SettingRow::TicketsFrom(project.id),
                ]
            })
            .collect()
    }

    /// A key on the Settings screen; the rest are inert here.
    pub(super) fn settings_key(&mut self, key: &KeyEvent) -> Vec<Action> {
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.move_selection(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_selection(1),
            KeyCode::Enter | KeyCode::Char(' ') => match self.settings_selected {
                Some(SettingRow::TicketsFrom(project)) => return self.choose_tickets_from(project),
                _ => return self.toggle_setting(),
            },
            KeyCode::Char('r') => {
                // Read again as they are shown, edits by hand included.
                self.settings.clear();
                self.say("Reading the settings again…");
            }
            _ => {}
        }
        self.settle()
    }

    /// Turns the selected setting on or off.
    fn toggle_setting(&mut self) -> Vec<Action> {
        let Some(SettingRow::OnlyMine(project)) = self.settings_selected else {
            return Vec::new();
        };
        let change = match self.settings.get(&project) {
            Some(Settings::Loaded(settings)) => {
                SettingChange::OnlyMyTickets(!settings.only_mine.on)
            }
            Some(Settings::Failed(error)) => {
                self.complain(format!("Its settings cannot be read: {error}"));
                return Vec::new();
            }
            // Read or changed just now: the answer is on its way.
            Some(Settings::Loading) | None => return Vec::new(),
        };
        self.settings.insert(project, Settings::Loading);
        vec![self.command(
            Command::ChangeSetting { project, change },
            Then::SettingChanged(project),
        )]
    }

    /// A setting was changed: the project's tickets are fetched again, as
    /// they are now chosen.
    pub(super) fn setting_changed(&mut self, project: ProjectId, settings: ProjectSettings) {
        let name = self
            .model
            .project(project)
            .map_or_else(|| "The project".to_owned(), |found| found.name.clone());
        let lists = if settings.only_mine.on {
            "only the tickets assigned to you"
        } else {
            "every open ticket"
        };
        self.say(format!("Tickets for {name} now lists {lists}."));
        self.settings.insert(project, Settings::Loaded(settings));
        self.tickets.remove(&project);
        self.ticket_texts.retain(|(of, _), _| *of != project);
    }

    /// A change that was refused: the reason is shown, and the settings
    /// read again, as they still are.
    pub(super) fn setting_refused(&mut self, project: ProjectId, reason: String) {
        self.settings.remove(&project);
        self.complain(reason);
    }

    /// Asks for the settings of projects shown without them, while the
    /// Settings screen is up.
    pub(super) fn fetch_settings(&mut self) -> Vec<Action> {
        if self.view != View::Settings || !matches!(self.mode, Mode::Browse | Mode::Dialog(_)) {
            return Vec::new();
        }
        let wanted: Vec<ProjectId> = self
            .model
            .projects()
            .iter()
            .map(|project| project.id)
            .filter(|project| !self.settings.contains_key(project))
            .collect();
        wanted
            .into_iter()
            .map(|project| {
                self.settings.insert(project, Settings::Loading);
                self.command(
                    Command::ProjectSettings { project },
                    Then::Settings(project),
                )
            })
            .collect()
    }

    /// Keeps the settings selection on a line that exists: the first
    /// setting, to begin with, so ⏎ does something at once.
    pub(super) fn keep_setting_selection(&mut self) {
        let rows = self.setting_rows();
        if self
            .settings_selected
            .is_some_and(|row| rows.contains(&row))
        {
            return;
        }
        self.settings_selected = self
            .settings_selected
            .map(|row| SettingRow::Project(row.project()))
            .filter(|row| rows.contains(row))
            .or_else(|| {
                rows.iter()
                    .find(|row| matches!(row, SettingRow::OnlyMine(_)))
                    .copied()
            });
    }
}

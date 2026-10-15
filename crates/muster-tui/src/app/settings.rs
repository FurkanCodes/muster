//! The Settings screen: each project's settings that the person can
//! change from here, read from the daemon as the screen is shown: whether
//! the Tickets screen lists only the tickets assigned to the person, in
//! whatever tracker the project's tickets are, where they come from
//! (`tickets_from`), and whether the branches Muster makes start with
//! `muster/`.
//!
//! ⏎ or space turns the selected setting on or off, or opens its dialog.
//! The daemon writes the change into the person's own settings for the
//! project, never into its repository (`ARC-05`); a change to its tickets
//! fetches them again.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::ProjectId;
use muster_core::protocol::Command;
use muster_core::settings::{MUSTER_PREFIX_PROTOCOL, ProjectSettings, SettingChange};

use super::{Action, App, Mode, Then, View};

/// One line of the settings list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum SettingRow {
    Project(ProjectId),
    /// Only the tickets assigned to the person.
    OnlyMine(ProjectId),
    /// Where its tickets come from.
    TicketsFrom(ProjectId),
    /// Whether the branches Muster makes start with `muster/`.
    MusterPrefix(ProjectId),
}

impl SettingRow {
    pub(crate) fn project(self) -> ProjectId {
        match self {
            Self::Project(project)
            | Self::OnlyMine(project)
            | Self::TicketsFrom(project)
            | Self::MusterPrefix(project) => project,
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
    /// The settings list: each project, then its settings; the branches'
    /// prefix only where the daemon can change it.
    pub(crate) fn setting_rows(&self) -> Vec<SettingRow> {
        let prefix = self.daemon_protocol >= MUSTER_PREFIX_PROTOCOL;
        self.model
            .projects()
            .iter()
            .flat_map(|project| {
                [
                    Some(SettingRow::Project(project.id)),
                    Some(SettingRow::OnlyMine(project.id)),
                    Some(SettingRow::TicketsFrom(project.id)),
                    prefix.then_some(SettingRow::MusterPrefix(project.id)),
                ]
            })
            .flatten()
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
        let (project, then) = match self.settings_selected {
            Some(SettingRow::OnlyMine(project)) => (project, Then::SettingChanged(project)),
            Some(SettingRow::MusterPrefix(project)) => (project, Then::BranchesChanged(project)),
            _ => return Vec::new(),
        };
        let change = match self.settings.get(&project) {
            Some(Settings::Loaded(settings)) => match then {
                Then::BranchesChanged(_) => match settings.muster_prefix {
                    Some(prefix) => SettingChange::MusterPrefix(!prefix.on),
                    None => return Vec::new(),
                },
                _ => SettingChange::OnlyMyTickets(!settings.only_mine.on),
            },
            Some(Settings::Failed(error)) => {
                self.complain(format!("Its settings cannot be read: {error}"));
                return Vec::new();
            }
            // Read or changed just now: the answer is on its way.
            Some(Settings::Loading) | None => return Vec::new(),
        };
        self.settings.insert(project, Settings::Loading);
        vec![self.command(Command::ChangeSetting { project, change }, then)]
    }

    /// Whether the project's branches start with `muster/` was changed.
    pub(super) fn branches_changed(&mut self, project: ProjectId, settings: ProjectSettings) {
        let name = self
            .model
            .project(project)
            .map_or_else(|| "the project".to_owned(), |found| found.name.clone());
        match settings.muster_prefix {
            Some(prefix) if prefix.on => {
                self.say(format!("New branches in {name} start with muster/."));
            }
            _ => self.say(format!(
                "New branches in {name} go without muster/: issue-42-fix-the-login."
            )),
        }
        self.settings.insert(project, Settings::Loaded(settings));
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

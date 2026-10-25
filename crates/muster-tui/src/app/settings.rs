//! The Settings screen: each project's settings that the person can
//! change from here, read from the daemon as the screen is shown: whether
//! the Tickets screen lists only the tickets assigned to the person, in
//! whatever tracker the project's tickets are, where they come from
//! (`tickets_from`), and whether the branches Muster makes start with
//! `muster/`. Then Muster's own: on a Mac, whether the notch shows the
//! agents; from how many columns a session sits beside the list; whose
//! colours the UI draws in and how many lines a session takes; and the
//! coding agents the person added in Muster's own settings, which are
//! written by hand, so the row says what reads and what does not. Then
//! the plugins (`app::plugins`).
//!
//! ⏎ or space turns the selected setting on or off, goes to its next
//! choice, or opens its dialog; ← → go to a smaller or larger choice.
//! The daemon writes the change into the person's own settings for the
//! project, never into its repository (`ARC-05`); a change to its tickets
//! fetches them again.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::ProjectId;
use muster_core::keys::Action as Does;
use muster_core::protocol::Command;
use muster_core::settings::{PreferenceChange, ProjectSettings, SettingChange};

use super::{Action, App, Mode, PluginKey, Then, View};

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
    /// Muster's own settings, for all of it.
    Muster,
    /// Whether the notch shows the agents.
    Notch,
    /// From how many columns a session sits beside the list.
    SideBySide,
    /// Whose colours the UI draws in.
    Colours,
    /// How many lines a session's line takes.
    SessionLines,
    /// Where the panels are, and whether the list is folded away.
    Panels,
    /// The order the projects are listed in.
    ProjectOrder,
    /// Whether an agent names chats.
    NamesByAgent,
    /// Which agent names them.
    NamedBy,
    /// The model it names them with.
    NamingModel,
    /// The keys, Muster's and the person's.
    Keys,
    /// Whether a project's worktrees build into one folder.
    SharedBuilds,
    /// Whether agents may set agents working (`PRD-01`).
    Delegation,
    /// Whether the person is told of what newly needs them.
    Notifications,
    /// Whether a kind of notification is told, and how it sounds.
    Tell(muster_core::telling::Kind),
    /// The coding agents the person added in Muster's own settings.
    Agents,
    /// The plugins, as a heading (`app::plugins`).
    Plugins,
    /// A plugin, by its id.
    Plugin(PluginKey),
    /// Adds a plugin.
    AddPlugin,
}

impl SettingRow {
    /// The project whose setting it is; `None` for Muster's own.
    pub(crate) fn project(self) -> Option<ProjectId> {
        match self {
            Self::Project(project)
            | Self::OnlyMine(project)
            | Self::TicketsFrom(project)
            | Self::MusterPrefix(project) => Some(project),
            Self::Muster
            | Self::Notch
            | Self::SideBySide
            | Self::Colours
            | Self::SessionLines
            | Self::Panels
            | Self::ProjectOrder
            | Self::NamesByAgent
            | Self::NamedBy
            | Self::NamingModel
            | Self::Keys
            | Self::SharedBuilds
            | Self::Delegation
            | Self::Notifications
            | Self::Tell(_)
            | Self::Agents
            | Self::Plugins
            | Self::Plugin(_)
            | Self::AddPlugin => None,
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
    /// The settings list: each project, then its settings. Then Muster's
    /// own, once the state has come: the notch, on a Mac, the
    /// notifications, the width a session sits beside the list from, the
    /// look, the layout, naming chats, keys, shared builds, delegation and
    /// the person's agents. Then the plugins.
    pub(crate) fn setting_rows(&self) -> Vec<SettingRow> {
        let mut rows: Vec<SettingRow> = self
            .model
            .projects()
            .iter()
            .flat_map(|project| {
                [
                    SettingRow::Project(project.id),
                    SettingRow::OnlyMine(project.id),
                    SettingRow::TicketsFrom(project.id),
                    SettingRow::MusterPrefix(project.id),
                ]
            })
            .collect();
        if self.model.loaded() {
            rows.push(SettingRow::Muster);
            rows.extend(self.notch_known().then_some(SettingRow::Notch));
            rows.extend([
                SettingRow::Notifications,
                SettingRow::Tell(muster_core::telling::Kind::Question),
                SettingRow::Tell(muster_core::telling::Kind::Finish),
                SettingRow::Tell(muster_core::telling::Kind::Error),
                SettingRow::SideBySide,
                SettingRow::Colours,
                SettingRow::SessionLines,
                SettingRow::Panels,
                SettingRow::ProjectOrder,
                SettingRow::NamesByAgent,
                SettingRow::NamedBy,
                SettingRow::NamingModel,
                SettingRow::Keys,
                SettingRow::SharedBuilds,
                SettingRow::Delegation,
                SettingRow::Agents,
            ]);
        }
        if self.model.loaded() {
            rows.extend(self.plugin_rows());
        }
        rows
    }

    /// ⏎ on *Shared Rust builds*: on, or off.
    fn toggle_shared_builds(&mut self) -> Vec<Action> {
        let on = !self.model.preferences().shared_builds;
        let said = if on {
            "A project's worktrees build into one folder beside them."
        } else {
            "Each worktree builds into its own folder again; the shared builds are deleted."
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::SharedBuilds(on)),
            Then::Say(said),
        )]
    }

    /// ⏎ on *Agents start agents*: on, or off.
    fn toggle_delegation(&mut self) -> Vec<Action> {
        let on = !self.model.preferences().delegation.allowed;
        let said = if on {
            "Agents may start agents through Muster's command line, within your limits."
        } else {
            "No agent starts agents: the command line refuses inside an agent's session."
        };
        vec![self.command(
            Command::ChangePreference(PreferenceChange::AgentsStartAgents(on)),
            Then::Say(said),
        )]
    }

    /// A key on the Settings screen; the rest are inert here.
    pub(super) fn settings_key(&mut self, key: &KeyEvent, does: Option<Does>) -> Vec<Action> {
        match (key.code, does) {
            (KeyCode::Up, _) | (_, Some(Does::MoveUp)) => self.move_selection(-1),
            (KeyCode::Down, _) | (_, Some(Does::MoveDown)) => self.move_selection(1),
            (KeyCode::Enter | KeyCode::Char(' '), _) => match self.settings.selected {
                Some(SettingRow::TicketsFrom(project)) => return self.choose_tickets_from(project),
                Some(SettingRow::Notch) => return self.toggle_notch(),
                Some(SettingRow::SideBySide) => return self.step_side_by_side(1, true),
                Some(SettingRow::Colours) => return self.toggle_palette(),
                Some(SettingRow::SessionLines) => return self.toggle_session_lines(),
                Some(SettingRow::Panels) => return self.put_panels_back(),
                Some(SettingRow::ProjectOrder) => return self.put_projects_back(),
                Some(SettingRow::NamesByAgent) => return self.toggle_names_by_agent(),
                Some(SettingRow::NamedBy) => return self.next_namer(),
                Some(SettingRow::NamingModel) => return self.choose_naming_model(),
                Some(SettingRow::Keys) => return self.open_rebinding(),
                Some(SettingRow::SharedBuilds) => return self.toggle_shared_builds(),
                Some(SettingRow::Delegation) => return self.toggle_delegation(),
                Some(SettingRow::Notifications) => return self.toggle_notifications(),
                Some(SettingRow::Tell(kind)) => return self.toggle_tell(kind),
                Some(SettingRow::Plugin(plugin)) => return self.toggle_plugin(plugin),
                Some(SettingRow::AddPlugin) => return self.open_add_plugin(),
                _ => return self.toggle_setting(),
            },
            (_, Some(Does::AddPlugin)) => return self.open_add_plugin(),
            (_, Some(Does::UpdatePlugin)) => return self.update_plugin(),
            (_, Some(Does::RemovePlugin)) => {
                return self.confirm_remove_plugin();
            }
            (KeyCode::Left | KeyCode::Right, _)
                if self.settings.selected == Some(SettingRow::SideBySide) =>
            {
                let by = if key.code == KeyCode::Left { -1 } else { 1 };
                return self.step_side_by_side(by, false);
            }
            (KeyCode::Left | KeyCode::Right, _) => {
                if let Some(SettingRow::Tell(kind)) = self.settings.selected {
                    let by = if key.code == KeyCode::Left { -1 } else { 1 };
                    return self.step_sound(kind, by);
                }
            }
            (_, Some(Does::ReadSettings)) => {
                // Read again as they are shown, edits by hand included,
                // Muster's own too.
                self.settings.by_project.clear();
                let mut actions = vec![self.command(
                    Command::ReadPreferences,
                    Then::Say("Read the settings again."),
                )];
                actions.extend(self.settle());
                return actions;
            }
            _ => {}
        }
        self.settle()
    }

    /// Turns the selected setting on or off.
    fn toggle_setting(&mut self) -> Vec<Action> {
        let (project, then) = match self.settings.selected {
            Some(SettingRow::OnlyMine(project)) => (project, Then::SettingChanged(project)),
            Some(SettingRow::MusterPrefix(project)) => (project, Then::BranchesChanged(project)),
            _ => return Vec::new(),
        };
        let change = match self.settings.by_project.get(&project) {
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
        self.settings.by_project.insert(project, Settings::Loading);
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
        self.settings
            .by_project
            .insert(project, Settings::Loaded(settings));
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
        self.settings
            .by_project
            .insert(project, Settings::Loaded(settings));
        self.tickets.by_project.remove(&project);
        self.tickets.texts.retain(|(of, _), _| *of != project);
    }

    /// A change that was refused: the reason is shown, and the settings
    /// read again, as they still are.
    pub(super) fn setting_refused(&mut self, project: ProjectId, reason: String) {
        self.settings.by_project.remove(&project);
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
            .filter(|project| !self.settings.by_project.contains_key(project))
            .collect();
        wanted
            .into_iter()
            .map(|project| {
                self.settings.by_project.insert(project, Settings::Loading);
                self.command(
                    Command::ProjectSettings { project },
                    Then::Settings(project),
                )
            })
            .collect()
    }

    /// Keeps the settings selection on a line that exists: the first
    /// setting, to begin with, so ⏎ does something at once; a plugin just
    /// installed once it is listed, and *add a plugin* for one removed.
    pub(super) fn keep_setting_selection(&mut self) {
        let rows = self.setting_rows();
        if let Some(wanted) = self.settings.plugin_wanted
            && rows.contains(&SettingRow::Plugin(wanted))
        {
            self.settings.plugin_wanted = None;
            self.settings.selected = Some(SettingRow::Plugin(wanted));
            return;
        }
        if self
            .settings
            .selected
            .is_some_and(|row| rows.contains(&row))
        {
            return;
        }
        // Chosen as the screen shows, so the first is a project's once
        // there is one, not Muster's own, listed before any project.
        if self.settings.selected.is_none() && self.view != View::Settings {
            return;
        }
        self.settings.selected = self
            .settings
            .selected
            .and_then(|row| match row {
                SettingRow::Plugin(_) => Some(SettingRow::AddPlugin),
                row => row.project().map(SettingRow::Project),
            })
            .filter(|row| rows.contains(row))
            .or_else(|| {
                rows.iter()
                    .find(|row| {
                        matches!(
                            row,
                            SettingRow::OnlyMine(_) | SettingRow::Notch | SettingRow::SideBySide
                        )
                    })
                    .copied()
            });
    }
}

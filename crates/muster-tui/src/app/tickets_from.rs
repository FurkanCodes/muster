//! Choosing where a project's tickets come from, on the Settings screen:
//! the tracker its git remote says, or one the person names, with what it
//! needs to find them: Jira its site and a project's key, Linear a team's
//! key. For a project whose remote is on a company's own server, an IP
//! address Muster cannot read a tracker from, this is how it gets one.
//!
//! ⇥ and ⇧⇥ go round the trackers, ↑↓ between Jira's two fields, and ⏎
//! saves. The daemon writes the choice into the person's own settings for
//! the project, never into its repository (`ARC-05`), and the project's
//! tickets are fetched again.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::ProjectId;
use muster_core::config::TrackerKind;
use muster_core::protocol::Command;
use muster_core::settings::{ProjectSettings, SettingChange, TICKETS_FROM_PROTOCOL, TicketsFrom};

use super::{Action, App, Dialog, Mode, Settings, Then};
use crate::input::{Edit, LineInput};

/// The choices, in the order ⇥ goes round them: the git remote's first.
pub(crate) const TRACKERS: [Option<TrackerKind>; 5] = [
    None,
    Some(TrackerKind::GitHub),
    Some(TrackerKind::GitLab),
    Some(TrackerKind::Jira),
    Some(TrackerKind::Linear),
];

/// What a choice is called in the dialog.
pub(crate) fn tracker_label(tracker: Option<TrackerKind>) -> &'static str {
    match tracker {
        None => "its git remote",
        Some(TrackerKind::GitHub) => "GitHub",
        Some(TrackerKind::GitLab) => "GitLab",
        Some(TrackerKind::Jira) => "Jira",
        Some(TrackerKind::Linear) => "Linear",
    }
}

/// Which field of the dialog has the cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Field {
    /// The tracker needs nothing typed.
    None,
    /// Jira's site.
    Site,
    /// Jira's project's key, or Linear's team's.
    Key,
}

/// The field with the cursor for `tracker`, `on_key` or not.
pub(crate) fn focused(tracker: Option<TrackerKind>, on_key: bool) -> Field {
    match tracker {
        Some(TrackerKind::Jira) if on_key => Field::Key,
        Some(TrackerKind::Jira) => Field::Site,
        Some(TrackerKind::Linear) => Field::Key,
        _ => Field::None,
    }
}

/// A field's text, trimmed; `None` when nothing is typed.
fn typed(input: &LineInput) -> Option<String> {
    Some(input.text().trim().to_owned()).filter(|text| !text.is_empty())
}

impl App {
    /// ⏎ on the project's Tickets from: the dialog, filled in with what is
    /// written now, and for Jira with the site another project uses.
    pub(super) fn choose_tickets_from(&mut self, project: ProjectId) -> Vec<Action> {
        let settings = match self.settings.get(&project) {
            Some(Settings::Loaded(settings)) => settings.clone(),
            Some(Settings::Failed(error)) => {
                self.complain(format!("Its settings cannot be read: {error}"));
                return Vec::new();
            }
            // Read or changed just now: the answer is on its way.
            Some(Settings::Loading) | None => return Vec::new(),
        };
        if self.daemon_protocol < TICKETS_FROM_PROTOCOL {
            self.complain(
                "The daemon runs an older muster that cannot change this: `muster reload` moves \
                 it onto this one, every session running on.",
            );
            return Vec::new();
        }
        let written = settings.tickets.as_ref();
        let site = written
            .and_then(|from| from.site.clone())
            .or_else(|| self.jira_site_in_use());
        let key = written.and_then(|from| from.project.clone().or_else(|| from.team.clone()));
        self.mode = Mode::Dialog(Dialog::TicketsFrom {
            project,
            tracker: written.map(|from| from.tracker),
            on_key: site.is_some(),
            site: LineInput::with(site.as_deref().unwrap_or_default()),
            key: LineInput::with(key.as_deref().unwrap_or_default()),
        });
        Vec::new()
    }

    /// The Jira site some project's tickets come from, if any: a company's
    /// projects are usually on one.
    fn jira_site_in_use(&self) -> Option<String> {
        self.settings.values().find_map(|settings| match settings {
            Settings::Loaded(ProjectSettings {
                tickets:
                    Some(TicketsFrom {
                        tracker: TrackerKind::Jira,
                        site: Some(site),
                        ..
                    }),
                ..
            }) => Some(site.clone()),
            _ => None,
        })
    }

    /// A key in the dialog.
    pub(super) fn tickets_from_key(&mut self, dialog: Dialog, key: &KeyEvent) -> Vec<Action> {
        let Dialog::TicketsFrom {
            project,
            mut tracker,
            mut site,
            key: mut key_input,
            mut on_key,
        } = dialog
        else {
            return Vec::new();
        };
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                let at = TRACKERS.iter().position(|t| *t == tracker).unwrap_or(0);
                let step = if key.code == KeyCode::Tab {
                    1
                } else {
                    TRACKERS.len() - 1
                };
                tracker = TRACKERS[(at + step) % TRACKERS.len()];
            }
            KeyCode::Up | KeyCode::Down if tracker == Some(TrackerKind::Jira) => on_key = !on_key,
            _ => {
                let edit = match focused(tracker, on_key) {
                    Field::Site => site.handle(key),
                    Field::Key => key_input.handle(key),
                    Field::None => match key.code {
                        KeyCode::Enter => Edit::Submit,
                        KeyCode::Esc => Edit::Cancel,
                        _ => Edit::Ignored,
                    },
                };
                match edit {
                    Edit::Submit => {
                        return self.save_tickets_from(project, tracker, site, key_input, on_key);
                    }
                    Edit::Cancel => {
                        self.mode = Mode::Browse;
                        return Vec::new();
                    }
                    Edit::Changed | Edit::Ignored => {}
                }
            }
        }
        self.mode = Mode::Dialog(Dialog::TicketsFrom {
            project,
            tracker,
            site,
            key: key_input,
            on_key,
        });
        Vec::new()
    }

    /// ⏎ in the dialog: the choice sent, or what it still needs said and
    /// the cursor put there.
    fn save_tickets_from(
        &mut self,
        project: ProjectId,
        tracker: Option<TrackerKind>,
        site: LineInput,
        key: LineInput,
        on_key: bool,
    ) -> Vec<Action> {
        let needs = match tracker {
            Some(TrackerKind::Jira) if typed(&site).is_none() => {
                Some(("Jira needs its site, such as acme.atlassian.net.", false))
            }
            Some(TrackerKind::Jira) if typed(&key).is_none() => {
                Some(("Jira needs the project's key, such as APP.", true))
            }
            Some(TrackerKind::Linear) if typed(&key).is_none() => {
                Some(("Linear needs the team's key, such as ENG.", on_key))
            }
            _ => None,
        };
        if let Some((what, on_key)) = needs {
            self.complain(what);
            self.mode = Mode::Dialog(Dialog::TicketsFrom {
                project,
                tracker,
                site,
                key,
                on_key,
            });
            return Vec::new();
        }
        // A query of the person's own, written by hand, stays with Jira.
        let jql = match self.settings.get(&project) {
            Some(Settings::Loaded(ProjectSettings {
                tickets: Some(written),
                ..
            })) if written.tracker == TrackerKind::Jira => written.jql.clone(),
            _ => None,
        };
        let (jira, linear) = (
            tracker == Some(TrackerKind::Jira),
            tracker == Some(TrackerKind::Linear),
        );
        let from = tracker.map(|tracker| TicketsFrom {
            tracker,
            site: typed(&site).filter(|_| jira),
            project: typed(&key).filter(|_| jira),
            jql: jql.filter(|_| jira),
            team: typed(&key).filter(|_| linear),
        });
        self.mode = Mode::Browse;
        self.settings.insert(project, Settings::Loading);
        vec![self.command(
            Command::ChangeSetting {
                project,
                change: SettingChange::TicketsFrom(from),
            },
            Then::TicketsFromChanged(project),
        )]
    }

    /// Where its tickets come from was changed: said, and they are fetched
    /// again from there.
    pub(super) fn tickets_from_changed(&mut self, project: ProjectId, settings: ProjectSettings) {
        let name = self
            .model
            .project(project)
            .map_or_else(|| "The project".to_owned(), |found| found.name.clone());
        self.say(format!(
            "{name}'s tickets now come from {}.",
            settings.tickets_from
        ));
        self.settings.insert(project, Settings::Loaded(settings));
        self.tickets.remove(&project);
        self.ticket_texts.retain(|(of, _), _| *of != project);
    }
}

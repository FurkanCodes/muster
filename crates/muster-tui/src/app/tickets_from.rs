//! Choosing where a project's tickets come from, on the Settings screen:
//! the tracker its git remote says, or a ticket plugin the person names,
//! Muster's own or one they installed, with what its manifest says it
//! needs (`[[tickets.settings]]`): Jira its site and a project's key,
//! Linear a team's key. For a project whose remote is on a company's own
//! server, an IP address Muster cannot read a tracker from, this is how it
//! gets one.
//!
//! ⇥ and ⇧⇥ go round the trackers, ↑↓ between the chosen one's fields,
//! and ⏎ saves. The daemon writes the choice into the person's own
//! settings for the project, never into its repository (`ARC-05`), and
//! the project's tickets are fetched again.

use std::collections::BTreeMap;

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::ProjectId;
use muster_core::plugins::{PluginSource, TicketSetting};
use muster_core::protocol::Command;
use muster_core::protocol::plugins::PluginInfo;
use muster_core::settings::{ProjectSettings, SettingChange, TicketsFrom};

use super::{Action, App, Dialog, Mode, Settings, Then};
use crate::input::{Edit, LineInput};

/// A tracker the dialog offers: a ticket plugin that is on and can run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Offered {
    /// Its plugin's id.
    pub(crate) id: String,
    /// As its manifest's `[tickets] name` says: `GitHub`.
    pub(crate) name: String,
    /// What its plugin says it is, if it says.
    pub(crate) description: Option<String>,
    pub(crate) settings: Vec<TicketSetting>,
}

/// The ticket plugins in `plugins` that can run, in the order ⇥ goes round
/// them: Muster's own as the daemon lists them, then anyone's by name.
pub(crate) fn offered(plugins: &[PluginInfo]) -> Vec<Offered> {
    let mut offered: Vec<(bool, Offered)> = plugins
        .iter()
        .filter(|plugin| plugin.runs())
        .filter_map(|plugin| {
            let details = plugin.details.as_ref()?;
            let tickets = details.tickets.as_ref()?;
            Some((
                plugin.source != PluginSource::Builtin,
                Offered {
                    id: plugin.id.clone(),
                    name: tickets.name.clone(),
                    description: details.description.clone(),
                    settings: tickets.settings.clone(),
                },
            ))
        })
        .collect();
    // Stable: Muster's own keep the daemon's order.
    offered.sort_by(|(a_theirs, a), (b_theirs, b)| {
        a_theirs.cmp(b_theirs).then_with(|| {
            if *a_theirs {
                a.name.cmp(&b.name)
            } else {
                std::cmp::Ordering::Equal
            }
        })
    });
    offered.into_iter().map(|(_, offered)| offered).collect()
}

/// A field of the dialog: one of the chosen plugin's settings, as typed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) setting: TicketSetting,
    pub(crate) input: LineInput,
}

/// A field's text, trimmed; `None` when nothing is typed.
fn typed(input: &LineInput) -> Option<String> {
    Some(input.text().trim().to_owned()).filter(|text| !text.is_empty())
}

impl App {
    /// The trackers the dialog offers, as the daemon's plugins say now.
    pub(crate) fn offered_trackers(&self) -> Vec<Offered> {
        offered(self.model.plugins())
    }

    /// ⏎ on the project's Tickets from: the dialog, filled in with what is
    /// written now.
    pub(super) fn choose_tickets_from(&mut self, project: ProjectId) -> Vec<Action> {
        let settings = match self.settings.by_project.get(&project) {
            Some(Settings::Loaded(settings)) => settings.clone(),
            Some(Settings::Failed(error)) => {
                self.complain(format!("Its settings cannot be read: {error}"));
                return Vec::new();
            }
            // Read or changed just now: the answer is on its way.
            Some(Settings::Loading) | None => return Vec::new(),
        };
        let tracker = settings.tickets.as_ref().map(|from| from.tracker.clone());
        let fields = self.fields_for(project, tracker.as_deref(), &BTreeMap::new());
        self.mode = Mode::Dialog(Dialog::TicketsFrom {
            project,
            at: first_empty(&fields),
            tracker,
            fields,
            typed: BTreeMap::new(),
        });
        Vec::new()
    }

    /// The fields of `tracker` for `project`: what the person typed for it
    /// in this dialog, else what is written now, else, for a site, the one
    /// another project of the same tracker uses (a company's projects are
    /// usually on one).
    fn fields_for(
        &self,
        project: ProjectId,
        tracker: Option<&str>,
        typed: &BTreeMap<(String, String), String>,
    ) -> Vec<Field> {
        let Some(tracker) = tracker else {
            return Vec::new();
        };
        let Some(offered) = self
            .offered_trackers()
            .into_iter()
            .find(|offered| offered.id == tracker)
        else {
            return Vec::new();
        };
        let written = |of: ProjectId| match self.settings.by_project.get(&of) {
            Some(Settings::Loaded(ProjectSettings {
                tickets: Some(from),
                ..
            })) if from.tracker == tracker => Some(from.fields.clone()),
            _ => None,
        };
        let here = written(project).unwrap_or_default();
        offered
            .settings
            .into_iter()
            .map(|setting| {
                let text = typed
                    .get(&(tracker.to_owned(), setting.key.clone()))
                    .cloned()
                    .or_else(|| here.get(&setting.key).cloned())
                    .or_else(|| {
                        (setting.key == "site").then(|| {
                            self.settings
                                .by_project
                                .keys()
                                .filter(|other| **other != project)
                                .find_map(|other| written(*other)?.get("site").cloned())
                        })?
                    })
                    .unwrap_or_default();
                Field {
                    setting,
                    input: LineInput::with(&text),
                }
            })
            .collect()
    }

    /// A key in the dialog.
    pub(super) fn tickets_from_key(&mut self, dialog: Dialog, key: &KeyEvent) -> Vec<Action> {
        let Dialog::TicketsFrom {
            project,
            mut tracker,
            mut fields,
            mut at,
            mut typed,
        } = dialog
        else {
            return Vec::new();
        };
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                if let Some(id) = &tracker {
                    for field in &fields {
                        typed.insert(
                            (id.clone(), field.setting.key.clone()),
                            field.input.text().to_owned(),
                        );
                    }
                }
                let mut choices: Vec<Option<String>> = vec![None];
                choices.extend(self.offered_trackers().into_iter().map(|o| Some(o.id)));
                let now = choices.iter().position(|t| *t == tracker).unwrap_or(0);
                let step = if key.code == KeyCode::Tab {
                    1
                } else {
                    choices.len() - 1
                };
                tracker = choices.swap_remove((now + step) % choices.len());
                fields = self.fields_for(project, tracker.as_deref(), &typed);
                at = first_empty(&fields);
            }
            KeyCode::Up if fields.len() > 1 => at = (at + fields.len() - 1) % fields.len(),
            KeyCode::Down if fields.len() > 1 => at = (at + 1) % fields.len(),
            _ => {
                let edit = match fields.get_mut(at) {
                    Some(field) => field.input.handle(key),
                    None => match key.code {
                        KeyCode::Enter => Edit::Submit,
                        KeyCode::Esc => Edit::Cancel,
                        _ => Edit::Ignored,
                    },
                };
                match edit {
                    Edit::Submit => {
                        return self.save_tickets_from(project, tracker, fields, typed);
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
            fields,
            at,
            typed,
        });
        Vec::new()
    }

    /// A paste in the dialog: into the field with the cursor.
    pub(super) fn tickets_from_paste(&mut self, text: &str) {
        if let Mode::Dialog(Dialog::TicketsFrom { fields, at, .. }) = &mut self.mode
            && let Some(field) = fields.get_mut(*at)
        {
            field.input.paste(text);
        }
    }

    /// ⏎ in the dialog: the choice sent, or what it still needs said and
    /// the cursor put there.
    fn save_tickets_from(
        &mut self,
        project: ProjectId,
        tracker: Option<String>,
        fields: Vec<Field>,
        typed: BTreeMap<(String, String), String>,
    ) -> Vec<Action> {
        if let Some(missing) = fields
            .iter()
            .position(|field| field.setting.required && self::typed(&field.input).is_none())
        {
            let name = self
                .offered_trackers()
                .into_iter()
                .find(|offered| Some(&offered.id) == tracker.as_ref())
                .map_or_else(|| "It".to_owned(), |offered| offered.name);
            self.complain(format!(
                "{name} needs its {}.",
                fields[missing].setting.label
            ));
            self.mode = Mode::Dialog(Dialog::TicketsFrom {
                project,
                tracker,
                fields,
                at: missing,
                typed,
            });
            return Vec::new();
        }
        let from = tracker.map(|tracker| TicketsFrom {
            tracker,
            fields: fields
                .iter()
                .filter_map(|field| Some((field.setting.key.clone(), self::typed(&field.input)?)))
                .collect(),
        });
        self.mode = Mode::Browse;
        self.settings.by_project.insert(project, Settings::Loading);
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
        self.settings
            .by_project
            .insert(project, Settings::Loaded(settings));
        self.tickets.by_project.remove(&project);
        self.tickets.texts.retain(|(of, _), _| *of != project);
    }
}

/// The field the cursor starts on: the first needed with nothing typed,
/// else the first with nothing typed, else the first.
fn first_empty(fields: &[Field]) -> usize {
    let empty = |field: &&Field| typed(&field.input).is_none();
    fields
        .iter()
        .position(|field| field.setting.required && empty(&field))
        .or_else(|| fields.iter().position(|field| empty(&field)))
        .unwrap_or(0)
}

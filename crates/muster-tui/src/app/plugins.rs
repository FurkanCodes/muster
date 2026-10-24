//! The Plugins section of the Settings screen (`PRODUCT.md` §6,
//! `ARCHITECTURE.md` §7a), under Muster's own settings: each plugin the
//! daemon has, then *add a plugin*. Space (or ⏎) turns one on or off,
//! refused with why where it cannot run; `a` adds one from a git
//! repository's address or a folder, `u` updates one, each fetching a
//! preview first, which can take minutes (Esc gives up), and showing
//! everything the plugin would run before the person says yes (`PRD-15`,
//! `PRD-01`); `x` removes one after asking. Shown by a daemon of protocol
//! 41 or later; the list follows the daemon's as it changes, from the
//! command line too.

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::plugins::{ID_MAX, PluginSource};
use muster_core::protocol::plugins::{
    ChangedBy, PluginCommand, PluginInfo, PluginOrigin, PluginPreview, PreviewKind,
};
use muster_core::protocol::{Command, ErrorBody, Reply};
use muster_core::text::one_line;

use super::{Action, App, Confirmed, Dialog, Mode, SettingRow, Then};
use crate::input::{Edit, LineInput};

/// A plugin's id as its settings row holds it: at most [`ID_MAX`] of
/// `a-z`, `0-9` and `-`, kept in place so the row copies as the others do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct PluginKey {
    len: u8,
    bytes: [u8; ID_MAX],
}

impl PluginKey {
    /// The key of the plugin `id`; `None` for an id no daemon gives.
    pub(crate) fn of(id: &str) -> Option<Self> {
        if !muster_core::plugins::is_id(id) {
            return None;
        }
        let mut bytes = [0; ID_MAX];
        bytes[..id.len()].copy_from_slice(id.as_bytes());
        Some(Self {
            len: u8::try_from(id.len()).ok()?,
            bytes,
        })
    }

    pub(crate) fn id(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).unwrap_or_default()
    }
}

/// Where to add a plugin from, as typed.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Typed {
    /// A git repository's address, or a folder's path.
    pub(crate) address: LineInput,
    /// The branch or tag to fetch, for git; empty for its default branch.
    pub(crate) git_ref: LineInput,
    /// Whether the branch or tag has the cursor.
    pub(crate) on_ref: bool,
}

impl Typed {
    /// Where it points: a folder, linked where it is, if it reads as a
    /// path (`/…`, `~/…`, `./…`); else a git repository's address.
    fn origin(&self, cwd: &std::path::Path) -> PluginOrigin {
        let address = self.address.text().trim();
        if is_folder(address) {
            return PluginOrigin::Folder {
                path: super::picker::resolve_path(address, cwd),
            };
        }
        let git_ref = self.git_ref.text().trim();
        PluginOrigin::Git {
            url: address.to_owned(),
            git_ref: (!git_ref.is_empty()).then(|| git_ref.to_owned()),
        }
    }
}

/// Whether `address` names a folder rather than a git repository.
pub(crate) fn is_folder(address: &str) -> bool {
    address.starts_with(['/', '~', '.'])
}

/// The steps of adding or updating a plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PluginDialog {
    /// Where to add one from, with why the last address was refused.
    Add {
        typed: Typed,
        refused: Option<String>,
    },
    /// Its preview on its way, asked as request `asked`: `what` is being
    /// fetched, from `typed` when adding (gone back to if refused).
    Fetching {
        asked: u64,
        what: String,
        typed: Option<Typed>,
    },
    /// Everything it would run, for the person to say yes or no; the
    /// first line shown.
    Preview {
        preview: Box<PluginPreview>,
        offset: usize,
    },
    /// Updating a plugin could not be, and why.
    Failed { title: String, message: String },
}

impl App {
    /// The Plugins section's rows: its heading, each plugin, and the row
    /// that adds one.
    pub(super) fn plugin_rows(&self) -> Vec<SettingRow> {
        let mut rows = vec![SettingRow::Plugins];
        rows.extend(
            self.model
                .plugins()
                .iter()
                .filter_map(|plugin| PluginKey::of(&plugin.id).map(SettingRow::Plugin)),
        );
        rows.push(SettingRow::AddPlugin);
        rows
    }

    /// The plugin `key` names, as the daemon last said.
    pub(crate) fn plugin(&self, key: PluginKey) -> Option<&PluginInfo> {
        self.model
            .plugins()
            .iter()
            .find(|plugin| plugin.id == key.id())
    }

    /// The plugin selected on the Settings screen.
    fn selected_plugin(&self) -> Option<&PluginInfo> {
        match self.settings.selected {
            Some(SettingRow::Plugin(key)) => self.plugin(key),
            _ => None,
        }
    }

    /// Asks the daemon to read its plugins again as the Settings screen is
    /// shown, so a manifest edited by hand is seen: what changed comes as a
    /// change to the state.
    pub(super) fn read_plugins(&mut self) -> Action {
        self.command(Command::Plugin(PluginCommand::List), Then::PluginsRead)
    }

    /// Space or ⏎ on a plugin: off if it runs, else on, refused with why
    /// where it cannot run.
    pub(super) fn toggle_plugin(&mut self, key: PluginKey) -> Vec<Action> {
        if self.settings.plugin_changing.is_some() {
            return Vec::new();
        }
        let Some(plugin) = self.plugin(key) else {
            return Vec::new();
        };
        let on = !plugin.runs();
        if let (true, Some(problem)) = (on, &plugin.problem) {
            let refused = format!(
                "{} cannot be turned on: {}.",
                one_line(&plugin.id),
                problem.to_string().trim_end_matches('.')
            );
            self.complain(refused);
            return Vec::new();
        }
        self.settings.plugin_changing = Some(key);
        let command = PluginCommand::SetEnabled {
            id: key.id().to_owned(),
            on,
            by: ChangedBy::Ui,
        };
        vec![self.command(Command::Plugin(command), Then::PluginToggled(key, on))]
    }

    /// The plugin was turned on or off, or could not be; the list as it
    /// is now comes as a change to the state.
    pub(super) fn plugin_toggled(
        &mut self,
        key: PluginKey,
        on: bool,
        outcome: Result<(), ErrorBody>,
    ) {
        self.settings.plugin_changing = None;
        match outcome {
            Ok(()) => self.say(format!(
                "Turned {} {}.",
                key.id(),
                if on { "on" } else { "off" }
            )),
            Err(error) => self.complain(error.message),
        }
    }

    /// `a`, or ⏎ on *add a plugin*: where from.
    pub(super) fn open_add_plugin(&mut self) -> Vec<Action> {
        self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Add {
            typed: Typed::default(),
            refused: None,
        }));
        Vec::new()
    }

    /// `u` on a plugin: the preview of what it is now, fetched again.
    pub(super) fn update_plugin(&mut self) -> Vec<Action> {
        let Some(plugin) = self.selected_plugin() else {
            self.complain("Select a plugin to update it.");
            return Vec::new();
        };
        let id = plugin.id.clone();
        if plugin.source == PluginSource::Builtin {
            self.complain(format!("{id} is Muster's own: it updates with Muster."));
            return Vec::new();
        }
        let asked = self.requests.next;
        let command = Command::Plugin(PluginCommand::Update { id: id.clone() });
        let action = self.command(command, Then::PluginPreview(asked));
        self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Fetching {
            asked,
            what: format!("{id} as it is now"),
            typed: None,
        }));
        vec![action]
    }

    /// `x` on a plugin: asks first. Muster's own are only turned off.
    pub(super) fn confirm_remove_plugin(&mut self) -> Vec<Action> {
        let Some(plugin) = self.selected_plugin() else {
            self.complain("Select a plugin to remove it.");
            return Vec::new();
        };
        let id = one_line(&plugin.id);
        let question = match &plugin.source {
            PluginSource::Builtin => {
                self.complain(format!(
                    "{id} is Muster's own: it cannot be removed, only turned off."
                ));
                return Vec::new();
            }
            PluginSource::Git { .. } => {
                format!("Remove {id}? Muster's copy of it and its entry in your settings go.")
            }
            PluginSource::Folder { path } => format!(
                "Remove {id}? Muster forgets it; its folder {} stays as it is.",
                one_line(&path.display().to_string())
            ),
        };
        let then = Confirmed::RemovePlugin(plugin.id.clone());
        self.mode = Mode::Dialog(Dialog::Confirm { question, then });
        Vec::new()
    }

    /// The person said yes to removing the plugin `id`.
    pub(super) fn remove_plugin(&mut self, id: String) -> Action {
        let command = Command::Plugin(PluginCommand::Remove { id: id.clone() });
        self.command(command, Then::PluginRemoved(id))
    }

    pub(super) fn plugin_removed(&mut self, id: &str, outcome: Result<(), ErrorBody>) {
        match outcome {
            Ok(()) => self.say(format!("Removed {id}.")),
            Err(error) => self.complain(error.message),
        }
    }

    /// A key in the dialog adding or updating a plugin.
    pub(super) fn plugin_dialog_key(
        &mut self,
        dialog: PluginDialog,
        key: &KeyEvent,
    ) -> Vec<Action> {
        match dialog {
            PluginDialog::Add { typed, refused } => self.add_plugin_key(typed, refused, key),
            PluginDialog::Fetching { what, .. } => {
                if key.code == KeyCode::Esc {
                    // The daemon lets the preview go in ten minutes.
                    self.mode = Mode::Browse;
                    self.say(format!("Stopped waiting for {}.", one_line(&what)));
                }
                Vec::new()
            }
            PluginDialog::Preview { preview, offset } => self.preview_key(preview, offset, key),
            PluginDialog::Failed { .. } => {
                if matches!(key.code, KeyCode::Esc | KeyCode::Enter) {
                    self.mode = Mode::Browse;
                }
                Vec::new()
            }
        }
    }

    fn add_plugin_key(
        &mut self,
        mut typed: Typed,
        refused: Option<String>,
        key: &KeyEvent,
    ) -> Vec<Action> {
        if matches!(
            key.code,
            KeyCode::Tab | KeyCode::BackTab | KeyCode::Up | KeyCode::Down
        ) {
            typed.on_ref = !typed.on_ref;
            self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Add { typed, refused }));
            return Vec::new();
        }
        let field = if typed.on_ref {
            &mut typed.git_ref
        } else {
            &mut typed.address
        };
        match field.handle(key) {
            Edit::Cancel => {
                self.mode = Mode::Browse;
                Vec::new()
            }
            Edit::Submit if typed.address.text().trim().is_empty() => {
                let refused =
                    Some("Type a git repository's address, or a folder's path.".to_owned());
                self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Add { typed, refused }));
                Vec::new()
            }
            Edit::Submit => {
                let origin = typed.origin(&self.cwd);
                let what = match &origin {
                    PluginOrigin::Git { url, .. } => one_line(url),
                    PluginOrigin::Folder { path } => one_line(&path.display().to_string()),
                };
                let asked = self.requests.next;
                let command = Command::Plugin(PluginCommand::Preview { origin });
                let action = self.command(command, Then::PluginPreview(asked));
                self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Fetching {
                    asked,
                    what,
                    typed: Some(typed),
                }));
                vec![action]
            }
            Edit::Changed | Edit::Ignored => {
                self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Add { typed, refused }));
                Vec::new()
            }
        }
    }

    /// A key on the preview: y or ⏎ installs, links or updates, n or Esc
    /// does not; the arrows and page keys scroll it.
    fn preview_key(
        &mut self,
        preview: Box<PluginPreview>,
        offset: usize,
        key: &KeyEvent,
    ) -> Vec<Action> {
        let scroll = match key.code {
            KeyCode::Up => Some(-1),
            KeyCode::Down => Some(1),
            KeyCode::PageUp => Some(-10),
            KeyCode::PageDown => Some(10),
            KeyCode::Home => Some(isize::MIN / 2),
            KeyCode::End => Some(isize::MAX / 2),
            _ => None,
        };
        if let Some(by) = scroll {
            let last = crate::view::preview_lines(&preview, crate::view::theme::of(self))
                .len()
                .saturating_sub(1);
            let offset = offset.saturating_add_signed(by).min(last);
            self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Preview { preview, offset }));
            return Vec::new();
        }
        match key.code {
            KeyCode::Char('y') | KeyCode::Enter => {
                self.mode = Mode::Browse;
                self.say(format!(
                    "{} {}…",
                    doing(preview.kind),
                    one_line(&preview.id)
                ));
                let command = PluginCommand::Install {
                    preview: preview.preview.clone(),
                    by: ChangedBy::Ui,
                };
                vec![self.command(Command::Plugin(command), Then::PluginInstalled(preview))]
            }
            KeyCode::Char('n') | KeyCode::Esc => {
                self.mode = Mode::Browse;
                self.say(match preview.kind {
                    PreviewKind::Install | PreviewKind::Link if preview.current.is_none() => {
                        "Nothing was added.".to_owned()
                    }
                    _ => format!("{} stays as it was.", one_line(&preview.id)),
                });
                Vec::new()
            }
            _ => {
                self.mode = Mode::Dialog(Dialog::Plugin(PluginDialog::Preview { preview, offset }));
                Vec::new()
            }
        }
    }

    /// Text pasted into the dialog adding a plugin: into the field with
    /// the cursor.
    pub(super) fn plugin_paste(&mut self, text: &str) {
        if let Mode::Dialog(Dialog::Plugin(PluginDialog::Add { typed, .. })) = &mut self.mode {
            if typed.on_ref {
                typed.git_ref.paste(text);
            } else {
                typed.address.paste(text);
            }
        }
    }

    /// A preview came, or why there is none: shown if the person still
    /// waits for it, in the dialog, which goes back to the address typed
    /// where one was refused.
    pub(super) fn on_plugin_preview(&mut self, asked: u64, outcome: Result<Reply, ErrorBody>) {
        let Mode::Dialog(Dialog::Plugin(PluginDialog::Fetching {
            asked: waiting,
            what,
            typed,
        })) = &self.mode
        else {
            return;
        };
        if *waiting != asked {
            return;
        }
        self.mode = Mode::Dialog(Dialog::Plugin(match (outcome, typed.clone()) {
            (Ok(Reply::PluginPreview(preview)), _) => PluginDialog::Preview { preview, offset: 0 },
            (Ok(_), _) => return,
            (Err(error), Some(typed)) => PluginDialog::Add {
                typed,
                refused: Some(error.message),
            },
            (Err(error), None) => PluginDialog::Failed {
                title: format!("Update {}", one_line(what)),
                message: error.message,
            },
        }));
    }

    /// The link to the daemon went while a preview was on its way: it
    /// never comes, so the dialog says so.
    pub(super) fn forget_plugin_fetch(&mut self) {
        if let Mode::Dialog(Dialog::Plugin(PluginDialog::Fetching { typed, what, .. })) = &self.mode
        {
            let message = "The daemon went away before it answered: try again.".to_owned();
            self.mode = Mode::Dialog(Dialog::Plugin(match typed.clone() {
                Some(typed) => PluginDialog::Add {
                    typed,
                    refused: Some(message),
                },
                None => PluginDialog::Failed {
                    title: format!("Update {}", one_line(what)),
                    message,
                },
            }));
        }
    }

    /// What a preview showed was installed, linked or updated, or could
    /// not be; the plugin is selected once the list has it.
    pub(super) fn plugin_installed(
        &mut self,
        preview: &PluginPreview,
        outcome: Result<(), ErrorBody>,
    ) {
        if let Err(error) = outcome {
            return self.complain(error.message);
        }
        let id = one_line(&preview.id);
        self.say(match preview.kind {
            PreviewKind::Install => format!("Installed {id} and turned it on."),
            PreviewKind::Link if preview.current.is_some() => {
                format!("Approved {id} as its folder holds it now.")
            }
            PreviewKind::Link => format!("Linked {id} and turned it on."),
            PreviewKind::Update => {
                format!("Updated {id} to {}.", one_line(&preview.details.version))
            }
        });
        self.settings.plugin_wanted = PluginKey::of(&preview.id);
    }
}

/// What approving a preview of `kind` is doing, for the note meanwhile.
fn doing(kind: PreviewKind) -> &'static str {
    match kind {
        PreviewKind::Install => "Installing",
        PreviewKind::Link => "Linking",
        PreviewKind::Update => "Updating",
    }
}

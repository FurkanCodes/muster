//! Picking the folder to add as a project. The dialog's path field is also
//! a folder browser: below it are the folders in the folder typed so far,
//! narrowed by what follows its last `/`, with git repositories marked.
//!
//! ↑↓ choose one, ⇥ opens it, ⇧⇥ goes up a folder, and ⏎ adds the chosen
//! folder, or the typed path when none is chosen. Typing a name chooses the
//! first folder it matches. Folders are listed by the runtime, on a blocking
//! thread (`Action::ListFolders`), and come back through
//! [`App::on_folders`].

use std::path::{Component, Path, PathBuf};

use crossterm::event::{KeyCode, KeyEvent};
use muster_core::protocol::Command;

use super::{Action, App, Dialog, Mode, Then};
use crate::folders::Folder;
use crate::input::{Edit, LineInput};

/// A page up or down the picker.
const PICKER_PAGE: isize = 8;
/// How many folders the dialog shows at once: a page.
pub(crate) const PICKER_ROWS: usize = PICKER_PAGE.unsigned_abs();

/// The folder browser under the path field.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct FolderPicker {
    /// The folder listed, and what is in it.
    pub(crate) listing: Option<(PathBuf, Listing)>,
    /// The chosen folder, among those shown; none is the typed path itself.
    pub(crate) selected: Option<usize>,
    /// The first folder shown: the list moves only when the choice would
    /// leave it.
    pub(crate) offset: usize,
}

/// A folder's folders, as listed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Listing {
    Loading,
    Loaded(Vec<Folder>),
    Failed(String),
}

/// What the browser shows for a typed path.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Shown<'a> {
    Loading,
    Failed(&'a str),
    Folders(Vec<&'a Folder>),
}

impl FolderPicker {
    /// The folders shown for `typed`: those in its folder whose names hold
    /// what follows its last `/`, ignoring case, those starting with it
    /// first. Hidden folders only once a `.` is typed.
    pub(crate) fn shown(&self, typed: &str) -> Shown<'_> {
        let filter = split(typed).1.to_lowercase();
        match self.listing.as_ref().map(|(_, listing)| listing) {
            None | Some(Listing::Loading) => Shown::Loading,
            Some(Listing::Failed(reason)) => Shown::Failed(reason),
            Some(Listing::Loaded(folders)) => {
                let mut matching: Vec<&Folder> = folders
                    .iter()
                    .filter(|folder| filter.starts_with('.') || !folder.name.starts_with('.'))
                    .filter(|folder| folder.name.to_lowercase().contains(&filter))
                    .collect();
                // Stable, so each group keeps its order by name.
                matching.sort_by_key(|folder| !folder.name.to_lowercase().starts_with(&filter));
                Shown::Folders(matching)
            }
        }
    }

    /// The chosen folder, if the choice is among those shown for `typed`.
    fn chosen<'a>(&'a self, typed: &str) -> Option<&'a Folder> {
        match self.shown(typed) {
            Shown::Folders(folders) => self.selected.and_then(|at| folders.get(at).copied()),
            _ => None,
        }
    }

    /// Moves the choice by `by` among `count` folders; up from the first
    /// is back to the typed path.
    fn step(&mut self, by: isize, count: usize) {
        self.selected = match (self.selected, by) {
            (_, _) if count == 0 => None,
            (None, by) if by > 0 => Some(0),
            (None, _) => None,
            (Some(0), by) if by < 0 => None,
            (Some(at), by) => Some(at.saturating_add_signed(by).min(count - 1)),
        };
        if let Some(at) = self.selected {
            if at < self.offset {
                self.offset = at;
            } else if at >= self.offset + PICKER_ROWS {
                self.offset = at + 1 - PICKER_ROWS;
            }
        }
    }

    /// After the typed path changed: a typed name chooses its first match.
    fn typed(&mut self, typed: &str) {
        self.selected = (!split(typed).1.is_empty()).then_some(0);
        self.offset = 0;
    }
}

impl App {
    /// `a`: the dialog, on the folder `muster` was started in, listed.
    pub(super) fn open_add_project(&mut self) -> Vec<Action> {
        let path = LineInput::with(&as_folder(&self.cwd));
        self.picker_shows(path, FolderPicker::default())
    }

    /// A key in the dialog.
    pub(super) fn picker_key(
        &mut self,
        mut path: LineInput,
        mut picker: FolderPicker,
        key: &KeyEvent,
    ) -> Vec<Action> {
        let count = match picker.shown(path.text()) {
            Shown::Folders(folders) => folders.len(),
            _ => 0,
        };
        match key.code {
            KeyCode::Down => picker.step(1, count),
            KeyCode::Up => picker.step(-1, count),
            KeyCode::PageDown => picker.step(PICKER_PAGE, count),
            KeyCode::PageUp => picker.step(-PICKER_PAGE, count),
            // Into the chosen folder, or the first shown.
            KeyCode::Tab => {
                let into = picker.chosen(path.text()).map(|folder| folder.name.clone());
                let first = match picker.shown(path.text()) {
                    Shown::Folders(folders) => folders.first().map(|folder| folder.name.clone()),
                    _ => None,
                };
                if let Some(name) = into.or(first) {
                    path = LineInput::with(&format!("{}{name}/", split(path.text()).0));
                    picker.typed(path.text());
                }
            }
            KeyCode::BackTab => {
                let folder = normalize(&resolve_path(split(path.text()).0, &self.cwd));
                let up = folder.parent().unwrap_or(&folder).to_owned();
                path = LineInput::with(&as_folder(&up));
                picker.typed(path.text());
            }
            _ => match path.handle(key) {
                Edit::Submit => {
                    let typed = path.text();
                    let root = match picker.chosen(typed) {
                        Some(folder) => resolve_path(split(typed).0, &self.cwd).join(&folder.name),
                        None => resolve_path(typed, &self.cwd),
                    };
                    self.mode = Mode::Browse;
                    let command = Command::AddProject { root, name: None };
                    return vec![self.command(command, Then::SelectProject)];
                }
                Edit::Cancel => {
                    self.mode = Mode::Browse;
                    return Vec::new();
                }
                Edit::Changed => picker.typed(path.text()),
                Edit::Ignored => {}
            },
        }
        self.picker_shows(path, picker)
    }

    /// Pasted text goes into the path, as typing does.
    pub(super) fn picker_paste(
        &mut self,
        mut path: LineInput,
        mut picker: FolderPicker,
        text: &str,
    ) -> Vec<Action> {
        path.paste(text);
        picker.typed(path.text());
        self.picker_shows(path, picker)
    }

    /// The folders listed in `dir`, for the dialog if it still shows them.
    pub(crate) fn on_folders(
        &mut self,
        dir: &Path,
        folders: Result<Vec<Folder>, String>,
    ) -> Vec<Action> {
        if let Mode::Dialog(Dialog::AddProject { picker, .. }) = &mut self.mode
            && let Some((listed, listing)) = &mut picker.listing
            && listed.as_path() == dir
        {
            *listing = match folders {
                Ok(folders) => Listing::Loaded(folders),
                Err(reason) => Listing::Failed(reason),
            };
        }
        Vec::new()
    }

    /// Shows the dialog, asking for the typed folder's folders if they are
    /// not the ones listed.
    fn picker_shows(&mut self, path: LineInput, mut picker: FolderPicker) -> Vec<Action> {
        let dir = normalize(&resolve_path(split(path.text()).0, &self.cwd));
        let mut actions = Vec::new();
        if picker
            .listing
            .as_ref()
            .is_none_or(|(listed, _)| *listed != dir)
        {
            picker.listing = Some((dir.clone(), Listing::Loading));
            actions.push(Action::ListFolders(dir));
        }
        self.mode = Mode::Dialog(Dialog::AddProject { path, picker });
        actions
    }
}

/// What narrows the folders shown for `typed`: the name after its last `/`.
pub(crate) fn picker_filter(typed: &str) -> &str {
    split(typed).1
}

/// A typed path's folder, up to its last `/`, and the name after it.
fn split(typed: &str) -> (&str, &str) {
    match typed.rfind('/') {
        Some(at) => typed.split_at(at + 1),
        None => ("", typed),
    }
}

/// `folder` as typed into the field, ending in `/` so its folders show.
fn as_folder(folder: &Path) -> String {
    let text = folder.display().to_string();
    if text.ends_with('/') {
        text
    } else {
        format!("{text}/")
    }
}

/// `path` without `.` and with `..` taken back, as the person means it.
fn normalize(path: &Path) -> PathBuf {
    let mut normal = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                normal.pop();
            }
            other => normal.push(other),
        }
    }
    normal
}

/// A typed path: `~` is the home directory, and a relative path is taken
/// from where `muster` was started.
pub(super) fn resolve_path(typed: &str, cwd: &Path) -> PathBuf {
    let typed = typed.trim();
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let path = match (typed.strip_prefix("~"), home) {
        (Some(rest), Some(home)) if rest.is_empty() || rest.starts_with('/') => {
            home.join(rest.trim_start_matches('/'))
        }
        _ => PathBuf::from(typed),
    };
    if path.is_absolute() {
        path
    } else {
        cwd.join(path)
    }
}

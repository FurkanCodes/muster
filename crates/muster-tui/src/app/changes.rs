//! A workspace's changes beside the list (`d`): what is not committed yet,
//! the branch's commits and what they changed, and one file's diff (⏎).
//! The pane has the keys until `d`, Shift-Q or Esc gives them back. It is
//! asked of the daemon (`WorkspaceChanges`, `FileDiff`), asked again on `r`
//! and whenever the workspace's summary in the state changes, as when an
//! agent's turn ends there.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::WorkspaceId;
use muster_core::changes::{
    CHANGES_PROTOCOL, ChangePart, ChangeSummary, Changes, Commit, FileChange, FileDiff,
};
use muster_core::protocol::Command;
use ratatui::layout::{Margin, Position, Rect};

use super::events::{leaves_terminal, screen_key};
use super::{Action, App, Mode, Then, View};

/// The pane's state while it is open.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ChangesPane {
    pub(crate) workspace: WorkspaceId,
    /// The last answer; `None` until the first comes.
    pub(crate) listing: Option<Result<Changes, String>>,
    /// An answer is on its way.
    asking: bool,
    /// The workspace's summary as the state last said: when it says
    /// another than the listing's, the listing is asked for again.
    seen: Option<ChangeSummary>,
    /// The selected file, by part and path, so it holds across answers.
    pub(crate) selected: Option<(ChangePart, String)>,
    /// The first line of the listing shown.
    pub(crate) offset: usize,
    /// The file whose diff is shown instead of the listing.
    pub(crate) diff: Option<DiffPane>,
}

/// One file's diff, shown in the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffPane {
    pub(crate) part: ChangePart,
    pub(crate) path: String,
    /// `None` until it comes.
    pub(crate) text: Option<Result<FileDiff, String>>,
    /// The first line shown.
    pub(crate) scroll: usize,
}

/// A line of the listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChangeRow<'a> {
    Heading(String),
    File(ChangePart, &'a FileChange),
    Commit(&'a Commit),
    Note(String),
}

/// The listing's lines: what is not committed, then the branch's commits
/// and the files they changed.
pub(crate) fn rows(changes: &Changes) -> Vec<ChangeRow<'_>> {
    let mut rows = Vec::new();
    let base = changes.base.as_deref().unwrap_or("its base");
    if !changes.uncommitted.is_empty() {
        rows.push(ChangeRow::Heading(format!(
            "Not committed · {}",
            files(changes.uncommitted.len())
        )));
        rows.extend(
            changes
                .uncommitted
                .iter()
                .map(|file| ChangeRow::File(ChangePart::Uncommitted, file)),
        );
    }
    if changes.base.is_none() {
        rows.push(ChangeRow::Note(
            "No branch to measure its commits against: set base_branch in muster.toml, or fetch \
             the remote."
                .to_owned(),
        ));
    } else if !changes.commits.is_empty() {
        let total = u64::try_from(changes.commits.len()).unwrap_or(u64::MAX) + changes.more_commits;
        rows.push(ChangeRow::Heading(format!(
            "Commits since {base} · {total}"
        )));
        rows.extend(changes.commits.iter().map(ChangeRow::Commit));
        if changes.more_commits > 0 {
            rows.push(ChangeRow::Note(format!(
                "and {} older",
                changes.more_commits
            )));
        }
        rows.push(ChangeRow::Heading(format!(
            "Changed on the branch · {}",
            files(changes.branch.len())
        )));
        rows.extend(
            changes
                .branch
                .iter()
                .map(|file| ChangeRow::File(ChangePart::Branch, file)),
        );
    }
    if rows.is_empty() {
        rows.push(ChangeRow::Note(format!(
            "Nothing has changed here since {base}."
        )));
    }
    rows
}

fn files(count: usize) -> String {
    if count == 1 {
        "1 file".to_owned()
    } else {
        format!("{count} files")
    }
}

impl ChangesPane {
    /// The listing's rows that are files, in order.
    fn files(&self) -> Vec<(ChangePart, String)> {
        match &self.listing {
            Some(Ok(changes)) => rows(changes)
                .into_iter()
                .filter_map(|row| match row {
                    ChangeRow::File(part, file) => Some((part, file.path.clone())),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Where the selected file is among the listing's rows.
    pub(crate) fn selected_row(&self) -> Option<usize> {
        let Some(Ok(changes)) = &self.listing else {
            return None;
        };
        let (part, path) = self.selected.as_ref()?;
        rows(changes).iter().position(
            |row| matches!(row, ChangeRow::File(at, file) if at == part && file.path == *path),
        )
    }
}

impl App {
    /// `d`: the changes of the selected worktree, or of the workspace the
    /// selected session runs in, beside the list, with the keys.
    pub(super) fn open_changes(&mut self) -> Vec<Action> {
        let workspace = match self.view {
            View::Sessions => self.selected.and_then(|row| self.model.worktree_of(row)),
            View::NeedsYou => self
                .needs_selected
                .and_then(|session| self.model.session(session))
                .map(|session| session.workspace),
            View::Tickets | View::Settings => None,
        };
        let Some(found) = workspace.and_then(|id| self.model.workspace(id)) else {
            self.complain("Select a worktree, or a session in one, to see what it changed.");
            return Vec::new();
        };
        if !found.kind.in_git() {
            self.complain("It is not a git repository, so git cannot tell what changed there.");
            return Vec::new();
        }
        if found.kind.run().is_some() {
            self.complain("A run's container keeps its own clone; its changes are the run's.");
            return Vec::new();
        }
        if self.daemon_protocol < CHANGES_PROTOCOL {
            self.complain(
                "The daemon runs an older muster that cannot show changes: `muster reload` moves \
                 it onto this one, every session running on.",
            );
            return Vec::new();
        }
        let workspace = found.id;
        self.changes = Some(ChangesPane {
            workspace,
            listing: None,
            asking: false,
            seen: found.changes,
            selected: None,
            offset: 0,
            diff: None,
        });
        self.mode = Mode::Changes;
        self.note = None;
        self.ask_changes().into_iter().collect()
    }

    /// Asks for the open pane's listing, unless an answer is on its way.
    fn ask_changes(&mut self) -> Option<Action> {
        let pane = self.changes.as_mut()?;
        if pane.asking {
            return None;
        }
        pane.asking = true;
        let workspace = pane.workspace;
        Some(self.command(
            Command::WorkspaceChanges { workspace },
            Then::Changes(workspace),
        ))
    }

    /// Asks for the diff of the selected file, shown in the pane.
    fn ask_diff(&mut self) -> Option<Action> {
        let pane = self.changes.as_mut()?;
        let (part, path) = pane.selected.clone()?;
        pane.diff = Some(DiffPane {
            part,
            path: path.clone(),
            text: None,
            scroll: 0,
        });
        let workspace = pane.workspace;
        Some(self.command(
            Command::FileDiff {
                workspace,
                part,
                path: path.clone(),
            },
            Then::Diff(workspace, part, path),
        ))
    }

    /// Closes the pane and gives the list the keys back.
    pub(super) fn close_changes(&mut self) {
        self.changes = None;
        if self.mode == Mode::Changes {
            self.mode = Mode::Browse;
        }
    }

    pub(super) fn changes_key(&mut self, key: &KeyEvent) -> Vec<Action> {
        if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
            return vec![Action::Quit];
        }
        if let Some(view) = screen_key(key) {
            self.close_changes();
            return self.show(view);
        }
        if leaves_terminal(key) {
            self.close_changes();
            return Vec::new();
        }
        let showing_diff = self
            .changes
            .as_ref()
            .is_some_and(|pane| pane.diff.is_some());
        let page = isize::try_from(self.pane_rows().saturating_sub(1))
            .unwrap_or(1)
            .max(1);
        match key.code {
            KeyCode::Tab | KeyCode::BackTab => {
                self.close_changes();
                return self.show(self.view.next(key.code == KeyCode::BackTab));
            }
            KeyCode::Char('d' | 'q') => self.close_changes(),
            KeyCode::Esc | KeyCode::Backspace | KeyCode::Left | KeyCode::Char('h')
                if showing_diff =>
            {
                if let Some(pane) = self.changes.as_mut() {
                    pane.diff = None;
                }
            }
            KeyCode::Esc | KeyCode::Backspace => self.close_changes(),
            KeyCode::Char('r') => {
                let mut actions: Vec<Action> = self.ask_changes().into_iter().collect();
                if showing_diff {
                    actions.extend(self.ask_diff());
                }
                return actions;
            }
            KeyCode::Up | KeyCode::Char('k') if showing_diff => self.scroll_diff(-1),
            KeyCode::Down | KeyCode::Char('j') if showing_diff => self.scroll_diff(1),
            KeyCode::PageUp if showing_diff => self.scroll_diff(-page),
            KeyCode::PageDown | KeyCode::Char(' ') if showing_diff => self.scroll_diff(page),
            KeyCode::Home | KeyCode::Char('g') if showing_diff => self.scroll_diff(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') if showing_diff => self.scroll_diff(isize::MAX / 2),
            KeyCode::Up | KeyCode::Char('k') => self.move_file(-1),
            KeyCode::Down | KeyCode::Char('j') => self.move_file(1),
            KeyCode::PageUp => self.move_file(-page),
            KeyCode::PageDown => self.move_file(page),
            KeyCode::Enter | KeyCode::Right | KeyCode::Char('l') if !showing_diff => {
                return self.ask_diff().into_iter().collect();
            }
            _ => {}
        }
        Vec::new()
    }

    /// The mouse while the pane has the keys: the wheel scrolls it
    /// wherever the pointer is, a click on a file shows its diff, and a
    /// click on the list gives the list the keys back.
    pub(super) fn changes_mouse(&mut self, event: MouseEvent) -> Vec<Action> {
        let window = Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let areas = crate::view::browse_areas(self, window);
        let showing_diff = self
            .changes
            .as_ref()
            .is_some_and(|pane| pane.diff.is_some());
        match event.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let by = if event.kind == MouseEventKind::ScrollUp {
                    -1
                } else {
                    1
                };
                if showing_diff {
                    self.scroll_diff(by * 3);
                } else {
                    self.move_file(by);
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let at = Position::new(event.column, event.row);
                if areas.list.contains(at) {
                    self.close_changes();
                    self.select_at(areas.list, event.row);
                    return Vec::new();
                }
                let inner = areas.preview.inner(Margin::new(1, 1));
                if showing_diff || !inner.contains(at) {
                    return Vec::new();
                }
                let Some(pane) = self.changes.as_mut() else {
                    return Vec::new();
                };
                let Some(Ok(changes)) = &pane.listing else {
                    return Vec::new();
                };
                let index = pane.offset + usize::from(event.row - inner.y);
                if let Some(ChangeRow::File(part, file)) = rows(changes).get(index) {
                    pane.selected = Some((*part, file.path.clone()));
                    return self.ask_diff().into_iter().collect();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Moves the selection `by` files.
    pub(super) fn move_file(&mut self, by: isize) {
        let height = self.pane_rows();
        let Some(pane) = self.changes.as_mut() else {
            return;
        };
        let files = pane.files();
        if files.is_empty() {
            return;
        }
        let at = pane
            .selected
            .as_ref()
            .and_then(|selected| files.iter().position(|file| file == selected));
        let next = match at {
            Some(at) => at.saturating_add_signed(by).min(files.len() - 1),
            None => 0,
        };
        pane.selected = Some(files[next].clone());
        keep_selected_in_view(pane, height);
    }

    /// Scrolls the diff `by` lines, keeping its last line at the bottom.
    pub(super) fn scroll_diff(&mut self, by: isize) {
        let height = self.pane_rows();
        let Some(diff) = self.changes.as_mut().and_then(|pane| pane.diff.as_mut()) else {
            return;
        };
        let lines = match &diff.text {
            Some(Ok(found)) => found.text.lines().count() + usize::from(found.cut),
            _ => 0,
        };
        let last = lines.saturating_sub(height);
        diff.scroll = diff.scroll.saturating_add_signed(by).min(last);
    }

    /// How many lines the pane shows inside its border.
    fn pane_rows(&self) -> usize {
        usize::from(self.session_size().rows).max(1)
    }

    /// The daemon's answer with the pane's listing.
    pub(super) fn on_changes(&mut self, workspace: WorkspaceId, outcome: Result<Changes, String>) {
        let height = self.pane_rows();
        let Some(pane) = self
            .changes
            .as_mut()
            .filter(|pane| pane.workspace == workspace)
        else {
            return;
        };
        pane.asking = false;
        pane.listing = Some(outcome);
        let files = pane.files();
        if !pane
            .selected
            .as_ref()
            .is_some_and(|selected| files.contains(selected))
        {
            pane.selected = files.first().cloned();
        }
        keep_selected_in_view(pane, height);
    }

    /// The daemon's answer with a file's diff.
    pub(super) fn on_diff(
        &mut self,
        workspace: WorkspaceId,
        part: ChangePart,
        path: &str,
        outcome: Result<FileDiff, String>,
    ) {
        let shown = self
            .changes
            .as_mut()
            .filter(|pane| pane.workspace == workspace)
            .and_then(|pane| pane.diff.as_mut())
            .filter(|diff| diff.part == part && diff.path == path);
        if let Some(diff) = shown {
            diff.text = Some(outcome);
        }
        // Asked again: the scroll stays where it was, if it still can.
        self.scroll_diff(0);
    }

    /// Keeps the pane in step with the state: closed once its workspace
    /// is gone, asked again once its summary says something changed.
    pub(super) fn settle_changes(&mut self) -> Option<Action> {
        let workspace = self.changes.as_ref()?.workspace;
        let Some(now) = self.model.workspace(workspace).map(|found| found.changes) else {
            self.close_changes();
            return None;
        };
        let pane = self.changes.as_mut()?;
        if now == pane.seen {
            return None;
        }
        pane.seen = now;
        // The listing's own summary is told to the state as it is
        // answered: only another says something changed since.
        let stale = match &pane.listing {
            Some(Ok(changes)) => changes.summary() != now,
            Some(Err(_)) => true,
            None => false,
        };
        if stale { self.ask_changes() } else { None }
    }
}

/// Moves the listing so the selected file is in view among `height` lines.
fn keep_selected_in_view(pane: &mut ChangesPane, height: usize) {
    let Some(row) = pane.selected_row() else {
        return;
    };
    // The heading above the first file shows with it.
    let top = if row == 1 { 0 } else { row };
    if top < pane.offset {
        pane.offset = top;
    } else if row >= pane.offset + height {
        pane.offset = row + 1 - height;
    }
}

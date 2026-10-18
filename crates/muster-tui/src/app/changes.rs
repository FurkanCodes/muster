//! A workspace's changes beside the list (`d`): the project's other work
//! on the same files, what is not committed yet, the branch's commits and
//! what they changed, the workspace's timeline of turns (`app::timeline`),
//! and one file's or one turn's diff (⏎). The
//! pane has the keys until `d`, Shift-Q or Esc gives them back. It is
//! asked of the daemon (`WorkspaceChanges`, `FileDiff`), asked again on `r`
//! and whenever the workspace's summary in the state changes, as when an
//! agent's turn ends there.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::WorkspaceId;
use muster_core::changes::{
    CHANGES_PROTOCOL, ChangePart, ChangeSummary, Changes, Commit, FileChange, FileDiff,
};
use muster_core::clashes::Clash;
use muster_core::protocol::Command;
use muster_core::timeline::{Entry, TIMELINE_PROTOCOL, Timeline};
use ratatui::layout::{Margin, Position, Rect};

use super::events::{leaves_terminal, screen_key};
use super::{Action, App, Mode, Then, View};

/// What the pane's selection is on: a file of the change, or an entry of
/// the workspace's timeline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pick {
    File(ChangePart, String),
    Entry(u64),
}

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
    /// The workspace's timeline, newest first; `None` until it comes, and
    /// from a daemon that keeps none.
    pub(crate) timeline: Option<Result<Timeline, String>>,
    pub(super) asking_timeline: bool,
    /// The newest entry as the state last said, to ask again when it
    /// says another.
    pub(super) timeline_seen: Option<u64>,
    /// The project's other work on the same files, as the state says.
    pub(crate) clashes: Vec<Clash>,
    /// The selected file or entry, so it holds across answers.
    pub(crate) selected: Option<Pick>,
    /// The first line of the listing shown.
    pub(crate) offset: usize,
    /// The file or entry whose diff is shown instead of the listing.
    pub(crate) diff: Option<DiffPane>,
}

/// One file's or one entry's diff, shown in the pane.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DiffPane {
    pub(crate) of: Pick,
    /// `None` until it comes.
    pub(crate) text: Option<Result<FileDiff, String>>,
    /// The first line shown.
    pub(crate) scroll: usize,
}

/// A line of the listing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ChangeRow<'a> {
    Heading(String),
    /// Another workspace's work on the same files.
    Clash(&'a Clash),
    File(ChangePart, &'a FileChange),
    Commit(&'a Commit),
    Entry(&'a Entry),
    Note(String),
    Error(String),
}

impl ChangeRow<'_> {
    /// What selecting this line picks, if it can be selected.
    pub(crate) fn pick(&self) -> Option<Pick> {
        match self {
            Self::File(part, file) => Some(Pick::File(*part, file.path.clone())),
            Self::Entry(entry) => Some(Pick::Entry(entry.number)),
            _ => None,
        }
    }
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
    /// The pane's lines: other work on the same files, the change as git
    /// last said, then the timeline.
    pub(crate) fn rows(&self) -> Vec<ChangeRow<'_>> {
        let mut lines = Vec::new();
        if !self.clashes.is_empty() {
            lines.push(ChangeRow::Heading(format!(
                "Other work on these files · {}",
                self.clashes.len()
            )));
            lines.extend(self.clashes.iter().map(ChangeRow::Clash));
        }
        lines.extend(match &self.listing {
            None => vec![ChangeRow::Note("Asking git…".to_owned())],
            Some(Err(error)) => vec![
                ChangeRow::Error(error.clone()),
                ChangeRow::Note("r asks again.".to_owned()),
            ],
            Some(Ok(changes)) => rows(changes),
        });
        lines.extend(super::timeline::rows(self.timeline.as_ref()));
        lines
    }

    /// What can be selected, in order.
    fn picks(&self) -> Vec<Pick> {
        self.rows().iter().filter_map(ChangeRow::pick).collect()
    }

    /// Where the selection is among the pane's rows.
    pub(crate) fn selected_row(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.rows()
            .iter()
            .position(|row| row.pick().as_ref() == Some(selected))
    }

    /// The entry the selection is on, in the listing or in its diff.
    pub(crate) fn selected_entry(&self) -> Option<u64> {
        let pick = match &self.diff {
            Some(diff) => &diff.of,
            None => self.selected.as_ref()?,
        };
        match pick {
            Pick::Entry(number) => Some(*number),
            Pick::File(..) => None,
        }
    }

    /// Keeps the selection on something still listed, else the first.
    fn keep_selection(&mut self) {
        let picks = self.picks();
        if !self
            .selected
            .as_ref()
            .is_some_and(|selected| picks.contains(selected))
        {
            self.selected = picks.into_iter().next();
        }
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
            timeline: None,
            asking_timeline: false,
            timeline_seen: found.timeline,
            clashes: found.clashes.clone(),
            selected: None,
            offset: 0,
            diff: None,
        });
        self.mode = Mode::Changes;
        self.note = None;
        let mut actions: Vec<Action> = self.ask_changes().into_iter().collect();
        actions.extend(self.ask_timeline());
        actions
    }

    /// Whether the daemon keeps workspaces' timelines.
    pub(super) fn knows_timelines(&self) -> bool {
        self.daemon_protocol >= TIMELINE_PROTOCOL
    }

    /// Asks for the open pane's listing, unless an answer is on its way.
    pub(super) fn ask_changes(&mut self) -> Option<Action> {
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

    /// Asks for the diff of the selected file or entry, shown in the pane.
    fn ask_diff(&mut self) -> Option<Action> {
        let pane = self.changes.as_mut()?;
        let pick = pane.selected.clone()?;
        pane.diff = Some(DiffPane {
            of: pick.clone(),
            text: None,
            scroll: 0,
        });
        let workspace = pane.workspace;
        let command = match &pick {
            Pick::File(part, path) => Command::FileDiff {
                workspace,
                part: *part,
                path: path.clone(),
            },
            Pick::Entry(number) => Command::EntryDiff {
                workspace,
                number: *number,
            },
        };
        Some(self.command(command, Then::Diff(workspace, pick)))
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
                actions.extend(self.ask_timeline());
                if showing_diff {
                    actions.extend(self.ask_diff());
                }
                return actions;
            }
            KeyCode::Char('z') => return self.ask_undo_plan().into_iter().collect(),
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
                let index = pane.offset + usize::from(event.row - inner.y);
                if let Some(pick) = pane.rows().get(index).and_then(ChangeRow::pick) {
                    pane.selected = Some(pick);
                    return self.ask_diff().into_iter().collect();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Moves the selection `by` files or entries.
    pub(super) fn move_file(&mut self, by: isize) {
        let height = self.pane_rows();
        let Some(pane) = self.changes.as_mut() else {
            return;
        };
        let picks = pane.picks();
        if picks.is_empty() {
            return;
        }
        let at = pane
            .selected
            .as_ref()
            .and_then(|selected| picks.iter().position(|pick| pick == selected));
        let next = match at {
            Some(at) => at.saturating_add_signed(by).min(picks.len() - 1),
            None => 0,
        };
        pane.selected = Some(picks[next].clone());
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
    pub(super) fn pane_rows(&self) -> usize {
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
        pane.keep_selection();
        keep_selected_in_view(pane, height);
    }

    /// The daemon's answer with the timeline.
    pub(super) fn on_timeline(
        &mut self,
        workspace: WorkspaceId,
        outcome: Result<Timeline, String>,
    ) {
        let height = self.pane_rows();
        let Some(pane) = self
            .changes
            .as_mut()
            .filter(|pane| pane.workspace == workspace)
        else {
            return;
        };
        pane.asking_timeline = false;
        pane.timeline = Some(outcome);
        pane.keep_selection();
        keep_selected_in_view(pane, height);
    }

    /// The daemon's answer with a file's or an entry's diff.
    pub(super) fn on_diff(
        &mut self,
        workspace: WorkspaceId,
        pick: &Pick,
        outcome: Result<FileDiff, String>,
    ) {
        let shown = self
            .changes
            .as_mut()
            .filter(|pane| pane.workspace == workspace)
            .and_then(|pane| pane.diff.as_mut())
            .filter(|diff| diff.of == *pick);
        if let Some(diff) = shown {
            diff.text = Some(outcome);
        }
        // Asked again: the scroll stays where it was, if it still can.
        self.scroll_diff(0);
    }

    /// Keeps the pane in step with the state: closed once its workspace
    /// is gone, asked again once its summary says something changed.
    pub(super) fn settle_changes(&mut self) -> Vec<Action> {
        let Some(workspace) = self.changes.as_ref().map(|pane| pane.workspace) else {
            return Vec::new();
        };
        let Some((now, newest, clashes)) = self
            .model
            .workspace(workspace)
            .map(|found| (found.changes, found.timeline, found.clashes.clone()))
        else {
            self.close_changes();
            return Vec::new();
        };
        let height = self.pane_rows();
        if let Some(pane) = self.changes.as_mut()
            && clashes != pane.clashes
        {
            pane.clashes = clashes;
            keep_selected_in_view(pane, height);
        }
        let mut actions = Vec::new();
        if let Some(pane) = self.changes.as_mut()
            && newest != pane.timeline_seen
        {
            pane.timeline_seen = newest;
            actions.extend(self.ask_timeline());
        }
        let Some(pane) = self.changes.as_mut() else {
            return actions;
        };
        if now == pane.seen {
            return actions;
        }
        pane.seen = now;
        // The listing's own summary is told to the state as it is
        // answered: only another says something changed since.
        let stale = match &pane.listing {
            Some(Ok(changes)) => changes.summary() != now,
            Some(Err(_)) => true,
            None => false,
        };
        if stale {
            actions.extend(self.ask_changes());
        }
        actions
    }
}

/// Moves the listing so the selection is in view among `height` lines.
fn keep_selected_in_view(pane: &mut ChangesPane, height: usize) {
    let Some(row) = pane.selected_row() else {
        return;
    };
    // The heading just above shows with it.
    let heading_above = row
        .checked_sub(1)
        .is_some_and(|above| matches!(pane.rows().get(above), Some(ChangeRow::Heading(_))));
    let top = if heading_above { row - 1 } else { row };
    if top < pane.offset {
        pane.offset = top;
    } else if row >= pane.offset + height {
        pane.offset = row + 1 - height;
    }
}

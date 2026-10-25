//! Folding the sessions list: a project's or a worktree's line folds away
//! what is under it (`e`, or a click on its triangle), keeping how it
//! stands on its line, and unfolds again. Which lines are folded is this
//! UI's own: the daemon is never told.
//!
//! The selection only ever rests on a line drawn. Folding the line a
//! session is under takes the selection to that line; something selected
//! for the person (a session just started, one picked in the notch, the
//! last alarm) unfolds what hides it.

use super::{App, View};
use crate::model::Row;

impl App {
    /// The sessions list as drawn: the space shown, as the model lists it
    /// (under everything, each space's heading, its projects with their
    /// worktrees and sessions, then its free terminals), but for what a
    /// folded line hides.
    pub(crate) fn rows(&self) -> Vec<Row> {
        let mut rows = Vec::new();
        let mut hiding: Option<Row> = None;
        for row in self.model.rows_in(self.list_view()) {
            if let Some(folded) = hiding
                && self.is_under(row, folded)
            {
                continue;
            }
            hiding = self.folded.contains(&row).then_some(row);
            rows.push(row);
        }
        rows
    }

    /// Whether `row` is folded.
    pub(crate) fn is_folded(&self, row: Row) -> bool {
        self.folded.contains(&row)
    }

    /// Whether `row` has anything to fold away: a project always has its
    /// workspaces, a space whatever it holds, a worktree only once a
    /// session is in it.
    pub(crate) fn foldable(&self, row: Row) -> bool {
        match row {
            Row::Project(_) => true,
            Row::Space(space) => self
                .model
                .space(space)
                .is_some_and(|s| !s.members.is_empty()),
            Row::Worktree(workspace) => self.model.sessions_of(workspace).next().is_some(),
            Row::Session(_) => false,
        }
    }

    /// How many sessions a folded `row` hides; none while it is unfolded.
    pub(crate) fn hidden_sessions(&self, row: Row) -> usize {
        if self.is_folded(row) {
            self.model.sessions_on(row).count()
        } else {
            0
        }
    }

    /// `e`: folds or unfolds the selected project or worktree; on a
    /// session, folds its worktree, which is then selected.
    pub(super) fn toggle_fold(&mut self) {
        if self.view != View::Sessions {
            return;
        }
        let row = match self.selected {
            Some(Row::Session(session)) => match self.model.session(session) {
                Some(stored) => match stored.workspace {
                    Some(workspace) => Row::Worktree(workspace),
                    // A free terminal folds its space.
                    None => match self.model.space_of(Row::Session(session)) {
                        Some(space) => Row::Space(space),
                        None => return,
                    },
                },
                None => return,
            },
            Some(row) => row,
            None => return,
        };
        self.fold(row);
    }

    /// Folds `row` if it is unfolded, or unfolds it; the selection leaves
    /// any line folding hides for `row` itself.
    pub(super) fn fold(&mut self, row: Row) {
        if self.folded.remove(&row) {
            return;
        }
        if !self.foldable(row) {
            self.complain("No sessions here to fold away.");
            return;
        }
        let hides_selection = self
            .selected
            .is_some_and(|selected| selected != row && self.is_under(selected, row));
        self.folded.insert(row);
        if hides_selection || self.selected.is_none() {
            self.selected = Some(row);
        }
    }

    /// Whether `row` is under `above`: a session under its worktree and
    /// its project, a worktree under its project, and all of them, and a
    /// free terminal, under their space.
    fn is_under(&self, row: Row, above: Row) -> bool {
        match above {
            Row::Project(project) => self.model.project_of(row) == Some(project),
            Row::Worktree(workspace) => self.model.worktree_of(row) == Some(workspace),
            Row::Space(space) => row != above && self.model.space_of(row) == Some(space),
            Row::Session(_) => false,
        }
    }

    /// Unfolds whatever hides `row`, so it can be selected.
    pub(super) fn unfold_to(&mut self, row: Row) {
        if let Some(project) = self.model.project_of(row)
            && row != Row::Project(project)
        {
            self.folded.remove(&Row::Project(project));
        }
        if let (Row::Session(_), Some(workspace)) = (row, self.model.worktree_of(row)) {
            self.folded.remove(&Row::Worktree(workspace));
        }
        if let Some(space) = self.model.space_of(row)
            && self.is_under(row, Row::Space(space))
        {
            self.folded.remove(&Row::Space(space));
        }
    }

    /// Forgets the folds of lines that are gone, and unfolds what hides a
    /// line selected for the person.
    pub(super) fn keep_folds(&mut self) {
        let model = &self.model;
        self.folded.retain(|row| match row {
            Row::Project(project) => model.project(*project).is_some(),
            // One left with no session has nothing to hide.
            Row::Worktree(workspace) => model.sessions_of(*workspace).next().is_some(),
            Row::Space(space) => model.space(*space).is_some(),
            Row::Session(_) => false,
        });
        if let Some(row) = self.selected
            && !self.rows().contains(&row)
            && self.model.rows_in(self.list_view()).contains(&row)
        {
            self.unfold_to(row);
        }
    }
}

//! What the person can do on the selected line of *Sessions* or *Needs
//! you*, each by its key: the bottom bar's hints and the menu a
//! right-click opens say the same, and choosing one in the menu presses
//! its key.

use crossterm::event::KeyCode;
use muster_core::WorkspaceKind;

use super::{App, View};
use crate::model::Row;

/// One thing to do on the selected line, and the key that does it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Here {
    pub(crate) key: KeyCode,
    /// What it does, in a word or two: `full screen`, `changes`.
    pub(crate) what: &'static str,
}

impl Here {
    const fn new(key: char, what: &'static str) -> Self {
        Self {
            key: KeyCode::Char(key),
            what,
        }
    }

    const fn enter(what: &'static str) -> Self {
        Self {
            key: KeyCode::Enter,
            what,
        }
    }

    /// The key as the person reads it: `⏎`, or its letter.
    pub(crate) fn key_name(self) -> String {
        match self.key {
            KeyCode::Enter => "⏎".to_owned(),
            KeyCode::Char(key) => key.to_string(),
            other => format!("{other}"),
        }
    }
}

impl App {
    /// What can be done on the selected line of *Sessions* or *Needs
    /// you*, most used first; nothing on the other screens.
    pub(crate) fn actions_here(&self) -> Vec<Here> {
        let session = self.focused_session().and_then(|id| self.model.session(id));
        match (self.view, self.selected, session) {
            (View::Tickets | View::Settings, _, _) => Vec::new(),
            (_, _, Some(session)) if session.ended_at.is_none() => vec![
                Here::enter("type in it"),
                Here::new('f', "full screen"),
                Here::new('d', "changes"),
                Here::new('x', "close"),
                Here::new('s', "stop"),
                Here::new('n', "session"),
            ],
            (_, _, Some(_)) => vec![
                Here::enter("open"),
                Here::new('f', "full screen"),
                Here::new('d', "changes"),
                Here::new('r', "restart"),
                Here::new('x', "remove"),
                Here::new('n', "session"),
            ],
            (View::Sessions, Some(Row::Worktree(id)), None) => self.worktree_actions(id),
            (View::Sessions, Some(Row::Project(id)), None) => {
                let mut here = vec![Here::enter(if self.model.is_plain_folder(id) {
                    "session"
                } else {
                    "worktree"
                })];
                here.extend(self.fold_action(Row::Project(id)));
                if !self.model.is_plain_folder(id) {
                    here.extend([Here::new('n', "session"), Here::new('i', "tickets")]);
                }
                here.push(Here::new('x', "remove"));
                here
            }
            _ => Vec::new(),
        }
    }

    fn worktree_actions(&self, id: muster_core::WorkspaceId) -> Vec<Here> {
        let workspace = self.model.workspace(id);
        let kind = workspace.map(|workspace| &workspace.kind);
        let mut here = vec![Here::enter("session")];
        if kind.is_some_and(WorkspaceKind::is_worktree) {
            here.push(Here::new('x', "remove"));
        }
        if kind.is_some_and(|kind| !kind.in_git()) {
            // A plain folder: its checks, and nothing that needs git.
            here.push(Here::new('c', "check"));
            here.extend(self.fold_action(Row::Worktree(id)));
            return here;
        }
        if kind.is_some_and(|kind| kind.run().is_none()) {
            here.extend([
                Here::new('d', "changes"),
                Here::new('c', "check"),
                Here::new('v', "review"),
                Here::new('t', "tidy"),
                Here::new('u', "onto base"),
                Here::new('o', "PR"),
            ]);
        }
        if workspace.is_some_and(|workspace| workspace.pull_request.is_some()) {
            here.push(Here::new('b', "browser"));
        }
        here.extend(self.fold_action(Row::Worktree(id)));
        here.extend([Here::new('w', "worktree"), Here::new('i', "tickets")]);
        here
    }

    /// What `e` does on a project's or a worktree's line, where it can.
    fn fold_action(&self, row: Row) -> Option<Here> {
        if self.is_folded(row) {
            Some(Here::new('e', "unfold"))
        } else {
            self.foldable(row).then_some(Here::new('e', "fold"))
        }
    }
}

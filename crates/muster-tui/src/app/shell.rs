//! Shells beside the agents (`PRODUCT.md` §6): a shell is a terminal the
//! person places as any is, as many in a workspace as they like. `T`
//! places the selected line's workspace's shell beside the panel with the
//! keys, with the keys: one running there off the screen, else a new one;
//! where one is on the screen already, the keys go to it. A shell that
//! ends leaves its panel, the daemon taking it out of the layout.

use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::Command;
use muster_core::{SessionId, WorkspaceId, harness};

use super::{Action, App, Mode, Then, View};
use crate::model::Row;

impl App {
    /// The workspace the selected line works in: a session's, a
    /// worktree's, or a project's own checkout.
    pub(super) fn selected_workspace(&self) -> Option<WorkspaceId> {
        match self.view {
            View::Sessions => self.selected.and_then(|row| match row {
                Row::Project(project) => self.model.checkout_of(project),
                row => self.model.worktree_of(row),
            }),
            View::NeedsYou => self
                .needs
                .selected
                .and_then(|session| self.model.worktree_of(Row::Session(session))),
            View::Tickets | View::Settings => None,
        }
    }

    /// `T`: the workspace's shell beside the panel with the keys, with the
    /// keys: the one on the screen already, else one running there off it,
    /// else a new one.
    pub(super) fn open_shell(&mut self) -> Vec<Action> {
        let Some(workspace) = self.selected_workspace() else {
            self.complain("Select a session, a worktree or a project to open a shell in it.");
            return Vec::new();
        };
        let placed = self.placed_beside();
        let running: Vec<SessionId> = self
            .model
            .sessions_of(workspace)
            .filter(|session| session.kind().is_shell() && session.ended_at.is_none())
            .map(|session| session.id)
            .collect();
        // The newest first: the one started last is the one used last.
        if let Some(shown) = running.iter().rev().find(|shell| placed.contains(shell)) {
            self.mode = Mode::Terminal(*shown);
            return Vec::new();
        }
        let place = self.beside_keys(Edge::Right);
        if let Some(off) = running.last().copied() {
            self.mode = Mode::Terminal(off);
            return self.place_at(off, place);
        }
        self.start_shell(workspace, place)
    }

    /// Where a panel put beside the one with the keys goes, at `edge` of
    /// it: beside the preview while the list has them.
    pub(super) fn beside_keys(&self, edge: Edge) -> Place {
        match self.panel_with_keys() {
            Panel::List => Place::Beside(Panel::Session, edge),
            panel => Place::Beside(panel, edge),
        }
    }

    /// Starts a shell in `workspace` for a panel of its own at `place`,
    /// placed there with the keys once the state has it.
    pub(super) fn start_shell(&mut self, workspace: WorkspaceId, place: Place) -> Vec<Action> {
        if self.narrow() {
            self.complain("Shells open beside the list in a window wide enough for both.");
            return Vec::new();
        }
        vec![self.command(
            Command::StartSession {
                workspace: Some(workspace),
                harness: harness::SHELL.to_owned(),
                title: None,
                size: self.session_size(),
                prompt: None,
                started_by: None,
                space: None,
            },
            Then::Place(place),
        )]
    }

    /// A session started for a panel of its own: placed, with the keys,
    /// once the state has it.
    pub(super) fn take_wanted_place(&mut self) -> Vec<Action> {
        let Some((session, place)) = self.wanted_place else {
            return Vec::new();
        };
        if self.model.session(session).is_none() {
            return Vec::new();
        }
        self.wanted_place = None;
        self.mode = Mode::Terminal(session);
        self.place_at(session, place)
    }
}

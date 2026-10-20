//! A shell beside the agent (`PRODUCT.md` §6): `T` opens the selected
//! line's workspace's shell in a panel of its own, beside the session and
//! both live, with the keys, starting one where the workspace has none;
//! `T` again on that workspace puts it away, still running. Where the
//! panels do not fit, the list folds first, then the shell opens over the
//! session. A click, or Shift-Q and `T`, moves the keys between them.

use muster_core::protocol::{Command, TermSize};
use muster_core::{SessionId, WorkspaceId, harness};

use super::{Action, App, Mode, Then, View};
use crate::model::Row;

impl App {
    /// The shell beside the session: one shown that still runs, and is not
    /// the session the list shows itself.
    pub(crate) fn shell_beside(&self) -> Option<SessionId> {
        self.shell
            .filter(|shell| {
                self.model
                    .session(*shell)
                    .is_some_and(|stored| stored.ended_at.is_none())
            })
            .filter(|shell| Some(*shell) != self.focused_session())
    }

    /// Whether the shell beside the session has the keys.
    pub(crate) fn typing_in_shell(&self) -> bool {
        matches!(self.mode, Mode::Terminal(session) if self.shell_beside() == Some(session))
    }

    /// The workspace the selected line works in: a session's, a
    /// worktree's, or a project's own checkout.
    pub(super) fn selected_workspace(&self) -> Option<WorkspaceId> {
        match self.view {
            View::Sessions => self.selected.and_then(|row| match row {
                Row::Project(project) => self.model.checkout_of(project),
                row => self.model.worktree_of(row),
            }),
            View::NeedsYou => self
                .needs_selected
                .and_then(|session| self.model.worktree_of(Row::Session(session))),
            View::Tickets | View::Settings => None,
        }
    }

    /// `T`: the workspace's shell beside the session, with the keys; again,
    /// put away.
    pub(super) fn toggle_shell(&mut self) -> Vec<Action> {
        let Some(workspace) = self.selected_workspace() else {
            self.complain("Select a session, a worktree or a project to open a shell in it.");
            return Vec::new();
        };
        let open = self
            .shell_beside()
            .and_then(|shell| self.model.session(shell))
            .is_some_and(|shell| shell.workspace == workspace);
        if open {
            self.shell = None;
            self.say("The shell is put away; it runs on.");
            return Vec::new();
        }
        let running = self
            .model
            .sessions_of(workspace)
            .find(|session| harness::is_shell(&session.harness) && session.ended_at.is_none())
            .map(|session| session.id);
        if let Some(shell) = running {
            self.shell = Some(shell);
            self.mode = Mode::Terminal(shell);
            return Vec::new();
        }
        vec![self.command(
            Command::StartSession {
                workspace,
                harness: harness::SHELL.to_owned(),
                title: None,
                size: self.session_size(),
                prompt: None,
            },
            Then::OpenShell,
        )]
    }

    /// A shell started for the panel: shown, with the keys, once the state
    /// has it.
    pub(super) fn take_wanted_shell(&mut self) {
        if let Some(shell) = self.wanted_shell
            && self.model.session(shell).is_some()
        {
            self.wanted_shell = None;
            self.shell = Some(shell);
            self.mode = Mode::Terminal(shell);
        }
    }

    /// The size the shell beside the session is given.
    pub(crate) fn shell_size(&self) -> TermSize {
        if self.fills_window() || self.narrow() {
            return self.terminal_size;
        }
        let window =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let pane = crate::view::shell_area(self, window);
        TermSize {
            cols: pane.width.max(1),
            rows: pane.height.max(1),
        }
    }
}

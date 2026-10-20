//! A plugin's panel beside the agent (`PRODUCT.md` §6, `PRD-15`): while
//! a plugin's panel runs in the selected line's workspace, it is shown in
//! a panel of its own, beside the session and the shell, all live; the
//! newest, where there are several. It follows the selection, as it is
//! tied to its workspace for now, as the shell is. A click on it gives it
//! the keys, and a click elsewhere or Shift-Q takes them back.

use muster_core::protocol::TermSize;
use muster_core::{SessionId, harness};

use super::{App, Mode, View};
use crate::model::Row;

impl App {
    /// The plugin's panel running in the selected line's workspace, the
    /// newest if there are more.
    fn plugin_panel(&self) -> Option<SessionId> {
        let workspace = self.selected_workspace()?;
        self.model
            .sessions_of(workspace)
            .filter(|session| harness::is_panel(&session.harness) && session.ended_at.is_none())
            .max_by_key(|session| session.created_at)
            .map(|session| session.id)
    }

    /// Whether `session` is the plugin's panel of the selected line's
    /// workspace, which the panel beside the session shows rather than
    /// the session panel, unless it is the line selected.
    pub(crate) fn is_plugin_panel(&self, session: SessionId) -> bool {
        self.plugin_panel() == Some(session)
    }

    /// The plugin's panel beside the session: one running in the selected
    /// line's workspace, and not the session the list shows itself.
    pub(crate) fn plugin_beside(&self) -> Option<SessionId> {
        self.plugin_panel()
            .filter(|panel| Some(*panel) != self.focused_session())
    }

    /// Whether the plugin's panel beside the session has the keys.
    pub(crate) fn typing_in_plugin(&self) -> bool {
        matches!(self.mode, Mode::Terminal(session) if self.plugin_beside() == Some(session))
    }

    /// A plugin's panel with the keys beside the session that is shown
    /// there no more, ended or its workspace no longer the selected
    /// line's, gives them back to the list. One opened from its own line
    /// keeps them, as any session does.
    pub(super) fn keep_plugin_keys(&mut self) {
        let Mode::Terminal(session) = self.mode else {
            return;
        };
        let opened = match self.view {
            View::Sessions => self.selected == Some(Row::Session(session)),
            View::NeedsYou => self.needs_selected == Some(session),
            View::Tickets | View::Settings => true,
        };
        let panel = self
            .model
            .session(session)
            .is_some_and(|stored| harness::is_panel(&stored.harness));
        if panel && !opened && self.plugin_beside() != Some(session) {
            self.mode = Mode::Browse;
        }
    }

    /// The size the plugin's panel beside the session is given.
    pub(crate) fn plugin_size(&self) -> TermSize {
        if self.fills_window() || self.narrow() {
            return self.terminal_size;
        }
        let window =
            ratatui::layout::Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let pane = crate::view::plugin_area(self, window);
        TermSize {
            cols: pane.width.max(1),
            rows: pane.height.max(1),
        }
    }
}

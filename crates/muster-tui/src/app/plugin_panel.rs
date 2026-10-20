//! A plugin's panel beside the agent (`PRODUCT.md` §6, `PRD-15`): the
//! panel opened beside the session selected is shown in a panel of its
//! own, beside the session and the shell, all live; on a line with none,
//! the panel opened on the line's workspace; the newest, where there are
//! several. It follows the selection. Its key, or its item in a line's
//! menu, opens it beside the session selected, or on its workspace as its
//! manifest says, and again puts it away, its program ended. A click on
//! it gives it the keys, and a click elsewhere or Shift-Q takes them back.

use muster_core::keys::PanelAction;
use muster_core::plugins::{PANELS_PROTOCOL, Panel, PanelOn};
use muster_core::protocol::plugins::{Beside, PluginCommand};
use muster_core::protocol::{Command, TermSize};
use muster_core::{SessionId, harness};

use super::{Action, App, Mode, Then, View};
use crate::model::Row;

impl App {
    /// The session the selected line is, whose panels are shown beside it.
    fn selected_line_session(&self) -> Option<SessionId> {
        match self.view {
            View::Sessions => match self.selected {
                Some(Row::Session(session)) => Some(session),
                _ => None,
            },
            View::NeedsYou => self.needs_selected,
            View::Tickets | View::Settings => None,
        }
    }

    /// The plugin's panel shown beside the selected line: the newest
    /// running beside the session selected, else the newest running on its
    /// workspace.
    fn plugin_panel(&self) -> Option<SessionId> {
        let workspace = self.selected_workspace()?;
        let selected = self.selected_line_session();
        let running = || {
            self.model
                .sessions_of(workspace)
                .filter(|session| harness::is_panel(&session.harness) && session.ended_at.is_none())
        };
        let newest = |beside: Option<SessionId>| {
            running()
                .filter(|session| session.beside == beside)
                .max_by_key(|session| session.created_at)
                .map(|session| session.id)
        };
        selected
            .and_then(|session| newest(Some(session)))
            .or_else(|| newest(None))
    }

    /// Whether `session` is the plugin's panel beside the selected line,
    /// which the panel beside the session shows rather than the session
    /// panel, unless it is the line selected.
    pub(crate) fn is_plugin_panel(&self, session: SessionId) -> bool {
        self.plugin_panel() == Some(session)
    }

    /// The plugin's panel beside the session: the one beside the selected
    /// line, and not the session the list shows itself.
    pub(crate) fn plugin_beside(&self) -> Option<SessionId> {
        self.plugin_panel()
            .filter(|panel| Some(*panel) != self.focused_session())
    }

    /// Whether the plugin's panel beside the session has the keys.
    pub(crate) fn typing_in_plugin(&self) -> bool {
        matches!(self.mode, Mode::Terminal(session) if self.plugin_beside() == Some(session))
    }

    /// A plugin's panel with the keys beside the session that is shown
    /// there no more, ended or no longer beside the selected line, gives
    /// them back to the list. One opened from its own line keeps them, as
    /// any session does.
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

    /// The panels of the plugins that run, as their keys open them: each
    /// plugin's in its manifest's order, the plugins by id.
    pub(crate) fn panel_actions(&self) -> Vec<PanelAction> {
        self.model
            .plugins()
            .iter()
            .filter(|plugin| plugin.runs())
            .filter_map(|plugin| Some((plugin, plugin.details.as_ref()?)))
            .flat_map(|(plugin, details)| {
                details.panels.iter().map(|panel| PanelAction {
                    plugin: plugin.id.clone(),
                    panel: panel.id.clone(),
                    plugin_name: details.name.clone(),
                    title: panel.title.clone(),
                    suggested: panel.key.clone(),
                })
            })
            .collect()
    }

    /// What the manifest says of `action`'s panel.
    pub(crate) fn manifest_panel(&self, action: &PanelAction) -> Option<&Panel> {
        self.model
            .plugins()
            .iter()
            .find(|plugin| plugin.id == action.plugin)?
            .details
            .as_ref()?
            .panels
            .iter()
            .find(|panel| panel.id == action.panel)
    }

    /// The panel `name`'s key, or its item in the menu: the panel opened
    /// beside the session selected, or on the selected line's workspace,
    /// as its manifest says; open there already, put away, its program
    /// ended.
    pub(super) fn toggle_panel(&mut self, name: &str) -> Vec<Action> {
        let Some(action) = self.keymap.panel_named(name).cloned() else {
            return Vec::new();
        };
        let Some(on) = self.manifest_panel(&action).map(|panel| panel.on) else {
            return Vec::new();
        };
        if self.daemon_protocol < PANELS_PROTOCOL {
            self.complain(
                "The daemon runs an older muster that opens no plugin's panel: `muster reload` \
                 moves it onto this one, every session running on.",
            );
            return Vec::new();
        }
        let beside = match (on, self.selected_line_session(), self.selected_workspace()) {
            (PanelOn::Session, Some(session), _) => Beside::Session(session),
            (PanelOn::Workspace, _, Some(workspace)) => Beside::Workspace(workspace),
            (PanelOn::Session, None, _) => {
                self.complain(format!(
                    "Select a session to open {} beside it.",
                    action.title
                ));
                return Vec::new();
            }
            (PanelOn::Workspace, _, None) => {
                self.complain(format!(
                    "Select a session, a worktree or a project to open {} on it.",
                    action.title
                ));
                return Vec::new();
            }
        };
        if let Some(open) = self.panel_open(&action, beside) {
            if self.mode == Mode::Terminal(open) {
                self.mode = Mode::Browse;
            }
            self.closing.push(open);
            self.say(format!("{} is put away.", action.title));
            return vec![self.command(Command::StopSession { session: open }, Then::Closing(open))];
        }
        vec![self.command(
            Command::Plugin(PluginCommand::OpenPanel {
                plugin: action.plugin,
                panel: action.panel,
                beside,
                size: self.session_size(),
            }),
            Then::Say("The plugin's panel is open."),
        )]
    }

    /// `action`'s panel running where `beside` says, if one is.
    fn panel_open(&self, action: &PanelAction, beside: Beside) -> Option<SessionId> {
        let harness = harness::panel(&action.plugin, &action.panel);
        self.model
            .sessions()
            .iter()
            .filter(|session| session.harness == harness && session.ended_at.is_none())
            .find(|session| match beside {
                Beside::Session(chat) => session.beside == Some(chat),
                Beside::Workspace(workspace) => {
                    session.workspace == workspace && session.beside.is_none()
                }
            })
            .map(|session| session.id)
    }
}

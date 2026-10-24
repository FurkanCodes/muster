//! Sessions pinned in panels of their own (`PRODUCT.md` §6 Sessions): an
//! agent's terminal beside the rest whatever the list selects, as many as
//! the person likes. A session's line dragged out of the list lands where
//! it is dropped, as a panel does; `p` pins the selected session beside
//! the panel with the keys, and again unpins it. The layout keeps them
//! (`muster_core::layout`, `session:<id>`), so the daemon has them for
//! every window and the next. Each is streamed at its own size, and a
//! click on it gives it the keys.

use muster_core::SessionId;
use muster_core::layout::{Edge, Panel, Place};
use muster_core::protocol::TermSize;
use ratatui::layout::Position;

use super::lines::line_at;
use super::{Action, App, Mode, View};
use crate::model::Row;

impl App {
    /// The sessions pinned in panels of their own that can be shown: those
    /// the state still has, in the layout's order; none in a window too
    /// narrow for panels beside the list.
    pub(crate) fn pinned_beside(&self) -> Vec<SessionId> {
        if self.narrow() {
            return Vec::new();
        }
        self.panels()
            .pinned()
            .into_iter()
            .filter(|session| self.model.session(*session).is_some())
            .collect()
    }

    /// The pinned session that has the keys, if one does.
    pub(crate) fn typing_in_pinned(&self) -> Option<SessionId> {
        match self.mode {
            Mode::Terminal(session) if self.pinned_beside().contains(&session) => Some(session),
            _ => None,
        }
    }

    /// The sessions drawn beside the one the list shows, each with its
    /// panel: the shell, a plugin's panel, and those pinned.
    pub(crate) fn terminals_beside(&self) -> Vec<(Panel, SessionId)> {
        let mut beside = Vec::new();
        beside.extend(self.shell_beside().map(|shell| (Panel::Shell, shell)));
        beside.extend(self.plugin_beside().map(|panel| (Panel::Plugin, panel)));
        beside.extend(
            self.pinned_beside()
                .into_iter()
                .map(|session| (Panel::Pinned(session), session)),
        );
        beside
    }

    /// The size the session pinned in `panel` is given, measured as the
    /// session beside the list is.
    pub(crate) fn panel_size(&self, panel: Panel) -> TermSize {
        if self.fills_window() || self.narrow() {
            return self.terminal_size;
        }
        let area = crate::view::panel_area(self, self.window(), panel);
        TermSize {
            cols: area.width.max(1),
            rows: area.height.max(1),
        }
    }

    /// `p`: pins the selected session in a panel of its own beside the
    /// panel with the keys, or unpins it where it is pinned.
    pub(super) fn toggle_pin(&mut self) -> Vec<Action> {
        let Some(session) = self.focused_session().or(self.typing_in_pinned()) else {
            self.complain("Select a session to pin it beside the others.");
            return Vec::new();
        };
        if self.panels().pinned().contains(&session) {
            return self.unpin(session);
        }
        if self.narrow() {
            self.complain("Sessions are pinned in a window wide enough for the list beside them.");
            return Vec::new();
        }
        let beside = match self.panel_with_keys() {
            Panel::List => Panel::Session,
            panel => panel,
        };
        self.pin_at(session, Place::Beside(beside, Edge::Left))
    }

    /// Pins `session` at `place`, or moves it there where it is pinned.
    pub(super) fn pin_at(&mut self, session: SessionId, place: Place) -> Vec<Action> {
        let name = self.session_name(session);
        let pinned = self.panels().pinning(session, place);
        self.say(format!("Pinned {name} in a panel of its own."));
        self.keep_panels(pinned)
    }

    /// Takes `session` out of its panel; the keys go back to the list.
    pub(super) fn unpin(&mut self, session: SessionId) -> Vec<Action> {
        if self.mode == Mode::Terminal(session) {
            self.mode = Mode::Browse;
        }
        let name = self.session_name(session);
        let unpinned = self.panels().unpinned(session);
        self.say(format!("Unpinned {name}."));
        self.keep_panels(unpinned)
    }

    /// The session whose line in the list is at `at`, which a drag out of
    /// the list pins.
    pub(super) fn session_line_at(&self, at: Position) -> Option<SessionId> {
        let list = crate::view::browse_areas(self, self.window()).list;
        if !list.contains(at) {
            return None;
        }
        match self.view {
            View::Sessions => {
                let rows = self.rows();
                let heights = self.row_heights(&rows);
                match rows[line_at(list, at.y, &heights, self.list_offset)?] {
                    Row::Session(session) => Some(session),
                    _ => None,
                }
            }
            View::NeedsYou => {
                let heights = self.needs_heights();
                let at = line_at(list, at.y, &heights, self.needs.offset)?;
                self.needs.list.get(at).copied()
            }
            View::Tickets | View::Settings => None,
        }
    }

    fn session_name(&self, session: SessionId) -> String {
        self.model
            .session(session)
            .map_or_else(|| "the session".to_owned(), |stored| stored.title.clone())
    }
}

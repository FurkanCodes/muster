//! Terminals placed in panels of their own (`PRODUCT.md` §6 Sessions): an
//! agent's or a shell's, beside the rest whatever the list selects, as
//! many as the person likes. A session's line dragged out of the list
//! lands where it is dropped, as a panel does; `P` places the selected
//! session beside the panel with the keys, and again takes it off the
//! screen, as its title's `✕` does. The layout keeps them
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
    /// The sessions placed in panels of their own that can be shown: those
    /// the state still has, in the layout's order, but a shell that ended,
    /// which the daemon is taking out of the layout; none in a window too
    /// narrow for panels beside the list.
    pub(crate) fn placed_beside(&self) -> Vec<SessionId> {
        if self.narrow() {
            return Vec::new();
        }
        self.panels()
            .placed()
            .into_iter()
            .filter(|session| {
                self.model
                    .session(*session)
                    .is_some_and(|stored| !(stored.kind().is_shell() && stored.ended_at.is_some()))
            })
            .collect()
    }

    /// The placed session that has the keys, if one does.
    pub(crate) fn typing_in_placed(&self) -> Option<SessionId> {
        match self.mode {
            Mode::Terminal(session) if self.placed_beside().contains(&session) => Some(session),
            _ => None,
        }
    }

    /// The sessions drawn beside the preview, each with its panel: a
    /// plugin's panel, and those placed.
    pub(crate) fn terminals_beside(&self) -> Vec<(Panel, SessionId)> {
        let mut beside = Vec::new();
        beside.extend(self.plugin_beside().map(|panel| (Panel::Plugin, panel)));
        beside.extend(
            self.placed_beside()
                .into_iter()
                .map(|session| (Panel::Placed(session), session)),
        );
        beside
    }

    /// The size the session placed in `panel` is given, measured as the
    /// preview is.
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

    /// `P`: places the selected session in a panel of its own beside the
    /// panel with the keys, or takes it off the screen where it is placed.
    pub(super) fn toggle_place(&mut self) -> Vec<Action> {
        let Some(session) = self.focused_session().or(self.typing_in_placed()) else {
            self.complain("Select a session to place it beside the others.");
            return Vec::new();
        };
        if self.panels().placed().contains(&session) {
            return self.take_off(session);
        }
        if self.narrow() {
            self.complain("Sessions are placed in a window wide enough for the list beside them.");
            return Vec::new();
        }
        let place = self.beside_keys(Edge::Left);
        self.place_at(session, place)
    }

    /// Places `session` at `place`, or moves it there where it is placed.
    pub(super) fn place_at(&mut self, session: SessionId, place: Place) -> Vec<Action> {
        let name = self.session_name(session);
        let placed = self.panels().placing(session, place);
        self.say(format!("Placed {name} in a panel of its own."));
        self.keep_panels(placed)
    }

    /// Takes `session` off the screen, running on; the keys go back to the
    /// list.
    pub(super) fn take_off(&mut self, session: SessionId) -> Vec<Action> {
        if self.mode == Mode::Terminal(session) {
            self.mode = Mode::Browse;
        }
        let name = self.session_name(session);
        let taken = self.panels().unplaced(session);
        self.say(format!("Took {name} off the screen; it runs on."));
        self.keep_panels(taken)
    }

    /// The session whose line in the list is at `at`, which a drag out of
    /// the list places.
    pub(super) fn session_line_at(&self, at: Position) -> Option<SessionId> {
        let list = crate::view::browse_areas(self, self.window()).list;
        // Inside its frame: the frame's edge is the line beside it, or its
        // scroll bar.
        if !list.inner(ratatui::layout::Margin::new(1, 1)).contains(at) {
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

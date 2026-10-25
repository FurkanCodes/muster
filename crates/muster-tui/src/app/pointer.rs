//! What the mouse does in each mode. In a session's terminal, a drag
//! selects its text and copies it (see `selection`); a program that asked
//! for the mouse gets it, but for a drag with Shift; otherwise the wheel
//! scrolls back through the session's output (or, on the alternate screen,
//! which keeps none, sends arrow keys as terminals do), wherever the
//! pointer is, since the session has the keys. A click on the list beside
//! it, or on the top bar over it full screen, gives the list the keys
//! back. In the list, the wheel moves the selection or scrolls the
//! preview (a session's output, or a ticket's details), and a click
//! selects, or opens the session shown.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, MouseTracking};
use ratatui::layout::{Position, Rect};

use super::lines::line_at;
use super::selection::cell;
use super::{Action, App, Mode, View};
use crate::keys::encode_key;
use crate::mouse::encode_mouse;
use crate::view::browse_areas;

/// Lines one turn of the wheel scrolls.
const WHEEL_LINES: isize = 3;

impl App {
    pub(crate) fn on_mouse(&mut self, event: MouseEvent) -> Vec<Action> {
        let at = Position::new(event.column, event.row);
        // Moving, the pointer only lights what it could take hold of, and
        // reaches the program in a session that asked for it.
        if event.kind == MouseEventKind::Moved {
            self.panels.hover = self.grab_under(at);
            return match self.mode {
                Mode::Terminal(session) if !self.welcome => self.terminal_mouse(session, event),
                _ => Vec::new(),
            };
        }
        let actions = self.pointer(event);
        self.panels.hover = self.grab_under(at);
        actions
    }

    fn pointer(&mut self, event: MouseEvent) -> Vec<Action> {
        if self.skip_welcome() {
            return Vec::new();
        }
        // A right-click on a panel's title: what the panel key does to it.
        if event.kind == MouseEventKind::Down(MouseButton::Right)
            && self.can_arrange()
            && self.panels.drag.is_none()
            && let Some(super::grab::Hold::Title(panel)) =
                self.hold_at(Position::new(event.column, event.row))
        {
            self.open_title_menu(panel, Position::new(event.column, event.row));
            return Vec::new();
        }
        if let Some(mut actions) = self.drag_mouse(event) {
            actions.extend(self.settle());
            return actions;
        }
        self.mode_mouse(event)
    }

    /// What `event` does in the mode the app is in.
    pub(super) fn mode_mouse(&mut self, event: MouseEvent) -> Vec<Action> {
        let mut actions = match self.mode {
            Mode::Terminal(session) => return self.terminal_mouse(session, event),
            Mode::Browse => self.browse_mouse(event),
            Mode::Changes => self.changes_mouse(event),
            Mode::Menu(ref menu) => {
                let menu = menu.clone();
                self.menu_mouse(&menu, event)
            }
            Mode::Help(ref help) => {
                let help = help.clone();
                self.help_mouse(help, event)
            }
            Mode::Dialog(_) => Vec::new(),
        };
        actions.extend(self.settle());
        actions
    }

    fn terminal_mouse(&mut self, session: SessionId, event: MouseEvent) -> Vec<Action> {
        let window = Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let at = Position::new(event.column, event.row);
        let pressed = event.kind == MouseEventKind::Down(MouseButton::Left);
        // Where the program's screen is: below the top bar when it fills
        // the window, else inside the pane beside the list.
        let screen = if self.fills_window() {
            let screen = crate::view::terminal_area(window);
            if pressed && !screen.contains(at) {
                // The top bar, as the list beside a session.
                let mut actions = self.leave_terminal(session);
                actions.extend(self.settle());
                return actions;
            }
            screen
        } else {
            let areas = browse_areas(self, window);
            let list = areas.list;
            let right = event.kind == MouseEventKind::Down(MouseButton::Right);
            if (pressed || right) && list.contains(at) {
                let mut actions = self.leave_terminal(session);
                if pressed {
                    self.click_list(list, at);
                } else {
                    self.open_menu(list, at);
                }
                actions.extend(self.settle());
                return actions;
            }
            // A press on another of the session and the terminals beside
            // it (its shell, a plugin's panel, a session pinned) gives that
            // one the keys.
            let beside = self.terminals_beside();
            let typing_beside = beside
                .iter()
                .find(|(_, other)| *other == session)
                .map(|(panel, _)| *panel);
            if pressed && typing_beside.is_some() && areas.preview.contains(at) {
                let mut actions = self.leave_terminal(session);
                self.open_selected();
                actions.extend(self.settle());
                return actions;
            }
            if pressed
                && let Some((_, other)) = beside
                    .iter()
                    .find(|(panel, other)| *other != session && areas.of(*panel).contains(at))
            {
                self.mode = Mode::Terminal(*other);
                return self.settle();
            }
            match typing_beside {
                Some(panel) => crate::view::panel_area(self, window, panel),
                None => crate::view::session_area(self, window),
            }
        };
        let modes = self.screen(session).modes;
        let tracked = modes.mouse != MouseTracking::Off;
        if let Some(actions) = self.select_with_mouse(session, event, screen, tracked) {
            return actions;
        }
        let wheel = matches!(
            event.kind,
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown
        );
        if wheel || matches!(event.kind, MouseEventKind::Down(_)) {
            // What is marked moves, or is clicked away, wherever.
            self.selection = None;
        }
        // The wheel is the session's wherever the pointer is: it has the
        // keys. Anything else is only over its screen.
        if !wheel && !screen.contains(at) {
            return Vec::new();
        }
        let (row, column) = cell(screen, event);
        if tracked {
            return encode_mouse(event.kind, event.modifiers, column, row, modes)
                .map(|data| Action::Send(ClientFrame::Input { session, data }))
                .into_iter()
                .collect();
        }
        let lines = match event.kind {
            MouseEventKind::ScrollUp => WHEEL_LINES,
            MouseEventKind::ScrollDown => -WHEEL_LINES,
            _ => return Vec::new(),
        };
        if modes.alternate_screen {
            // A full-screen program keeps no scrollback; terminals send it
            // arrow keys instead, which is how `less` scrolls.
            let code = if lines > 0 {
                KeyCode::Up
            } else {
                KeyCode::Down
            };
            let Some(arrow) = encode_key(&KeyEvent::new(code, KeyModifiers::NONE), modes) else {
                return Vec::new();
            };
            let data = arrow.repeat(WHEEL_LINES.unsigned_abs());
            return vec![Action::Send(ClientFrame::Input { session, data })];
        }
        self.screen(session).scroll(lines);
        Vec::new()
    }

    fn browse_mouse(&mut self, event: MouseEvent) -> Vec<Action> {
        let full = Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let areas = browse_areas(self, full);
        let at = Position::new(event.column, event.row);
        match event.kind {
            MouseEventKind::ScrollUp | MouseEventKind::ScrollDown => {
                let up = event.kind == MouseEventKind::ScrollUp;
                if areas.list.contains(at) {
                    self.move_selection(if up { -1 } else { 1 });
                } else if areas.preview.contains(at) && self.view == View::Tickets {
                    self.scroll_ticket(if up { -WHEEL_LINES } else { WHEEL_LINES });
                } else if areas.preview.contains(at)
                    && let Some(session) = self.session_beside_list()
                {
                    let modes = self.screen(session).modes;
                    if modes.mouse != MouseTracking::Off {
                        // A program that takes the mouse scrolls its own
                        // view (Claude Code's transcript, on the alternate
                        // screen, which keeps no scrollback here).
                        let (row, column) = cell(crate::view::session_area(self, full), event);
                        return encode_mouse(event.kind, event.modifiers, column, row, modes)
                            .map(|data| Action::Send(ClientFrame::Input { session, data }))
                            .into_iter()
                            .collect();
                    }
                    self.screen(session)
                        .scroll(if up { WHEEL_LINES } else { -WHEEL_LINES });
                }
            }
            MouseEventKind::Down(MouseButton::Right) if areas.list.contains(at) => {
                self.open_menu(areas.list, at);
            }
            MouseEventKind::Down(MouseButton::Left) => {
                let beside = self
                    .terminals_beside()
                    .into_iter()
                    .find(|(panel, _)| areas.of(*panel).contains(at));
                if areas.list.contains(at) {
                    self.click_list(areas.list, at);
                } else if let Some((_, session)) = beside {
                    self.mode = Mode::Terminal(session);
                } else if areas.preview.contains(at)
                    && matches!(self.view, View::Sessions | View::NeedsYou)
                    && self.focused_session().is_some()
                {
                    // A session's pane opens it; a project's, a worktree's,
                    // a ticket's or a setting's details are only to read:
                    // a click there never opens a dialog, which would take
                    // no clicks.
                    self.open_beside_list();
                    if let Mode::Terminal(session) = self.mode {
                        let screen = crate::view::session_area(self, full);
                        self.select_from_preview(session, event, screen);
                    }
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// A click on the list at `at`: selects the line there, and on a
    /// project's or a worktree's triangle folds or unfolds it.
    fn click_list(&mut self, list: Rect, at: Position) {
        self.select_at(list, at.y);
        if self.view != View::Sessions {
            return;
        }
        let rows = self.rows();
        let heights = self.row_heights(&rows);
        let Some(row) = line_at(list, at.y, &heights, self.list_offset).map(|index| rows[index])
        else {
            return;
        };
        let on_triangle =
            crate::view::fold_column(row).is_some_and(|column| at.x == list.x + 1 + column);
        if on_triangle && (self.foldable(row) || self.is_folded(row)) {
            self.fold(row);
        }
    }

    /// Selects the list line drawn at `row`, if there is one.
    pub(super) fn select_at(&mut self, list: Rect, row: u16) {
        match self.view {
            View::Sessions => {
                let rows = self.rows();
                let heights = self.row_heights(&rows);
                if let Some(index) = line_at(list, row, &heights, self.list_offset) {
                    self.selected = Some(rows[index]);
                }
            }
            View::NeedsYou => {
                let heights = self.needs_heights();
                if let Some(index) = line_at(list, row, &heights, self.needs.offset) {
                    self.needs.selected = Some(self.needs.list[index]);
                }
            }
            View::Tickets => {
                let rows = self.ticket_rows();
                let heights = vec![1; rows.len()];
                if let Some(index) = line_at(list, row, &heights, self.tickets.offset) {
                    self.tickets.selected = Some(rows[index].clone());
                }
            }
            View::Settings => {
                let rows = self.setting_rows();
                let heights = vec![1; rows.len()];
                if let Some(index) = line_at(list, row, &heights, self.settings.offset) {
                    self.settings.selected = Some(rows[index]);
                }
            }
        }
    }

    /// Scrolls the preview of the selected session a page back or forward.
    pub(super) fn page_preview(&mut self, back: bool) {
        let full = Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        let page = browse_areas(self, full)
            .preview
            .height
            .saturating_sub(3)
            .max(1);
        let lines = isize::from(i16::try_from(page).unwrap_or(i16::MAX));
        if let Some(session) = self.session_beside_list() {
            self.screen(session)
                .scroll(if back { lines } else { -lines });
        }
    }

    /// Returns the selected session's preview to the newest output.
    pub(super) fn preview_to_bottom(&mut self) {
        if let Some(session) = self.session_beside_list() {
            self.screen(session).scroll_to_bottom();
        }
    }
}

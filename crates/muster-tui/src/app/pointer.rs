//! What the mouse does in each mode. In a session's terminal, a drag
//! selects its text and copies it (see `selection`); a program that asked
//! for the mouse gets it, but for a drag with Shift; otherwise the wheel
//! scrolls back through the session's output (or, on the alternate screen,
//! which keeps none, sends arrow keys as terminals do), wherever the
//! pointer is, since the session has the keys. A click on the list beside
//! it, or on the top bar over it full screen, gives the list the keys
//! back. In the list, the wheel moves the selection or scrolls the
//! preview, and a click selects, or opens the session shown.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, MouseTracking};
use ratatui::layout::{Position, Rect};

use super::selection::cell;
use super::{Action, App, Mode, View};
use crate::keys::encode_key;
use crate::mouse::encode_mouse;
use crate::view::browse_areas;

/// Lines one turn of the wheel scrolls.
const WHEEL_LINES: isize = 3;

impl App {
    pub(crate) fn on_mouse(&mut self, event: MouseEvent) -> Vec<Action> {
        if self.skip_welcome() {
            return Vec::new();
        }
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
        let screen = if self.full_screen {
            let screen = crate::view::terminal_area(window);
            if pressed && !screen.contains(at) {
                // The top bar, as the list beside a session.
                let mut actions = self.leave_terminal(session);
                actions.extend(self.settle());
                return actions;
            }
            screen
        } else {
            let list = browse_areas(self, window).list;
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
            crate::view::session_area(window)
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
                } else if areas.preview.contains(at)
                    && let Some(session) = self.focused_session()
                {
                    let modes = self.screen(session).modes;
                    if modes.mouse != MouseTracking::Off {
                        // A program that takes the mouse scrolls its own
                        // view (Claude Code's transcript, on the alternate
                        // screen, which keeps no scrollback here).
                        let (row, column) = cell(crate::view::session_area(full), event);
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
                if areas.list.contains(at) {
                    self.click_list(areas.list, at);
                } else if areas.preview.contains(at)
                    && matches!(self.view, View::Sessions | View::NeedsYou)
                    && self.focused_session().is_some()
                {
                    // A session's pane opens it; a project's, a worktree's,
                    // a ticket's or a setting's details are only to read:
                    // a click there never opens a dialog, which would take
                    // no clicks.
                    self.open_selected();
                    if let Mode::Terminal(session) = self.mode {
                        self.select_from_preview(session, event, crate::view::session_area(full));
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
        let Some(row) = line_at(list, at.y, rows.len(), self.list_offset).map(|index| rows[index])
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
                if let Some(index) = line_at(list, row, rows.len(), self.list_offset) {
                    self.selected = Some(rows[index]);
                }
            }
            View::NeedsYou => {
                let len = self.needs_list.len();
                if let Some(index) = line_at(list, row, len, self.needs_offset) {
                    self.needs_selected = Some(self.needs_list[index]);
                }
            }
            View::Tickets => {
                let rows = self.ticket_rows();
                if let Some(index) = line_at(list, row, rows.len(), self.tickets_offset) {
                    self.tickets_selected = Some(rows[index].clone());
                }
            }
            View::Settings => {
                let rows = self.setting_rows();
                if let Some(index) = line_at(list, row, rows.len(), self.settings_offset) {
                    self.settings_selected = Some(rows[index]);
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
        if let Some(session) = self.focused_session() {
            self.screen(session)
                .scroll(if back { lines } else { -lines });
        }
    }

    /// Returns the selected session's preview to the newest output.
    pub(super) fn preview_to_bottom(&mut self) {
        if let Some(session) = self.focused_session() {
            self.screen(session).scroll_to_bottom();
        }
    }
}

/// Which of `len` list lines is drawn at `row` in the bordered `list`,
/// whose first line shown is `offset`.
fn line_at(list: Rect, row: u16, len: usize, offset: usize) -> Option<usize> {
    let top = list.y + 1;
    let height = usize::from(list.height.saturating_sub(2));
    if row < top || usize::from(row - top) >= height {
        return None;
    }
    let index = offset + usize::from(row - top);
    (index < len).then_some(index)
}

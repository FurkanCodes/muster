//! What the mouse does in each mode. In a session's terminal, a program
//! that asked for the mouse gets it; otherwise the wheel scrolls back
//! through the session's output (or, on the alternate screen, which keeps
//! none, sends arrow keys as terminals do). In the list, the wheel moves the
//! selection or scrolls the preview, and a click selects or opens. A click
//! on the list beside a session's terminal gives the list the keys back.

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, MouseTracking};
use ratatui::layout::{Position, Rect};

use super::{Action, App, Mode, View};
use crate::keys::encode_key;
use crate::mouse::encode_mouse;
use crate::view::browse_areas;

/// Lines one turn of the wheel scrolls.
const WHEEL_LINES: isize = 3;

impl App {
    pub(crate) fn on_mouse(&mut self, event: MouseEvent) -> Vec<Action> {
        let mut actions = match self.mode {
            Mode::Terminal(session) => return self.terminal_mouse(session, event),
            Mode::Browse => self.browse_mouse(event),
            Mode::Dialog(_) => Vec::new(),
        };
        actions.extend(self.settle());
        actions
    }

    fn terminal_mouse(&mut self, session: SessionId, event: MouseEvent) -> Vec<Action> {
        let window = Rect::new(0, 0, self.terminal_size.cols, self.terminal_size.rows + 1);
        // Where the program's screen is: below the top bar when it fills
        // the window, else inside the pane beside the list.
        let screen = if self.full_screen {
            crate::view::terminal_area(window)
        } else {
            let list = browse_areas(self, window).list;
            if event.kind == MouseEventKind::Down(MouseButton::Left)
                && list.contains(Position::new(event.column, event.row))
            {
                let mut actions = self.leave_terminal(session);
                self.select_at(list, event.row);
                actions.extend(self.settle());
                return actions;
            }
            crate::view::session_area(window)
        };
        if !screen.contains(Position::new(event.column, event.row)) {
            return Vec::new();
        }
        let (column, row) = (event.column - screen.x, event.row - screen.y);
        let modes = self.screen(session).modes;
        if modes.mouse != MouseTracking::Off {
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
                    self.screen(session)
                        .scroll(if up { WHEEL_LINES } else { -WHEEL_LINES });
                }
            }
            MouseEventKind::Down(MouseButton::Left) => {
                if areas.list.contains(at) {
                    self.select_at(areas.list, event.row);
                } else if areas.preview.contains(at) {
                    self.open_selected();
                }
            }
            _ => {}
        }
        Vec::new()
    }

    /// Selects the list line drawn at `row`, if there is one.
    fn select_at(&mut self, list: Rect, row: u16) {
        match self.view {
            View::Sessions => {
                let rows = self.model.rows();
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

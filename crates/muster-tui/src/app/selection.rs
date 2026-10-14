//! Selecting a session's text with the mouse, and copying it. A drag over
//! the session's screen marks its cells in reading order, as a terminal
//! does, and letting go puts their text on the person's clipboard. Only the
//! session's own text is taken: the person's terminal, selecting, takes
//! whole lines of the window, the list and borders beside the session
//! with them. A program that asked for the mouse gets drags itself; with
//! Shift held they select anyway.

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use muster_core::SessionId;
use muster_core::protocol::MouseTracking;
use ratatui::layout::{Position, Rect};

use super::{Action, App};

/// Text selected, or being selected, in a session's screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Selection {
    pub(crate) session: SessionId,
    /// The cell the drag began on, as (row, column) of the screen as shown.
    anchor: (u16, u16),
    /// The cell the drag is on now, or ended on.
    head: (u16, u16),
    /// The button is still held.
    dragging: bool,
    /// The drag has left the cell it began on: a click selects nothing.
    moved: bool,
}

impl Selection {
    fn new(session: SessionId, at: (u16, u16)) -> Self {
        Self {
            session,
            anchor: at,
            head: at,
            dragging: true,
            moved: false,
        }
    }

    /// The first and last cell selected, in reading order.
    fn span(&self) -> ((u16, u16), (u16, u16)) {
        if self.anchor <= self.head {
            (self.anchor, self.head)
        } else {
            (self.head, self.anchor)
        }
    }

    /// Whether the cell at `row`, `column` of the screen is selected.
    pub(crate) fn contains(&self, row: u16, column: u16) -> bool {
        let (start, end) = self.span();
        self.moved && (start..=end).contains(&(row, column))
    }

    /// The selected text of `screen`, without the blanks that end its lines.
    pub(crate) fn text(&self, screen: &vt100::Screen) -> String {
        let (rows, cols) = screen.size();
        let last = (rows.saturating_sub(1), cols.saturating_sub(1));
        let ((start_row, start_col), (end_row, end_col)) = self.span();
        let (start, end) = (
            (start_row.min(last.0), start_col.min(last.1)),
            (end_row.min(last.0), end_col.min(last.1)),
        );
        screen
            .contents_between(start.0, start.1, end.0, end.1 + 1)
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n")
    }
}

/// The cell of `screen` (an area of the window) under `event`, kept inside
/// it: a drag may leave the session's pane.
fn cell(screen: Rect, event: &MouseEvent) -> (u16, u16) {
    let row = event
        .row
        .clamp(screen.y, screen.bottom().saturating_sub(1).max(screen.y));
    let column = event
        .column
        .clamp(screen.x, screen.right().saturating_sub(1).max(screen.x));
    (row - screen.y, column - screen.x)
}

impl App {
    /// The mouse selecting text in `session`'s screen, drawn in `screen`,
    /// if it is: a left press there (with Shift, if the program asked for
    /// the mouse, `tracked`), and the drag and release that follow it,
    /// wherever they are. `None` leaves the event to the rest.
    pub(super) fn select_with_mouse(
        &mut self,
        session: SessionId,
        event: &MouseEvent,
        screen: Rect,
        tracked: bool,
    ) -> Option<Vec<Action>> {
        let inside = screen.contains(Position::new(event.column, event.row));
        let held = self
            .selection
            .as_mut()
            .filter(|selection| selection.session == session && selection.dragging);
        match (event.kind, held) {
            (MouseEventKind::Drag(MouseButton::Left), Some(selection)) => {
                selection.head = cell(screen, event);
                selection.moved |= selection.head != selection.anchor;
                Some(Vec::new())
            }
            (MouseEventKind::Up(MouseButton::Left), Some(_)) => Some(self.copy_selection()),
            (MouseEventKind::Down(MouseButton::Left), _)
                if inside && (!tracked || event.modifiers.contains(KeyModifiers::SHIFT)) =>
            {
                self.selection = Some(Selection::new(session, cell(screen, event)));
                Some(Vec::new())
            }
            _ => None,
        }
    }

    /// A press on the list's preview opened `session`, now drawn in
    /// `screen`: a drag that follows selects from where it was pressed.
    pub(super) fn select_from_preview(
        &mut self,
        session: SessionId,
        event: &MouseEvent,
        screen: Rect,
    ) {
        let tracked = self
            .screens
            .get(&session)
            .is_some_and(|shown| shown.modes.mouse != MouseTracking::Off);
        if !tracked && screen.contains(Position::new(event.column, event.row)) {
            self.selection = Some(Selection::new(session, cell(screen, event)));
        }
    }

    /// The button let go: the selected text copied, and said, the cells
    /// staying marked until the next click, key or turn of the wheel.
    fn copy_selection(&mut self) -> Vec<Action> {
        let Some(mut selection) = self.selection.take() else {
            return Vec::new();
        };
        selection.dragging = false;
        let text = match self.screens.get(&selection.session) {
            Some(screen) if selection.moved => selection.text(screen.screen()),
            _ => return Vec::new(),
        };
        if text.trim().is_empty() {
            return Vec::new();
        }
        self.selection = Some(selection);
        match text.lines().count() {
            0 | 1 => self.say("Copied."),
            lines => self.say(format!("Copied {lines} lines.")),
        }
        vec![Action::Copy(text)]
    }
}

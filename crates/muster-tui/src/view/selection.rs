//! The text selected in a session's screen, drawn reversed, as terminals
//! mark a selection.

use muster_core::SessionId;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Modifier;

use crate::app::{App, Mode};

/// Marks what is selected of `session`'s screen, drawn in `area`, while it
/// has the keys.
pub(super) fn draw_selection(frame: &mut Frame<'_>, app: &App, session: SessionId, area: Rect) {
    let Some(selection) = app
        .selection
        .filter(|selection| selection.session == session)
    else {
        return;
    };
    if app.mode != Mode::Terminal(session) {
        return;
    }
    let buffer = frame.buffer_mut();
    for row in 0..area.height {
        for column in 0..area.width {
            if selection.contains(row, column) {
                buffer[(area.x + column, area.y + row)]
                    .modifier
                    .toggle(Modifier::REVERSED);
            }
        }
    }
}

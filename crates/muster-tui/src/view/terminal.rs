//! A session's terminal: beside the list, watched or typed into, and full
//! screen under a bar of its own.

use muster_core::protocol::DetachReason;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use tui_term::widget::PseudoTerminal;

use super::{look, panel, selection, status_symbol, usage_text};
use crate::app::App;

/// A session's terminal beside the list: watched there, or `typing` into
/// it, when it has the keys, its border lit and its cursor shown.
pub(super) fn draw_session(
    frame: &mut Frame<'_>,
    app: &App,
    session: muster_core::SessionId,
    area: Rect,
    typing: bool,
) {
    let Some(stored) = app.model.session(session) else {
        return;
    };
    let back = app
        .screens
        .get(&session)
        .map_or(0, crate::screen::Screen::scrolled_back);
    let mut title = format!(" {} · {} ", stored.title, look::doing(stored));
    if let Some(time) = look::session_time(app, stored) {
        title.push_str(&format!("· {time} "));
    }
    if let Some(usage) = app.model.usage(session) {
        title.push_str(&format!("· {} ", usage_text(usage)));
    }
    if back > 0 {
        title.push_str(&format!("· ↑ {back} lines back "));
    } else if typing {
        title.push_str("· Shift-Q back to the list ");
    }
    let block = panel(title, typing);
    match app.screens.get(&session) {
        Some(screen) if screen.ended == Some(DetachReason::NotRunning) => {
            frame.render_widget(
                Paragraph::new(ENDED_WITHOUT_OUTPUT)
                    .wrap(Wrap { trim: false })
                    .block(block)
                    .dim(),
                area,
            );
        }
        Some(screen) => {
            let inner = block.inner(area);
            frame.render_widget(PseudoTerminal::new(screen.screen()).block(block), area);
            if typing {
                selection::draw_selection(frame, app, session, inner);
                show_cursor(frame, screen, inner);
            }
        }
        None => frame.render_widget(Paragraph::new("Connecting…").block(block).dim(), area),
    }
}

/// Puts the person's cursor where the program's is, in `area`, unless the
/// program hides it or the person is looking back.
fn show_cursor(frame: &mut Frame<'_>, screen: &crate::screen::Screen, area: Rect) {
    if screen.screen().hide_cursor() || screen.scrolled_back() > 0 {
        return;
    }
    let (row, col) = screen.screen().cursor_position();
    frame.set_cursor_position(Position {
        x: area.x + col.min(area.width.saturating_sub(1)),
        y: area.y + row.min(area.height.saturating_sub(1)),
    });
}
/// What is shown for a session whose output the daemon no longer holds.
const ENDED_WITHOUT_OUTPUT: &str = "This session ended and its output is no longer kept \
     (the daemon was restarted). Press r to start it again, or x to remove it.";

pub(super) fn draw_terminal(
    frame: &mut Frame<'_>,
    app: &App,
    session: muster_core::SessionId,
    area: Rect,
) {
    let [top, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let mut spans = vec![Span::from(" Muster ").bold().reversed(), Span::raw(" ")];
    if let Some(stored) = app.model.session(session) {
        let (symbol, style) = status_symbol(stored.status, app.motion_step());
        spans.push(Span::styled(symbol, style));
        spans.push(Span::raw(format!(" {}  ", stored.title)));
        spans.push(Span::from(look::doing(stored).to_owned()).dim());
        if let Some(time) = look::session_time(app, stored) {
            spans.push(Span::from(format!(" · {time}")).dim());
        }
        if let Some(workspace) = app.model.workspace(stored.workspace) {
            let mark = if workspace.kind.is_checkout() {
                "⌂"
            } else {
                "⎇"
            };
            spans.push(Span::from(format!("  {mark} {}", workspace.kind.branch())).dim());
        }
    }
    let back = app
        .screens
        .get(&session)
        .map_or(0, crate::screen::Screen::scrolled_back);
    match &app.note {
        Some(note) if note.error => spans.push(Span::from(format!("   {}", note.text)).red()),
        Some(note) => spans.push(Span::from(format!("   {}", note.text)).green()),
        None if back > 0 => spans.push(
            Span::from(format!(
                "   ↑ {back} lines back · scroll down or type to return"
            ))
            .yellow(),
        ),
        None => spans.push(Span::from("   Shift-Q back to the list (or click here)").dim()),
    }
    frame.render_widget(Line::from(spans), top);
    match app.screens.get(&session) {
        Some(screen) if screen.ended == Some(DetachReason::NotRunning) => {
            frame.render_widget(
                Paragraph::new(ENDED_WITHOUT_OUTPUT)
                    .wrap(Wrap { trim: false })
                    .dim(),
                body,
            );
        }
        Some(screen) => {
            frame.render_widget(PseudoTerminal::new(screen.screen()), body);
            selection::draw_selection(frame, app, session, body);
            show_cursor(frame, screen, body);
        }
        None => frame.render_widget(Paragraph::new("Connecting…").dim(), body),
    }
}

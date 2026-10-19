//! A session's terminal: beside the list, watched or typed into, and full
//! screen under a bar of its own.

use muster_core::keys::Action as Does;
use muster_core::protocol::DetachReason;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};
use std::fmt::Write as _;
use tui_term::widget::PseudoTerminal;

use super::{look, mark, panel, selection, usage_text};
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
    // Its name stands out; what follows is quieter.
    let mut title = format!("· {} ", look::doing(stored));
    if let Some(time) = look::session_time(app, stored) {
        let _ = write!(title, "· {time} ");
    }
    if let Some(usage) = app.model.usage(session) {
        let _ = write!(title, "· {} ", usage_text(usage));
    }
    if let Some(spend) = &stored.spend {
        let _ = write!(title, "· {} ", spend.text());
    }
    if back > 0 {
        let _ = write!(title, "· ↑ {back} lines back ");
    } else if typing && !app.keymap.chords(Does::LeaveSession).is_empty() {
        let _ = write!(title, "· {} back to the list ", app.key(Does::LeaveSession));
    }
    let theme = super::theme::of(app);
    let name = if typing {
        Style::default().fg(theme.accent)
    } else {
        Style::default().fg(theme.text)
    };
    let title = Line::from(vec![
        Span::raw(" "),
        Span::styled(
            format!("{} ", stored.title),
            name.add_modifier(Modifier::BOLD)
                .remove_modifier(Modifier::DIM),
        ),
        Span::styled(title, theme.quiet),
    ]);
    let block = panel(app, title, typing);
    match app.screens.get(&session) {
        Some(screen) if screen.ended == Some(DetachReason::NotRunning) => {
            frame.render_widget(
                Paragraph::new(ended_without_output(app))
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
fn ended_without_output(app: &App) -> String {
    format!(
        "This session ended and its output is no longer kept (the daemon was restarted). Press \
         {} to start it again, or {} to remove it.",
        app.key(Does::RestartSession),
        app.key(Does::Close)
    )
}

pub(super) fn draw_terminal(
    frame: &mut Frame<'_>,
    app: &App,
    session: muster_core::SessionId,
    area: Rect,
) {
    let [top, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let mut spans = vec![super::top_bar::name(app), Span::raw(" ")];
    if let Some(stored) = app.model.session(session) {
        let (symbol, style) = mark(app, stored.status);
        spans.push(Span::styled(symbol, style));
        spans.push(Span::raw(format!(" {}  ", stored.title)));
        spans.push(Span::from(look::doing(stored).into_owned()).dim());
        if let Some(time) = look::session_time(app, stored) {
            spans.push(Span::from(format!(" · {time}")).dim());
        }
        if let Some(spend) = &stored.spend {
            spans.push(Span::from(format!(" · {}", spend.text())).dim());
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
        None => spans.push(
            Span::from(format!(
                "   {} back to the list (or click here)",
                app.key(Does::LeaveSession)
            ))
            .dim(),
        ),
    }
    frame.render_widget(Line::from(spans), top);
    match app.screens.get(&session) {
        Some(screen) if screen.ended == Some(DetachReason::NotRunning) => {
            frame.render_widget(
                Paragraph::new(ended_without_output(app))
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

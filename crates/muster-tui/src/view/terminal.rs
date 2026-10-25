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

use muster_core::layout::Panel;

use super::panel as framed;
use super::{BrowseAreas, look, mark, pane, selection, usage_text};
use crate::app::App;

/// A session's terminal beside the list, in `panel` (the preview, a
/// placed one's or a plugin's): watched there, or typed into while it has
/// the keys, its border lit and its cursor shown. Its title names it, and
/// where it works once the panels hold others; the preview says it is
/// one while terminals are placed beside it; the panel with the keys says
/// how many a window too narrow for them hides; and the buttons it offers
/// are at its end (`pane`).
pub(super) fn draw_session(
    frame: &mut Frame<'_>,
    app: &App,
    session: muster_core::SessionId,
    area: Rect,
    panel: Panel,
    areas: &BrowseAreas,
) {
    let Some(stored) = app.model.session(session) else {
        return;
    };
    let typing = app.mode == crate::app::Mode::Terminal(session);
    let others = !areas.placed.is_empty();
    let back = app
        .screens
        .get(&session)
        .map_or(0, crate::screen::Screen::scrolled_back);
    // Its name stands out; what follows is quieter.
    let mut title = String::new();
    if (matches!(panel, Panel::Placed(_)) || others)
        && let Some(place) = where_it_works(app, stored)
    {
        let _ = write!(title, "{place} ");
    }
    let _ = write!(title, "· {} ", look::doing(stored));
    if let Some(time) = look::session_time(app, stored) {
        let _ = write!(title, "· {time} ");
    }
    if let Some(usage) = app.model.usage(session) {
        let _ = write!(title, "· {} ", usage_text(usage));
    }
    if let Some(spend) = &stored.spend {
        let _ = write!(title, "· {} ", spend.text());
    }
    if areas.hidden > 0 && panel == app.panel_with_keys() {
        let _ = write!(title, "· +{} more ", areas.hidden);
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
    let mut spans = vec![Span::raw(" ")];
    // While the panel key waits, each panel's number, to go to it.
    if app.panels.waiting.is_some()
        && let Some(at) = app
            .numbered_panels()
            .iter()
            .position(|(numbered, _)| *numbered == panel)
    {
        spans.push(Span::styled(
            format!("{} ", at + 1),
            Style::default().fg(theme.accent).bold(),
        ));
    }
    if panel == Panel::Session && others {
        spans.push(Span::styled("preview · ", theme.quiet));
    }
    spans.push(Span::styled(
        format!("{} ", stored.title),
        name.add_modifier(Modifier::BOLD)
            .remove_modifier(Modifier::DIM),
    ));
    spans.push(Span::styled(title, theme.quiet));
    let mut block = framed(app, Line::from(spans), typing);
    let buttons: Vec<Span<'static>> = pane::button_areas(app, panel, area)
        .into_iter()
        .map(|(button, _)| Span::styled(format!(" {} ", button.mark()), theme.quiet))
        .collect();
    if !buttons.is_empty() {
        block = block.title(Line::from(buttons).right_aligned());
    }
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

/// Where `stored` works, for a title among others': its project and its
/// branch, or the project alone in its checkout of a plain folder.
fn where_it_works(app: &App, stored: &muster_core::Session) -> Option<String> {
    let workspace = app.model.workspace(stored.workspace)?;
    let project = app.model.project(workspace.project)?;
    Some(match workspace.kind.branch() {
        "" => project.name.clone(),
        branch => format!("{}/{branch}", project.name),
    })
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
    let theme = super::theme::of(app);
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
        Some(note) if note.error => {
            spans.push(Span::from(format!("   {}", note.text)).fg(theme.waiting));
        }
        Some(note) => spans.push(Span::from(format!("   {}", note.text)).fg(theme.done)),
        None if back > 0 => spans.push(
            Span::from(format!(
                "   ↑ {back} lines back · scroll down or type to return"
            ))
            .fg(theme.working),
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

//! The Tickets screen: each project's open tickets with the agents on
//! them, and the selected ticket's details and text.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{describe, panel, shorten, status_symbol};
use crate::app::{App, TicketRow, TicketText, Tickets};

/// The most agents shown on a ticket's line; the rest are counted.
const AGENTS_SHOWN: usize = 4;

pub(super) fn draw_tickets(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = panel(" Tickets ", app.list_has_keys());
    let rows = app.ticket_rows();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No projects yet: add one in the sessions view with a.")
                .wrap(Wrap { trim: false })
                .block(block)
                .dim(),
            area,
        );
        return;
    }
    let width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem<'_>> = rows
        .iter()
        .map(|row| ListItem::new(row_line(app, *row, width)))
        .collect();
    let mut state = ListState::default()
        .with_offset(app.tickets_offset)
        .with_selected(
            app.tickets_selected
                .and_then(|selected| rows.iter().position(|row| *row == selected)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

/// A project's heading with how its tickets stand, or a ticket with the
/// agents on it; the title gives way so the agents stay in view.
fn row_line(app: &App, row: TicketRow, width: usize) -> Line<'static> {
    match row {
        TicketRow::Project(project) => {
            let name = app
                .model
                .project(project)
                .map_or_else(|| "?".to_owned(), |found| found.name.clone());
            let state = match app.tickets.get(&project) {
                None | Some(Tickets::Loading) => Span::from("  fetching…").dim(),
                Some(Tickets::Loaded(tickets)) if tickets.is_empty() => {
                    Span::from("  no open tickets").dim()
                }
                Some(Tickets::Loaded(tickets)) => {
                    Span::from(format!("  {} open", tickets.len())).dim()
                }
                Some(Tickets::Failed(error)) => {
                    Span::from(format!("  {}", shorten(error, width.saturating_sub(4)))).red()
                }
            };
            Line::from(vec![Span::from(name).bold(), state])
        }
        TicketRow::Ticket(project, number) => {
            let Some(ticket) = app.ticket(project, number) else {
                return Line::from(format!("  #{number}"));
            };
            let agents = app.model.agents_on(project, number);
            let mut marks: Vec<Span<'static>> = agents
                .iter()
                .take(AGENTS_SHOWN)
                .map(|session| {
                    let (symbol, style) = status_symbol(session.status);
                    Span::styled(format!(" {symbol}"), style)
                })
                .collect();
            if agents.len() > AGENTS_SHOWN {
                marks.push(Span::from(format!(" +{}", agents.len() - AGENTS_SHOWN)).dim());
            }
            let marks_width: usize = marks.iter().map(|mark| mark.content.chars().count()).sum();
            let number_text = format!("  #{number} ");
            let room = width
                .saturating_sub(number_text.chars().count() + marks_width + 1)
                .max(8);
            let mut spans = vec![
                Span::from(number_text).dim(),
                Span::raw(shorten(&ticket.title, room)),
            ];
            spans.extend(marks);
            Line::from(spans)
        }
    }
}

/// The selected ticket, or the selected project's tickets as a whole.
pub(super) fn draw_ticket_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = match app.tickets_selected {
        Some(TicketRow::Ticket(project, number)) => ticket_text(app, project, number),
        Some(TicketRow::Project(project)) => project_text(app, project),
        None => Text::from("Select a ticket to see it here.").dim(),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(panel(" Ticket ", false)),
        area,
    );
}

fn project_text(app: &App, project: muster_core::ProjectId) -> Text<'static> {
    let Some(found) = app.model.project(project) else {
        return Text::default();
    };
    let mut lines = vec![
        Line::from(found.name.clone()).bold(),
        Line::from(found.root.display().to_string()).dim(),
        Line::from(""),
    ];
    match app.tickets.get(&project) {
        Some(Tickets::Failed(error)) => {
            lines.push(Line::from("Its tickets could not be fetched:").red());
            lines.push(Line::from(error.clone()));
            lines.push(Line::from(""));
            lines.push(
                Line::from("Tickets come from its GitHub issues through `gh`: is `gh` signed in?")
                    .dim(),
            );
        }
        Some(Tickets::Loaded(_)) | Some(Tickets::Loading) | None => {
            lines.push(
                Line::from("Its open GitHub issues, newest first. ⏎ on one starts an agent on it.")
                    .dim(),
            );
        }
    }
    Text::from(lines)
}

fn ticket_text(app: &App, project: muster_core::ProjectId, number: u64) -> Text<'static> {
    let Some(ticket) = app.ticket(project, number) else {
        return Text::default();
    };
    let project_name = app
        .model
        .project(project)
        .map_or_else(|| "?".to_owned(), |found| found.name.clone());
    let mut lines = vec![
        Line::from(ticket.title.clone()).bold(),
        Line::from(format!("#{number} · {project_name} · {}", ticket.url)).dim(),
    ];
    let mut about = Vec::new();
    if let Some(author) = &ticket.author {
        about.push(Span::from(format!("by {author}")).dim());
    }
    if !ticket.labels.is_empty() {
        if !about.is_empty() {
            about.push(Span::from(" · ").dim());
        }
        about.push(Span::from(ticket.labels.join(", ")).cyan());
    }
    if !about.is_empty() {
        lines.push(Line::from(about));
    }
    lines.push(Line::from(""));

    let agents = app.model.agents_on(project, number);
    if agents.is_empty() {
        lines.push(
            Line::from("No agent on it yet: ⏎ starts Claude Code on it, in a worktree of its own.")
                .dim(),
        );
    } else {
        lines.push(Line::from("Agents").bold());
        for session in &agents {
            // Their titles all name the ticket: what tells them apart is
            // how each is doing and where it works.
            let (symbol, style) = status_symbol(session.status);
            let branch = app
                .model
                .branch(session.workspace)
                .unwrap_or("?")
                .to_owned();
            lines.push(Line::from(vec![
                Span::styled(format!(" {symbol} "), style),
                Span::raw(describe(session.status)),
                Span::from(format!("  {branch}")).dim(),
            ]));
        }
        lines.push(
            Line::from("⏎ opens the latest; a starts another, in a worktree of its own.").dim(),
        );
    }
    lines.push(Line::from(""));
    match app.ticket_texts.get(&(project, number)) {
        None | Some(TicketText::Loading) => lines.push(Line::from("Fetching its text…").dim()),
        Some(TicketText::Failed(error)) => {
            lines.push(Line::from(format!("Its text could not be fetched: {error}")).red());
        }
        Some(TicketText::Loaded(body)) if body.trim().is_empty() => {
            lines.push(Line::from("(No description.)").dim());
        }
        Some(TicketText::Loaded(body)) => {
            lines.extend(body.lines().map(|line| Line::from(line.to_owned())));
        }
    }
    Text::from(lines)
}

/// The keys that apply on the Tickets screen, for what is selected.
pub(super) fn hints(app: &App) -> Vec<&'static str> {
    let mut hints = Vec::new();
    if let Some(TicketRow::Ticket(project, number)) = app.tickets_selected {
        if app.model.agents_on(project, number).is_empty() {
            hints.push("⏎ start an agent");
        } else {
            hints.extend(["⏎ open agent", "a another agent"]);
        }
        hints.extend(["b browser", "x close on GitHub"]);
    }
    hints.push("r refresh");
    hints
}

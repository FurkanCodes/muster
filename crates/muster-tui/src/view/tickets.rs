//! The Tickets screen: each project's open tickets with the agents on
//! them, and the selected ticket's details and text.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{describe, look, panel, pull_request, shorten, status_symbol};
use crate::app::{App, HarnessChoice, TicketRow, TicketText, Tickets};

/// The most agents shown on a ticket's line; the rest are counted.
const AGENTS_SHOWN: usize = 4;

/// The least of a ticket's title its line keeps to show its pull request.
const TITLE_KEPT: usize = 12;

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
        .map(|row| ListItem::new(row_line(app, row, width)))
        .collect();
    let mut state = ListState::default()
        .with_offset(app.tickets_offset)
        .with_selected(
            app.tickets_selected
                .as_ref()
                .and_then(|selected| rows.iter().position(|row| row == selected)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(look::selected_style(app.list_has_keys()));
    frame.render_stateful_widget(list, area, &mut state);
}

/// A project's heading with how its tickets stand, or a ticket with the
/// agents on it; the title gives way so the agents stay in view.
fn row_line(app: &App, row: &TicketRow, width: usize) -> Line<'static> {
    let project = match row {
        TicketRow::Project(project) | TicketRow::Ticket(project, _) => *project,
    };
    let edge = look::edge(
        app,
        Some(project),
        app.tickets_selected.as_ref() == Some(row),
    );
    let width = width.saturating_sub(1);
    let mut line = row_text(app, row, width);
    line.spans.insert(0, edge);
    line
}

/// A row's line after its edge.
fn row_text(app: &App, row: &TicketRow, width: usize) -> Line<'static> {
    match row {
        TicketRow::Project(project) => {
            let project = *project;
            let name = app
                .model
                .project(project)
                .map_or_else(|| "?".to_owned(), |found| found.name.clone());
            let state = match app.tickets.get(&project) {
                None | Some(Tickets::Loading) => Span::from("  fetching…").dim(),
                Some(Tickets::Loaded(tickets, _, mine)) if tickets.is_empty() => {
                    Span::from(if *mine {
                        "  none of yours open"
                    } else {
                        "  no open tickets"
                    })
                    .dim()
                }
                Some(Tickets::Loaded(tickets, _, mine)) => {
                    let yours = if *mine { " of yours" } else { "" };
                    Span::from(format!("  {}{yours} open", tickets.len())).dim()
                }
                Some(Tickets::Failed(error)) => {
                    Span::from(format!("  {}", shorten(error, width.saturating_sub(4)))).red()
                }
            };
            let colour = look::project_colour(app, project);
            Line::from(vec![
                Span::styled(name, Style::default().fg(colour)).bold(),
                state,
            ])
        }
        TicketRow::Ticket(project, key) => {
            let project = *project;
            let Some(ticket) = app.ticket(project, key) else {
                return Line::from(format!("  {}", key.shown()));
            };
            let agents = app.model.agents_on(project, key);
            let mut marks: Vec<Span<'static>> = agents
                .iter()
                .take(AGENTS_SHOWN)
                .flat_map(|session| {
                    // Which agent, then how it is doing.
                    let (symbol, style) = status_symbol(session.status, app.motion_step());
                    [
                        Span::from(format!(" {}", look::harness_letter(&session.harness))).dim(),
                        Span::styled(symbol, style),
                    ]
                })
                .collect();
            if agents.len() > AGENTS_SHOWN {
                marks.push(Span::from(format!(" +{}", agents.len() - AGENTS_SHOWN)).dim());
            }
            let number_text = format!("  {} ", key.shown());
            let room_beside = |marks: &[Span<'_>]| {
                let marks_width: usize =
                    marks.iter().map(|mark| mark.content.chars().count()).sum();
                width.saturating_sub(number_text.chars().count() + marks_width + 1)
            };
            // Its pull request where the title keeps a readable length;
            // the ticket's details show it anyway.
            if let Some(found) = app.model.ticket_pull_request(project, key) {
                let mut with = marks.clone();
                with.push(Span::raw("  "));
                with.extend(pull_request::badge(found, true));
                if room_beside(&with) >= TITLE_KEPT {
                    marks = with;
                }
            }
            let room = room_beside(&marks).max(8);
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
    let text = match &app.tickets_selected {
        Some(TicketRow::Ticket(project, key)) => ticket_text(app, *project, key),
        Some(TicketRow::Project(project)) => project_text(app, *project),
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
                Line::from(
                    "Muster tries again when you come back to this screen or to this window; \
                     r tries now.",
                )
                .dim(),
            );
        }
        Some(Tickets::Loaded(_, tracker, mine)) => {
            let whose = if *mine { " assigned to you" } else { "" };
            lines.push(
                Line::from(format!(
                    "Its open tickets{whose} from {tracker}, newest first. ⏎ on one assigns an agent to it."
                ))
                .dim(),
            );
            let change = if *mine {
                "Settings (4) can list every open ticket instead."
            } else {
                "Settings (4) can list only those assigned to you."
            };
            lines.push(Line::from(change).dim());
        }
        Some(Tickets::Loading) | None => {
            lines.push(Line::from("Fetching its tickets…").dim());
        }
    }
    Text::from(lines)
}

fn ticket_text(
    app: &App,
    project: muster_core::ProjectId,
    key: &muster_core::TicketKey,
) -> Text<'static> {
    let Some(ticket) = app.ticket(project, key) else {
        return Text::default();
    };
    let project_name = app
        .model
        .project(project)
        .map_or_else(|| "?".to_owned(), |found| found.name.clone());
    let mut lines = vec![
        Line::from(ticket.title.clone()).bold(),
        Line::from(format!("{} · {project_name} · {}", key.shown(), ticket.url)).dim(),
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

    let agents = app.model.agents_on(project, key);
    if agents.is_empty() {
        lines.push(
            Line::from(
                "No agent on it yet: ⏎ chooses one to start on it, in a worktree of its own.",
            )
            .dim(),
        );
    } else {
        let spent =
            muster_core::Spend::total(agents.iter().filter_map(|agent| agent.spend.as_ref()));
        let mut heading = vec![Span::from("Agents").bold()];
        if let Some(spent) = spent {
            heading.push(Span::from(format!(" · {}", spent.text())).dim());
        }
        lines.push(Line::from(heading));
        for session in &agents {
            // Their titles all name the ticket: what tells them apart is
            // which agent each is, how it is doing and where it works.
            let (symbol, style) = status_symbol(session.status, app.motion_step());
            let branch = app
                .model
                .branch(session.workspace)
                .unwrap_or("?")
                .to_owned();
            let mut spans = vec![
                Span::styled(format!(" {symbol} "), style),
                Span::from(format!("{}  ", agent_name(&session.harness))).bold(),
                Span::raw(describe(session.status)),
            ];
            if let Some(spend) = &session.spend {
                spans.push(Span::from(format!("  {}", spend.text())).dim());
            }
            spans.push(Span::from(format!("  {branch}")).dim());
            if let Some(found) = app
                .model
                .workspace(session.workspace)
                .and_then(|workspace| workspace.pull_request.as_ref())
            {
                spans.push(Span::raw("  "));
                spans.extend(pull_request::badge(found, false));
            }
            lines.push(Line::from(spans));
        }
        lines.push(
            Line::from("⏎ opens the latest; a starts another, in a worktree of its own.").dim(),
        );
    }
    lines.push(Line::from(""));
    match app.ticket_texts.get(&(project, key.clone())) {
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
    if let Some(TicketRow::Ticket(project, key)) = &app.tickets_selected {
        if app.model.agents_on(*project, key).is_empty() {
            hints.push("⏎ start an agent");
        } else {
            hints.extend(["⏎ open agent", "a another agent"]);
        }
        hints.extend(["b browser", "x close"]);
    }
    hints.push("r refresh");
    hints
}

/// An agent's name as the person knows it: `Codex`, not `codex`.
fn agent_name(harness: &str) -> &str {
    match HarnessChoice::from_id(harness) {
        Some(choice) => choice.label(),
        None => harness,
    }
}

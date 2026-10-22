//! The Tickets screen: each project's open tickets with the agents on
//! them, and the selected ticket's details and text. Projects sharing
//! their tickets have one heading, their names in their colours.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{describe, list_block, loading, look, mark, markdown, panel, pull_request, shorten};
use crate::app::{App, HarnessChoice, TicketRow, TicketText, Tickets};

/// The most agents shown on a ticket's line; the rest are counted.
const AGENTS_SHOWN: usize = 4;

/// The least of a ticket's title its line keeps to show its pull request.
const TITLE_KEPT: usize = 12;

pub(super) fn draw_tickets(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = list_block(app, "tickets", app.list_has_keys());
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
        .highlight_style(look::selected_style(app, app.list_has_keys()));
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
            let mut spans = names(app, project);
            let state = match app.tickets.get(&project) {
                None | Some(Tickets::Loading) => {
                    // The rest of its heading is the wait.
                    let named: usize = spans.iter().map(|span| span.content.chars().count()).sum();
                    spans.push(Span::raw(" "));
                    spans.extend(
                        loading::rule(app, "Loading tickets", width.saturating_sub(named + 1))
                            .spans,
                    );
                    return Line::from(spans);
                }
                Some(Tickets::Loaded { tickets, mine, .. }) if tickets.is_empty() => {
                    Span::from(if *mine {
                        "  none of yours open"
                    } else {
                        "  no open tickets"
                    })
                    .dim()
                }
                Some(Tickets::Loaded { tickets, mine, .. }) => {
                    let yours = if *mine { " of yours" } else { "" };
                    Span::from(format!("  {}{yours} open", tickets.len())).dim()
                }
                Some(Tickets::Failed(error)) => {
                    Span::from(format!("  {}", shorten(error, width.saturating_sub(4)))).red()
                }
            };
            spans.push(state);
            Line::from(spans)
        }
        TicketRow::Ticket(project, key) => {
            let project = *project;
            let Some(ticket) = app.ticket(project, key) else {
                return Line::from(format!("  {}", key.shown()));
            };
            let agents = app.ticket_agents(project, key);
            let mut marks: Vec<Span<'static>> = agents
                .iter()
                .take(AGENTS_SHOWN)
                .flat_map(|session| {
                    // Which agent, then how it is doing.
                    let (symbol, style) = mark(app, session.status);
                    [
                        look::agent_mark(app, &session.harness),
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
            if let Some(found) = app.model.ticket_pull_request(&app.sharing(project), key) {
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

/// The selected ticket, or the selected project's tickets as a whole,
/// scrolled as far as the person paged (`App::ticket_scrolled`).
pub(super) fn draw_ticket_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let width = usize::from(area.width.saturating_sub(2));
    let height = usize::from(area.height.saturating_sub(2));
    let text = details_text(app, width);
    let last = wrapped_height(&text, width).saturating_sub(height);
    let scrolled = app.ticket_scrolled().min(last);
    let mut block = panel(app, " Ticket ", false);
    if last > 0 {
        let more = if scrolled < last {
            " PgDn more "
        } else {
            " PgUp back "
        };
        block = block.title_bottom(Line::from(more).right_aligned().dim());
    }
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .scroll((u16::try_from(scrolled).unwrap_or(u16::MAX), 0))
            .block(block),
        area,
    );
}

/// How many lines the selected ticket's details take, wrapped at `width`.
pub(crate) fn details_height(app: &App, width: usize) -> usize {
    wrapped_height(&details_text(app, width), width)
}

fn details_text(app: &App, width: usize) -> Text<'static> {
    match &app.tickets_selected {
        Some(TicketRow::Ticket(project, key)) => ticket_text(app, *project, key, width),
        Some(TicketRow::Project(project)) => project_text(app, *project, width),
        None => Text::from("Select a ticket to see it here.").dim(),
    }
}

/// Lines `text` takes wrapped at `width`, a line its words' worth: what
/// ratatui's wrapping breaks at a word may take a line more, so this is
/// counted as the words fall.
fn wrapped_height(text: &Text<'_>, width: usize) -> usize {
    let width = width.max(1);
    text.lines
        .iter()
        .map(|line| {
            let mut rows = 1;
            let mut used = 0;
            let content: String = line
                .spans
                .iter()
                .map(|span| span.content.as_ref())
                .collect();
            for word in content.split_inclusive(' ') {
                let len = word.chars().count();
                if used + len.min(width) > width && used > 0 {
                    rows += 1;
                    used = 0;
                }
                used += len;
                while used > width {
                    rows += 1;
                    used -= width;
                }
            }
            rows
        })
        .sum()
}

/// The names of the projects sharing `project`'s tickets, each in its
/// colour.
fn names(app: &App, project: muster_core::ProjectId) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    for (at, sharing) in app.sharing(project).into_iter().enumerate() {
        if at > 0 {
            spans.push(Span::from(" · ").dim());
        }
        let name = app
            .model
            .project(sharing)
            .map_or_else(|| "?".to_owned(), |found| found.name.clone());
        let colour = look::project_colour(app, sharing);
        spans.push(Span::styled(name, Style::default().fg(colour)).bold());
    }
    spans
}

fn project_text(app: &App, project: muster_core::ProjectId, width: usize) -> Text<'static> {
    let sharing = app.sharing(project);
    let mut lines = Vec::new();
    for found in sharing.iter().filter_map(|id| app.model.project(*id)) {
        lines.push(Line::from(found.name.clone()).bold());
        lines.push(Line::from(found.root.display().to_string()).dim());
    }
    if lines.is_empty() {
        return Text::default();
    }
    lines.push(Line::from(""));
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
        Some(Tickets::Loaded { tracker, mine, .. }) => {
            let whose = if *mine { " assigned to you" } else { "" };
            if sharing.len() > 1 {
                lines.push(
                    Line::from(format!(
                        "Their open tickets{whose} from {tracker}, newest first: these projects \
                         list the same ones. ⏎ on one asks which of them it is for, and \
                         assigns an agent to it there."
                    ))
                    .dim(),
                );
            } else {
                lines.push(
                    Line::from(format!(
                        "Its open tickets{whose} from {tracker}, newest first. ⏎ on one assigns an agent to it."
                    ))
                    .dim(),
                );
            }
            let change = if *mine {
                "Settings (4) can list every open ticket instead."
            } else {
                "Settings (4) can list only those assigned to you."
            };
            lines.push(Line::from(change).dim());
        }
        Some(Tickets::Loading) | None => {
            lines.push(loading::rule(app, "Loading tickets", width));
        }
    }
    Text::from(lines)
}

#[expect(
    clippy::too_many_lines,
    reason = "a ticket's whole view, top to bottom"
)]
fn ticket_text(
    app: &App,
    project: muster_core::ProjectId,
    key: &muster_core::TicketKey,
    width: usize,
) -> Text<'static> {
    let Some(ticket) = app.ticket(project, key) else {
        return Text::default();
    };
    let sharing = app.sharing(project);
    let project_names: Vec<&str> = sharing
        .iter()
        .filter_map(|id| app.model.project(*id))
        .map(|found| found.name.as_str())
        .collect();
    // A plugin's ticket may have no web address.
    let mut said = vec![key.shown(), project_names.join(", ")];
    if !ticket.url.is_empty() {
        said.push(ticket.url.clone());
    }
    let mut lines = vec![
        Line::from(ticket.title.clone()).bold(),
        Line::from(said.join(" · ")).dim(),
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

    let agents = app.ticket_agents(project, key);
    if agents.is_empty() {
        let first = if sharing.len() > 1 {
            "No agent on it yet: ⏎ chooses which project it is for and an agent to start on it \
             there, in a worktree of its own."
        } else {
            "No agent on it yet: ⏎ chooses one to start on it, in a worktree of its own."
        };
        lines.push(Line::from(first).dim());
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
            let (symbol, style) = mark(app, session.status);
            let branch = app
                .model
                .branch(session.workspace)
                .unwrap_or("?")
                .to_owned();
            let mut spans = vec![
                Span::styled(format!(" {symbol} "), style),
                Span::from(format!("{}  ", agent_name(app, &session.harness))).bold(),
                Span::raw(describe(session.status)),
            ];
            if let Some(spend) = &session.spend {
                spans.push(Span::from(format!("  {}", spend.text())).dim());
            }
            if sharing.len() > 1
                && let Some(workspace) = app.model.workspace(session.workspace)
                && let Some(found) = app.model.project(workspace.project)
            {
                // Which of the projects sharing the ticket it works in.
                let colour = look::project_colour(app, found.id);
                spans.push(Span::styled(
                    format!("  {}", found.name),
                    Style::default().fg(colour),
                ));
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
        let another = app.key(muster_core::keys::Action::AnotherAgent);
        let another = if sharing.len() > 1 {
            format!("⏎ opens the latest; {another} starts another, in the project you choose.")
        } else {
            format!("⏎ opens the latest; {another} starts another, in a worktree of its own.")
        };
        lines.push(Line::from(another).dim());
    }
    lines.push(Line::from(""));
    match app.ticket_texts.get(&(project, key.clone())) {
        None | Some(TicketText::Loading) => lines.push(loading::rule(app, "Loading ticket", width)),
        Some(TicketText::Failed(error)) => {
            lines.push(Line::from(format!("Its text could not be fetched: {error}")).red());
        }
        Some(TicketText::Loaded { body, comments }) => {
            let theme = super::theme::of(app);
            if body.trim().is_empty() {
                lines.push(Line::from("(No description.)").dim());
            } else {
                lines.extend(markdown::lines(body, &theme));
            }
            conversation(app, comments, width, &mut lines);
        }
    }
    Text::from(lines)
}

/// What was said on a ticket after it was filed, oldest first: each
/// comment under who wrote it and when, its text drawn as the ticket's is.
fn conversation(
    app: &App,
    comments: &[muster_core::IssueComment],
    width: usize,
    lines: &mut Vec<Line<'static>>,
) {
    if comments.is_empty() {
        return;
    }
    let theme = super::theme::of(app);
    lines.push(Line::from(""));
    let count = match comments.len() {
        1 => "1 comment".to_owned(),
        many => format!("{many} comments"),
    };
    let heading = format!("── {count} ");
    let rest = width.saturating_sub(heading.chars().count());
    lines.push(Line::from(vec![
        Span::styled(heading, theme.quiet),
        Span::styled("─".repeat(rest), Style::default().fg(theme.line)),
    ]));
    for comment in comments {
        lines.push(Line::from(""));
        let mut said = vec![
            Span::from(
                comment
                    .author
                    .clone()
                    .unwrap_or_else(|| "someone".to_owned()),
            )
            .bold(),
        ];
        if let (Some(at), Some(now)) = (comment.at, app.now) {
            let ago = muster_core::time::between(at, now);
            said.push(Span::styled(
                format!(" · {} ago", look::short_span(ago, false)),
                theme.quiet,
            ));
        }
        lines.push(Line::from(said));
        lines.extend(markdown::lines(&comment.body, &theme));
    }
}

/// The keys that apply on the Tickets screen, for what is selected.
pub(super) fn hints(app: &App) -> Vec<String> {
    use super::bottom_bar::hint;
    use muster_core::keys::Action as Does;
    let mut hints = Vec::new();
    if let Some(TicketRow::Ticket(project, key)) = &app.tickets_selected {
        if app.ticket_agents(*project, key).is_empty() {
            hints.push("⏎ start an agent".to_owned());
        } else {
            hints.push("⏎ open agent".to_owned());
            hints.extend(hint(app, Does::AnotherAgent, "another agent"));
        }
        if app
            .ticket(*project, key)
            .is_some_and(|ticket| !ticket.url.is_empty())
        {
            hints.extend(hint(app, Does::TicketInBrowser, "browser"));
        }
        hints.extend(hint(app, Does::CloseTicket, "close"));
    }
    hints.extend(hint(app, Does::RefreshTickets, "refresh"));
    hints
}

/// An agent's name as the person knows it: `Codex`, not `codex`.
fn agent_name(app: &App, harness: &str) -> String {
    match HarnessChoice::from_id(harness, app.added_agents()) {
        Some(choice) => choice.label().to_owned(),
        None => harness.to_owned(),
    }
}

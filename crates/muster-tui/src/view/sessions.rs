//! The Sessions and Needs you lists: projects, their worktrees and their
//! sessions' lines, and the queue of what waits on the person.

use muster_core::{Session, SessionStatus, WorkspaceKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{changes, look, panel, pull_request, shorten, status_symbol};
use crate::app::{App, View};
use crate::model::Row;
use crate::motion;

pub(super) fn draw_sessions(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = app.model.rows();
    let block = panel(" Sessions ", app.list_has_keys());
    if !app.model.loaded() {
        frame.render_widget(
            Paragraph::new("Waiting for the daemon…").block(block).dim(),
            area,
        );
        return;
    }
    if rows.is_empty() {
        let text = Text::from(vec![
            Line::from("No projects yet."),
            Line::from(""),
            Line::from("Press a to add a project: a git repository, or any folder."),
        ]);
        frame.render_widget(Paragraph::new(text).block(block).dim(), area);
        return;
    }
    let width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem<'_>> = rows.iter().map(|row| row_item(app, *row, width)).collect();
    let mut state = ListState::default()
        .with_offset(app.list_offset)
        .with_selected(app.selected.and_then(|s| rows.iter().position(|r| *r == s)));
    let selected = match app.selected {
        Some(Row::Session(session)) => Some(session),
        _ => None,
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(session_highlight(app, selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn row_item(app: &App, row: Row, width: usize) -> ListItem<'_> {
    let selected = app.view == View::Sessions && app.selected == Some(row);
    let edge = look::edge(app, app.model.project_of(row), selected);
    match row {
        Row::Project(id) => {
            let name = app
                .model
                .project(id)
                .map_or("?", |project| project.name.as_str());
            ListItem::new(Line::from(vec![
                edge,
                Span::styled(
                    name.to_owned(),
                    Style::default()
                        .fg(look::project_colour(app, id))
                        .add_modifier(Modifier::BOLD),
                ),
            ]))
        }
        Row::Worktree(id) => {
            let branch = app.model.branch(id).unwrap_or("?").to_owned();
            let kind = app.model.workspace(id).map(|workspace| &workspace.kind);
            let mut spans = vec![
                edge,
                Span::raw(" "),
                Span::from(match kind {
                    Some(WorkspaceKind::Checkout { .. }) => "⌂ ",
                    Some(WorkspaceKind::Container { .. }) => "⧉ ",
                    _ => "⎇ ",
                })
                .dim(),
            ];
            if kind.is_some_and(|kind| !kind.in_git()) {
                spans.push(Span::from("not a git repository").dim());
            } else if let Some(run) = app.model.run_of(id) {
                // The stage first: the branch is long, and says less.
                spans.push(Span::raw(format!("#{} ", run.ticket.number)));
                spans.push(Span::from(run.stage.label()).dim());
                spans.push(Span::raw(format!("  {branch}")));
            } else {
                let lead = pull_request::width(&spans);
                let workspace = app.model.workspace(id);
                let found = workspace.and_then(|workspace| workspace.pull_request.as_ref());
                let summary = workspace.and_then(|workspace| workspace.changes.as_ref());
                spans.extend(branch_and_pull_request(
                    branch,
                    summary,
                    found,
                    width.saturating_sub(lead),
                ));
            }
            ListItem::new(Line::from(spans))
        }
        Row::Session(id) => match app.model.session(id) {
            Some(session) => {
                let line = session_line(app, session, vec![edge, Span::raw("   ")], width, false);
                ListItem::new(flashed(app, session, line))
            }
            None => ListItem::new(""),
        },
    }
}

/// A worktree's branch and, at the line's end, what its work has changed
/// and the pull request from it, in `room` cells: short of room, the pull
/// request's word goes first, then what changed, then the branch is cut
/// short.
fn branch_and_pull_request(
    branch: String,
    summary: Option<&muster_core::ChangeSummary>,
    found: Option<&muster_core::PullRequestStatus>,
    room: usize,
) -> Vec<Span<'static>> {
    let fits = |tail: &[Span<'_>]| {
        tail.is_empty() || branch.chars().count() + 1 + pull_request::width(tail) <= room
    };
    let tail = |summary: Option<&muster_core::ChangeSummary>, compact: bool| {
        let mut tail: Vec<Span<'static>> = summary.map(changes::summary).unwrap_or_default();
        if let Some(found) = found {
            if !tail.is_empty() {
                tail.push(Span::raw(" "));
            }
            tail.extend(pull_request::badge(found, compact));
        }
        tail
    };
    let mut chosen = tail(summary, false);
    for (summary, compact) in [(summary, true), (None, false), (None, true)] {
        if fits(&chosen) {
            break;
        }
        chosen = tail(summary, compact);
    }
    if chosen.is_empty() {
        return vec![Span::raw(branch)];
    }
    let tail_width = pull_request::width(&chosen);
    let branch = shorten(&branch, room.saturating_sub(tail_width + 1).max(6));
    let gap = room
        .saturating_sub(branch.chars().count() + tail_width)
        .max(1);
    let mut spans = vec![Span::raw(branch), Span::raw(" ".repeat(gap))];
    spans.extend(chosen);
    spans
}

/// The fewest characters of a session's title a list line keeps before it
/// leaves out the session's usage.
const READABLE_TITLE: usize = 12;

/// The most of a project's name a session's line gives it.
const PROJECT_NAME_ROOM: usize = 20;
/// A session's line after `lead`: its mark, its title and what it is
/// doing (with `place`, also its project, before the title in the
/// project's colour, and its branch), and at the end how long it has been
/// so. Where it is short of room the title gives way first, down to a
/// readable length; then a finished or ended session's time goes, then
/// what it is doing, which is cut short only while it works or waits (a
/// status alone says no more than its mark). The mark always stays, and a
/// working or waiting session's time, and its project's name.
fn session_line(
    app: &App,
    session: &Session,
    lead: Vec<Span<'static>>,
    width: usize,
    place: bool,
) -> Line<'static> {
    let (symbol, style) = status_symbol(session.status, app.motion_step());
    let live = matches!(
        session.status,
        SessionStatus::Running | SessionStatus::NeedsYou { .. }
    );
    let unseen = if session.unseen { " •" } else { "" };
    let mut doing = look::doing(session).to_owned();
    let mut project = Vec::new();
    if place {
        let branch = app.model.branch(session.workspace).unwrap_or("?");
        doing = format!("{doing} · {branch}");
        if let Some(id) = app.model.project_of(Row::Session(session.id))
            && let Some(found) = app.model.project(id)
        {
            project = vec![
                Span::styled(
                    shorten(&found.name, PROJECT_NAME_ROOM),
                    Style::default()
                        .fg(look::project_colour(app, id))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw("  "),
            ];
        }
    }
    let lead_width: usize = lead.iter().chain(&project).map(Span::width).sum();
    let fixed = lead_width + 2 + unseen.chars().count();
    let title_least = session.title.chars().count().min(READABLE_TITLE);
    let wanted = title_least + 2 + doing.chars().count();
    let mut time = look::session_time(app, session).unwrap_or_default();
    let time_width = |time: &str| {
        if time.is_empty() {
            0
        } else {
            time.chars().count() + 1
        }
    };
    if !live && width.saturating_sub(fixed + time_width(&time)) < wanted {
        time.clear();
    }
    let room = width.saturating_sub(fixed + time_width(&time));
    let doing_room = room.saturating_sub(title_least + 2);
    let doing = if doing.chars().count() <= doing_room {
        doing
    } else if live && doing_room >= 8 {
        shorten(&doing, doing_room)
    } else {
        String::new()
    };
    let doing_width = if doing.is_empty() {
        0
    } else {
        doing.chars().count() + 2
    };
    let title = shorten(&session.title, room.saturating_sub(doing_width).max(4));
    let doing_style = match (&session.activity, session.status) {
        (_, SessionStatus::NeedsYou { .. }) => Style::default().fg(Color::Red),
        (Some(_), SessionStatus::Running) => Style::default(),
        _ => Style::default().add_modifier(Modifier::DIM),
    };
    let mut spans = lead;
    spans.extend([Span::styled(symbol, style), Span::raw(" ")]);
    spans.extend(project);
    spans.push(Span::raw(title));
    if !doing.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(doing, doing_style));
    }
    if session.unseen {
        spans.push(Span::from(unseen).cyan().bold());
    }
    if !time.is_empty() {
        let used: usize = spans.iter().map(Span::width).sum();
        let gap = width.saturating_sub(used + time.chars().count()).max(1);
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::from(time).dim());
    }
    Line::from(spans)
}

/// The selected line's look in a list of sessions: none while that line
/// is lit by its flash, which would be hidden under it.
fn session_highlight(app: &App, selected: Option<muster_core::SessionId>) -> Style {
    let lit = selected
        .and_then(|session| app.flash_age(session))
        .is_some_and(|age| age < crate::motion::FLASH_STEPS / 2);
    if lit {
        Style::default()
    } else {
        look::selected_style(app.list_has_keys())
    }
}

/// `line`, flashing if `session`'s status has just changed: lit in the
/// status's colour, then tinted with it, then as it was.
fn flashed<'a>(app: &App, session: &Session, mut line: Line<'a>) -> Line<'a> {
    let Some(age) = app.flash_age(session.id) else {
        return line;
    };
    let colour = status_symbol(session.status, None)
        .1
        .fg
        .unwrap_or(Color::White);
    match motion::flash(colour, age) {
        Some(motion::Flash::Lit(style)) => {
            for span in &mut line.spans {
                span.style = Style::default();
            }
            line.style = style;
        }
        Some(motion::Flash::Tinted(style)) => line.style = style,
        None => {}
    }
    line
}

pub(super) fn draw_needs_you(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = panel(" Needs you ", app.list_has_keys());
    if app.needs_list.is_empty() {
        let text = Text::from(vec![
            Line::from(""),
            Line::from(" ✓ All clear").green().bold(),
            Line::from(""),
            Line::from(" Nothing is waiting on you.").dim(),
            Line::from(" Agents that ask you something, or finish while").dim(),
            Line::from(" you look elsewhere, come here.").dim(),
        ]);
        frame.render_widget(
            Paragraph::new(text).wrap(Wrap { trim: false }).block(block),
            area,
        );
        return;
    }
    let width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem<'_>> = app
        .needs_list
        .iter()
        .filter_map(|id| app.model.session(*id))
        .map(|session| {
            let selected = app.needs_selected == Some(session.id);
            let project = app.model.project_of(Row::Session(session.id));
            let lead = vec![look::edge(app, project, selected)];
            // Mixed from every project, so each line says whose it is.
            let line = session_line(app, session, lead, width, true);
            ListItem::new(flashed(app, session, line))
        })
        .collect();
    let mut state = ListState::default()
        .with_offset(app.needs_offset)
        .with_selected(
            app.needs_selected
                .and_then(|s| app.needs_list.iter().position(|id| *id == s)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(session_highlight(app, app.needs_selected));
    frame.render_stateful_widget(list, area, &mut state);
}

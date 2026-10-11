//! Drawing the app. Pure: reads the app, writes the frame.

use muster_core::protocol::DetachReason;
use muster_core::{Session, SessionStatus, WaitReason, WorkspaceKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{
    Block, BorderType, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap,
};
use tui_term::widget::PseudoTerminal;

use crate::app::{App, Dialog, HarnessChoice, Link, Mode, View};
use crate::model::Row;

/// The terminal area a session gets when it fills the screen: everything
/// but the top bar.
pub(crate) fn terminal_area(area: Rect) -> Rect {
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if let Mode::Terminal(session) = app.mode {
        draw_terminal(frame, app, session, area);
        return;
    }
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(bottom_height(app, area.width)),
    ])
    .areas(area);
    draw_top_bar(frame, app, top);
    let [list, preview] =
        Layout::horizontal([Constraint::Percentage(38), Constraint::Percentage(62)]).areas(body);
    match app.view {
        View::Sessions => draw_sessions(frame, app, list),
        View::NeedsYou => draw_needs_you(frame, app, list),
    }
    draw_preview(frame, app, preview);
    draw_bottom_bar(frame, app, bottom);
    if let Mode::Dialog(dialog) = &app.mode {
        draw_dialog(frame, app, dialog, area);
    }
}

fn draw_top_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let waiting = app.model.needs_you().len();
    let mut spans = vec![Span::from(" Muster ").bold().reversed(), Span::raw("  ")];
    match &app.link {
        Link::Connecting => spans.push(Span::from("connecting to the daemon…").dim()),
        Link::Connected { .. } if !app.model.loaded() => {
            spans.push(Span::from("loading…").dim());
        }
        Link::Connected { daemon_version } => {
            spans.push(Span::from(format!("daemon {daemon_version}")).dim());
            if let Some(report) = app.model.resources() {
                spans.push(
                    Span::from(format!(
                        "  ·  memory {} of {}",
                        bytes(report.host_memory_used),
                        bytes(report.host_memory_total)
                    ))
                    .dim(),
                );
            }
            spans.push(Span::raw(format!(
                "  ·  {}  ·  ",
                count(app.model.sessions().len(), "session")
            )));
            let needs = Span::from(format!("{waiting} need you"));
            spans.push(if waiting > 0 {
                needs.red().bold()
            } else {
                needs.dim()
            });
        }
        Link::Lost(_) => spans.push(Span::from("daemon unreachable; retrying…").red()),
    }
    frame.render_widget(Line::from(spans), area);
}

/// The most lines a message gets under the list.
const MAX_NOTE_LINES: u16 = 3;

/// A message on one line: the daemon's messages can carry line breaks.
fn note_text(text: &str) -> String {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect();
    format!(" {}", lines.join(" · "))
}

/// What the bottom bar says instead of hints, if anything: the last note,
/// or why the daemon cannot be reached; and whether it is an error.
fn message(app: &App) -> Option<(&str, bool)> {
    match (&app.note, &app.link) {
        (Some(note), _) => Some((note.text.as_str(), note.error)),
        (None, Link::Lost(reason)) => Some((reason.as_str(), true)),
        (None, _) => None,
    }
}

/// How many lines the bottom bar needs: hints take one, a long message up
/// to three, so the reason in an error is not cut off.
fn bottom_height(app: &App, width: u16) -> u16 {
    let Some((text, _)) = message(app) else {
        return 1;
    };
    let length = u16::try_from(note_text(text).chars().count()).unwrap_or(u16::MAX);
    length.div_ceil(width.max(1)).clamp(1, MAX_NOTE_LINES)
}

fn draw_bottom_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if let Some((text, error)) = message(app) {
        let text = Line::from(note_text(text));
        let text = if error { text.red() } else { text.green() };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }
    frame.render_widget(Line::from(format!(" {}", hints(app))).dim(), area);
}

/// The keys that do something for what is selected.
fn hints(app: &App) -> String {
    let session = app.focused_session().and_then(|id| app.model.session(id));
    let mut hints = vec!["↑↓ move"];
    match (app.view, app.selected, session) {
        (_, _, Some(session)) if session.ended_at.is_none() => {
            hints.extend(["⏎ open", "s stop", "n session"]);
        }
        (_, _, Some(_)) => hints.extend(["⏎ open", "r restart", "x remove", "n session"]),
        (View::Sessions, Some(Row::Worktree(_)), None) => {
            hints.extend(["⏎ session", "x remove", "w worktree", "i issues"]);
        }
        (View::Sessions, Some(Row::Project(_)), None) => {
            hints.extend(["⏎ worktree", "i issues"]);
        }
        _ => {}
    }
    if app.view == View::Sessions {
        hints.extend(["a project", "⇥ needs you", "q quit"]);
    } else {
        hints.extend(["⇥ all sessions", "q quit"]);
    }
    hints.join("  ")
}

fn draw_sessions(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = app.model.rows();
    let block = panel(" Sessions ", app.view == View::Sessions);
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
            Line::from("Press a to add a git repository."),
        ]);
        frame.render_widget(Paragraph::new(text).block(block).dim(), area);
        return;
    }
    let items: Vec<ListItem<'_>> = rows.iter().map(|row| row_item(app, *row)).collect();
    let mut state = ListState::default()
        .with_selected(app.selected.and_then(|s| rows.iter().position(|r| *r == s)));
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn row_item<'a>(app: &'a App, row: Row) -> ListItem<'a> {
    match row {
        Row::Project(id) => {
            let name = app
                .model
                .project(id)
                .map_or("?", |project| project.name.as_str());
            ListItem::new(Line::from(Span::from(name.to_owned()).bold()))
        }
        Row::Worktree(id) => {
            let branch = app.model.branch(id).unwrap_or("?").to_owned();
            ListItem::new(Line::from(vec![
                Span::raw("  "),
                Span::from("⎇ ").dim(),
                Span::raw(branch),
            ]))
        }
        Row::Session(id) => match app.model.session(id) {
            Some(session) => {
                let mut line = session_line(session, "    ");
                if let Some(usage) = app.model.usage(id) {
                    line.spans
                        .push(Span::from(format!("  {}", usage_text(usage))).dim());
                }
                ListItem::new(line)
            }
            None => ListItem::new(""),
        },
    }
}

fn session_line<'a>(session: &'a Session, indent: &'a str) -> Line<'a> {
    let (symbol, style) = status_symbol(session.status);
    let mut spans = vec![
        Span::raw(indent),
        Span::styled(symbol, style),
        Span::raw(" "),
        Span::raw(session.title.as_str()),
        Span::raw("  "),
        Span::from(describe(session.status)).dim(),
    ];
    if session.unseen {
        spans.push(Span::from(" •").cyan().bold());
    }
    Line::from(spans)
}

fn draw_needs_you(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = panel(" Needs you ", true);
    if app.needs_list.is_empty() {
        frame.render_widget(
            Paragraph::new("Nothing is waiting on you.")
                .block(block)
                .dim(),
            area,
        );
        return;
    }
    let items: Vec<ListItem<'_>> = app
        .needs_list
        .iter()
        .filter_map(|id| app.model.session(*id))
        .map(|session| {
            let place = app.model.branch(session.workspace).unwrap_or("?");
            let mut line = session_line(session, "");
            line.spans.push(Span::from(format!("  {place}")).dim());
            ListItem::new(line)
        })
        .collect();
    let mut state = ListState::default().with_selected(
        app.needs_selected
            .and_then(|s| app.needs_list.iter().position(|id| *id == s)),
    );
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn draw_preview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if let Some(session) = app.focused_session().and_then(|id| app.model.session(id)) {
        let title = format!(" {} · {} ", session.title, describe(session.status));
        let block = panel(title, false);
        match app.screens.get(&session.id) {
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
                frame.render_widget(PseudoTerminal::new(screen.screen()).block(block), area)
            }
            None => frame.render_widget(Paragraph::new("Connecting…").block(block).dim(), area),
        }
        return;
    }
    let text = match app
        .view
        .eq(&View::Sessions)
        .then_some(app.selected)
        .flatten()
    {
        Some(Row::Worktree(id)) => match app.model.workspace(id) {
            Some(workspace) => {
                let WorkspaceKind::Worktree { path, branch } = &workspace.kind;
                let sessions = app.model.sessions_of(id).count();
                Text::from(vec![
                    Line::from(vec![
                        Span::from("Branch  ").dim(),
                        Span::raw(branch.clone()),
                    ]),
                    Line::from(vec![
                        Span::from("Path    ").dim(),
                        Span::raw(path.display().to_string()),
                    ]),
                    Line::from(vec![
                        Span::from("Sessions ").dim(),
                        Span::raw(sessions.to_string()),
                    ]),
                    Line::from(""),
                    Line::from("Press n or ⏎ to start a session here, x to remove the worktree.")
                        .dim(),
                ])
            }
            None => Text::default(),
        },
        Some(Row::Project(id)) => match app.model.project(id) {
            Some(project) => {
                let worktrees = app.model.workspaces_of(id).count();
                Text::from(vec![
                    Line::from(vec![
                        Span::from("Repository  ").dim(),
                        Span::raw(project.root.display().to_string()),
                    ]),
                    Line::from(vec![
                        Span::from("Worktrees   ").dim(),
                        Span::raw(worktrees.to_string()),
                    ]),
                    Line::from(""),
                    Line::from("Press w or ⏎ to create a worktree for a new piece of work.").dim(),
                ])
            }
            None => Text::default(),
        },
        _ => Text::from("Select a session to watch it here.").dim(),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(panel(" Details ", false)),
        area,
    );
}

/// What is shown for a session whose output the daemon no longer holds.
const ENDED_WITHOUT_OUTPUT: &str = "This session ended and its output is no longer kept \
     (the daemon was restarted). Press r to start it again, or x to remove it.";

fn draw_terminal(frame: &mut Frame<'_>, app: &App, session: muster_core::SessionId, area: Rect) {
    let [top, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
    let mut spans = vec![Span::from(" Muster ").bold().reversed(), Span::raw(" ")];
    if let Some(stored) = app.model.session(session) {
        let (symbol, style) = status_symbol(stored.status);
        spans.push(Span::styled(symbol, style));
        spans.push(Span::raw(format!(" {}  ", stored.title)));
        spans.push(Span::from(describe(stored.status)).dim());
        if let Some(branch) = app.model.branch(stored.workspace) {
            spans.push(Span::from(format!("  ⎇ {branch}")).dim());
        }
    }
    match &app.note {
        Some(note) => spans.push(Span::from(format!("   {}", note.text)).red()),
        None => spans.push(Span::from("   Ctrl-] back").dim()),
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
            if !screen.screen().hide_cursor() {
                let (row, col) = screen.screen().cursor_position();
                frame.set_cursor_position(Position {
                    x: body.x + col.min(body.width.saturating_sub(1)),
                    y: body.y + row.min(body.height.saturating_sub(1)),
                });
            }
        }
        None => frame.render_widget(Paragraph::new("Connecting…").dim(), body),
    }
}

/// What the issues dialog shows.
struct IssuesView<'a> {
    project: muster_core::ProjectId,
    issues: Option<&'a Vec<muster_core::Issue>>,
    selected: usize,
    harness: HarnessChoice,
    error: Option<&'a String>,
}

fn draw_issues(frame: &mut Frame<'_>, app: &App, area: Rect, shown: IssuesView<'_>) {
    let width = area.width.saturating_sub(4).min(96);
    let height = area.height.saturating_sub(4).min(22);
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height,
    };
    frame.render_widget(Clear, rect);
    let name = app
        .model
        .project(shown.project)
        .map_or("?", |found| found.name.as_str());
    let block = Block::default()
        .title(format!(" Issues of {name} "))
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [list_area, footer] =
        Layout::vertical([Constraint::Min(1), Constraint::Length(2)]).areas(inner);
    match (shown.issues, shown.error) {
        (_, Some(error)) => frame.render_widget(
            Paragraph::new(error.clone())
                .wrap(Wrap { trim: false })
                .red(),
            list_area,
        ),
        (None, None) => frame.render_widget(Paragraph::new("Loading…").dim(), list_area),
        (Some(list), None) if list.is_empty() => {
            frame.render_widget(Paragraph::new("No open issues.").dim(), list_area);
        }
        (Some(list), None) => {
            let items: Vec<ListItem<'_>> = list
                .iter()
                .map(|issue| {
                    let mut spans = vec![
                        Span::from(format!("#{:<5} ", issue.number)).dim(),
                        Span::raw(issue.title.clone()),
                    ];
                    if !issue.labels.is_empty() {
                        spans.push(Span::from(format!("  {}", issue.labels.join(", "))).cyan());
                    }
                    ListItem::new(Line::from(spans))
                })
                .collect();
            let mut state = ListState::default().with_selected(Some(shown.selected));
            frame.render_stateful_widget(
                List::new(items).highlight_style(Style::default().add_modifier(Modifier::REVERSED)),
                list_area,
                &mut state,
            );
        }
    }
    let choice = |option: HarnessChoice| {
        let label = format!(" {} ", option.label());
        if option == shown.harness {
            Span::from(label).reversed()
        } else {
            Span::from(label).dim()
        }
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                Span::raw("Run  "),
                choice(HarnessChoice::Claude),
                Span::raw(" "),
                choice(HarnessChoice::Shell),
            ]),
            Line::from("↑↓ choose  ⏎ start work  ⇥ switch  esc close").dim(),
        ]),
        footer,
    );
}

fn draw_dialog(frame: &mut Frame<'_>, app: &App, dialog: &Dialog, area: Rect) {
    if let Dialog::Issues {
        project,
        issues,
        selected,
        harness,
        error,
    } = dialog
    {
        let shown = IssuesView {
            project: *project,
            issues: issues.as_ref(),
            selected: *selected,
            harness: *harness,
            error: error.as_ref(),
        };
        draw_issues(frame, app, area, shown);
        return;
    }
    let width = area.width.saturating_sub(4).min(72);
    let (title, lines, input) = match dialog {
        Dialog::AddProject { path } => (
            " Add a project ",
            vec![Line::from("Path to a git repository:")],
            Some(path),
        ),
        Dialog::NewWorktree { branch, .. } => (
            " New worktree ",
            vec![Line::from(
                "Branch (checked out if it exists; empty for muster/<random>):",
            )],
            Some(branch),
        ),
        Dialog::NewSession {
            workspace,
            harness,
            prompt,
        } => {
            let place = app.model.branch(*workspace).unwrap_or("?");
            let choice = |option: HarnessChoice| {
                let label = format!(" {} ", option.label());
                if option == *harness {
                    Span::from(label).reversed()
                } else {
                    Span::from(label).dim()
                }
            };
            let mut lines = vec![
                Line::from(format!("In {place}")).dim(),
                Line::from(vec![
                    Span::raw("Run  "),
                    choice(HarnessChoice::Claude),
                    Span::raw(" "),
                    choice(HarnessChoice::Shell),
                    Span::from("   ⇥ switch").dim(),
                ]),
            ];
            match harness {
                HarnessChoice::Claude => {
                    lines.push(Line::from("First prompt (optional):"));
                    (" New session ", lines, Some(prompt))
                }
                HarnessChoice::Shell => (" New session ", lines, None),
            }
        }
        Dialog::Confirm { question, .. } => (
            " Confirm ",
            vec![
                Line::from(question.clone()),
                Line::from(""),
                Line::from("y yes   n no").dim(),
            ],
            None,
        ),
        Dialog::Issues { .. } => return,
    };
    let height = lines.len() as u16 + 2 + u16::from(input.is_some()) + 2;
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height: height.min(area.height),
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let mut text = lines;
    if let Some(input) = input {
        text.push(Line::from(input.text().to_owned()).bold());
        text.push(Line::from(""));
        text.push(Line::from("⏎ confirm   esc cancel").dim());
    }
    frame.render_widget(Paragraph::new(text), inner);
    if let Some(input) = input {
        let line = inner.y + inner.height.saturating_sub(3);
        let column = u16::try_from(input.before_cursor().chars().count()).unwrap_or(u16::MAX);
        frame.set_cursor_position(Position {
            x: (inner.x + column).min(inner.right().saturating_sub(1)),
            y: line,
        });
    }
}

fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    if focused {
        block.border_style(Style::default().fg(Color::Cyan))
    } else {
        block.border_style(Style::default().fg(Color::DarkGray))
    }
}

fn status_symbol(status: SessionStatus) -> (&'static str, Style) {
    match status {
        SessionStatus::Fresh => ("○", Style::default().fg(Color::Gray)),
        SessionStatus::Running => ("●", Style::default().fg(Color::Yellow)),
        SessionStatus::NeedsYou { .. } => ("▲", Style::default().fg(Color::Red).bold()),
        SessionStatus::Finished => ("✓", Style::default().fg(Color::Green)),
        SessionStatus::Exited { success: true } => ("■", Style::default().fg(Color::DarkGray)),
        SessionStatus::Exited { success: false } => ("✕", Style::default().fg(Color::Red)),
    }
}

pub(crate) fn describe(status: SessionStatus) -> &'static str {
    match status {
        SessionStatus::Fresh => "idle",
        SessionStatus::Running => "running",
        SessionStatus::NeedsYou {
            reason: WaitReason::Permission,
        } => "needs you: permission",
        SessionStatus::NeedsYou {
            reason: WaitReason::Question,
        } => "needs you: question",
        SessionStatus::Finished => "finished",
        SessionStatus::Exited { success: true } => "exited",
        SessionStatus::Exited { success: false } => "exited with an error",
    }
}

/// `312 MB · 4%`: memory, and CPU as a share of one core.
pub(crate) fn usage_text(usage: &muster_core::protocol::SessionUsage) -> String {
    format!(
        "{} · {}%",
        bytes(usage.memory_bytes),
        usage.cpu_permille.div_ceil(10)
    )
}

/// A byte count for people: `880 MB`, `9.1 GB`.
pub(crate) fn bytes(count: u64) -> String {
    const MB: u64 = 1_000_000;
    const GB: u64 = 1_000_000_000;
    if count >= GB {
        format!("{:.1} GB", count as f64 / GB as f64)
    } else {
        format!("{} MB", count.div_ceil(MB))
    }
}

fn count(n: usize, noun: &str) -> String {
    if n == 1 {
        format!("1 {noun}")
    } else {
        format!("{n} {noun}s")
    }
}

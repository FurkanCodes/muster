//! Drawing the app. Pure: reads the app, writes the frame.

use muster_core::protocol::DetachReason;
use muster_core::{Session, SessionStatus, WaitReason, WorkspaceKind};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Position, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Block, BorderType, Borders, List, ListItem, ListState, Paragraph, Wrap};
use tui_term::widget::PseudoTerminal;

use crate::app::{App, Link, Mode, View};
use crate::model::Row;

mod dialogs;
mod tickets;

/// The terminal area a session gets when it fills the screen: everything
/// but the top bar.
pub(crate) fn terminal_area(area: Rect) -> Rect {
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// Where the list view's parts are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BrowseAreas {
    pub(crate) top: Rect,
    pub(crate) list: Rect,
    pub(crate) preview: Rect,
    pub(crate) bottom: Rect,
}

/// The list view's layout in `area`, shared with the app so the mouse
/// lands where things are drawn. While a session beside the list has the
/// keys, the bottom bar keeps to one line, so the session keeps its size.
pub(crate) fn browse_areas(app: &App, area: Rect) -> BrowseAreas {
    let bottom = if matches!(app.mode, Mode::Terminal(_)) {
        1
    } else {
        bottom_height(app, area.width)
    };
    layout(area, bottom)
}

/// Where a session's terminal is drawn beside the list in `area`, inside
/// its border: the size its program is given.
pub(crate) fn session_area(area: Rect) -> Rect {
    layout(area, 1).preview.inner(Margin::new(1, 1))
}

fn layout(area: Rect, bottom: u16) -> BrowseAreas {
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(bottom),
    ])
    .areas(area);
    // The list takes what its lines need, and the session beside it, where
    // the person works with Claude Code, the rest.
    let list_width = u16::try_from((u32::from(body.width) * 3 / 10).clamp(34, 48))
        .unwrap_or(48)
        .min(body.width);
    let [list, preview] =
        Layout::horizontal([Constraint::Length(list_width), Constraint::Min(0)]).areas(body);
    BrowseAreas {
        top,
        list,
        preview,
        bottom,
    }
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if let Mode::Terminal(session) = app.mode
        && app.full_screen
    {
        draw_terminal(frame, app, session, area);
        return;
    }
    let BrowseAreas {
        top,
        list,
        preview,
        bottom,
    } = browse_areas(app, area);
    draw_top_bar(frame, app, top);
    match app.view {
        View::Sessions => draw_sessions(frame, app, list),
        View::NeedsYou => draw_needs_you(frame, app, list),
        View::Tickets => tickets::draw_tickets(frame, app, list),
    }
    if let Mode::Terminal(session) = app.mode {
        draw_session(frame, app, session, preview, true);
    } else if app.view == View::Tickets {
        tickets::draw_ticket_details(frame, app, preview);
    } else {
        draw_preview(frame, app, preview);
    }
    draw_bottom_bar(frame, app, bottom);
    if let Mode::Dialog(dialog) = &app.mode {
        dialogs::draw_dialog(frame, app, dialog, area);
    }
}

fn draw_top_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let waiting = app.model.needs_you().len();
    let mut spans = vec![Span::from(" Muster ").bold().reversed(), Span::raw(" ")];
    // The screens as tabs, the one shown lit; ⇥ goes round them.
    for (at, (view, name)) in [
        (View::Sessions, "Sessions"),
        (View::NeedsYou, "Needs you"),
        (View::Tickets, "Tickets"),
    ]
    .into_iter()
    .enumerate()
    {
        if at > 0 {
            spans.push(Span::from("│").dim());
        }
        let mut tab = vec![Span::raw(format!(" {name} "))];
        if view == View::NeedsYou && waiting > 0 {
            tab.push(Span::from(format!("{waiting} ")).red().bold());
        }
        if app.view == view {
            tab = tab.into_iter().map(|span| span.reversed().bold()).collect();
        }
        spans.extend(tab);
    }
    spans.push(Span::from(" ⇥   ").dim());
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
            spans.push(
                Span::from(format!(
                    "  ·  {}",
                    count(app.model.sessions().len(), "session")
                ))
                .dim(),
            );
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
pub(crate) fn bottom_height(app: &App, width: u16) -> u16 {
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
    if matches!(app.mode, Mode::Terminal(_)) {
        return "keys go to the session   Ctrl-Q back to the list (or click it)".to_owned();
    }
    let session = app.focused_session().and_then(|id| app.model.session(id));
    let mut hints = vec!["↑↓ move"];
    if app.view == View::Tickets {
        hints.extend(tickets::hints(app));
        hints.extend(["⇥ next tab", "esc back", "q quit"]);
        return hints.join("  ");
    }
    match (app.view, app.selected, session) {
        (_, _, Some(session)) if session.ended_at.is_none() => {
            hints.extend([
                "⏎ type in it",
                "f full screen",
                "x close",
                "s stop",
                "n session",
            ]);
        }
        (_, _, Some(_)) => hints.extend([
            "⏎ open",
            "f full screen",
            "r restart",
            "x remove",
            "n session",
        ]),
        (View::Sessions, Some(Row::Worktree(id)), None) => {
            let kind = app.model.workspace(id).map(|workspace| &workspace.kind);
            hints.push("⏎ session");
            if kind.is_some_and(WorkspaceKind::is_worktree) {
                hints.push("x remove");
            }
            if kind.is_some_and(|kind| !kind.in_git()) {
                // A plain folder: its checks, and nothing that needs git.
                hints.push("c check");
            } else {
                if kind.is_some_and(|kind| kind.run().is_none()) {
                    hints.extend(["c check", "v review", "o PR"]);
                }
                hints.extend(["w worktree", "i tickets"]);
            }
        }
        (View::Sessions, Some(Row::Project(id)), None) if app.model.is_plain_folder(id) => {
            hints.extend(["⏎ session", "x remove"]);
        }
        (View::Sessions, Some(Row::Project(_)), None) => {
            hints.extend(["⏎ worktree", "n session", "i tickets", "x remove"]);
        }
        _ => {}
    }
    match app.view {
        View::Sessions => hints.extend(["a project", "⇥ next tab", "q quit"]),
        View::NeedsYou => hints.extend(["⇥ next tab", "esc back", "q quit"]),
        View::Tickets => {}
    }
    hints.join("  ")
}

fn draw_sessions(frame: &mut Frame<'_>, app: &App, area: Rect) {
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
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

fn row_item<'a>(app: &'a App, row: Row, width: usize) -> ListItem<'a> {
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
            let kind = app.model.workspace(id).map(|workspace| &workspace.kind);
            let mut spans = vec![
                Span::raw("  "),
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
                spans.push(Span::raw(branch));
            }
            ListItem::new(Line::from(spans))
        }
        Row::Session(id) => match app.model.session(id) {
            Some(session) => {
                let usage = app
                    .model
                    .usage(id)
                    .map(|usage| format!("  {}", usage_text(usage)))
                    .unwrap_or_default();
                // The status stays in view. The title gives way, but not
                // below a readable length: then the usage goes instead,
                // which the session's pane also shows.
                let base = session_line(session, "    ", "").width();
                let readable = session.title.chars().count().min(READABLE_TITLE);
                let usage = if width.saturating_sub(base + usage.chars().count()) >= readable {
                    usage
                } else {
                    String::new()
                };
                let rest = base + usage.chars().count();
                let title = shorten(&session.title, width.saturating_sub(rest).max(8));
                let mut line = session_line(session, "    ", &title);
                if !usage.is_empty() {
                    line.spans.push(Span::from(usage).dim());
                }
                ListItem::new(line)
            }
            None => ListItem::new(""),
        },
    }
}

/// The fewest characters of a session's title a list line keeps before it
/// leaves out the session's usage.
const READABLE_TITLE: usize = 12;

/// `text`, cut to `max` characters with an ellipsis if it is longer.
pub(super) fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

fn session_line<'a>(session: &'a Session, indent: &'a str, title: &str) -> Line<'a> {
    let (symbol, style) = status_symbol(session.status);
    let mut spans = vec![
        Span::raw(indent),
        Span::styled(symbol, style),
        Span::raw(" "),
        Span::raw(title.to_owned()),
        Span::raw("  "),
        Span::from(describe(session.status)).dim(),
    ];
    if session.unseen {
        spans.push(Span::from(" •").cyan().bold());
    }
    Line::from(spans)
}

fn draw_needs_you(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = panel(" Needs you ", app.list_has_keys());
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
            let mut line = session_line(session, "", &session.title);
            line.spans.push(Span::from(format!("  {place}")).dim());
            ListItem::new(line)
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
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

/// A session's terminal beside the list: watched there, or `typing` into
/// it, when it has the keys, its border lit and its cursor shown.
fn draw_session(
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
    let mut title = format!(" {} · {} ", stored.title, describe(stored.status));
    if let Some(usage) = app.model.usage(session) {
        title.push_str(&format!("· {} ", usage_text(usage)));
    }
    if back > 0 {
        title.push_str(&format!("· ↑ {back} lines back "));
    } else if typing {
        title.push_str("· Ctrl-Q back to the list ");
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

fn draw_preview(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if let Some(session) = app.focused_session() {
        draw_session(frame, app, session, area, false);
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
                let sessions = app.model.sessions_of(id).count();
                let mut lines = Vec::new();
                if !workspace.kind.in_git() {
                    lines.push(
                        Line::from("The project's own folder, where you work too. It is not a git repository.")
                            .dim(),
                    );
                } else if workspace.kind.is_checkout() {
                    lines.push(Line::from("The project's own checkout, where you work too.").dim());
                }
                if let Some(run) = app.model.run_of(id) {
                    lines.push(
                        Line::from(format!(
                            "The container of the run on #{} ({}), with a clone of its own.",
                            run.ticket.number,
                            run.stage.label()
                        ))
                        .dim(),
                    );
                }
                if workspace.kind.in_git() {
                    lines.push(Line::from(vec![
                        Span::from("Branch  ").dim(),
                        Span::raw(workspace.kind.branch().to_owned()),
                    ]));
                }
                lines.extend([
                    Line::from(vec![
                        Span::from("Path    ").dim(),
                        Span::raw(workspace.kind.host_path().map_or_else(
                            || "in the run's container".to_owned(),
                            |path| path.display().to_string(),
                        )),
                    ]),
                    Line::from(vec![
                        Span::from("Sessions ").dim(),
                        Span::raw(sessions.to_string()),
                    ]),
                    Line::from(""),
                ]);
                lines.push(
                    Line::from(match &workspace.kind {
                        WorkspaceKind::Checkout { .. } => "Press n or ⏎ to start a session here.",
                        WorkspaceKind::Worktree { .. } => {
                            "Press n or ⏎ to start a session here, x to remove the worktree."
                        }
                        WorkspaceKind::Container { .. } => {
                            "Press n or ⏎ for a shell in it; it goes when the run ends."
                        }
                    })
                    .dim(),
                );
                Text::from(lines)
            }
            None => Text::default(),
        },
        Some(Row::Project(id)) => match app.model.project(id) {
            Some(project) if app.model.is_plain_folder(id) => Text::from(vec![
                Line::from(vec![
                    Span::from("Folder  ").dim(),
                    Span::raw(project.root.display().to_string()),
                ]),
                Line::from(""),
                Line::from("Press n or ⏎ to start a session in the folder.").dim(),
                Line::from(
                    "It is not a git repository, so it has no worktrees or tickets; git init there makes it one.",
                )
                .dim(),
            ]),
            Some(project) => {
                let worktrees = app
                    .model
                    .workspaces_of(id)
                    .filter(|workspace| !workspace.kind.is_checkout())
                    .count();
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
                    Line::from("Press w or ⏎ to create a worktree for a new piece of work,").dim(),
                    Line::from("n to start a session in the project's own checkout.").dim(),
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
        Some(note) => spans.push(Span::from(format!("   {}", note.text)).red()),
        None if back > 0 => spans.push(
            Span::from(format!(
                "   ↑ {back} lines back · scroll down or type to return"
            ))
            .yellow(),
        ),
        None => spans.push(Span::from("   Ctrl-Q back to the list").dim()),
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
            show_cursor(frame, screen, body);
        }
        None => frame.render_widget(Paragraph::new("Connecting…").dim(), body),
    }
}

pub(super) fn panel<'a>(title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
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

pub(super) fn status_symbol(status: SessionStatus) -> (&'static str, Style) {
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
        SessionStatus::NeedsYou {
            reason: WaitReason::Startup,
        } => "needs you: asking before it starts",
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

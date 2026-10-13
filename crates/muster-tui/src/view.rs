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
use crate::motion;

mod dialogs;
pub(crate) mod look;
mod settings;
mod tickets;
mod top_bar;

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
    top_bar::draw_top_bar(frame, app, top);
    match app.view {
        View::Sessions => draw_sessions(frame, app, list),
        View::NeedsYou => draw_needs_you(frame, app, list),
        View::Tickets => tickets::draw_tickets(frame, app, list),
        View::Settings => settings::draw_settings(frame, app, list),
    }
    if let Mode::Terminal(session) = app.mode {
        draw_session(frame, app, session, preview, true);
    } else if app.view == View::Tickets {
        tickets::draw_ticket_details(frame, app, preview);
    } else if app.view == View::Settings {
        settings::draw_setting_details(frame, app, preview);
    } else {
        draw_preview(frame, app, preview);
    }
    draw_bottom_bar(frame, app, bottom);
    if let Mode::Dialog(dialog) = &app.mode {
        dialogs::draw_dialog(frame, app, dialog, area);
    }
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
        let text = match (error, app.note_fading()) {
            (true, _) => text.red(),
            (false, true) => text.green().dim(),
            (false, false) => text.green(),
        };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }
    frame.render_widget(Line::from(format!(" {}", hints(app))).dim(), area);
}

/// The keys that do something for what is selected.
fn hints(app: &App) -> String {
    if matches!(app.mode, Mode::Terminal(_)) {
        return "keys go to the session   Shift-Q back to the list (or click it)".to_owned();
    }
    let session = app.focused_session().and_then(|id| app.model.session(id));
    let mut hints = vec!["↑↓ move"];
    if app.view == View::Tickets {
        hints.extend(tickets::hints(app));
        hints.extend(["⇥ next tab", "esc back", "q quit"]);
        return hints.join("  ");
    }
    if app.view == View::Settings {
        hints.extend(settings::hints(app));
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
        View::Tickets | View::Settings => {}
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
    let selected = match app.selected {
        Some(Row::Session(session)) => Some(session),
        _ => None,
    };
    let list = List::new(items)
        .block(block)
        .highlight_style(session_highlight(app, selected));
    frame.render_stateful_widget(list, area, &mut state);
}

fn row_item<'a>(app: &'a App, row: Row, width: usize) -> ListItem<'a> {
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
                spans.push(Span::raw(branch));
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

/// The fewest characters of a session's title a list line keeps before it
/// leaves out the session's usage.
const READABLE_TITLE: usize = 12;

/// The most of a project's name a session's line gives it.
const PROJECT_NAME_ROOM: usize = 20;

/// `text`, cut to `max` characters with an ellipsis if it is longer.
pub(super) fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

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

fn draw_needs_you(frame: &mut Frame<'_>, app: &App, area: Rect) {
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
        _ if !app.model.loaded() => Text::from(look::wordmark("Connecting to the daemon…")),
        _ if app.model.projects().is_empty() => {
            let mut lines = look::wordmark("Agents on your tickets, side by side.");
            lines.extend([
                Line::from(""),
                Line::from("  Press a to add a project: a git repository, or any folder."),
            ]);
            Text::from(lines)
        }
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
        Some(note) => spans.push(Span::from(format!("   {}", note.text)).red()),
        None if back > 0 => spans.push(
            Span::from(format!(
                "   ↑ {back} lines back · scroll down or type to return"
            ))
            .yellow(),
        ),
        None => spans.push(Span::from("   Shift-Q back to the list").dim()),
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

/// A session's mark: at a motion `step`, a running one's spins and one
/// that needs the person pulses; without one, they stand still.
pub(super) fn status_symbol(status: SessionStatus, step: Option<u64>) -> (&'static str, Style) {
    match status {
        SessionStatus::Fresh => ("○", Style::default().fg(Color::Gray)),
        SessionStatus::Running => (
            step.map_or("●", motion::spinner),
            Style::default().fg(Color::Yellow),
        ),
        SessionStatus::NeedsYou { .. } => {
            let style = Style::default().fg(Color::Red).bold();
            ("▲", step.map_or(style, |step| motion::pulse(style, step)))
        }
        SessionStatus::Finished => ("✓", Style::default().fg(Color::Green)),
        SessionStatus::Exited { success: true } => ("■", Style::default().fg(Color::Gray)),
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
        SessionStatus::NeedsYou {
            reason: WaitReason::Action,
        } => "needs you: action required",
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

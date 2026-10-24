//! The Sessions and Needs you lists: projects, their worktrees and their
//! sessions' lines, and the queue of what waits on the person.

use muster_core::{Session, SessionStatus, WorkspaceId, WorkspaceKind};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{
    changes, clashes, list_block, logos, look, mark, pull_request, shorten, slid, status_symbol,
};
use crate::app::{App, View};
use crate::cells;
use crate::model::Row;
use crate::motion;

pub(super) fn draw_sessions(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let rows = app.rows();
    let block = list_block(app, "projects", app.list_has_keys());
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
            Line::from(format!(
                "Press {} to add a project: a git repository, or any folder.",
                app.key(muster_core::keys::Action::AddProject)
            )),
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
    let sessions: Vec<Option<&Session>> = rows
        .iter()
        .map(|row| match row {
            Row::Session(id) => app.model.session(*id),
            _ => None,
        })
        .collect();
    let lead = pull_request::width(&session_lead(Span::raw(" ")));
    let heights = app.row_heights(&rows);
    draw_logos(
        frame,
        app,
        area,
        (&sessions, &heights, state.offset()),
        lead,
    );
}

/// The columns the list takes, its frame included. Beside a session (on
/// Sessions and Needs you, or one open from another screen), as many as
/// the longest name on the sessions
/// list needs (a project's, a worktree's branch, a session's), from
/// `LIST_LEAST` to `LIST_MOST`: every line counts, folded or not, and
/// only names, not what changes as agents work (what one is doing, for
/// how long, a pull request's checks), so the list keeps its width, and
/// the session beside it its size, as the person goes about and agents
/// work.
pub(crate) fn list_width(app: &App, beside_session: bool, window: u16) -> u16 {
    use crate::layout::{LIST_LEAST, LIST_MOST};
    if !beside_session {
        // No session beside it: tickets' titles are long, and take what
        // the `window` spares; the settings, a plugin's line saying why it
        // is off among them, a little more.
        let (tenths, most) = if app.view == crate::app::View::Settings {
            (4, 64)
        } else {
            (3, 48)
        };
        let wide = u32::from(window) * tenths / 10;
        return u16::try_from(wide.clamp(34, u32::from(most))).unwrap_or(most);
    }
    // On one line, a session's name, what it is doing and for how long
    // share it.
    if app.session_height() == 1 {
        return LIST_MOST;
    }
    let wanted = app
        .model
        .rows()
        .into_iter()
        .map(|row| line_wants(app, row))
        .max()
        .unwrap_or(0);
    u16::try_from(wanted + 2)
        .unwrap_or(u16::MAX)
        .clamp(LIST_LEAST, LIST_MOST)
}

/// The cells `row`'s line needs for its name or branch whole.
fn line_wants(app: &App, row: Row) -> usize {
    let edge = Span::raw(" ");
    match row {
        Row::Project(id) => {
            3 + app
                .model
                .project(id)
                .map_or(1, |project| cells::width(&project.name))
        }
        Row::Worktree(id) => {
            let branch = app.model.branch(id).unwrap_or("?");
            // A run's container says its ticket and stage first.
            let run = app.model.run_of(id).map_or(0, |run| {
                format!("#{} {}  ", run.ticket.number, run.stage.label())
                    .chars()
                    .count()
            });
            pull_request::width(&worktree_lead(app, id, edge)) + run + cells::width(branch)
        }
        Row::Session(id) => app.model.session(id).map_or(0, |session| {
            pull_request::width(&session_lead(edge))
                + 2
                + logos::width(app.graphics)
                + cells::width(&session.title)
        }),
    }
}

/// The agents' pictures on the first lines of `sessions`, whose rows take
/// `heights` lines, shown in `area` from `offset`, each after its line's
/// lead of `lead` cells and its mark, where the terminal draws pictures.
fn draw_logos(
    frame: &mut Frame<'_>,
    app: &App,
    area: Rect,
    (sessions, heights, offset): (&[Option<&Session>], &[usize], usize),
    lead: usize,
) {
    if app.graphics == logos::Graphics::Text {
        return;
    }
    let inner = area.inner(ratatui::layout::Margin::new(1, 1));
    let x = inner
        .x
        .saturating_add(u16::try_from(lead + 2).unwrap_or(u16::MAX));
    let bottom = usize::from(inner.y) + usize::from(inner.height);
    let mut line = usize::from(inner.y);
    for (session, lines) in sessions.iter().zip(heights).skip(offset) {
        // A row with no room for all its lines is left out.
        if line + lines > bottom {
            break;
        }
        if let Some(session) = session {
            let at = (x, u16::try_from(line).unwrap_or(u16::MAX));
            // The colour the list left under it: the selection's tint, a
            // flash's, or none of its own.
            let under = frame.buffer_mut().cell(at).map(|cell| cell.bg);
            let backdrop = under
                .and_then(|colour| super::theme::shown(colour, &app.colours))
                .map(|colour| logos::Backdrop::new(colour, app.cell_pixels));
            logos::draw(
                frame.buffer_mut(),
                app.graphics,
                session.shown_harness(),
                at,
                backdrop,
            );
        }
        line += lines;
    }
}

fn row_item(app: &App, row: Row, width: usize) -> ListItem<'_> {
    let selected = app.view == View::Sessions && app.selected == Some(row);
    let edge = look::edge(app, app.model.project_of(row), selected);
    let by = app.slid_by(View::Sessions, row);
    match row {
        Row::Project(id) => {
            let name = app
                .model
                .project(id)
                .map_or("?", |project| project.name.as_str());
            let mut spans = vec![
                edge,
                fold_mark(app, row),
                Span::raw(" "),
                Span::styled(
                    name.to_owned(),
                    Style::default()
                        .fg(look::project_colour(app, id))
                        .add_modifier(Modifier::BOLD),
                ),
            ];
            spans.extend(standing(app, row));
            ListItem::new(Line::from(spans))
        }
        Row::Worktree(id) => ListItem::new(worktree_line(app, id, edge, width, by)),
        Row::Session(id) => match app.model.session(id) {
            Some(session) if crate::model::stands_beside(session) => {
                ListItem::new(shell_line(app, session, session_lead(edge), width, by))
            }
            Some(session) => {
                let lines = session_lines(app, session, session_lead(edge), width, false, by);
                ListItem::new(flashed(app, session, lines))
            }
            None => ListItem::new(""),
        },
    }
}

/// A session's line in the sessions list starts under its worktree's.
fn session_lead(edge: Span<'static>) -> Vec<Span<'static>> {
    vec![edge, Span::raw("   ")]
}

/// The names a shell goes by while it waits at its prompt: what it runs
/// then is itself.
const SHELLS: [&str; 12] = [
    "zsh", "bash", "fish", "sh", "dash", "ksh", "tcsh", "csh", "nu", "elvish", "xonsh", "pwsh",
];

/// A shell's one line after `lead`: under its worktree's one chat, set in
/// with `└`, else after the chats it serves; its prompt `❯` and what runs
/// in it (slid `by` characters where it is cut short), quietly while it
/// waits at its prompt or has ended, and how long a program has run.
fn shell_line(
    app: &App,
    shell: &Session,
    lead: Vec<Span<'static>>,
    width: usize,
    by: usize,
) -> Line<'static> {
    let theme = super::theme::of(app);
    let mut spans = lead;
    if app.model.under_its_chat(shell) {
        spans.push(Span::styled("  └ ", theme.quiet));
    }
    spans.push(Span::from("❯ ").dim());
    let ended = shell.ended_at.is_some() || matches!(shell.status, SessionStatus::Exited { .. });
    let waiting = SHELLS.contains(&shell.title.as_str());
    let time = if ended || waiting {
        String::new()
    } else {
        look::session_time(app, shell).unwrap_or_default()
    };
    let tail = if ended { "  ended" } else { "" };
    let used: usize = spans.iter().map(Span::width).sum();
    let room = width.saturating_sub(used + tail.len() + cells::width(&time) + 1);
    let name = slid(&shell.title, room.max(4), by);
    spans.push(if ended || waiting {
        Span::styled(name, theme.quiet)
    } else {
        Span::raw(name)
    });
    if ended {
        spans.push(Span::styled(tail, theme.quiet));
    }
    if !time.is_empty() {
        let used: usize = spans.iter().map(Span::width).sum();
        let gap = width.saturating_sub(used + cells::width(&time)).max(1);
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(time, theme.quiet));
    }
    Line::from(spans)
}

/// Where a project's or a worktree's triangle is drawn on its line, in
/// cells from the list's inside edge: what a click folds it on.
pub(crate) fn fold_column(row: Row) -> Option<u16> {
    match row {
        Row::Project(_) => Some(1),
        Row::Worktree(_) => Some(2),
        Row::Session(_) => None,
    }
}

/// The triangle a project's or a worktree's line folds on: `▾` unfolded,
/// `▸` folded, blank where there is nothing under it to fold.
fn fold_mark(app: &App, row: Row) -> Span<'static> {
    let mark = if app.is_folded(row) {
        "▸"
    } else if app.foldable(row) {
        "▾"
    } else {
        " "
    };
    Span::from(mark).dim()
}

/// How the sessions on a project's or a worktree's line stand, after its
/// name, while it is folded: the mark of the one that says most
/// (`Model::most_urgent`), as its own line draws it, with the dot of news
/// not seen, and how many sessions it hides. Unfolded, each session's
/// line says it, and only there.
fn standing(app: &App, row: Row) -> Vec<Span<'static>> {
    let mut spans = Vec::new();
    if !app.is_folded(row) {
        return spans;
    }
    if let Some(session) = app.model.most_urgent(row) {
        let (symbol, style) = mark(app, session.status);
        spans.push(Span::raw(" "));
        spans.push(Span::styled(symbol, style));
        if session.unseen && !matches!(session.status, SessionStatus::NeedsYou { .. }) {
            spans.push(news_dot(app, ""));
        }
    }
    let hidden = app.hidden_sessions(row);
    if hidden > 0 {
        let plural = if hidden == 1 { "" } else { "s" };
        spans.push(Span::from(format!(" {hidden} session{plural}")).dim());
    }
    spans
}

/// A worktree's line before its branch: its edge, its triangle and its
/// kind's mark, which stands over its sessions' marks.
fn worktree_lead(app: &App, id: WorkspaceId, edge: Span<'static>) -> Vec<Span<'static>> {
    let kind = app.model.workspace(id).map(|workspace| &workspace.kind);
    vec![
        edge,
        Span::raw(" "),
        fold_mark(app, Row::Worktree(id)),
        Span::raw(" "),
        Span::from(match kind {
            Some(WorkspaceKind::Checkout { .. }) => "⌂ ",
            Some(WorkspaceKind::Container { .. }) => "⧉ ",
            _ => "⎇ ",
        })
        .dim(),
    ]
}

/// A worktree's line, its branch slid `by` characters where it is cut
/// short.
fn worktree_line(
    app: &App,
    id: WorkspaceId,
    edge: Span<'static>,
    width: usize,
    by: usize,
) -> Line<'static> {
    let branch = app.model.branch(id).unwrap_or("?").to_owned();
    let kind = app.model.workspace(id).map(|workspace| &workspace.kind);
    let mut spans = worktree_lead(app, id, edge);
    let standing = standing(app, Row::Worktree(id));
    if kind.is_some_and(|kind| !kind.in_git()) {
        spans.push(Span::from("not a git repository").dim());
        spans.extend(standing);
    } else if let Some(run) = app.model.run_of(id) {
        // The stage first: the branch is long, and says less.
        spans.push(Span::raw(format!("#{} ", run.ticket.number)));
        spans.push(Span::from(run.stage.label()).dim());
        spans.push(Span::raw(format!("  {branch}")));
        spans.extend(standing);
    } else {
        // How its sessions stand stays beside the branch, whatever goes.
        let room =
            width.saturating_sub(pull_request::width(&spans) + pull_request::width(&standing));
        let (tail, branch_room) = branch_and_tail(app, id, &branch, room);
        let branch = slid(&branch, branch_room, by);
        let gap = room
            .saturating_sub(cells::width(&branch) + pull_request::width(&tail))
            .max(1);
        spans.push(Span::raw(branch));
        spans.extend(standing);
        if !tail.is_empty() {
            spans.push(Span::raw(" ".repeat(gap)));
            spans.extend(tail);
        }
    }
    Line::from(spans)
}

/// What ends a worktree's line after its branch, in `room` cells, and
/// the room left to the branch, which cut short keeps six characters.
fn branch_and_tail(
    app: &App,
    id: WorkspaceId,
    branch: &str,
    room: usize,
) -> (Vec<Span<'static>>, usize) {
    let theme = super::theme::of(app);
    let workspace = app.model.workspace(id);
    let found = workspace.and_then(|workspace| workspace.pull_request.as_ref());
    let summary = workspace.and_then(|workspace| workspace.changes.as_ref());
    let mark = clashes::mark(
        workspace.map_or(&[][..], |workspace| &workspace.clashes),
        theme,
    );
    let tail = worktree_tail(branch, mark.as_ref(), summary, found, room, theme);
    let branch_room = if tail.is_empty() {
        room
    } else {
        room.saturating_sub(pull_request::width(&tail) + 1).max(6)
    };
    (tail, branch_room)
}

/// What follows a worktree's branch on its line, in `room` cells: whether
/// its work meets other work, what it has changed and the pull request
/// from it. Short of room, the pull request's word goes first, then what
/// changed, then the branch is cut short; the word on other work stays.
fn worktree_tail(
    branch: &str,
    clash: Option<&Span<'static>>,
    summary: Option<&muster_core::ChangeSummary>,
    found: Option<&muster_core::PullRequestStatus>,
    room: usize,
    theme: super::theme::Theme,
) -> Vec<Span<'static>> {
    let fits = |tail: &[Span<'_>]| {
        tail.is_empty() || cells::width(branch) + 1 + pull_request::width(tail) <= room
    };
    let tail = |summary: Option<&muster_core::ChangeSummary>, compact: bool| {
        let mut tail: Vec<Span<'static>> = clash.cloned().into_iter().collect();
        if let Some(summary) = summary {
            if !tail.is_empty() {
                tail.push(Span::raw(" "));
            }
            tail.extend(changes::summary(summary, theme));
        }
        if let Some(found) = found {
            if !tail.is_empty() {
                tail.push(Span::raw(" "));
            }
            tail.extend(pull_request::badge(found, compact, theme));
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
    chosen
}

/// The fewest characters of a session's title a list line keeps before it
/// leaves out the session's usage.
const READABLE_TITLE: usize = 12;

/// The most of a project's name a session's line gives it.
const PROJECT_NAME_ROOM: usize = 20;

/// What a session's line holds beside its mark, as its room allows.
struct SessionParts {
    /// With its place, its project's name, in the project's colour.
    project: Vec<Span<'static>>,
    /// The cells its title is given.
    title_room: usize,
    /// What it is doing, or nothing where there is no room for it.
    doing: String,
    /// How long it has been so, or nothing where there is no room for it.
    time: String,
}

/// How a session's line after `lead_width` cells shares out `width`
/// (with `place`, its project's name comes first, and its branch after
/// what it is doing). Where it is short of room the title gives way
/// first, down to a readable length; then a finished or ended session's
/// time goes, then what it is doing, which is cut short only while it
/// works or waits, or where it says more than a status (a tidy's outcome,
/// why an ended session was not resumed); a status alone says no more
/// than its mark. The mark
/// always stays, and a working or waiting session's time, and its
/// project's name.
fn session_parts(
    app: &App,
    session: &Session,
    lead_width: usize,
    width: usize,
    place: bool,
) -> SessionParts {
    let live = matches!(
        session.status,
        SessionStatus::Running | SessionStatus::NeedsYou { .. }
    );
    let unseen = if session.unseen { 2 } else { 0 };
    let mut doing = look::doing(session).into_owned();
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
    let lead_width = lead_width + project.iter().map(Span::width).sum::<usize>();
    let fixed = lead_width + 2 + logos::width(app.graphics) + unseen;
    let title_least = cells::width(&session.title).min(READABLE_TITLE);
    let wanted = title_least + 2 + cells::width(&doing);
    let mut time = look::session_time(app, session).unwrap_or_default();
    let time_width = |time: &str| {
        if time.is_empty() {
            0
        } else {
            cells::width(time) + 1
        }
    };
    if !live && width.saturating_sub(fixed + time_width(&time)) < wanted {
        time.clear();
    }
    let room = width.saturating_sub(fixed + time_width(&time));
    let doing_room = room.saturating_sub(title_least + 2);
    // A tidy's outcome is worth a shortened line, as a working agent's is.
    let doing = if cells::width(&doing) <= doing_room {
        doing
    } else if (live || session.tidy.is_some() || session.activity.is_some()) && doing_room >= 8 {
        shorten(&doing, doing_room)
    } else {
        String::new()
    };
    let doing_width = if doing.is_empty() {
        0
    } else {
        cells::width(&doing) + 2
    };
    SessionParts {
        project,
        title_room: room.saturating_sub(doing_width).max(4),
        doing,
        time,
    }
}

/// The dot of news not seen, after `before`.
fn news_dot(app: &App, before: &str) -> Span<'static> {
    Span::styled(
        format!("{before}•"),
        Style::default()
            .fg(super::theme::of(app).news)
            .add_modifier(Modifier::BOLD),
    )
}

/// A session's lines after `lead`, as many as the person's settings say
/// (`Preferences::session_lines`): [`session_line`] alone, or
/// [`session_pair`].
fn session_lines(
    app: &App,
    session: &Session,
    lead: Vec<Span<'static>>,
    width: usize,
    place: bool,
    by: usize,
) -> Text<'static> {
    if app.session_height() == 1 {
        Text::from(session_line(app, session, lead, width, place, by))
    } else {
        Text::from(session_pair(app, session, lead, width, place, by).to_vec())
    }
}

/// How a session's line says what it is doing: as a wait, as what went
/// wrong, as work in hand, or quietly.
fn doing_style(app: &App, session: &Session) -> Style {
    let theme = super::theme::of(app);
    match (&session.activity, session.status) {
        (_, SessionStatus::NeedsYou { .. }) => Style::default().fg(theme.waiting),
        _ if session
            .tidy
            .as_ref()
            .is_some_and(muster_core::tidy::Tidy::went_wrong) =>
        {
            Style::default().fg(theme.waiting)
        }
        (Some(_), SessionStatus::Running) => Style::default(),
        _ => theme.quiet,
    }
}

/// The cells a session's name is given on the first of its two lines
/// after `lead_width` cells, out of `width`: all but its mark, its logo,
/// its project's name (with `place`) and the dot of news not seen.
fn pair_title_room(
    app: &App,
    session: &Session,
    lead_width: usize,
    width: usize,
    place: bool,
) -> usize {
    let project = if place {
        app.model
            .project_of(Row::Session(session.id))
            .and_then(|id| app.model.project(id))
            .map_or(0, |found| {
                cells::width(&shorten(&found.name, PROJECT_NAME_ROOM)) + 2
            })
    } else {
        0
    };
    let unseen = if session.unseen { 2 } else { 0 };
    width
        .saturating_sub(lead_width + 2 + logos::width(app.graphics) + project + unseen)
        .max(4)
}

/// A session's two lines after `lead`: its mark and its name, whole where
/// it fits (slid `by` characters where it does not), with its project's
/// name first with `place`; then, under the name, what it is doing and
/// how long it has been so, with its branch with `place`, quietly.
fn session_pair(
    app: &App,
    session: &Session,
    lead: Vec<Span<'static>>,
    width: usize,
    place: bool,
    by: usize,
) -> [Line<'static>; 2] {
    let theme = super::theme::of(app);
    let (symbol, style) = mark(app, session.status);
    let lead_width: usize = lead.iter().map(Span::width).sum();
    let mut first = lead.clone();
    first.extend([Span::styled(symbol, style), Span::raw(" ")]);
    first.extend(logos::spans(app.graphics, session.shown_harness()));
    if place
        && let Some(id) = app.model.project_of(Row::Session(session.id))
        && let Some(found) = app.model.project(id)
    {
        first.extend([
            Span::styled(
                shorten(&found.name, PROJECT_NAME_ROOM),
                Style::default()
                    .fg(look::project_colour(app, id))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::raw("  "),
        ]);
    }
    let room = pair_title_room(app, session, lead_width, width, place);
    first.push(Span::styled(
        slid(&session.title, room, by),
        Style::default().add_modifier(Modifier::BOLD),
    ));
    if session.unseen {
        first.push(news_dot(app, " "));
    }

    // Under the name, past the mark and the logo.
    let indent = 2 + logos::width(app.graphics);
    let mut second = lead;
    second.push(Span::raw(" ".repeat(indent)));
    let mut doing = look::doing(session).into_owned();
    if place {
        let branch = app.model.branch(session.workspace).unwrap_or("?");
        doing = format!("{doing} · {branch}");
    }
    let time = look::session_time(app, session).unwrap_or_default();
    let room = width.saturating_sub(lead_width + indent);
    let time_room = if time.is_empty() {
        0
    } else {
        cells::width(&time) + 3
    };
    let doing = shorten(&doing, room.saturating_sub(time_room).max(4));
    second.push(Span::styled(doing, doing_style(app, session)));
    if !time.is_empty() {
        second.push(Span::styled(format!(" · {time}"), theme.quiet));
    }
    [Line::from(first), Line::from(second)]
}

/// A session's line after `lead`: its mark, its title (slid `by`
/// characters where it is cut short) and what it is doing, and at the end
/// how long it has been so, as [`session_parts`] shares out the room.
fn session_line(
    app: &App,
    session: &Session,
    lead: Vec<Span<'static>>,
    width: usize,
    place: bool,
    by: usize,
) -> Line<'static> {
    let (symbol, style) = mark(app, session.status);
    let lead_width = lead.iter().map(Span::width).sum();
    let SessionParts {
        project,
        title_room,
        doing,
        time,
    } = session_parts(app, session, lead_width, width, place);
    let title = slid(&session.title, title_room, by);
    let doing_style = doing_style(app, session);
    let mut spans = lead;
    spans.extend([Span::styled(symbol, style), Span::raw(" ")]);
    spans.extend(logos::spans(app.graphics, session.shown_harness()));
    spans.extend(project);
    spans.push(Span::raw(title));
    if !doing.is_empty() {
        spans.push(Span::raw("  "));
        spans.push(Span::styled(doing, doing_style));
    }
    if session.unseen {
        spans.push(news_dot(app, " "));
    }
    if !time.is_empty() {
        let used: usize = spans.iter().map(Span::width).sum();
        let gap = width.saturating_sub(used + cells::width(&time)).max(1);
        spans.push(Span::raw(" ".repeat(gap)));
        spans.push(Span::styled(time, super::theme::of(app).quiet));
    }
    Line::from(spans)
}

/// The name on the selected line of the list shown, `width` cells inside
/// its border, where it is cut short: a session's title, or a worktree's
/// branch. How many characters it has and how many it is given.
pub(super) fn cut_name(app: &App, width: usize) -> Option<(usize, usize)> {
    let (name, room) = match app.view {
        View::Sessions => match app.selected? {
            Row::Project(_) => return None,
            Row::Worktree(id) => {
                let kind = &app.model.workspace(id)?.kind;
                if !kind.in_git() || app.model.run_of(id).is_some() {
                    return None;
                }
                let branch = app.model.branch(id)?;
                let lead = worktree_lead(app, id, Span::raw(" "));
                let standing = standing(app, Row::Worktree(id));
                let room = width
                    .saturating_sub(pull_request::width(&lead) + pull_request::width(&standing));
                (branch, branch_and_tail(app, id, branch, room).1)
            }
            Row::Session(id) => {
                let session = app.model.session(id)?;
                let lead = pull_request::width(&session_lead(Span::raw(" ")));
                (
                    session.title.as_str(),
                    title_room(app, session, lead, width, false),
                )
            }
        },
        View::NeedsYou => {
            let session = app.model.session(app.needs.selected?)?;
            // Led by its edge alone, a cell.
            (
                session.title.as_str(),
                title_room(app, session, 1, width, true),
            )
        }
        View::Tickets | View::Settings => return None,
    };
    let len = cells::width(name);
    (len > room).then_some((len, room))
}

/// The cells a session's name is given on its line, as many lines as it
/// takes.
fn title_room(app: &App, session: &Session, lead_width: usize, width: usize, place: bool) -> usize {
    if app.session_height() == 1 {
        session_parts(app, session, lead_width, width, place).title_room
    } else {
        pair_title_room(app, session, lead_width, width, place)
    }
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
        look::selected_style(app, app.list_has_keys())
    }
}

/// `text`, a session's lines, flashing if its status has just changed:
/// lit in the status's colour, then tinted with it, then as it was.
fn flashed(app: &App, session: &Session, mut text: Text<'static>) -> Text<'static> {
    let Some(age) = app.flash_age(session.id) else {
        return text;
    };
    let theme = super::theme::of(app);
    let colour = status_symbol(&theme, session.status, None)
        .1
        .fg
        .unwrap_or(theme.text);
    let flash = motion::flash(colour, theme.on_accent, age);
    for line in &mut text.lines {
        match flash {
            Some(motion::Flash::Lit(style)) => {
                for span in &mut line.spans {
                    span.style = Style::default();
                }
                line.style = style;
            }
            Some(motion::Flash::Tinted(style)) => line.style = style,
            None => {}
        }
    }
    text
}

pub(super) fn draw_needs_you(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = super::theme::of(app);
    let block = list_block(app, "waiting on you", app.list_has_keys());
    if app.needs.list.is_empty() {
        let text = Text::from(vec![
            Line::from(""),
            Line::from(" ✓ All clear").fg(theme.done).bold(),
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
        .needs
        .list
        .iter()
        .filter_map(|id| app.model.session(*id))
        .map(|session| {
            let selected = app.needs.selected == Some(session.id);
            let project = app.model.project_of(Row::Session(session.id));
            let lead = vec![look::edge(app, project, selected)];
            let by = app.slid_by(View::NeedsYou, Row::Session(session.id));
            // Mixed from every project, so each line says whose it is.
            let lines = session_lines(app, session, lead, width, true, by);
            ListItem::new(flashed(app, session, lines))
        })
        .collect();
    let mut state = ListState::default()
        .with_offset(app.needs.offset)
        .with_selected(
            app.needs
                .selected
                .and_then(|s| app.needs.list.iter().position(|id| *id == s)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(session_highlight(app, app.needs.selected));
    frame.render_stateful_widget(list, area, &mut state);
    let sessions: Vec<Option<&Session>> = app
        .needs
        .list
        .iter()
        .filter_map(|id| app.model.session(*id))
        .map(Some)
        .collect();
    // Led by its edge alone, a cell.
    let heights = app.needs_heights();
    draw_logos(frame, app, area, (&sessions, &heights, state.offset()), 1);
}

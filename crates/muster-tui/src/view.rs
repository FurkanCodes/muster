//! Drawing the app. Pure: reads the app, writes the frame.

use muster_core::layout::Panel;
use muster_core::num::fraction;
use muster_core::{SessionStatus, WaitReason};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

use crate::app::{App, Mode, View};
use crate::layout::{Divider, Solved};
use crate::motion;

pub(crate) mod areas;
mod bottom_bar;
mod changes;
mod clashes;
mod dialogs;
mod drag;
mod help;
pub(crate) mod kep;
pub(crate) mod logos;
pub(crate) mod look;
mod menu;
pub(crate) mod picture;
mod plugin_dialog;
mod plugins;
mod preview;
mod pull_request;
mod rebind;
mod selection;
mod sessions;
mod settings;
mod telling;
mod terminal;
pub(crate) mod theme;
mod tickets;
mod top_bar;
mod version;

use bottom_bar::{bottom_height, draw_bottom_bar};
pub(crate) use menu::menu_area;
pub(crate) use plugin_dialog::preview_lines;
use preview::draw_preview;
use sessions::{draw_needs_you, draw_sessions};
pub(crate) use sessions::{fold_column, list_width};
use terminal::{draw_session, draw_terminal};

/// The terminal area a session gets when it fills the screen: everything
/// but the top bar.
pub(crate) fn terminal_area(area: Rect) -> Rect {
    Rect {
        y: area.y + 1,
        height: area.height.saturating_sub(1),
        ..area
    }
}

/// Where the list view's parts are drawn: the top and bottom bars, and
/// each panel shown (an empty area for one that is not).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BrowseAreas {
    pub(crate) top: Rect,
    pub(crate) list: Rect,
    /// The session panel: the selected session's terminal, or what the
    /// screen shows of the line selected.
    pub(crate) preview: Rect,
    pub(crate) changes: Rect,
    /// The shell beside the session.
    pub(crate) shell: Rect,
    pub(crate) bottom: Rect,
    /// The lines between panels, which a drag moves.
    pub(crate) dividers: Vec<Divider>,
}

/// The list view's layout in `area`, shared with the app so the mouse
/// lands where things are drawn. While a session beside the list has the
/// keys, the bottom bar keeps to one line, so the session keeps its size.
pub(crate) fn browse_areas(app: &App, area: Rect) -> BrowseAreas {
    let typing = matches!(app.mode, Mode::Terminal(_));
    let bottom = if typing {
        1
    } else {
        bottom_height(app, area.width)
    };
    // The tickets' and the settings' lists have no session beside them,
    // but one opened from there.
    let beside_session = typing || matches!(app.view, View::Sessions | View::NeedsYou);
    areas(app, area, bottom, beside_session)
}

/// Where a session's terminal is drawn in `area`, inside its border: the
/// size its program is given. Measured as while it has the keys, with a
/// bottom bar of one line, so the program keeps its size as the person
/// goes into it and out.
pub(crate) fn session_area(app: &App, area: Rect) -> Rect {
    areas(app, area, 1, true).preview.inner(Margin::new(1, 1))
}

/// Where the shell beside the session is drawn in `area`, inside its
/// border, measured as [`session_area`] is.
pub(crate) fn shell_area(app: &App, area: Rect) -> Rect {
    areas(app, area, 1, true).shell.inner(Margin::new(1, 1))
}

fn areas(app: &App, area: Rect, bottom: u16, beside_session: bool) -> BrowseAreas {
    let key = areas::Key::of(app, area, bottom, beside_session);
    app.areas_cache
        .borrow_mut()
        .get(key, || areas_uncached(app, area, bottom, beside_session))
}

fn areas_uncached(app: &App, area: Rect, bottom: u16, beside_session: bool) -> BrowseAreas {
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(bottom),
    ])
    .areas(area);
    let solved = if !app.narrow() {
        app.solve_panels(body, list_width(app, beside_session, body.width))
    } else if app.pane_fills_width() {
        // Too narrow for both: the changes, which have the keys, or the
        // list.
        Solved {
            panels: vec![(Panel::Changes, body)],
            dividers: Vec::new(),
        }
    } else {
        Solved {
            panels: vec![(Panel::List, body)],
            dividers: Vec::new(),
        }
    };
    BrowseAreas {
        top,
        list: solved.of(Panel::List),
        preview: solved.of(Panel::Session),
        changes: solved.of(Panel::Changes),
        shell: solved.of(Panel::Shell),
        bottom,
        dividers: solved.dividers,
    }
}

pub(crate) fn draw(frame: &mut Frame<'_>, app: &App) {
    let area = frame.area();
    if app.welcome {
        kep::draw_welcome(frame, app, area);
        return;
    }
    if let Mode::Terminal(session) = app.mode
        && app.fills_window()
    {
        draw_terminal(frame, app, session, area);
        return;
    }
    let theme = theme::of(app);
    if theme.text != Color::Reset {
        // Muster's own text, under everything drawn over it.
        frame
            .buffer_mut()
            .set_style(area, Style::default().fg(theme.text));
    }
    let areas = browse_areas(app, area);
    let BrowseAreas {
        top,
        list,
        preview,
        changes,
        shell,
        bottom,
        ..
    } = areas.clone();
    top_bar::draw_top_bar(frame, app, top);
    if !list.is_empty() {
        match app.view {
            View::Sessions => draw_sessions(frame, app, list),
            View::NeedsYou => draw_needs_you(frame, app, list),
            View::Tickets => tickets::draw_tickets(frame, app, list),
            View::Settings => settings::draw_settings(frame, app, list),
        }
    }
    if !preview.is_empty() {
        if let Mode::Terminal(session) = app.mode
            && !app.typing_in_shell()
        {
            draw_session(frame, app, session, preview, true);
        } else if app.view == View::Tickets {
            tickets::draw_ticket_details(frame, app, preview);
        } else if app.view == View::Settings {
            settings::draw_setting_details(frame, app, preview);
        } else {
            draw_preview(frame, app, preview);
        }
    }
    if !changes.is_empty() {
        changes::draw_changes(frame, app, changes);
    }
    if !shell.is_empty()
        && let Some(beside) = app.shell_beside()
    {
        draw_session(frame, app, beside, shell, app.typing_in_shell());
    }
    drag::draw_moving(frame, app, &areas);
    draw_bottom_bar(frame, app, bottom);
    match &app.mode {
        Mode::Dialog(dialog) => dialogs::draw_dialog(frame, app, dialog, area),
        Mode::Menu(open) => menu::draw_menu(frame, open, area),
        Mode::Help(open) => help::draw_help(frame, app, open, area),
        _ => {}
    }
}

/// `text`, cut to `max` characters with an ellipsis if it is longer.
pub(super) fn shorten(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max.saturating_sub(1)).collect();
    format!("{kept}…")
}

/// `text` in `max` characters, slid `by` characters to the left as a
/// name cut short slides along (`motion::slide`): an ellipsis stands for
/// what is out of view at either end. Slid by nothing, it is `text` as
/// [`shorten`] cuts it.
pub(super) fn slid(text: &str, max: usize, by: usize) -> String {
    let len = text.chars().count();
    if by == 0 || len <= max {
        return shorten(text, max);
    }
    let by = by.min(len - max);
    let mut shown: Vec<char> = text.chars().skip(by).take(max).collect();
    if let Some(first) = shown.first_mut() {
        *first = '…';
    }
    if by + max < len
        && let Some(last) = shown.last_mut()
    {
        *last = '…';
    }
    shown.into_iter().collect()
}

/// The name on the selected line where the list cuts it short: how many
/// characters it has and how many it is given, for it to slide.
pub(crate) fn cut_name(app: &App) -> Option<(usize, usize)> {
    let window = Rect::new(0, 0, app.terminal_size.cols, app.terminal_size.rows + 1);
    let list = browse_areas(app, window).list;
    if list.is_empty() {
        return None;
    }
    let width = usize::from(list.width.saturating_sub(2));
    sessions::cut_name(app, width)
}

/// A pane beside the list, framed: its title in its border, the frame in
/// the accent while it has the keys and quiet otherwise.
pub(super) fn panel<'a>(app: &App, title: impl Into<Line<'a>>, focused: bool) -> Block<'a> {
    let theme = theme::of(app);
    let (frame, title_style) = if focused {
        (theme.accent, Style::default().fg(theme.accent).bold())
    } else {
        (theme.line, theme.quiet)
    };
    Block::default()
        .title(title)
        .title_style(title_style)
        .borders(Borders::ALL)
        .border_style(Style::default().fg(frame))
}

/// A list, framed square like the pane beside it: its label in its
/// border, the frame and label in the accent while the list has the keys,
/// so one frame at a time is lit.
pub(super) fn list_block(app: &App, label: &str, has_keys: bool) -> Block<'static> {
    panel(app, format!(" {label} "), has_keys)
}

/// A session's mark as `app` draws it now, in its palette and at its
/// motion's step.
pub(super) fn mark(app: &App, status: SessionStatus) -> (&'static str, Style) {
    status_symbol(&theme::of(app), status, app.motion_step())
}

/// A session's mark in `theme`: at a motion `step`, a running one's spins
/// and one that needs the person pulses; without one, they stand still.
pub(super) fn status_symbol(
    theme: &theme::Theme,
    status: SessionStatus,
    step: Option<u64>,
) -> (&'static str, Style) {
    match status {
        SessionStatus::Fresh => ("○", Style::default().fg(theme.still)),
        SessionStatus::Running => (
            step.map_or("●", motion::spinner),
            Style::default().fg(theme.working),
        ),
        SessionStatus::NeedsYou { .. } => {
            let style = Style::default().fg(theme.waiting).bold();
            ("▲", step.map_or(style, |step| motion::pulse(style, step)))
        }
        SessionStatus::Finished => ("✓", Style::default().fg(theme.done)),
        SessionStatus::Exited { success: true } => ("■", Style::default().fg(theme.still)),
        SessionStatus::Exited { success: false } => ("✕", Style::default().fg(theme.waiting)),
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
        format!("{:.1} GB", fraction(count) / fraction(GB))
    } else {
        format!("{} MB", count.div_ceil(MB))
    }
}

//! Drawing the app. Pure: reads the app, writes the frame.

use muster_core::num::fraction;
use muster_core::{SessionStatus, WaitReason};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Padding};

use crate::app::{App, Mode, View};
use crate::motion;

mod bottom_bar;
mod changes;
mod clashes;
mod dialogs;
mod help;
pub(crate) mod kep;
pub(crate) mod logos;
pub(crate) mod look;
mod menu;
mod preview;
mod pull_request;
mod selection;
mod sessions;
mod settings;
mod terminal;
pub(crate) mod theme;
mod tickets;
mod top_bar;
mod version;

use bottom_bar::{bottom_height, draw_bottom_bar};
pub(crate) use menu::menu_area;
use preview::draw_preview;
pub(crate) use sessions::fold_column;
use sessions::{draw_needs_you, draw_sessions};
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

/// Where the list view's parts are drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct BrowseAreas {
    pub(crate) top: Rect,
    pub(crate) list: Rect,
    pub(crate) preview: Rect,
    pub(crate) bottom: Rect,
}

/// How the body of the window is shared between the list and the pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Shape {
    /// The list, and the pane beside it.
    SideBySide,
    /// The list alone: a window too narrow for both.
    List,
    /// The pane alone, which has the keys, in a window too narrow for both.
    Pane,
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
    let shape = if !app.narrow() {
        Shape::SideBySide
    } else if app.pane_fills_width() {
        Shape::Pane
    } else {
        Shape::List
    };
    layout(area, bottom, shape)
}

/// Where a session's terminal is drawn beside the list in `area`, inside
/// its border: the size its program is given.
pub(crate) fn session_area(area: Rect) -> Rect {
    layout(area, 1, Shape::SideBySide)
        .preview
        .inner(Margin::new(1, 1))
}

fn layout(area: Rect, bottom: u16, shape: Shape) -> BrowseAreas {
    let [top, body, bottom] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(3),
        Constraint::Length(bottom),
    ])
    .areas(area);
    // The list takes what its lines need, and the session beside it, where
    // the person works with Claude Code, the rest.
    let list_width = match shape {
        Shape::SideBySide => u16::try_from((u32::from(body.width) * 3 / 10).clamp(34, 48))
            .unwrap_or(48)
            .min(body.width),
        Shape::List => body.width,
        Shape::Pane => 0,
    };
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
    } else if app.mode == Mode::Changes
        || matches!(
            app.mode,
            Mode::Dialog(crate::app::Dialog::Undo { .. })
                | Mode::Help(crate::app::Help {
                    over_changes: true,
                    ..
                })
        )
    {
        changes::draw_changes(frame, app, preview);
    } else if app.view == View::Tickets {
        tickets::draw_ticket_details(frame, app, preview);
    } else if app.view == View::Settings {
        settings::draw_setting_details(frame, app, preview);
    } else {
        draw_preview(frame, app, preview);
    }
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
    let width = usize::from(browse_areas(app, window).list.width.saturating_sub(2));
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

/// A list, unframed: a quiet label over it, in the accent while the list
/// has the keys. Its lines sit where a frame would leave them, a cell in
/// from each side and a line down, so the list keeps its place.
pub(super) fn list_block(app: &App, label: &str, has_keys: bool) -> Block<'static> {
    let theme = theme::of(app);
    let style = if has_keys {
        Style::default().fg(theme.accent).bold()
    } else {
        theme.quiet
    };
    Block::default()
        .title(Line::styled(format!(" {label}"), style))
        .padding(Padding::new(1, 1, 0, 1))
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

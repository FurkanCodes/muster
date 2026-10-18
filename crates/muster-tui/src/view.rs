//! Drawing the app. Pure: reads the app, writes the frame.

use muster_core::num::fraction;
use muster_core::{SessionStatus, WaitReason};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, BorderType, Borders};

use crate::app::{App, Mode, View};
use crate::motion;

mod bottom_bar;
mod changes;
mod clashes;
mod dialogs;
pub(crate) mod kep;
pub(crate) mod look;
mod preview;
mod pull_request;
mod selection;
mod sessions;
mod settings;
mod terminal;
mod tickets;
mod top_bar;
mod version;

use bottom_bar::{bottom_height, draw_bottom_bar};
use preview::draw_preview;
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
    if app.welcome {
        kep::draw_welcome(frame, app, area);
        return;
    }
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
    } else if app.mode == Mode::Changes
        || matches!(app.mode, Mode::Dialog(crate::app::Dialog::Undo { .. }))
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
    if let Mode::Dialog(dialog) = &app.mode {
        dialogs::draw_dialog(frame, app, dialog, area);
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
        format!("{:.1} GB", fraction(count) / fraction(GB))
    } else {
        format!("{} MB", count.div_ceil(MB))
    }
}

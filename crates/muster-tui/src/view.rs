//! Drawing the app. Pure: reads the app, writes the frame.

use muster_core::layout::Panel;
use muster_core::num::fraction;
use muster_core::{SessionId, SessionStatus, WaitReason};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Margin, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};

use crate::app::{App, Link, Mode, View};
use crate::cells;
use crate::layout::{Divider, Solved};
use crate::motion;

pub(crate) mod areas;
pub(crate) mod bottom_bar;
mod changes;
mod clashes;
mod delegation;
mod dialogs;
mod drag;
mod floating;
mod help;
pub(crate) mod kep;
mod loading;
pub(crate) mod logos;
pub(crate) mod look;
mod markdown;
mod menu;
mod naming;
pub(crate) mod pane;
pub(crate) mod picture;
mod plugin_dialog;
mod plugins;
mod preview;
mod pull_request;
mod rebind;
mod selection;
mod sessions;
mod settings;
mod shared_builds;
mod telling;
mod terminal;
pub(crate) mod theme;
mod tickets;
pub(crate) mod top_bar;
mod version;

use bottom_bar::{bottom_height, draw_bottom_bar};
pub(crate) use menu::menu_area;
pub(crate) use plugin_dialog::preview_lines;
use preview::draw_preview;
use sessions::{draw_needs_you, draw_sessions};
pub(crate) use sessions::{fold_column, list_width};
use terminal::{draw_session, draw_terminal};
pub(crate) use tickets::{
    details_height as ticket_details_height, part_lines as ticket_part_lines,
};

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
    /// The preview: the selected session's terminal, or what the screen
    /// shows of the line selected.
    pub(crate) preview: Rect,
    pub(crate) changes: Rect,
    /// A plugin's panel beside the session.
    pub(crate) plugin: Rect,
    /// The sessions placed in panels of their own.
    pub(crate) placed: Vec<(SessionId, Rect)>,
    /// How many panels the window is too narrow for, shown over by the one
    /// with the keys.
    pub(crate) hidden: usize,
    pub(crate) bottom: Rect,
    /// The lines between panels, which a drag moves.
    pub(crate) dividers: Vec<Divider>,
}

impl BrowseAreas {
    /// Where `panel` is drawn; nowhere (an empty area) when not shown.
    pub(crate) fn of(&self, panel: Panel) -> Rect {
        match panel {
            Panel::List => self.list,
            Panel::Session => self.preview,
            Panel::Changes => self.changes,
            Panel::Plugin => self.plugin,
            Panel::Placed(session) => self
                .placed
                .iter()
                .find(|(placed, _)| *placed == session)
                .map_or(Rect::default(), |(_, area)| *area),
        }
    }

    /// Every panel shown, each where it is drawn.
    pub(crate) fn shown(&self) -> Vec<(Panel, Rect)> {
        [
            (Panel::List, self.list),
            (Panel::Session, self.preview),
            (Panel::Changes, self.changes),
            (Panel::Plugin, self.plugin),
        ]
        .into_iter()
        .chain(
            self.placed
                .iter()
                .map(|(session, area)| (Panel::Placed(*session), *area)),
        )
        .filter(|(_, area)| !area.is_empty())
        .collect()
    }
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

/// Where `panel` (a plugin's panel, a session placed) is drawn
/// in `area`, inside its border, measured as [`session_area`] is.
pub(crate) fn panel_area(app: &App, area: Rect, panel: Panel) -> Rect {
    areas(app, area, 1, true).of(panel).inner(Margin::new(1, 1))
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
            ..Solved::default()
        }
    } else {
        Solved {
            panels: vec![(Panel::List, body)],
            ..Solved::default()
        }
    };
    BrowseAreas {
        top,
        list: solved.of(Panel::List),
        preview: solved.of(Panel::Session),
        changes: solved.of(Panel::Changes),
        plugin: solved.of(Panel::Plugin),
        placed: solved
            .panels
            .iter()
            .filter_map(|(panel, area)| panel.placed().map(|session| (session, *area)))
            .collect(),
        bottom,
        hidden: solved.hidden,
        dividers: solved.dividers,
    }
}

/// The hints for the selected line of *Tickets* or *Settings*, which its
/// right-click menu offers too.
pub(crate) fn line_hints(app: &App) -> Vec<String> {
    match app.view {
        View::Tickets => tickets::hints(app),
        View::Settings => settings::hints(app),
        View::Sessions | View::NeedsYou => Vec::new(),
    }
}

/// Where the list's fold button is (`app::controls`): ` ‹ ` at the right
/// end of the list's top border, or while the list is folded away, `›` in
/// the corner of the window's body, where it comes back.
pub(crate) fn fold_button(app: &App, areas: &BrowseAreas) -> Option<Rect> {
    if app.narrow() || !matches!(app.view, View::Sessions | View::NeedsYou) {
        return None;
    }
    if !areas.list.is_empty() {
        let list = areas.list;
        return (list.width >= 16).then(|| Rect::new(list.right() - 4, list.y, 3, 1));
    }
    let body_y = areas.top.bottom();
    (areas.bottom.y > body_y).then(|| Rect::new(areas.top.x, body_y, 1, 1))
}

/// Draws the fold button, lit under the pointer.
fn draw_fold_button(frame: &mut Frame<'_>, app: &App, areas: &BrowseAreas) {
    let Some(cells) = fold_button(app, areas) else {
        return;
    };
    let theme = theme::of(app);
    let mark = if areas.list.is_empty() {
        "›"
    } else {
        " ‹ "
    };
    let style = if app.person.pointer.is_some_and(|at| cells.contains(at)) {
        theme.pressable()
    } else {
        theme.quiet
    };
    frame.render_widget(Line::styled(mark, style), cells);
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
        plugin,
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
            && !app.typing_in_plugin()
            && app.typing_in_placed().is_none()
        {
            draw_session(frame, app, session, preview, Panel::Session, &areas);
        } else if app.view == View::Tickets {
            tickets::draw_ticket_details(frame, app, preview);
        } else if app.view == View::Settings {
            settings::draw_setting_details(frame, app, preview);
        } else {
            draw_preview(frame, app, preview, &areas);
        }
    }
    if !changes.is_empty() {
        changes::draw_changes(frame, app, changes);
    }
    if !plugin.is_empty()
        && let Some(beside) = app.plugin_beside()
    {
        draw_session(frame, app, beside, plugin, Panel::Plugin, &areas);
    }
    for (session, area) in &areas.placed {
        draw_session(frame, app, *session, *area, Panel::Placed(*session), &areas);
    }
    draw_fold_button(frame, app, &areas);
    drag::draw_moving(frame, app, &areas);
    // The daemon out of reach: what it last said stays, dimmed, until it
    // answers again; the bars above and below say so.
    if matches!(app.link, Link::Lost(_)) {
        let between = Rect {
            y: top.bottom(),
            height: bottom.y.saturating_sub(top.bottom()),
            ..area
        };
        frame
            .buffer_mut()
            .set_style(between, Style::default().add_modifier(Modifier::DIM));
    }
    draw_bottom_bar(frame, app, bottom);
    match &app.mode {
        Mode::Dialog(dialog) => dialogs::draw_dialog(frame, app, dialog, area),
        Mode::Menu(open) => menu::draw_menu(frame, open, area, theme::of(app)),
        Mode::Help(open) => help::draw_help(frame, app, open, area),
        _ => {}
    }
}

/// `text`, cut to `max` columns with an ellipsis if it is longer.
pub(super) fn shorten(text: &str, max: usize) -> String {
    if cells::width(text) <= max {
        return text.to_owned();
    }
    format!("{}…", cells::start_within(text, max.saturating_sub(1)))
}

/// `text` in `max` columns, slid `by` columns to the left as a name cut
/// short slides along (`motion::slide`): an ellipsis stands for what is
/// out of view at either end. Slid by nothing, it is `text` as [`shorten`]
/// cuts it.
pub(super) fn slid(text: &str, max: usize, by: usize) -> String {
    let len = cells::width(text);
    if by == 0 || len <= max {
        return shorten(text, max);
    }
    let by = by.min(len - max);
    let more = by + max < len;
    let room = max.saturating_sub(1 + usize::from(more));
    let shown = cells::start_within(cells::after(text, by + 1), room);
    format!("…{shown}{}", if more { "…" } else { "" })
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

/// What a screen that lists projects says while there are none.
fn no_projects(app: &App) -> String {
    format!(
        "No projects yet: add one in the sessions view with {}.",
        app.key(muster_core::keys::Action::AddProject)
    )
}

#[cfg(test)]
mod tests {
    use super::{shorten, slid};
    use crate::cells;

    #[test]
    fn a_name_is_cut_to_the_columns_it_is_given_wide_characters_and_all() {
        assert_eq!(shorten("fix login", 20), "fix login");
        assert_eq!(shorten("fix the login", 6), "fix t…");
        let wide = "修正ログイン画面";
        let cut = shorten(wide, 7);
        assert_eq!(cut, "修正ロ…");
        assert!(cells::width(&cut) <= 7);
        for by in 0..10 {
            let shown = slid(wide, 7, by);
            assert!(cells::width(&shown) <= 7, "{by}: {shown}");
        }
        assert_eq!(slid("abcdefgh", 5, 1), "…cde…");
        assert_eq!(slid("abcdefgh", 5, 3), "…efgh");
    }
}

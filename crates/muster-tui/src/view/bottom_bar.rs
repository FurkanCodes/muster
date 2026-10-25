//! The bottom bar: the keys that do something for what is selected, or
//! the last note, or why the daemon cannot be reached, and a warning when
//! this UI is older than the daemon. Each hint that names one key is a
//! button too: a click on it presses that key (`hint_at`), so whatever the
//! keys do there the mouse does (`ARC-32`).

use muster_core::keys::Action as Does;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::{settings, tickets, version};
use crate::app::{App, Link, Mode, View};
use crate::cells;

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

/// Over the hints while the list has the keys: that this UI is older than
/// the daemon, and how to start the new one.
fn stale(app: &App) -> Option<String> {
    match (&app.link, &app.mode) {
        (_, Mode::Terminal(_)) => None,
        (Link::Connected { daemon_version }, _) => version::stale_warning(daemon_version),
        _ => None,
    }
}

/// How many lines the bottom bar needs: hints take one, a long message up
/// to three, so the reason in an error is not cut off; a warning that this
/// UI is old, up to two over the hints.
pub(crate) fn bottom_height(app: &App, width: u16) -> u16 {
    let lines = |text: &str| {
        let length = u16::try_from(cells::width(&note_text(text))).unwrap_or(u16::MAX);
        length.div_ceil(width.max(1))
    };
    match (message(app), stale(app)) {
        (Some((text, _)), _) => lines(text).clamp(1, MAX_NOTE_LINES),
        (None, Some(warning)) => 1 + lines(&warning).clamp(1, MAX_NOTE_LINES - 1),
        (None, None) => 1,
    }
}

pub(super) fn draw_bottom_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = super::theme::of(app);
    if message(app).is_none()
        && let Some(warning) = stale(app)
    {
        let [above, hints_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let warning = Line::from(note_text(&warning)).fg(theme.working);
        frame.render_widget(Paragraph::new(warning).wrap(Wrap { trim: false }), above);
        draw_hints(frame, app, hints_area);
        return;
    }
    if let Some((text, error)) = message(app) {
        let text = Line::from(note_text(text));
        let text = match (error, app.note_fading()) {
            (true, _) => text.fg(theme.waiting),
            (false, true) => text.fg(theme.done).dim(),
            (false, false) => text.fg(theme.done),
        };
        frame.render_widget(Paragraph::new(text).wrap(Wrap { trim: false }), area);
        return;
    }
    draw_hints(frame, app, area);
}

/// The hints in `area`, as many as fit whole, and at its end the key of
/// the help, where it opens, so the way to every other key is always in
/// view while the list has the keys.
fn draw_hints(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = super::theme::of(app);
    let hover = app.person.pointer;
    for (text, cells) in placed_hints(app, area) {
        // One under the pointer that a click presses is lit.
        let lit = hover.is_some_and(|at| cells.contains(at)) && clickable(&text).is_some();
        let line = if lit {
            Line::styled(text, theme.pressable())
        } else {
            Line::from(text).dim()
        };
        frame.render_widget(line, cells);
    }
}

/// Each hint drawn in the bar at `area`, and the cells it takes.
fn placed_hints(app: &App, area: Rect) -> Vec<(String, Rect)> {
    let help_hint = (app.mode == Mode::Browse)
        .then(|| hint(app, Does::Help, "help"))
        .flatten();
    let help_width = help_hint
        .as_ref()
        .map_or(0, |hint| u16::try_from(cells::width(hint) + 1).unwrap_or(0));
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(help_width)]).areas(area);
    // A space before them, and two before the help.
    let room = usize::from(left.width).saturating_sub(if help_hint.is_some() { 3 } else { 1 });
    let mut placed = Vec::new();
    let mut x = left.x + 1;
    for (at, text) in fitting(&hints(app), room).into_iter().enumerate() {
        if at > 0 {
            x += 2;
        }
        let width = u16::try_from(cells::width(&text)).unwrap_or(u16::MAX);
        let shown = width.min(left.right().saturating_sub(x));
        placed.push((text, Rect::new(x, area.y, shown, 1)));
        x = x.saturating_add(width);
    }
    if let Some(help_hint) = help_hint {
        let width = u16::try_from(cells::width(&help_hint)).unwrap_or(0);
        placed.push((
            help_hint,
            Rect::new(right.x, area.y, width.min(right.width), 1),
        ));
    }
    placed
}

/// The key a click on `hint` presses: the one its label names first.
fn clickable(hint: &str) -> Option<crossterm::event::KeyEvent> {
    crate::keys::labelled(hint.split(' ').next()?)
}

/// Each hint in the bar at `area` that names a key, the key and where the
/// hint is: a click on it presses that key. None while the bar says a
/// note instead.
pub(crate) fn hint_areas(app: &App, area: Rect) -> Vec<(crossterm::event::KeyEvent, Rect)> {
    if message(app).is_some() || area.height == 0 {
        return Vec::new();
    }
    // Under a warning that this UI is old, the hints are on the last line.
    let line = Rect {
        y: area.bottom().saturating_sub(1),
        height: 1,
        ..area
    };
    placed_hints(app, line)
        .into_iter()
        .filter_map(|(text, cells)| clickable(&text).map(|key| (key, cells)))
        .collect()
}

/// As many of `hints` as fit in `room` cells, whole, two spaces apart:
/// the first however long, cut where the bar ends.
fn fitting(hints: &[String], room: usize) -> Vec<String> {
    let mut fitted: Vec<String> = Vec::new();
    let mut used = 0;
    for hint in hints {
        let width = cells::width(hint);
        if !fitted.is_empty() {
            if used + 2 + width > room {
                break;
            }
            used += 2;
        }
        used += width;
        fitted.push(hint.clone());
    }
    fitted
}

/// `does`'s key and `what`, as a hint; none where it has no key.
pub(super) fn hint(app: &App, does: Does, what: &str) -> Option<String> {
    app.keymap
        .chords(does)
        .first()
        .map(|chord| format!("{} {what}", chord.label()))
}

/// The keys that do something for what is selected, most used first.
fn hints(app: &App) -> Vec<String> {
    if app.panels.arranging.is_some() {
        return [
            "←→↑↓ move it",
            "+ wider",
            "- narrower",
            "⇥ another",
            "⏎ keep",
            "esc put back",
        ]
        .map(str::to_owned)
        .to_vec();
    }
    if app.panels.waiting.is_some() {
        return panel_key_hints(app);
    }
    if app.panels.resizing.is_some() {
        return ["hjkl move its edge", "HJKL five", "⏎ keep", "esc put back"]
            .map(str::to_owned)
            .to_vec();
    }
    if let Mode::Menu(_) = app.mode {
        return ["↑↓ choose", "⏎ do it, or press its key", "esc close"]
            .map(str::to_owned)
            .to_vec();
    }
    if let Mode::Help(_) = app.mode {
        return ["type to narrow", "↑↓ scroll", "esc close"]
            .map(str::to_owned)
            .to_vec();
    }
    if matches!(app.mode, Mode::Terminal(_)) {
        let back = hint(app, Does::LeaveSession, "back to the list (or click it)")
            .unwrap_or_else(|| "a click on the list goes back to it".to_owned());
        let mut hints = vec!["keys go to the session".to_owned(), back];
        hints.extend(hint(app, Does::PanelKey, "panels"));
        return hints;
    }
    if app.mode == Mode::Changes {
        let diff = app.changes.as_ref().is_some_and(|pane| pane.diff.is_some());
        let entry = app
            .changes
            .as_ref()
            .is_some_and(|pane| pane.selected_entry().is_some());
        let again = hint(app, Does::ChangesAgain, "again");
        let undo = entry
            .then(|| hint(app, Does::PutBack, "put back as before it"))
            .flatten();
        let mut hints: Vec<String> = if diff {
            vec![
                "↑↓ scroll".to_owned(),
                "PgUp PgDn page".to_owned(),
                "esc the changes".to_owned(),
            ]
        } else {
            vec!["↑↓ move".to_owned(), "⏎ its diff".to_owned()]
        };
        hints.extend(again);
        hints.extend(undo);
        hints.extend(if diff {
            hint(app, Does::CloseChanges, "back")
        } else {
            hint(app, Does::CloseChanges, "back to the list")
        });
        hints.extend(hint(app, Does::ChangesHelp, "keys"));
        return hints;
    }
    let mut hints = vec!["↑↓ move".to_owned()];
    let general = match app.view {
        View::Tickets => {
            hints.extend(tickets::hints(app));
            [
                hint(app, Does::NextTab, "next tab"),
                hint(app, Does::Back, "back"),
                hint(app, Does::Quit, "quit"),
            ]
        }
        View::Settings => {
            hints.extend(settings::hints(app));
            [
                hint(app, Does::NextTab, "next tab"),
                hint(app, Does::Back, "back"),
                hint(app, Does::Quit, "quit"),
            ]
        }
        View::Sessions => [
            hint(app, Does::AddProject, "project"),
            hint(app, Does::NextTab, "next tab"),
            hint(app, Does::Quit, "quit"),
        ],
        View::NeedsYou => [
            hint(app, Does::NextTab, "next tab"),
            hint(app, Does::Back, "back"),
            hint(app, Does::Quit, "quit"),
        ],
    };
    hints.extend(
        app.actions_here()
            .into_iter()
            .filter(|here| here.key != "none")
            .map(|here| format!("{} {}", here.key_name(), here.what)),
    );
    hints.extend(general.into_iter().flatten());
    hints
}

/// What the key after the panel key does, while it waits: the most used
/// first, as many as fit.
fn panel_key_hints(app: &App) -> Vec<String> {
    let after = |does: Does, what: &str| hint(app, does, what);
    let moves = [
        Does::FocusLeft,
        Does::FocusDown,
        Does::FocusUp,
        Does::FocusRight,
    ]
    .map(|does| app.keymap.label(does))
    .join("");
    let swaps = [
        Does::SwapLeft,
        Does::SwapDown,
        Does::SwapUp,
        Does::SwapRight,
    ]
    .map(|does| {
        app.keymap
            .label(does)
            .trim_start_matches("Shift-")
            .to_owned()
    })
    .join("");
    [
        after(Does::SplitRight, "split →"),
        after(Does::SplitDown, "split ↓"),
        Some(format!("{moves}/←↓↑→ move")),
        Some(format!("Shift-{swaps} swap")),
        after(Does::Zoom, "zoom"),
        after(Does::TakeOff, "off the screen"),
        after(Does::Resize, "resize"),
        Some("1-9 a panel".to_owned()),
        after(Does::PanelKeys, "keys"),
        Some("esc give up".to_owned()),
    ]
    .into_iter()
    .flatten()
    .collect()
}

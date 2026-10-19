//! The bottom bar: the keys that do something for what is selected, or
//! the last note, or why the daemon cannot be reached, and a warning when
//! this UI is older than the daemon.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::{settings, tickets, version};
use crate::app::{App, Link, Mode, View};

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
        let length = u16::try_from(note_text(text).chars().count()).unwrap_or(u16::MAX);
        length.div_ceil(width.max(1))
    };
    match (message(app), stale(app)) {
        (Some((text, _)), _) => lines(text).clamp(1, MAX_NOTE_LINES),
        (None, Some(warning)) => 1 + lines(&warning).clamp(1, MAX_NOTE_LINES - 1),
        (None, None) => 1,
    }
}

pub(super) fn draw_bottom_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    if message(app).is_none()
        && let Some(warning) = stale(app)
    {
        let [above, hints_area] =
            Layout::vertical([Constraint::Min(0), Constraint::Length(1)]).areas(area);
        let warning = Line::from(note_text(&warning)).yellow();
        frame.render_widget(Paragraph::new(warning).wrap(Wrap { trim: false }), above);
        draw_hints(frame, app, hints_area);
        return;
    }
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
    draw_hints(frame, app, area);
}

/// What `h` is, at the bar's end while the list has the keys, so the
/// way to every other key is always in view.
const HELP_HINT: &str = "h help ";

/// The hints in `area`, as many as fit whole, and at its end the key of
/// the help, where it opens.
fn draw_hints(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let help = app.mode == Mode::Browse;
    let help_width = if help {
        u16::try_from(HELP_HINT.chars().count()).unwrap_or(0)
    } else {
        0
    };
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(help_width)]).areas(area);
    // A space before them, and two before the help.
    let room = usize::from(left.width).saturating_sub(if help { 3 } else { 1 });
    let text = fitting(&hints(app), room);
    frame.render_widget(Line::from(format!(" {text}")).dim(), left);
    if help {
        frame.render_widget(Line::from(HELP_HINT).dim(), right);
    }
}

/// As many of `hints` as fit in `room` cells, whole, two spaces apart:
/// the first however long, cut where the bar ends.
fn fitting(hints: &[String], room: usize) -> String {
    let mut text = String::new();
    for hint in hints {
        if !text.is_empty() {
            if text.chars().count() + 2 + hint.chars().count() > room {
                break;
            }
            text.push_str("  ");
        }
        text.push_str(hint);
    }
    text
}

/// The keys that do something for what is selected, most used first.
fn hints(app: &App) -> Vec<String> {
    if app.arranging.is_some() {
        return vec![
            "←→↑↓ move it  + − wider or narrower  ⇥ another  ⏎ keep  esc put back".to_owned(),
        ];
    }
    if let Mode::Menu(_) = app.mode {
        return vec!["↑↓ choose  ⏎ do it, or press its key  esc close".to_owned()];
    }
    if let Mode::Help(_) = app.mode {
        return vec!["type to narrow  ↑↓ scroll  esc close".to_owned()];
    }
    if matches!(app.mode, Mode::Terminal(_)) {
        return vec!["keys go to the session   Shift-Q back to the list (or click it)".to_owned()];
    }
    if app.mode == Mode::Changes {
        let diff = app.changes.as_ref().is_some_and(|pane| pane.diff.is_some());
        let entry = app
            .changes
            .as_ref()
            .is_some_and(|pane| pane.selected_entry().is_some());
        let undo = if entry {
            "  z put back as before it"
        } else {
            ""
        };
        return vec![if diff {
            format!("↑↓ scroll  PgUp PgDn page  esc the changes  r again{undo}  d back")
        } else {
            format!("↑↓ move  ⏎ its diff  r again{undo}  esc back to the list")
        }];
    }
    let mut hints = vec!["↑↓ move".to_owned()];
    let general: &[&str] = match app.view {
        View::Tickets => {
            hints.extend(tickets::hints(app).into_iter().map(str::to_owned));
            &["⇥ next tab", "esc back", "q quit"]
        }
        View::Settings => {
            hints.extend(settings::hints(app).into_iter().map(str::to_owned));
            &["⇥ next tab", "esc back", "q quit"]
        }
        View::Sessions => &["a project", "⇥ next tab", "q quit"],
        View::NeedsYou => &["⇥ next tab", "esc back", "q quit"],
    };
    hints.extend(
        app.actions_here()
            .into_iter()
            .map(|here| format!("{} {}", here.key_name(), here.what)),
    );
    hints.extend(general.iter().map(|hint| (*hint).to_owned()));
    hints
}

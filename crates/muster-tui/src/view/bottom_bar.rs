//! The bottom bar: the keys that do something for what is selected, or
//! the last note, or why the daemon cannot be reached, and a warning when
//! this UI is older than the daemon.

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
    let help_hint = (app.mode == Mode::Browse)
        .then(|| hint(app, Does::Help, "help "))
        .flatten();
    let help = help_hint.is_some();
    let help_width = help_hint
        .as_ref()
        .map_or(0, |hint| u16::try_from(cells::width(hint)).unwrap_or(0));
    let [left, right] =
        Layout::horizontal([Constraint::Min(0), Constraint::Length(help_width)]).areas(area);
    // A space before them, and two before the help.
    let room = usize::from(left.width).saturating_sub(if help { 3 } else { 1 });
    let text = fitting(&hints(app), room);
    frame.render_widget(Line::from(format!(" {text}")).dim(), left);
    if let Some(help_hint) = help_hint {
        frame.render_widget(Line::from(help_hint).dim(), right);
    }
}

/// As many of `hints` as fit in `room` cells, whole, two spaces apart:
/// the first however long, cut where the bar ends.
fn fitting(hints: &[String], room: usize) -> String {
    let mut text = String::new();
    for hint in hints {
        if !text.is_empty() {
            if cells::width(&text) + 2 + cells::width(hint) > room {
                break;
            }
            text.push_str("  ");
        }
        text.push_str(hint);
    }
    text
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
        return vec![
            "←→↑↓ move it  + − wider or narrower  ⇥ another  ⏎ keep  esc put back".to_owned(),
        ];
    }
    if app.panels.waiting.is_some() {
        return vec![panel_key_hints(app)];
    }
    if app.panels.resizing.is_some() {
        return vec!["h j k l move its edge  H J K L five  ⏎ keep  esc put back".to_owned()];
    }
    if let Mode::Menu(_) = app.mode {
        return vec!["↑↓ choose  ⏎ do it, or press its key  esc close".to_owned()];
    }
    if let Mode::Help(_) = app.mode {
        return vec!["type to narrow  ↑↓ scroll  esc close".to_owned()];
    }
    if matches!(app.mode, Mode::Terminal(_)) {
        let back = hint(app, Does::LeaveSession, "back to the list (or click it)")
            .unwrap_or_else(|| "a click on the list goes back to it".to_owned());
        let panels = hint(app, Does::PanelKey, "panels")
            .map_or_else(String::new, |hint| format!("   {hint}"));
        return vec![format!("keys go to the session   {back}{panels}")];
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
            Some("esc back to the list".to_owned())
        });
        return vec![hints.join("  ")];
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
fn panel_key_hints(app: &App) -> String {
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
    let hints: Vec<String> = [
        after(Does::SplitRight, "split →"),
        after(Does::SplitDown, "split ↓"),
        Some(format!("{moves} or ←↓↑→ move")),
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
    .collect();
    hints.join("  ")
}

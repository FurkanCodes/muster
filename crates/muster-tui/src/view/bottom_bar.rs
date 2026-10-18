//! The bottom bar: the keys that do something for what is selected, or
//! the last note, or why the daemon cannot be reached, and a warning when
//! this UI is older than the daemon.

use muster_core::WorkspaceKind;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Stylize;
use ratatui::text::Line;
use ratatui::widgets::{Paragraph, Wrap};

use super::{settings, tickets, version};
use crate::app::{App, Link, Mode, View};
use crate::model::Row;

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
        frame.render_widget(Line::from(format!(" {}", hints(app))).dim(), hints_area);
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
    frame.render_widget(Line::from(format!(" {}", hints(app))).dim(), area);
}

/// The keys that do something for what is selected.
fn hints(app: &App) -> String {
    if matches!(app.mode, Mode::Terminal(_)) {
        return "keys go to the session   Shift-Q back to the list (or click it)".to_owned();
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
        return if diff {
            format!("↑↓ scroll  PgUp PgDn page  esc the changes  r again{undo}  d back")
        } else {
            format!("↑↓ move  ⏎ its diff  r again{undo}  esc back to the list")
        };
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
                "d changes",
                "x close",
                "s stop",
                "n session",
            ]);
        }
        (_, _, Some(_)) => hints.extend([
            "⏎ open",
            "f full screen",
            "d changes",
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
                    hints.extend(["d changes", "c check", "v review", "o PR"]);
                }
                if app
                    .model
                    .workspace(id)
                    .is_some_and(|workspace| workspace.pull_request.is_some())
                {
                    hints.push("b browser");
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

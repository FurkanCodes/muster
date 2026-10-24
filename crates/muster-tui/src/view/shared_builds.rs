//! Whether a project's worktrees build into one folder, on the Settings
//! screen: its row under *Muster*, and what it does.

use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};

use crate::app::App;

/// What the row is called.
pub(super) const LABEL: &str = "Shared Rust builds";

/// On, or off.
pub(super) fn value(app: &App) -> Span<'static> {
    let theme = super::theme::of(app);
    if app.model.preferences().shared_builds {
        Span::from("on").fg(theme.done).bold()
    } else {
        Span::from("off").dim()
    }
}

/// What ⏎ does on the row.
pub(super) fn hint(app: &App) -> &'static str {
    if app.model.preferences().shared_builds {
        "⏎ turn off"
    } else {
        "⏎ turn on"
    }
}

/// What it does, and where it is kept.
pub(super) fn text(app: &App) -> Text<'static> {
    Text::from(vec![
        Line::from(vec![Span::from(format!("{LABEL}  ")).bold(), value(app)]),
        Line::from(
            "The worktrees Muster makes for a project build into one folder beside them, so a \
             Rust project's dependencies are built and kept once, not again in every worktree.",
        ),
        Line::from(""),
        Line::from(
            "Muster writes a Cargo config in the folder that holds the project's worktrees: your \
             repository and your own checkout are never touched, and a build folder your project \
             or CARGO_TARGET_DIR names still comes first. Builds share one lock: agents \
             building in two worktrees at once take turns, one waiting while the other builds.",
        ),
        Line::from(
            "Turned off, each worktree builds into a folder of its own again and the shared \
             builds are deleted; they go too as a project's last worktree goes.",
        )
        .dim(),
        Line::from(""),
        Line::from(vec![
            Span::from("Saved in ").dim(),
            Span::raw(format!(
                "{}  [worktrees] shared_builds",
                app.model.preferences().file.display()
            )),
        ]),
    ])
}

//! Whether agents may set agents working, on the Settings screen: its row
//! under *Muster*, and what it does (`PRD-01`).

use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};

use crate::app::App;

/// What the row is called.
pub(super) const LABEL: &str = "Agents start agents";

/// On, or off.
pub(super) fn value(app: &App) -> Span<'static> {
    let theme = super::theme::of(app);
    if app.model.preferences().delegation.allowed {
        Span::from("on").fg(theme.done).bold()
    } else {
        Span::from("off").dim()
    }
}

/// What ⏎ does on the row.
pub(super) fn hint(app: &App) -> &'static str {
    if app.model.preferences().delegation.allowed {
        "⏎ turn off"
    } else {
        "⏎ turn on"
    }
}

/// What it does, its limits, and where it is kept.
pub(super) fn text(app: &App) -> Text<'static> {
    let delegation = app.model.preferences().delegation;
    Text::from(vec![
        Line::from(vec![Span::from(format!("{LABEL}  ")).bold(), value(app)]),
        Line::from(
            "An agent may start agents through Muster's command line (`muster claude \"…\"`), \
             as you could, and give them prompts. Each one it starts says so in the list.",
        ),
        Line::from(""),
        Line::from(format!(
            "One agent has at most {} it started running at once, and agents start agents \
             {} deep at most: set `at_once` and `depth` under [delegation] in the file below.",
            delegation.at_once, delegation.depth
        )),
        Line::from(
            "Turned off, the command line refuses to start a session from inside an agent's; \
             from your own shell in Muster, or a terminal of yours, it always may.",
        )
        .dim(),
        Line::from(""),
        Line::from(vec![
            Span::from("Saved in ").dim(),
            Span::raw(format!(
                "{}  [delegation] allowed",
                app.model.preferences().file.display()
            )),
        ]),
    ])
}

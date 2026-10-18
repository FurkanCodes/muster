//! The look shared by every screen: each project's colour and the edge
//! drawn in it, the selected line, how long ago something happened, the
//! agents' letters, and Muster's wordmark. Colours are the terminal's own
//! sixteen, so they follow the person's theme.

use std::borrow::Cow;

use muster_core::{ProjectId, Session, SessionStatus};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::Span;

use crate::app::App;

/// The projects' colours, in the order the projects are listed: apart
/// from the statuses' yellow, red and green, so a project never reads as
/// a status.
const PROJECT_COLOURS: [Color; 6] = [
    Color::Blue,
    Color::Magenta,
    Color::Cyan,
    Color::LightBlue,
    Color::LightMagenta,
    Color::LightCyan,
];

/// The colour of `project`: steady while the list of projects is.
pub(crate) fn project_colour(app: &App, project: ProjectId) -> Color {
    let at = app
        .model
        .projects()
        .iter()
        .position(|found| found.id == project)
        .unwrap_or(0);
    PROJECT_COLOURS[at % PROJECT_COLOURS.len()]
}

/// The edge at the start of a line that belongs to `project`, in its
/// colour; thicker on the selected line, as the selection's bar.
pub(crate) fn edge(app: &App, project: Option<ProjectId>, selected: bool) -> Span<'static> {
    let Some(project) = project else {
        return Span::raw(" ");
    };
    let mark = if selected { "█" } else { "▌" };
    Span::styled(mark, Style::default().fg(project_colour(app, project)))
}

/// How the selected line of a list is drawn: a soft tint, its text at
/// full strength, and bold where the list has the keys.
pub(crate) fn selected_style(focused: bool) -> Style {
    let style = Style::default()
        .bg(Color::DarkGray)
        .remove_modifier(Modifier::DIM);
    if focused {
        style.add_modifier(Modifier::BOLD)
    } else {
        style
    }
}

/// `millis` as a short span: `38s`, `4m12s`, `1h04m`, `2d`; to the
/// minute without `seconds`.
pub(crate) fn short_span(millis: u64, seconds: bool) -> String {
    let total = millis / 1000;
    match total {
        0..60 if seconds => format!("{total}s"),
        0..60 => "<1m".to_owned(),
        60..3600 if seconds => format!("{}m{:02}s", total / 60, total % 60),
        60..3600 => format!("{}m", total / 60),
        3600..86_400 => format!("{}h{:02}m", total / 3600, total % 3600 / 60),
        _ => format!("{}d", total / 86_400),
    }
}

/// How long `session` has been as it is: `4m12s` while it runs or waits,
/// `12m ago` once it has finished or ended; `None` before the clock is
/// read. To the second only while things move.
pub(crate) fn session_time(app: &App, session: &Session) -> Option<String> {
    let now = app.now?;
    let since = muster_core::time::between(session.status_since, now);
    Some(match session.status {
        SessionStatus::Running | SessionStatus::NeedsYou { .. } => short_span(since, app.motion),
        _ => format!("{} ago", short_span(since, false)),
    })
}

/// What `session` is doing, for its line: its activity while it works or
/// waits and its hooks say, or why it ended where the daemon says (a
/// restart did not resume it), else its status.
pub(crate) fn doing(session: &Session) -> Cow<'_, str> {
    match (&session.activity, session.status, &session.tidy) {
        (
            Some(activity),
            SessionStatus::Running | SessionStatus::NeedsYou { .. } | SessionStatus::Exited { .. },
            _,
        ) => Cow::Borrowed(activity.text.as_str()),
        // A tidy says what it comes to, until its agent waits or ends.
        (
            _,
            SessionStatus::Running | SessionStatus::Finished | SessionStatus::Fresh,
            Some(tidy),
        ) => Cow::Owned(tidy.words()),
        _ => Cow::Borrowed(crate::view::describe(session.status)),
    }
}

/// The mark an agent is known by on a ticket's line, after a space: one of
/// Muster's own by its letter, dim; one the person added by the mark its
/// settings give it, in its colour where they give one.
pub(crate) fn agent_mark(app: &App, harness: &str) -> Span<'static> {
    let added = app
        .added_agents()
        .iter()
        .find(|agent| agent.name == harness);
    match added {
        Some(agent) => {
            let colour = agent
                .colour
                .as_deref()
                .and_then(|colour| colour.parse::<Color>().ok());
            let mark = Span::from(format!(" {}", agent.mark));
            match colour {
                Some(colour) => mark.fg(colour),
                None => mark.dim(),
            }
        }
        None => Span::from(format!(" {}", harness_letter(harness))).dim(),
    }
}

/// The letter an agent is known by on a ticket's line.
pub(crate) fn harness_letter(harness: &str) -> String {
    match harness {
        "claude" => "C".to_owned(),
        "codex" => "X".to_owned(),
        "gemini" => "G".to_owned(),
        "opencode" => "O".to_owned(),
        "copilot" => "P".to_owned(),
        "review" => "R".to_owned(),
        "shell" => "$".to_owned(),
        other => other
            .chars()
            .next()
            .map_or_else(|| "?".to_owned(), |c| c.to_uppercase().collect()),
    }
}

/// Muster's name, drawn large where there is nothing else to show yet.
pub(crate) const WORDMARK: [&str; 3] = [
    "┏┳┓╻ ╻┏━┓╺┳╸┏━╸┏━┓",
    "┃┃┃┃ ┃┗━┓ ┃ ┣╸ ┣┳┛",
    "╹ ╹┗━┛┗━┛ ╹ ┗━╸╹┗╸",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spans_read_short_and_to_the_minute_when_still() {
        assert_eq!(short_span(38_000, true), "38s");
        assert_eq!(short_span(38_000, false), "<1m");
        assert_eq!(short_span(252_000, true), "4m12s");
        assert_eq!(short_span(252_000, false), "4m");
        assert_eq!(short_span(3_840_000, true), "1h04m");
        assert_eq!(short_span(3 * 86_400_000, true), "3d");
    }

    #[test]
    fn agents_have_their_letters() {
        assert_eq!(harness_letter("claude"), "C");
        assert_eq!(harness_letter("codex"), "X");
        assert_eq!(harness_letter("opencode"), "O");
        assert_eq!(harness_letter("aider"), "A");
        assert_eq!(harness_letter(""), "?");
    }

    #[test]
    fn the_wordmark_s_lines_are_one_width() {
        let widths: Vec<usize> = WORDMARK.iter().map(|line| line.chars().count()).collect();
        assert_eq!(widths, [18, 18, 18]);
    }
}

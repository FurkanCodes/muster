//! The top bar: Kep's face, the screens as tabs, the one shown lit in the
//! palette's accent, then how the agents
//! stand (working, waiting on the person, done) in their statuses'
//! colours, the machine's memory as a bar, and the daemon's version where
//! there is room.

use muster_core::SessionStatus;
use muster_core::num::fraction;
use muster_core::protocol::ResourceReport;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use super::{bytes, kep, mark, theme};
use crate::app::{App, Link, View};
use crate::motion;

pub(super) fn draw_top_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let mut spans = tabs(app);
    let room = usize::from(area.width).saturating_sub(width(&spans));
    // Kep comes first, and is the first to go short of room: only while
    // all the rest fits beside it, in full.
    if width(&standing(app, usize::MAX)) + KEP_ROOM <= room {
        let mut first = vec![Span::raw(" ")];
        first.extend(kep::face(kep::mood(app), app.motion_step()));
        spans.splice(0..0, first);
    }
    spans.extend(standing(app, room));
    frame.render_widget(Line::from(spans), area);
}

/// How things stand, after the tabs, as much as fits in `room`.
fn standing(app: &App, room: usize) -> Vec<Span<'static>> {
    match &app.link {
        Link::Connecting => vec![Span::from("connecting to the daemon…").dim()],
        Link::Connected { .. } if !app.model.loaded() => vec![Span::from("loading…").dim()],
        Link::Connected { daemon_version } => fleet(app, daemon_version, room),
        Link::Lost(_) => vec![Span::styled(
            match &app.machine {
                Some(machine) => format!("{machine} unreachable; reconnecting…"),
                None => "daemon unreachable; retrying…".to_owned(),
            },
            Style::default().fg(theme::of(app).waiting),
        )],
    }
}

/// What Kep's face takes at the top bar's start, with the space before it.
const KEP_ROOM: usize = 1 + kep::FACE_WIDTH;

/// Muster's name, where a bar starts: in the accent, or reversed in the
/// terminal's own colours.
pub(super) fn name(app: &App) -> Span<'static> {
    let theme = theme::of(app);
    match theme.palette {
        muster_core::settings::Palette::Muster => {
            Span::styled(" Muster ", Style::default().fg(theme.accent).bold())
        }
        muster_core::settings::Palette::Terminal => Span::from(" Muster ").bold().reversed(),
    }
}

/// The name, then the screens as tabs, the one shown lit and the others
/// quiet; ⇥ goes round.
fn tabs(app: &App) -> Vec<Span<'static>> {
    let theme = theme::of(app);
    let waiting = app.model.needs_you().len();
    let mut spans = vec![name(app), Span::raw(" ")];
    // Another machine's Muster says so first, in the accent.
    if let Some(machine) = &app.machine {
        spans.push(Span::styled(
            format!("on {} ", muster_core::text::one_line(machine)),
            Style::default().fg(theme.accent),
        ));
    }
    for (at, (view, name)) in [
        (View::Sessions, "Sessions"),
        (View::NeedsYou, "Needs you"),
        (View::Tickets, "Tickets"),
        (View::Settings, "Settings"),
    ]
    .into_iter()
    .enumerate()
    {
        if at > 0 {
            spans.push(Span::raw(" "));
        }
        let shown = app.view == view;
        let mut tab = vec![if shown {
            Span::raw(format!(" {name} "))
        } else {
            Span::styled(format!(" {name} "), theme.quiet)
        }];
        if view == View::NeedsYou && waiting > 0 {
            let style = Style::default().fg(theme.waiting).bold();
            let style = app
                .motion_step()
                .map_or(style, |step| motion::pulse(style, step));
            tab.push(Span::styled(format!("{waiting} "), style));
        }
        if shown {
            let lit = theme.tab();
            tab = tab
                .into_iter()
                .map(|span| {
                    // On Muster's accent the count of what waits takes the
                    // tab's colours: its own would not show there.
                    let style = match theme.palette {
                        muster_core::settings::Palette::Muster => lit,
                        muster_core::settings::Palette::Terminal => span.style.patch(lit),
                    };
                    span.style(style)
                })
                .collect();
        }
        spans.extend(tab);
    }
    spans.push(Span::from(" ⇥   ").dim());
    spans
}

/// How the agents and the machine stand, as much of it as fits in
/// `room`: counts with words, the memory, the version; then less.
fn fleet(app: &App, daemon_version: &str, room: usize) -> Vec<Span<'static>> {
    let memory = app.model.resources().map(|report| memory(app, report));
    let version = vec![Span::from(format!("daemon {daemon_version}")).dim()];
    let candidates = [
        (true, true, true),
        (true, true, false),
        (false, true, true),
        (false, true, false),
        (false, false, false),
    ];
    let mut fallback = Vec::new();
    for (words, with_memory, with_version) in candidates {
        let mut spans = counts(app, words);
        for (wanted, group) in [
            (with_memory, memory.clone()),
            (with_version, Some(version.clone())),
        ] {
            if let (true, Some(group)) = (wanted, group) {
                if !spans.is_empty() {
                    spans.push(Span::raw("   "));
                }
                spans.extend(group);
            }
        }
        if width(&spans) <= room {
            return spans;
        }
        fallback = spans;
    }
    fallback
}

/// How many agents work, wait on the person and are done, each in its
/// status's look; those of none are left out. With `words`, what each
/// count is.
fn counts(app: &App, words: bool) -> Vec<Span<'static>> {
    // The agents: a shell is the person's own, a panel a plugin's.
    let counts = app.model.counts();
    let groups = [
        (counts.working, SessionStatus::Running, "working"),
        (
            counts.waiting,
            SessionStatus::NeedsYou {
                reason: muster_core::WaitReason::Question,
            },
            "waiting",
        ),
        (counts.done, SessionStatus::Finished, "done"),
    ];
    let mut spans = Vec::new();
    for (count, status, word) in groups {
        if count == 0 {
            continue;
        }
        if !spans.is_empty() {
            spans.push(Span::raw("  "));
        }
        let (symbol, style) = mark(app, status);
        let text = if words {
            format!("{symbol} {count} {word}")
        } else {
            format!("{symbol} {count}")
        };
        spans.push(Span::styled(text, style));
    }
    if spans.is_empty() && words {
        spans.push(Span::from("no agent at work").dim());
    }
    spans
}

/// The machine's memory as a bar of eight cells, in the colour of what
/// went right, of work past 70% and of what waits past 90%, with how much
/// of how much.
fn memory(app: &App, report: &ResourceReport) -> Vec<Span<'static>> {
    const CELLS: u64 = 8;
    const PARTS: [&str; 8] = ["", "▏", "▎", "▍", "▌", "▋", "▊", "▉"];
    let theme = theme::of(app);
    let total = report.host_memory_total.max(1);
    let used = report.host_memory_used.min(total);
    let eighths = used.saturating_mul(CELLS * 8) / total;
    let full = usize::try_from(eighths / 8).unwrap_or(0);
    let part = PARTS[usize::try_from(eighths % 8).unwrap_or(0)];
    let mut bar = "█".repeat(full);
    bar.push_str(part);
    let empty = usize::try_from(CELLS).unwrap_or(8) - bar.chars().count();
    let colour = match used * 100 / total {
        0..70 => theme.done,
        70..90 => theme.working,
        _ => theme.waiting,
    };
    vec![
        Span::from("▕").dim(),
        Span::styled(bar, Style::default().fg(colour)),
        Span::from(format!("{}▏", " ".repeat(empty))).dim(),
        Span::from(format!(" {}", amount(used, total))).dim(),
    ]
}

/// `9.1/16.0 GB`: how much of how much, short enough to sit beside the
/// counts.
fn amount(used: u64, total: u64) -> String {
    const GB: f64 = 1_000_000_000.0;
    if fraction(total) >= GB {
        format!("{:.1}/{:.1} GB", fraction(used) / GB, fraction(total) / GB)
    } else {
        format!("{}/{}", bytes(used), bytes(total))
    }
}

fn width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(Span::width).sum()
}

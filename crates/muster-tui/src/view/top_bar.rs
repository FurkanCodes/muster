//! The top bar: Kep's face, the screens as tabs, the one shown lit in the
//! palette's accent, then how the agents
//! stand (working, waiting on the person, done) in their statuses'
//! colours, the machine's memory as a bar, and the daemon's version where
//! there is room. A tab, a count and the ZOOM sign are pressed as their
//! keys are (`top_controls`): a tab shows its screen, the agents working
//! go to *Sessions*, those waiting or done to *Needs you*, and ZOOM puts
//! the zoomed panel back.

use muster_core::SessionStatus;
use muster_core::num::fraction;
use muster_core::protocol::ResourceReport;
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};

use super::{bytes, kep, mark, theme};
use crate::app::{App, Link, View};
use crate::cells;
use crate::motion;

/// What a click on part of the top bar does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TopControl {
    /// Shows this screen.
    Tab(View),
    /// Puts the zoomed panel back.
    Zoom,
}

/// A span of the bar, and what a click on it does, if anything.
type Tagged = (Span<'static>, Option<TopControl>);

pub(super) fn draw_top_bar(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = theme::of(app);
    let hovered = app
        .person
        .pointer
        .and_then(|at| {
            top_controls(app, area)
                .into_iter()
                .find(|(_, cells)| cells.contains(at))
        })
        .map(|(control, _)| control);
    let spans: Vec<Span<'static>> = bar(app, area)
        .into_iter()
        .map(|(span, control)| match control {
            Some(control) if Some(control) == hovered => span.patch_style(theme.pressable()),
            _ => span,
        })
        .collect();
    frame.render_widget(Line::from(spans), area);
}

/// The whole bar in `area`, each span with what a click on it does.
fn bar(app: &App, area: Rect) -> Vec<Tagged> {
    let mut spans = tabs(app);
    let room = usize::from(area.width).saturating_sub(width(&spans));
    // Kep comes first, and is the first to go short of room: only while
    // all the rest fits beside it, in full.
    if width(&standing(app, usize::MAX)) + KEP_ROOM <= room {
        let mut first: Vec<Tagged> = vec![(Span::raw(" "), None)];
        first.extend(
            kep::face(kep::mood(app), app.motion_step())
                .into_iter()
                .map(|span| (span, None)),
        );
        spans.splice(0..0, first);
    }
    spans.extend(standing(app, room));
    spans
}

/// Where each part of the bar a click does something on is, in `area`.
pub(crate) fn top_controls(app: &App, area: Rect) -> Vec<(TopControl, Rect)> {
    let mut x = area.x;
    let mut controls: Vec<(TopControl, Rect)> = Vec::new();
    for (span, control) in bar(app, area) {
        let span_width = u16::try_from(span.width()).unwrap_or(u16::MAX);
        let shown = span_width.min(area.right().saturating_sub(x));
        if let Some(control) = control {
            match controls.last_mut() {
                // A control in several spans, as a tab and its count, is one.
                Some((last, cells)) if *last == control && cells.right() == x => {
                    cells.width += shown;
                }
                _ => controls.push((control, Rect::new(x, area.y, shown, 1))),
            }
        }
        x = x.saturating_add(span_width);
    }
    controls
}

/// How things stand, after the tabs, as much as fits in `room`.
fn standing(app: &App, room: usize) -> Vec<Tagged> {
    match &app.link {
        Link::Connecting => vec![(Span::from("connecting to the daemon…").dim(), None)],
        Link::Connected { .. } if !app.model.loaded() => {
            vec![(Span::from("loading…").dim(), None)]
        }
        Link::Connected { daemon_version } => fleet(app, daemon_version, room),
        Link::Lost(_) => vec![(
            Span::styled(
                match &app.machine {
                    Some(machine) => format!("{machine} unreachable; reconnecting…"),
                    None => "daemon unreachable; retrying…".to_owned(),
                },
                Style::default().fg(theme::of(app).waiting),
            ),
            None,
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
fn tabs(app: &App) -> Vec<Tagged> {
    let theme = theme::of(app);
    let waiting = app.model.needs_you().len();
    let mut spans: Vec<Tagged> = vec![(name(app), None), (Span::raw(" "), None)];
    // Another machine's Muster says so first, in the accent.
    if let Some(machine) = &app.machine {
        spans.push((
            Span::styled(
                format!("on {} ", muster_core::text::one_line(machine)),
                Style::default().fg(theme.accent),
            ),
            None,
        ));
    }
    let sessions = app.sessions_tab_name();
    for (at, (view, name)) in [
        (View::Sessions, sessions.as_str()),
        (View::NeedsYou, "Needs you"),
        (View::Tickets, "Tickets"),
        (View::Settings, "Settings"),
    ]
    .into_iter()
    .enumerate()
    {
        if at > 0 {
            spans.push((Span::raw(" "), None));
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
        spans.extend(
            tab.into_iter()
                .map(|span| (span, Some(TopControl::Tab(view)))),
        );
    }
    spans.push((Span::from(" ⇥   ").dim(), None));
    // A panel zoomed over the others says so, as the tab lit does.
    if app.panels.zoomed.is_some() {
        spans.push((Span::styled(" ZOOM ", theme.tab()), Some(TopControl::Zoom)));
        spans.push((Span::raw("  "), None));
    }
    spans
}

/// How the agents and the machine stand, as much of it as fits in
/// `room`: counts with words, the memory, the version; then less.
fn fleet(app: &App, daemon_version: &str, room: usize) -> Vec<Tagged> {
    let untagged = |spans: Vec<Span<'static>>| -> Vec<Tagged> {
        spans.into_iter().map(|span| (span, None)).collect()
    };
    let memory = app
        .model
        .resources()
        .map(|report| untagged(memory(app, report)));
    let version = untagged(vec![Span::from(format!("daemon {daemon_version}")).dim()]);
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
                    spans.push((Span::raw("   "), None));
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
fn counts(app: &App, words: bool) -> Vec<Tagged> {
    // The agents: a shell is the person's own, a panel a plugin's.
    let counts = app.model.counts();
    let groups = [
        (
            counts.working,
            SessionStatus::Running,
            "working",
            View::Sessions,
        ),
        (
            counts.waiting,
            SessionStatus::NeedsYou {
                reason: muster_core::WaitReason::Question,
            },
            "waiting",
            View::NeedsYou,
        ),
        (counts.done, SessionStatus::Finished, "done", View::NeedsYou),
    ];
    let mut spans: Vec<Tagged> = Vec::new();
    for (count, status, word, view) in groups {
        if count == 0 {
            continue;
        }
        if !spans.is_empty() {
            spans.push((Span::raw("  "), None));
        }
        let (symbol, style) = mark(app, status);
        let text = if words {
            format!("{symbol} {count} {word}")
        } else {
            format!("{symbol} {count}")
        };
        spans.push((Span::styled(text, style), Some(TopControl::Tab(view))));
    }
    if spans.is_empty() && words {
        spans.push((Span::from("no agent at work").dim(), None));
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
    let empty = usize::try_from(CELLS).unwrap_or(8) - cells::width(&bar);
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

fn width(spans: &[Tagged]) -> usize {
    spans.iter().map(|(span, _)| span.width()).sum()
}

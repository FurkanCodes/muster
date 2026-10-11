//! Runs on screen (`PRODUCT.md` §6): the runs list, waiting on the person
//! first; a run's details, with its timeline and why it stopped; its line
//! in the Needs you queue; and the keys that apply to it.

use muster_core::time::{between, span};
use muster_core::{Run, RunId, RunStage};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{panel, shorten};
use crate::app::{App, RunActions, View};

pub(super) fn draw_runs(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = panel(" Runs ", app.view == View::Runs);
    if app.runs_list.is_empty() {
        frame.render_widget(
            Paragraph::new(
                "No runs yet. Press n to hand one of a project's issues to the autopilot.",
            )
            .wrap(Wrap { trim: false })
            .block(block)
            .dim(),
            area,
        );
        return;
    }
    let width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem<'_>> = app
        .runs_list
        .iter()
        .filter_map(|id| app.model.run(*id))
        .map(|run| {
            let line = run_line(run, width);
            ListItem::new(if run.stage.is_over() {
                line.dim()
            } else {
                line
            })
        })
        .collect();
    let mut state = ListState::default()
        .with_offset(app.runs_offset)
        .with_selected(
            app.runs_selected
                .and_then(|selected| app.runs_list.iter().position(|id| *id == selected)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED));
    frame.render_stateful_widget(list, area, &mut state);
}

/// `▲ #42 needs you  Fix the login redirect`: the stage before the title,
/// which gives way.
pub(super) fn run_line(run: &Run, width: usize) -> Line<'static> {
    let (symbol, style) = run_symbol(run.stage);
    let head = format!(" #{} {}  ", run.ticket.number, run.stage.label());
    let room = width.saturating_sub(head.chars().count() + 1).max(8);
    Line::from(vec![
        Span::styled(symbol, style),
        Span::raw(format!(" #{} ", run.ticket.number)),
        Span::styled(run.stage.label(), style),
        Span::raw("  "),
        Span::raw(shorten(&run.ticket.title, room)),
    ])
}

fn run_symbol(stage: RunStage) -> (&'static str, Style) {
    match stage {
        RunStage::NeedsYou => ("▲", Style::default().fg(Color::Red).bold()),
        RunStage::ReadyForYou => ("✓", Style::default().fg(Color::Green).bold()),
        RunStage::TakenOver => ("◆", Style::default().fg(Color::Magenta)),
        RunStage::Queued => ("○", Style::default().fg(Color::Gray)),
        RunStage::Merged => ("✓", Style::default().fg(Color::DarkGray)),
        RunStage::Cancelled => ("✕", Style::default().fg(Color::DarkGray)),
        RunStage::Provisioning
        | RunStage::Building
        | RunStage::Checking
        | RunStage::OpeningPr
        | RunStage::AgentReview => ("●", Style::default().fg(Color::Yellow)),
    }
}

/// The run's details, in the preview's place.
pub(super) fn draw_run_details(frame: &mut Frame<'_>, app: &App, run: RunId, area: Rect) {
    let Some(run) = app.model.run(run) else {
        return;
    };
    let mut lines = vec![
        Line::from(run.ticket.title.clone()).bold(),
        Line::from(format!(
            "#{} · {} · {}",
            run.ticket.number,
            app.model
                .project(run.project)
                .map_or("?", |project| project.name.as_str()),
            run.ticket.url
        ))
        .dim(),
        Line::from(""),
    ];
    let field = |name: &'static str, value: String| {
        Line::from(vec![
            Span::from(format!("{name:<9}")).dim(),
            Span::raw(value),
        ])
    };
    let (_, style) = run_symbol(run.stage);
    let mut stage = vec![
        Span::from(format!("{:<9}", "Stage")).dim(),
        Span::styled(run.stage.label(), style),
    ];
    if let Some(reason) = &run.reason {
        stage.push(Span::raw(format!(": {reason}")));
    }
    lines.push(Line::from(stage));
    lines.push(field(
        "Attempt",
        if run.attempt == 0 {
            format!("none yet, {} allowed", run.max_attempts)
        } else {
            format!("{} of {}", run.attempt, run.max_attempts)
        },
    ));
    let took = between(run.created_at, run.ended_at.unwrap_or(app.now));
    let mut time = span(took);
    if let Some(cost) = run.cost_micros {
        time.push_str(&format!(" · ${:.2}", cost as f64 / 1_000_000.0));
    }
    lines.push(field("Time", time));
    lines.push(field("Branch", run.branch.clone()));
    if let Some(pr) = &run.pr {
        lines.push(field("PR", format!("#{} {}", pr.number, pr.url)));
    }
    if let Some(session) = app
        .model
        .agent_session_of(run.id)
        .filter(|_| run.stage == RunStage::TakenOver)
    {
        let state = app
            .model
            .session(session)
            .map_or("", |stored| super::describe(stored.status));
        lines.push(field("Agent", format!("yours, {state}: ⏎ opens it")));
    }
    if let Some(timeline) = app.run_history.get(&run.id).filter(|t| !t.is_empty()) {
        lines.push(Line::from(""));
        lines.push(Line::from("Timeline").bold());
        let offsets: Vec<String> = timeline
            .iter()
            .map(|step| format!("+{}", span(between(run.created_at, step.at))))
            .collect();
        let width = offsets.iter().map(String::len).max().unwrap_or(0);
        for (step, offset) in timeline.iter().zip(&offsets) {
            lines.push(Line::from(vec![
                Span::from(format!(" {offset:<width$}  ")).dim(),
                Span::styled(format!("{:<13}", step.to.label()), run_symbol(step.to).1),
                Span::raw(step.reason.clone()),
            ]));
        }
    }
    if let Some(next) = next_step(run) {
        lines.push(Line::from(""));
        lines.push(Line::from(next).dim());
    }
    frame.render_widget(
        Paragraph::new(Text::from(lines))
            .wrap(Wrap { trim: false })
            .block(panel(" Run ", false)),
        area,
    );
}

/// What the person can do next, said in words.
fn next_step(run: &Run) -> Option<&'static str> {
    match run.stage {
        RunStage::NeedsYou => Some(
            "Press r to send it back to work with notes for its agent, t to drive its agent \
             yourself, or x to cancel it.",
        ),
        RunStage::ReadyForYou => Some(
            "Review its pull request on GitHub; merging it ends the run. Press c to send it \
             back to its agent with what to change, or t to change it yourself.",
        ),
        RunStage::TakenOver => Some(
            "Its agent is yours: ⏎ opens it. Press h to hand the run back; your work goes \
             to the checks.",
        ),
        _ => None,
    }
}

/// The keys that apply to the run in focus.
pub(super) fn hints(app: &App, run: RunId) -> Vec<&'static str> {
    let Some(run) = app.model.run(run) else {
        return Vec::new();
    };
    let can = RunActions::of(run.stage);
    let mut hints = Vec::new();
    if app.model.agent_session_of(run.id).is_some() && run.stage == RunStage::TakenOver {
        hints.push("⏎ open agent");
    }
    for (applies, hint) in [
        (can.take_over, "t take over"),
        (can.hand_back, "h hand back"),
        (can.retry, "r retry"),
        (can.changes, "c changes"),
        (can.cancel, "x cancel"),
    ] {
        if applies {
            hints.push(hint);
        }
    }
    hints
}

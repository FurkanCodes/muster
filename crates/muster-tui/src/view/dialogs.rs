//! The dialogs drawn over the list: questions and inputs.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Dialog, HarnessChoice};

pub(super) fn draw_dialog(frame: &mut Frame<'_>, app: &App, dialog: &Dialog, area: Rect) {
    let width = area.width.saturating_sub(4).min(72);
    let (title, lines, input) = match dialog {
        Dialog::AddProject { path } => (
            " Add a project ",
            vec![Line::from("Path to a git repository:")],
            Some(path),
        ),
        Dialog::NewWorktree { branch, .. } => (
            " New worktree ",
            vec![Line::from(
                "Branch (checked out if it exists; empty for muster/<random>):",
            )],
            Some(branch),
        ),
        Dialog::NewSession {
            workspace,
            harness,
            prompt,
        } => {
            let place = app.model.branch(*workspace).unwrap_or("?");
            let choice = |option: HarnessChoice| {
                let label = format!(" {} ", option.label());
                if option == *harness {
                    Span::from(label).reversed()
                } else {
                    Span::from(label).dim()
                }
            };
            let mut lines = vec![
                Line::from(format!("In {place}")).dim(),
                Line::from(vec![
                    Span::raw("Run  "),
                    choice(HarnessChoice::Claude),
                    Span::raw(" "),
                    choice(HarnessChoice::Shell),
                    Span::from("   ⇥ switch").dim(),
                ]),
            ];
            match harness {
                HarnessChoice::Claude => {
                    lines.push(Line::from("First prompt (optional):"));
                    (" New session ", lines, Some(prompt))
                }
                HarnessChoice::Shell => (" New session ", lines, None),
            }
        }
        Dialog::Confirm { question, .. } => (
            " Confirm ",
            vec![
                Line::from(question.clone()),
                Line::from(""),
                Line::from("y yes   n no").dim(),
            ],
            None,
        ),
    };
    let inner_width = width.saturating_sub(2).max(1);
    // Text wraps rather than being cut off; an input takes its line, a
    // blank one and its hint.
    let text_height: u16 = lines
        .iter()
        .map(|line| wrapped_height(line.width(), inner_width))
        .sum();
    let height = 2 + text_height + if input.is_some() { 3 } else { 0 };
    let rect = Rect {
        x: area.x + (area.width.saturating_sub(width)) / 2,
        y: area.y + (area.height.saturating_sub(height)) / 2,
        width,
        height: height.min(area.height),
    };
    frame.render_widget(Clear, rect);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_type(BorderType::Rounded);
    let inner = block.inner(rect);
    frame.render_widget(block, rect);
    let [text_area, input_area] =
        Layout::vertical([Constraint::Length(text_height), Constraint::Min(0)]).areas(inner);
    frame.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), text_area);
    if let Some(input) = input {
        let (shown, column) = input_window(input.text(), input.before_cursor(), inner_width);
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(shown).bold(),
                Line::from(""),
                Line::from("⏎ confirm   esc cancel").dim(),
            ]),
            input_area,
        );
        if input_area.height > 0 {
            frame.set_cursor_position(Position {
                x: input_area.x + column,
                y: input_area.y,
            });
        }
    }
}

/// How many lines text `width` columns wide takes when wrapped at `columns`.
fn wrapped_height(width: usize, columns: u16) -> u16 {
    let lines = width.div_ceil(usize::from(columns.max(1))).max(1);
    u16::try_from(lines).unwrap_or(u16::MAX)
}

/// The part of an input's `text` that fits in `columns`, scrolled so the
/// cursor (after `before_cursor`) stays in view, and the cursor's column
/// within it.
fn input_window(text: &str, before_cursor: &str, columns: u16) -> (String, u16) {
    let columns = usize::from(columns.max(1));
    let cursor = before_cursor.chars().count();
    // The cursor sits after the last character shown, so keep a column for it.
    let start = (cursor + 1).saturating_sub(columns);
    let shown: String = text.chars().skip(start).take(columns).collect();
    (shown, u16::try_from(cursor - start).unwrap_or(0))
}

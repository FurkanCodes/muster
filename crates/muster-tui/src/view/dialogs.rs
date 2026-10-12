//! The dialogs drawn over the list: questions and inputs.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Dialog, FolderPicker, HarnessChoice, PICKER_ROWS, Shown};

pub(super) fn draw_dialog(frame: &mut Frame<'_>, app: &App, dialog: &Dialog, area: Rect) {
    let width = area.width.saturating_sub(4).min(72);
    let inner_width = width.saturating_sub(2).max(1);
    // Lines between an input and its hint, and the hint.
    let mut below = Vec::new();
    let mut hint = "⏎ confirm   esc cancel";
    let (title, lines, input) = match dialog {
        Dialog::AddProject { path, picker } => {
            below = picker_lines(picker, path.text(), usize::from(inner_width));
            hint = "↑↓ choose   ⇥ open   ⇧⇥ up   ⏎ add   esc cancel";
            (
                " Add a project ",
                vec![Line::from(
                    "Folder to add (a git repository, or any folder):",
                )],
                Some(path),
            )
        }
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
            // A branch, or, in a plain folder, which has none, the project.
            let place = match app.model.workspace(*workspace) {
                Some(found) if !found.kind.in_git() => app
                    .model
                    .project(found.project)
                    .map_or("?", |project| project.name.as_str()),
                _ => app.model.branch(*workspace).unwrap_or("?"),
            };
            let choice = |option: HarnessChoice| {
                let label = format!(" {} ", option.label());
                if option == *harness {
                    Span::from(label).reversed()
                } else {
                    Span::from(label).dim()
                }
            };
            let mut run = vec![Span::raw("Run ")];
            for option in HarnessChoice::ALL {
                run.push(choice(option));
            }
            let mut lines = vec![Line::from(format!("In {place}")).dim(), Line::from(run)];
            hint = "⇥ ⇧⇥ choose   ⏎ start   esc cancel";
            if harness.is_agent() {
                lines.push(Line::from("First prompt (optional):"));
                (" New session ", lines, Some(prompt))
            } else {
                (" New session ", lines, None)
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
    // Text wraps rather than being cut off; an input takes its line, a
    // blank one and its hint, with what goes below it and a blank between.
    let text_height: u16 = lines
        .iter()
        .map(|line| wrapped_height(line.width(), inner_width))
        .sum();
    let below_height = match below.len() {
        0 => 0,
        rows => u16::try_from(rows + 1).unwrap_or(u16::MAX),
    };
    let height = 2 + text_height + if input.is_some() { 3 + below_height } else { 0 };
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
        let mut input_lines = vec![Line::from(shown).bold(), Line::from("")];
        if !below.is_empty() {
            input_lines.append(&mut below);
            input_lines.push(Line::from(""));
        }
        input_lines.push(Line::from(hint).dim());
        frame.render_widget(Paragraph::new(input_lines), input_area);
        if input_area.height > 0 {
            frame.set_cursor_position(Position {
                x: input_area.x + column,
                y: input_area.y,
            });
        }
    }
}

/// The folder picker's lines: the folders shown for `typed`, the chosen one
/// marked, at most [`PICKER_ROWS`] of them, and where they are among all;
/// always as many lines, so the dialog keeps its place as the person types.
fn picker_lines(picker: &FolderPicker, typed: &str, columns: usize) -> Vec<Line<'static>> {
    let mut lines = folder_lines(picker, typed, columns);
    lines.resize(PICKER_ROWS + 1, Line::from(""));
    lines
}

fn folder_lines(picker: &FolderPicker, typed: &str, columns: usize) -> Vec<Line<'static>> {
    let folders = match picker.shown(typed) {
        Shown::Loading => return vec![Line::from("Listing…").dim()],
        Shown::Failed(reason) => {
            return vec![
                Line::from(super::shorten(&format!("This folder: {reason}"), columns)).dim(),
            ];
        }
        Shown::Folders(folders) => folders,
    };
    if folders.is_empty() {
        let filter = crate::app::picker_filter(typed);
        let said = if filter.is_empty() {
            "No folders in it.".to_owned()
        } else {
            format!("No folder here matches {filter:?}.")
        };
        return vec![Line::from(super::shorten(&said, columns)).dim()];
    }
    let mut lines: Vec<Line<'static>> = folders
        .iter()
        .enumerate()
        .skip(picker.offset)
        .take(PICKER_ROWS)
        .map(|(at, folder)| {
            let mark = if folder.git { "  git" } else { "" };
            let name = super::shorten(
                &format!("{}/", folder.name),
                columns.saturating_sub(4 + mark.len()),
            );
            let chosen = picker.selected == Some(at);
            let name = if chosen {
                Span::from(name).reversed()
            } else {
                Span::raw(name)
            };
            Line::from(vec![
                Span::raw(if chosen { "› " } else { "  " }),
                name,
                Span::from(mark).dim(),
            ])
        })
        .collect();
    if folders.len() > PICKER_ROWS {
        let first = picker.offset + 1;
        let last = (picker.offset + PICKER_ROWS).min(folders.len());
        lines.push(Line::from(format!("  {first}–{last} of {}", folders.len())).dim());
    }
    lines
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

//! The dialogs drawn over the list: questions and inputs.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap};

use muster_core::config::TrackerKind;

use crate::app::tickets_from::{Field, TRACKERS, focused, tracker_label};
use crate::app::{App, Dialog, FolderPicker, HarnessChoice, PICKER_ROWS, Shown};
use crate::input::LineInput;

/// A harness offered in a dialog, the chosen one stood out.
fn harness_option(option: HarnessChoice, chosen: HarnessChoice) -> Span<'static> {
    choice(option.label(), option == chosen)
}

/// One of a dialog's choices, the chosen one stood out.
fn choice(label: &str, chosen: bool) -> Span<'static> {
    let label = format!(" {label} ");
    if chosen {
        Span::from(label).reversed()
    } else {
        Span::from(label).dim()
    }
}

/// A field of a dialog that does not have the cursor, as typed.
fn typed_line(input: &LineInput) -> Line<'static> {
    match input.text() {
        "" => Line::from("(nothing yet)").dim(),
        text => Line::from(text.to_owned()),
    }
}

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
            let mut run = vec![Span::raw("Run ")];
            for option in HarnessChoice::ALL {
                run.push(harness_option(option, *harness));
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
        Dialog::AssignAgent {
            project,
            key,
            harness,
            another,
            into,
        } => {
            let title = app
                .ticket(*project, key)
                .map_or(String::new(), |ticket| format!(" {}", ticket.title));
            let mut lines = vec![Line::from(format!("{}{title}", key.shown()))];
            let sharing = app.sharing(*project);
            let keys = if sharing.len() > 1 {
                // Nothing in the ticket says which repository it is for.
                lines.push(Line::from("Which project is it for?").dim());
                for choice_of in sharing {
                    let name = app
                        .model
                        .project(choice_of)
                        .map_or("?", |found| found.name.as_str());
                    let mut line = vec![Span::raw(" "), choice(name, choice_of == *into)];
                    let agents = app.model.agents_on(&[choice_of], key).len();
                    if agents > 0 {
                        let noun = if agents == 1 { "agent" } else { "agents" };
                        line.push(Span::from(format!("  {agents} {noun} on it")).dim());
                    }
                    lines.push(Line::from(line));
                }
                "↑↓ project   ⇥ ⇧⇥ agent   ⏎ start   esc cancel"
            } else {
                "⇥ ⇧⇥ choose   ⏎ start   esc cancel"
            };
            let mut run = vec![Span::raw("Start ")];
            for option in HarnessChoice::ALL.into_iter().filter(|o| o.is_agent()) {
                run.push(harness_option(option, *harness));
            }
            let place = if app.starts_another(*into, key, *another) {
                "In a worktree of its own, with the ticket as its first prompt."
            } else {
                "In the ticket's worktree, with the ticket as its first prompt."
            };
            lines.extend([
                Line::from(""),
                Line::from(run),
                Line::from(place).dim(),
                Line::from(""),
                Line::from(keys).dim(),
            ]);
            (" Assign an agent ", lines, None)
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
        Dialog::TicketsFrom {
            project,
            tracker,
            site,
            key,
            on_key,
        } => {
            let name = app
                .model
                .project(*project)
                .map_or("?", |found| found.name.as_str());
            let mut from = vec![Span::raw("From ")];
            for option in TRACKERS {
                from.push(choice(tracker_label(option), option == *tracker));
            }
            let mut lines = vec![
                Line::from(format!("Where {name}'s tickets come from")).bold(),
                Line::from(from),
                Line::from(""),
            ];
            let jira = *tracker == Some(TrackerKind::Jira);
            let site_label = "Jira site, such as acme.atlassian.net:";
            let key_label = if jira {
                "Project key, such as APP:"
            } else {
                "Team key, such as ENG:"
            };
            hint = if jira {
                "⇥ ⇧⇥ choose   ↑↓ field   ⏎ save   esc cancel"
            } else {
                "⇥ ⇧⇥ choose   ⏎ save   esc cancel"
            };
            match focused(*tracker, *on_key) {
                Field::None => {
                    lines.push(
                        Line::from(match tracker {
                            Some(TrackerKind::GitHub) => {
                                "Its GitHub issues, through your gh login."
                            }
                            Some(TrackerKind::GitLab) => {
                                "Its GitLab issues, through your glab login."
                            }
                            _ => "Where its git remote's host keeps them, as Muster can tell.",
                        })
                        .dim(),
                    );
                    lines.push(Line::from(""));
                    lines.push(Line::from(hint).dim());
                    (" Tickets from ", lines, None)
                }
                Field::Site => {
                    lines.push(Line::from(site_label));
                    below = vec![Line::from(key_label), typed_line(key)];
                    (" Tickets from ", lines, Some(site))
                }
                Field::Key => {
                    if jira {
                        lines.push(Line::from(site_label));
                        lines.push(typed_line(site));
                    }
                    lines.push(Line::from(key_label));
                    (" Tickets from ", lines, Some(key))
                }
            }
        }
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

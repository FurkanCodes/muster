//! The Settings screen: each project with the settings the person can
//! change here, then Muster's own, and what the selected one does and
//! where it is kept.

use muster_core::ProjectId;
use muster_core::settings::{ProjectSettings, SettingSource};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{look, panel, shorten};
use crate::app::tickets_from::tracker_label;
use crate::app::{App, SettingRow, Settings};

/// What the only-my-tickets setting is called in the list.
const ONLY_MINE: &str = "Only my tickets";
/// What the setting of where the tickets come from is called.
const TICKETS_FROM: &str = "Tickets from";
/// What the setting of the branches' prefix is called.
const MUSTER_PREFIX: &str = "Branches with muster/";
/// What the setting of the notch is called.
const NOTCH: &str = "Agents in the notch";

pub(super) fn draw_settings(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = panel(" Settings ", app.list_has_keys());
    let rows = app.setting_rows();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new("No projects yet: add one in the sessions view with a.")
                .wrap(Wrap { trim: false })
                .block(block)
                .dim(),
            area,
        );
        return;
    }
    let width = usize::from(area.width.saturating_sub(2));
    let items: Vec<ListItem<'_>> = rows
        .iter()
        .map(|row| ListItem::new(row_line(app, *row, width)))
        .collect();
    let mut state = ListState::default()
        .with_offset(app.settings_offset)
        .with_selected(
            app.settings_selected
                .and_then(|selected| rows.iter().position(|row| *row == selected)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(look::selected_style(app.list_has_keys()));
    frame.render_stateful_widget(list, area, &mut state);
}

/// A project's heading, or a setting with its value at the line's end.
fn row_line(app: &App, row: SettingRow, width: usize) -> Line<'static> {
    let edge = look::edge(app, row.project(), app.settings_selected == Some(row));
    let mut line = row_text(app, row, width.saturating_sub(1));
    line.spans.insert(0, edge);
    line
}

/// A row's line after its edge.
fn row_text(app: &App, row: SettingRow, width: usize) -> Line<'static> {
    let settings = row.project().and_then(|project| app.settings.get(&project));
    match row {
        SettingRow::Muster => Line::from(Span::from("Muster").bold()),
        SettingRow::Notch => labelled(NOTCH, notch_value(app), width),
        SettingRow::Project(project) => {
            let colour = look::project_colour(app, project);
            let mut spans =
                vec![Span::styled(project_name(app, project), Style::default().fg(colour)).bold()];
            match settings {
                None | Some(Settings::Loading) => spans.push(Span::from("  reading…").dim()),
                Some(Settings::Failed(_)) => spans.push(Span::from("  cannot be read").red()),
                Some(Settings::Loaded(_)) => {}
            }
            Line::from(spans)
        }
        SettingRow::OnlyMine(_) => {
            let value = match settings {
                Some(Settings::Loaded(found)) if found.only_mine.on => {
                    Span::from("on").green().bold()
                }
                Some(Settings::Loaded(_)) => Span::from("off").dim(),
                Some(Settings::Failed(_)) => Span::from("?").red(),
                None | Some(Settings::Loading) => Span::from("…").dim(),
            };
            labelled(ONLY_MINE, value, width)
        }
        SettingRow::TicketsFrom(_) => {
            let value = match settings {
                Some(Settings::Loaded(found)) => tickets_value(found),
                Some(Settings::Failed(_)) => Span::from("?").red(),
                None | Some(Settings::Loading) => Span::from("…").dim(),
            };
            labelled(TICKETS_FROM, value, width)
        }
        SettingRow::MusterPrefix(_) => {
            let value = match settings {
                Some(Settings::Loaded(found)) => match found.muster_prefix {
                    Some(prefix) if prefix.on => Span::from("on").green().bold(),
                    Some(_) => Span::from("off").dim(),
                    None => Span::from("?").red(),
                },
                Some(Settings::Failed(_)) => Span::from("?").red(),
                None | Some(Settings::Loading) => Span::from("…").dim(),
            };
            labelled(MUSTER_PREFIX, value, width)
        }
    }
}

/// A setting's line: its name, and its value at the line's end.
fn labelled(label: &str, value: Span<'static>, width: usize) -> Line<'static> {
    let label = format!("  {label}");
    let room = width.saturating_sub(value.content.chars().count() + 1);
    let label = shorten(&label, room);
    let gap = room.saturating_sub(label.chars().count()) + 1;
    Line::from(vec![Span::raw(label), Span::raw(" ".repeat(gap)), value])
}

/// Whether the notch is on, or being turned on or off.
fn notch_value(app: &App) -> Span<'static> {
    if app.notch_changing {
        Span::from("…").dim()
    } else if app.model.preferences().notch {
        Span::from("on").green().bold()
    } else {
        Span::from("off").dim()
    }
}

/// Where the tickets come from, in a word or two: `Jira TURPAT`; `choose`
/// where nothing can tell.
fn tickets_value(settings: &ProjectSettings) -> Span<'static> {
    match &settings.tickets {
        Some(from) => {
            let which = from.project.as_ref().or(from.team.as_ref());
            let tracker = tracker_label(Some(from.tracker));
            Span::from(match which {
                Some(which) => format!("{tracker} {which}"),
                None => tracker.to_owned(),
            })
            .bold()
        }
        None if settings.tickets_untold => Span::from("choose").red().bold(),
        None => Span::from("git remote").dim(),
    }
}

fn project_name(app: &App, project: ProjectId) -> String {
    app.model
        .project(project)
        .map_or_else(|| "?".to_owned(), |found| found.name.clone())
}

/// The selected setting: what it does, where its value comes from and
/// where a change goes; or the selected project's settings as a whole.
pub(super) fn draw_setting_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let text = match app.settings_selected {
        Some(SettingRow::Muster | SettingRow::Notch) => notch_text(app),
        Some(row) => match row.project().and_then(|project| app.settings.get(&project)) {
            Some(Settings::Loaded(settings)) => match row {
                SettingRow::OnlyMine(_) => only_mine_text(settings),
                SettingRow::TicketsFrom(_) => tickets_from_text(settings),
                SettingRow::MusterPrefix(_) => muster_prefix_text(settings),
                SettingRow::Project(project) => project_text(app, project, settings),
                SettingRow::Muster | SettingRow::Notch => notch_text(app),
            },
            Some(Settings::Failed(error)) => Text::from(vec![
                Line::from(
                    row.project()
                        .map(|project| project_name(app, project))
                        .unwrap_or_default(),
                )
                .bold(),
                Line::from(""),
                Line::from("Its settings cannot be read:").red(),
                Line::from(error.clone()),
                Line::from(""),
                Line::from("Fix the file it names, then r reads them again.").dim(),
            ]),
            None | Some(Settings::Loading) => Text::from("Reading its settings…").dim(),
        },
        None => Text::from("Select a setting to see it here.").dim(),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(panel(" Setting ", false)),
        area,
    );
}

fn only_mine_text(settings: &ProjectSettings) -> Text<'static> {
    let toggle = settings.only_mine;
    let (state, does, key) = if toggle.on {
        (
            Span::from("on").green().bold(),
            "The Tickets screen lists only the open tickets assigned to you.",
            "⏎ or space turns it off, to list every open ticket.",
        )
    } else {
        (
            Span::from("off").bold(),
            "The Tickets screen lists every open ticket.",
            "⏎ or space turns it on, to list only those assigned to you.",
        )
    };
    let from = match toggle.from {
        SettingSource::Yours => "your settings",
        SettingSource::Project => "the project's muster.toml",
        SettingSource::Default => "nothing yet: off unless set",
    };
    Text::from(vec![
        Line::from(vec![Span::from(format!("{ONLY_MINE}  ")).bold(), state]),
        Line::from(does),
        Line::from(""),
        detail("Tickets from", settings.tickets_from.clone()),
        detail(
            "Assigned to",
            "whoever is signed in: your gh or glab login, your Jira or Linear sign-in".to_owned(),
        ),
        detail("Set by", from.to_owned()),
        detail("Saved in", settings.file.display().to_string()),
        Line::from(""),
        Line::from(key).dim(),
        Line::from(
            "A change goes in your own settings file, never in the repository, and the tickets \
             are fetched again.",
        )
        .dim(),
    ])
}

fn notch_text(app: &App) -> Text<'static> {
    let preferences = app.model.preferences();
    let (state, does, key) = if preferences.notch {
        (
            Span::from("on").green().bold(),
            "On a MacBook with a notch, the agents show around it while you look away from \
             Muster: how many work, how many wait on you. Point at it for the list, and click one \
             to come back to it here.",
            "⏎ or space turns it off.",
        )
    } else {
        (
            Span::from("off").bold(),
            "Muster leaves the notch alone.",
            "⏎ or space turns it on.",
        )
    };
    Text::from(vec![
        Line::from(vec![Span::from(format!("{NOTCH}  ")).bold(), state]),
        Line::from(does),
        Line::from(""),
        detail(
            "Shown",
            "while an agent works or waits on you, and no Muster window has your eyes".to_owned(),
        ),
        detail("Saved in", preferences.file.display().to_string()),
        Line::from(""),
        Line::from(key).dim(),
        Line::from("Muster's own settings are yours, for all of it, not any project's.").dim(),
    ])
}

fn muster_prefix_text(settings: &ProjectSettings) -> Text<'static> {
    let Some(toggle) = settings.muster_prefix else {
        return Text::from("The daemon did not say how it names branches.").dim();
    };
    let (state, does, key) = if toggle.on {
        (
            Span::from("on").green().bold(),
            "The branches Muster makes start with muster/: muster/issue-42-fix-the-login for a \
             ticket, muster/4f2k9q for a worktree you give no name.",
            "⏎ or space turns it off, for issue-42-fix-the-login and worktree-4f2k9q.",
        )
    } else {
        (
            Span::from("off").bold(),
            "The branches Muster makes go without muster/: issue-42-fix-the-login for a ticket, \
             worktree-4f2k9q for a worktree you give no name.",
            "⏎ or space turns it on, for muster/issue-42-fix-the-login.",
        )
    };
    let from = match toggle.from {
        SettingSource::Yours => "your settings",
        SettingSource::Project => "the project's muster.toml",
        SettingSource::Default => "nothing yet: on unless set",
    };
    Text::from(vec![
        Line::from(vec![Span::from(format!("{MUSTER_PREFIX}  ")).bold(), state]),
        Line::from(does),
        Line::from(""),
        detail("Set by", from.to_owned()),
        detail("Saved in", settings.file.display().to_string()),
        Line::from(""),
        Line::from(key).dim(),
        Line::from(
            "Branches already made keep their names, and a ticket's agents are found by either \
             name. A change goes in your own settings file, never in the repository.",
        )
        .dim(),
    ])
}

fn tickets_from_text(settings: &ProjectSettings) -> Text<'static> {
    let from = match settings.tickets_set_by {
        SettingSource::Yours => "your settings",
        SettingSource::Project => "the project's muster.toml",
        SettingSource::Default => "its git remote, as nothing names a tracker",
    };
    let said = Line::from(settings.tickets_from.clone());
    let mut lines = vec![
        Line::from(vec![
            Span::from(format!("{TICKETS_FROM}  ")).bold(),
            tickets_value(settings),
        ]),
        if settings.tickets_untold {
            said.red()
        } else {
            said
        },
        Line::from(""),
        detail("Set by", from.to_owned()),
        detail("Saved in", settings.file.display().to_string()),
        Line::from(""),
        Line::from(
            "⏎ chooses where they come from: its git remote, GitHub, GitLab, Jira (its site and a \
             project's key) or Linear (a team's key).",
        )
        .dim(),
        Line::from(
            "A change goes in your own settings file, never in the repository, and the tickets \
             are fetched again.",
        )
        .dim(),
    ];
    if settings.tickets_untold {
        lines.insert(2, Line::from(""));
        lines.insert(
            3,
            Line::from(
                "A remote on a company's own server, such as an IP address, says nothing of \
                 where its tickets are: choose them here.",
            ),
        );
    }
    Text::from(lines)
}

fn project_text(app: &App, project: ProjectId, settings: &ProjectSettings) -> Text<'static> {
    let root = app
        .model
        .project(project)
        .map(|found| found.root.display().to_string())
        .unwrap_or_default();
    Text::from(vec![
        Line::from(project_name(app, project)).bold(),
        Line::from(root).dim(),
        Line::from(""),
        detail("Tickets from", settings.tickets_from.clone()),
        detail("Your settings", settings.file.display().to_string()),
        Line::from(""),
        Line::from(
            "Its settings are the muster.toml in its folder, with yours over it. What you change \
             here goes in yours.",
        )
        .dim(),
    ])
}

/// A labelled line of details.
fn detail(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<14}")).dim(),
        Span::raw(value),
    ])
}

/// The keys that apply on the Settings screen, for what is selected.
pub(super) fn hints(app: &App) -> Vec<&'static str> {
    let mut hints = Vec::new();
    match app.settings_selected {
        Some(SettingRow::OnlyMine(project)) => {
            if let Some(Settings::Loaded(settings)) = app.settings.get(&project) {
                hints.push(if settings.only_mine.on {
                    "⏎ turn off"
                } else {
                    "⏎ turn on"
                });
            }
        }
        Some(SettingRow::TicketsFrom(project)) => {
            if let Some(Settings::Loaded(_)) = app.settings.get(&project) {
                hints.push("⏎ choose");
            }
        }
        Some(SettingRow::MusterPrefix(project)) => {
            if let Some(Settings::Loaded(settings)) = app.settings.get(&project)
                && let Some(prefix) = settings.muster_prefix
            {
                hints.push(if prefix.on {
                    "⏎ turn off"
                } else {
                    "⏎ turn on"
                });
            }
        }
        Some(SettingRow::Notch) if !app.notch_changing => {
            hints.push(if app.model.preferences().notch {
                "⏎ turn off"
            } else {
                "⏎ turn on"
            });
        }
        Some(SettingRow::Project(_) | SettingRow::Muster | SettingRow::Notch) | None => {}
    }
    hints.push("r read again");
    hints
}

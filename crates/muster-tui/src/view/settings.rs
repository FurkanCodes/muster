//! The Settings screen: each project with the settings the person can
//! change here, then Muster's own, and what the selected one does and
//! where it is kept.

use muster_core::ProjectId;
use muster_core::settings::{Palette, ProjectSettings, SessionLines, SettingSource};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{
    delegation, list_block, look, naming, panel, plugins, shared_builds, shorten, telling,
};
use crate::app::{App, SettingRow, Settings};

/// What the only-my-tickets setting is called in the list.
const ONLY_MINE: &str = "Only my tickets";
/// What the setting of where the tickets come from is called.
const TICKETS_FROM: &str = "Tickets from";
/// What the setting of the branches' prefix is called.
const MUSTER_PREFIX: &str = "Branches with muster/";
/// What the setting of the notch is called.
const NOTCH: &str = "Agents in the notch";
/// What the agents the person added are called.
const AGENTS: &str = "Your agents";
/// What the setting of the width a session sits beside the list from is
/// called.
const SIDE_BY_SIDE: &str = "Session beside the list from";
/// What the setting of whose colours the UI draws in is called.
const COLOURS: &str = "Colours";
/// What the setting of how many lines a session takes is called.
const SESSION_LINES: &str = "Lines for each session";
/// What the setting of where the panels are is called.
const PANELS: &str = "Panels";
/// What the setting of the keys is called.
const KEYS: &str = "Keys";
/// What the setting of notifications is called.
const NOTIFICATIONS: &str = "Notifications";

pub(super) fn draw_settings(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let block = list_block(app, "settings", app.list_has_keys());
    let rows = app.setting_rows();
    if rows.is_empty() {
        frame.render_widget(
            Paragraph::new(super::no_projects(app))
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
        .highlight_style(look::selected_style(app, app.list_has_keys()));
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
        SettingRow::Agents => labelled(AGENTS, agents_value(app), width),
        SettingRow::SideBySide => labelled(SIDE_BY_SIDE, side_by_side_value(app), width),
        SettingRow::Colours => labelled(COLOURS, colours_value(app), width),
        SettingRow::SessionLines => labelled(SESSION_LINES, session_lines_value(app), width),
        SettingRow::Panels => labelled(PANELS, panels_value(app), width),
        SettingRow::NamesByAgent => {
            labelled(naming::NAMES_BY_AGENT, naming::by_agent_value(app), width)
        }
        SettingRow::NamedBy => labelled(naming::NAMED_BY, Span::from(app.namer_label()), width),
        SettingRow::NamingModel => labelled(naming::NAMING_MODEL, naming::model_value(app), width),
        SettingRow::Keys => labelled(KEYS, keys_value(app), width),
        SettingRow::SharedBuilds => {
            labelled(shared_builds::LABEL, shared_builds::value(app), width)
        }
        SettingRow::Delegation => labelled(delegation::LABEL, delegation::value(app), width),
        SettingRow::Notifications => labelled(NOTIFICATIONS, notifications_value(app), width),
        SettingRow::Tell(kind) => labelled(&telling::label(kind), telling::value(app, kind), width),
        SettingRow::Plugins | SettingRow::Plugin(_) | SettingRow::AddPlugin => {
            plugins::row_text(app, row, width)
        }
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
                Some(Settings::Loaded(found)) => tickets_value(app, found),
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
    if app.notch_changing() {
        Span::from("…").dim()
    } else if app.model.preferences().notch {
        Span::from("on").green().bold()
    } else {
        Span::from("off").dim()
    }
}

/// Whose colours the UI draws in, or that it is being changed.
fn colours_value(app: &App) -> Span<'static> {
    if app.look_changing() {
        return Span::from("…").dim();
    }
    match app.model.preferences().palette {
        Palette::Muster => Span::from("Muster's").bold(),
        Palette::Terminal => Span::from("the terminal's").bold(),
    }
}

/// How many lines a session takes, or that it is being changed.
fn session_lines_value(app: &App) -> Span<'static> {
    if app.look_changing() {
        return Span::from("…").dim();
    }
    match app.model.preferences().session_lines {
        SessionLines::Two => Span::from("two").bold(),
        SessionLines::One => Span::from("one").bold(),
    }
}

/// The agents the person added, by name; why none can be read where
/// Muster's own settings do not read.
fn agents_value(app: &App) -> Span<'static> {
    let preferences = app.model.preferences();
    if preferences.problem.is_some() {
        return Span::from("cannot be read").red();
    }
    if preferences.agents.is_empty() {
        return Span::from("none").dim();
    }
    let labels: Vec<&str> = preferences
        .agents
        .iter()
        .map(|agent| agent.label.as_str())
        .collect();
    Span::from(labels.join(", ")).bold()
}

/// From how many columns a session sits beside the list, or that it is
/// being changed.
fn side_by_side_value(app: &App) -> Span<'static> {
    if app.side_by_side_changing() {
        return Span::from("…").dim();
    }
    match app.model.preferences().side_by_side_from {
        0 => Span::from("any width").bold(),
        columns => Span::from(format!("{columns} columns")).bold(),
    }
}

/// Where the tickets come from, in a word or two: `Jira TURPAT`, the
/// tracker's name and the first of its settings said, past a site or a
/// query; `choose` where nothing can tell.
fn tickets_value(app: &App, settings: &ProjectSettings) -> Span<'static> {
    match &settings.tickets {
        Some(from) => {
            let offered = app
                .offered_trackers()
                .into_iter()
                .find(|offered| offered.id == from.tracker);
            let which = offered.as_ref().and_then(|offered| {
                offered
                    .settings
                    .iter()
                    .filter(|setting| !matches!(setting.key.as_str(), "site" | "jql"))
                    .find_map(|setting| from.fields.get(&setting.key))
            });
            let tracker = offered
                .as_ref()
                .map_or(from.tracker.as_str(), |offered| offered.name.as_str());
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
    let width = usize::from(area.width.saturating_sub(2));
    let text = match app.settings_selected {
        Some(SettingRow::Muster | SettingRow::Notch) if app.notch_known() => notch_text(app),
        Some(SettingRow::SideBySide) => side_by_side_text(app),
        Some(SettingRow::Colours) => colours_text(app),
        Some(SettingRow::SessionLines) => session_lines_text(app),
        Some(SettingRow::Panels) => panels_text(app),
        Some(SettingRow::Keys) => keys_text(app),
        Some(SettingRow::SharedBuilds) => shared_builds::text(app),
        Some(SettingRow::Delegation) => delegation::text(app),
        Some(SettingRow::Notifications) => notifications_text(app),
        Some(SettingRow::Tell(kind)) => telling::text(app, kind),
        Some(SettingRow::NamesByAgent | SettingRow::NamedBy | SettingRow::NamingModel) => {
            naming::text(app)
        }
        Some(SettingRow::Muster | SettingRow::Agents | SettingRow::Notch) => agents_text(app),
        Some(row @ (SettingRow::Plugins | SettingRow::Plugin(_) | SettingRow::AddPlugin)) => {
            plugins::details(app, row)
        }
        Some(row) => match row.project().and_then(|project| app.settings.get(&project)) {
            Some(Settings::Loaded(settings)) => match row {
                SettingRow::OnlyMine(_) => only_mine_text(settings),
                SettingRow::TicketsFrom(_) => tickets_from_text(app, settings),
                SettingRow::MusterPrefix(_) => muster_prefix_text(settings),
                SettingRow::Project(project) => project_text(app, project, settings),
                SettingRow::Muster | SettingRow::Notch => notch_text(app),
                SettingRow::Agents => agents_text(app),
                SettingRow::SideBySide => side_by_side_text(app),
                SettingRow::Colours => colours_text(app),
                SettingRow::SessionLines => session_lines_text(app),
                SettingRow::Panels => panels_text(app),
                SettingRow::Keys => keys_text(app),
                SettingRow::SharedBuilds => shared_builds::text(app),
                SettingRow::Delegation => delegation::text(app),
                SettingRow::Notifications => notifications_text(app),
                SettingRow::Tell(kind) => telling::text(app, kind),
                SettingRow::NamesByAgent | SettingRow::NamedBy | SettingRow::NamingModel => {
                    naming::text(app)
                }
                SettingRow::Plugins | SettingRow::Plugin(_) | SettingRow::AddPlugin => {
                    plugins::details(app, row)
                }
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
                Line::from(format!(
                    "Fix the file it names, then {} reads them again.",
                    app.key(muster_core::keys::Action::ReadSettings)
                ))
                .dim(),
            ]),
            None | Some(Settings::Loading) => {
                Text::from(super::loading::rule(app, "Loading settings", width))
            }
        },
        None => Text::from("Select a setting to see it here.").dim(),
    };
    frame.render_widget(
        Paragraph::new(text)
            .wrap(Wrap { trim: false })
            .block(panel(app, " Setting ", false)),
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

fn colours_text(app: &App) -> Text<'static> {
    let preferences = app.model.preferences();
    let (does, key) = match preferences.palette {
        Palette::Muster => (
            "Muster draws in its own colours: soft text, one accent for what has your keys, and \
             the statuses' colours, the same in any terminal. The selected line is a shade of \
             your terminal's own background, light or dark.",
            "⏎ or space draws in your terminal's own sixteen colours instead.",
        ),
        Palette::Terminal => (
            "Muster draws in your terminal's own sixteen colours, so it follows your terminal's \
             theme.",
            "⏎ or space draws in Muster's own colours instead.",
        ),
    };
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{COLOURS}  ")).bold(),
            colours_value(app),
        ]),
        Line::from(does),
        Line::from(""),
        detail("Saved in", preferences.file.display().to_string()),
        Line::from(""),
        Line::from(key).dim(),
        Line::from("A session's own program keeps the colours it draws in.").dim(),
    ])
}

fn session_lines_text(app: &App) -> Text<'static> {
    let preferences = app.model.preferences();
    let (does, key) = match preferences.session_lines {
        SessionLines::Two => (
            "Each session takes two lines in the lists: its name, whole where it fits, and under \
             it what it is doing and for how long.",
            "⏎ or space puts each on one line, so twice as many fit.",
        ),
        SessionLines::One => (
            "Each session takes one line in the lists: its name, what it is doing and for how \
             long, side by side, so twice as many fit.",
            "⏎ or space gives each two lines, so its name fits.",
        ),
    };
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{SESSION_LINES}  ")).bold(),
            session_lines_value(app),
        ]),
        Line::from(does),
        Line::from(""),
        detail("Saved in", preferences.file.display().to_string()),
        Line::from(""),
        Line::from(key).dim(),
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

/// The coding agents the person added in Muster's own settings, and how
/// to add one.
fn agents_text(app: &App) -> Text<'static> {
    let preferences = app.model.preferences();
    let mut lines = vec![
        Line::from(vec![
            Span::from(format!("{AGENTS}  ")).bold(),
            agents_value(app),
        ]),
        Line::from(
            "Coding agents Muster does not know, added in your own settings, are offered beside \
             its own: for a new session, and for a ticket where they take a first prompt.",
        ),
        Line::from(""),
    ];
    if let Some(problem) = &preferences.problem {
        lines.extend([
            Line::from("Muster's own settings cannot be read:").red(),
            Line::from(problem.clone()),
            Line::from(""),
        ]);
    }
    for agent in &preferences.agents {
        let mark = look::agent_mark(app, &agent.name);
        let prompt = if agent.takes_prompt {
            "takes a first prompt"
        } else {
            "takes no prompt"
        };
        lines.push(Line::from(vec![
            mark,
            Span::from(format!(" {}", agent.label)).bold(),
            Span::from(format!("  {}, {prompt}", agent.name)).dim(),
        ]));
    }
    if !preferences.agents.is_empty() {
        lines.push(Line::from(""));
    }
    lines.extend([
        detail("Saved in", preferences.file.display().to_string()),
        Line::from(""),
        Line::from(format!(
            "Add one there, then {} reads it:",
            app.key(muster_core::keys::Action::ReadSettings)
        ))
        .dim(),
        Line::from("  [agents.my-agent]").dim(),
        Line::from("  program = \"my-agent\"").dim(),
        Line::from("  prompt = [\"--message\", \"{prompt}\"]").dim(),
        Line::from("  resume = [\"--continue\"]").dim(),
        Line::from(""),
        Line::from(
            "Muster hears them by their terminal's progress, their window title and their exit, \
             and never changes their own settings.",
        )
        .dim(),
    ]);
    Text::from(lines)
}

/// Whether the person is told of what newly needs them, and how.
fn notifications_value(app: &App) -> Span<'static> {
    match (app.model.preferences().notifications, app.model.notifier()) {
        (false, _) => Span::from("off").dim(),
        (true, true) => Span::from("the Mac's").green().bold(),
        (true, false) => Span::from("on").green().bold(),
    }
}

fn notifications_text(app: &App) -> Text<'static> {
    let how = if app.model.notifier() {
        "Muster's helper tells you with the Mac's own notifications, in any terminal and with \
         every window closed; a click on one opens its session in the Muster window you used \
         last."
    } else if app.notch_here {
        "On a Mac, Muster's helper tells you with the Mac's own notifications once you allow \
         them: macOS asks once, for Muster. Allowed later in System Settings, Notifications, \
         Muster. Until then, this window's terminal tells you, as a desktop notification and \
         its bell."
    } else {
        "This window's terminal tells you, as a desktop notification and its bell, while the \
         window is not in front."
    };
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{NOTIFICATIONS}  ")).bold(),
            notifications_value(app),
        ]),
        Line::from(
            "You are told when a session newly needs you (a question, a permission, a finish \
             not yet seen) while you look away from every Muster window, once for each.",
        ),
        Line::from(""),
        Line::from(how),
        Line::from(""),
        detail(
            "Saved in",
            app.model.preferences().file.display().to_string(),
        ),
    ])
}

/// Whose keys they are: Muster's, or how many the person set, a panel's
/// among them while its plugin runs.
fn keys_value(app: &App) -> Span<'static> {
    let set = app
        .model
        .preferences()
        .keys
        .keys()
        .filter(|name| {
            muster_core::keys::Action::named(name).is_some()
                || app.keymap.panel_named(name).is_some()
        })
        .count();
    match set {
        0 => Span::from("Muster's").dim(),
        1 => Span::from("1 of yours"),
        set => Span::from(format!("{set} of yours")),
    }
}

/// What setting the keys does, where they are kept, and which of them do
/// nothing.
fn keys_text(app: &App) -> Text<'static> {
    let mut lines = vec![
        Line::from(vec![
            Span::from(format!("{KEYS}  ")).bold(),
            keys_value(app),
        ]),
        Line::from(
            "Every key of every screen can be another: ⏎ lists them, by what they do. A key \
             another action has on the same screen is refused, and in a session a key needs \
             Ctrl or Alt, or is a capital, as the rest go to its program. The hints, the help \
             and the menus show your keys.",
        ),
        Line::from(""),
        detail(
            "Saved in",
            app.model.preferences().file.display().to_string(),
        ),
        Line::from(""),
        Line::from("Moving about (the arrows, the page keys) and ⏎ stay as they are.").dim(),
    ];
    for warning in app.keymap.warnings() {
        lines.extend([Line::from(""), Line::from(warning.to_string()).yellow()]);
    }
    Text::from(lines)
}

/// Where the panels are: as at first, or moved, and the list folded.
fn panels_value(app: &App) -> Span<'static> {
    let moved = *app.panels() != muster_core::layout::Layout::default();
    match (moved, app.list_folded()) {
        (false, false) => Span::from("as at first").dim(),
        (true, false) => Span::from("moved"),
        (false, true) => Span::from("list folded"),
        (true, true) => Span::from("moved, list folded"),
    }
}

fn panels_text(app: &App) -> Text<'static> {
    let folded = if app.list_folded() {
        "folded away"
    } else {
        "beside them"
    };
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{PANELS}  ")).bold(),
            panels_value(app),
        ]),
        Line::from(format!(
            "Drag a panel by its title to another edge of the window or of another panel, and \
             the line between two to share out their place. {} moves them with the keys. \
             {} folds the list away and brings it back, as you type in a session too.",
            app.key(muster_core::keys::Action::MovePanels),
            app.key(muster_core::keys::Action::FoldList),
        )),
        Line::from(""),
        detail("Now", app.panels().to_string()),
        detail("The list", folded.to_owned()),
        detail(
            "Saved in",
            app.model.preferences().file.display().to_string(),
        ),
        Line::from(""),
        Line::from("⏎ puts every panel back where it was at first.").dim(),
    ])
}

fn side_by_side_text(app: &App) -> Text<'static> {
    let preferences = app.model.preferences();
    let does = match preferences.side_by_side_from {
        0 => "A session's terminal sits beside the list in a window of any width.".to_owned(),
        columns => format!(
            "A session's terminal sits beside the list in a window of {columns} columns or more. \
             In a narrower one, the list takes the whole width: ⏎ opens a session over it, as \
             full screen does, and {} goes back.",
            app.key(muster_core::keys::Action::LeaveSession)
        ),
    };
    let now = if app.narrow() {
        format!("{} columns: the list or a session", app.terminal_size.cols)
    } else {
        format!("{} columns: both, side by side", app.terminal_size.cols)
    };
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{SIDE_BY_SIDE}  ")).bold(),
            side_by_side_value(app),
        ]),
        Line::from(does),
        Line::from(""),
        detail("This window", now),
        detail("Saved in", preferences.file.display().to_string()),
        Line::from(""),
        Line::from("⏎ or space goes to the next width, ← → to a smaller or larger one.").dim(),
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

fn tickets_from_text(app: &App, settings: &ProjectSettings) -> Text<'static> {
    let from = match settings.tickets_set_by {
        SettingSource::Yours => "your settings",
        SettingSource::Project => "the project's muster.toml",
        SettingSource::Default => "its git remote, as nothing names a tracker",
    };
    let said = Line::from(settings.tickets_from.clone());
    let mut lines = vec![
        Line::from(vec![
            Span::from(format!("{TICKETS_FROM}  ")).bold(),
            tickets_value(app, settings),
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
            "⏎ chooses where they come from: its git remote, or a ticket plugin that is on, \
             Muster's own (GitHub, GitLab, Jira, Linear) or one you added, with what it asks for.",
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
pub(super) fn detail(label: &str, value: String) -> Line<'static> {
    Line::from(vec![
        Span::from(format!("{label:<14}")).dim(),
        Span::raw(value),
    ])
}

/// The keys that apply on the Settings screen, for what is selected.
pub(super) fn hints(app: &App) -> Vec<String> {
    let mut hints = match app.settings_selected {
        Some(row @ (SettingRow::Plugins | SettingRow::Plugin(_) | SettingRow::AddPlugin)) => {
            plugins::hints(app, row)
        }
        selected => setting_hints(app, selected)
            .into_iter()
            .map(str::to_owned)
            .collect(),
    };
    hints.extend(super::bottom_bar::hint(
        app,
        muster_core::keys::Action::ReadSettings,
        "read again",
    ));
    hints
}

/// The keys that apply to the selected setting, the plugins' aside.
fn setting_hints(app: &App, selected: Option<SettingRow>) -> Vec<&'static str> {
    let mut hints = Vec::new();
    match selected {
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
        Some(SettingRow::Notch) if !app.notch_changing() => {
            hints.push(if app.model.preferences().notch {
                "⏎ turn off"
            } else {
                "⏎ turn on"
            });
        }
        Some(SettingRow::SideBySide) if !app.side_by_side_changing() => {
            hints.extend(["⏎ next width", "← → smaller, larger"]);
        }
        Some(SettingRow::Colours) if !app.look_changing() => {
            hints.push(match app.model.preferences().palette {
                Palette::Muster => "⏎ the terminal's",
                Palette::Terminal => "⏎ Muster's",
            });
        }
        Some(SettingRow::SessionLines) if !app.look_changing() => {
            hints.push(match app.model.preferences().session_lines {
                SessionLines::Two => "⏎ one line",
                SessionLines::One => "⏎ two lines",
            });
        }
        Some(SettingRow::Panels) => hints.push("⏎ put back as at first"),
        Some(SettingRow::NamesByAgent) => {
            hints.push(if app.model.preferences().naming.by_agent {
                "⏎ turn off"
            } else {
                "⏎ turn on"
            });
        }
        Some(SettingRow::NamedBy) => hints.push("⏎ next agent"),
        Some(SettingRow::NamingModel) => hints.push("⏎ choose"),
        Some(SettingRow::Keys) => hints.push("⏎ set them"),
        Some(SettingRow::SharedBuilds) => hints.push(shared_builds::hint(app)),
        Some(SettingRow::Delegation) => hints.push(delegation::hint(app)),
        Some(SettingRow::Notifications) => {
            hints.push(if app.model.preferences().notifications {
                "⏎ turn off"
            } else {
                "⏎ turn on"
            });
        }
        Some(SettingRow::Tell(kind)) if !app.telling_changing() => {
            hints.push(if app.model.preferences().telling.of(kind).on {
                "⏎ turn off"
            } else {
                "⏎ turn on"
            });
            hints.push("← → sound");
        }
        Some(
            SettingRow::Plugins
            | SettingRow::Plugin(_)
            | SettingRow::AddPlugin
            | SettingRow::Project(_)
            | SettingRow::Tell(_)
            | SettingRow::Muster
            | SettingRow::Notch
            | SettingRow::SideBySide
            | SettingRow::Colours
            | SettingRow::SessionLines
            | SettingRow::Agents,
        )
        | None => {}
    }
    hints
}

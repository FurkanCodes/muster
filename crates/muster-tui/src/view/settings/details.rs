//! What the selected setting does, where its value comes from and where a
//! change goes, beside the Settings list; or the selected project's
//! settings as a whole.

use muster_core::ProjectId;
use muster_core::settings::{Palette, ProjectSettings, SessionLines, SettingSource};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};
use ratatui::widgets::{Paragraph, Wrap};

use super::{
    AGENTS, COLOURS, KEYS, MUSTER_PREFIX, NOTCH, NOTIFICATIONS, ONLY_MINE, PANELS, PROJECT_ORDER,
    SESSION_LINES, SIDE_BY_SIDE, TICKETS_FROM, agents_value, colours_value, detail, keys_value,
    notifications_value, panels_value, project_name, project_order_value, session_lines_value,
    side_by_side_value, tickets_value,
};
use crate::app::{App, SettingRow, Settings};
use crate::view::theme::Theme;
use crate::view::{delegation, look, naming, panel, plugins, shared_builds, telling};

/// The selected setting: what it does, where its value comes from and
/// where a change goes; or the selected project's settings as a whole.
pub(in crate::view) fn draw_setting_details(frame: &mut Frame<'_>, app: &App, area: Rect) {
    let theme = crate::view::theme::of(app);
    let width = usize::from(area.width.saturating_sub(2));
    let text = match app.settings.selected {
        Some(SettingRow::Muster | SettingRow::Notch) if app.notch_known() => notch_text(app),
        Some(SettingRow::SideBySide) => side_by_side_text(app),
        Some(SettingRow::Colours) => colours_text(app),
        Some(SettingRow::SessionLines) => session_lines_text(app),
        Some(SettingRow::Panels) => panels_text(app),
        Some(SettingRow::ProjectOrder) => project_order_text(app),
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
        Some(row) => match row
            .project()
            .and_then(|project| app.settings.by_project.get(&project))
        {
            Some(Settings::Loaded(settings)) => match row {
                SettingRow::OnlyMine(_) => only_mine_text(settings, theme),
                SettingRow::TicketsFrom(_) => tickets_from_text(app, settings),
                SettingRow::MusterPrefix(_) => muster_prefix_text(settings, theme),
                SettingRow::Project(project) => project_text(app, project, settings),
                SettingRow::Muster | SettingRow::Notch => notch_text(app),
                SettingRow::Agents => agents_text(app),
                SettingRow::SideBySide => side_by_side_text(app),
                SettingRow::Colours => colours_text(app),
                SettingRow::SessionLines => session_lines_text(app),
                SettingRow::Panels => panels_text(app),
                SettingRow::ProjectOrder => project_order_text(app),
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
                Line::from("Its settings cannot be read:").fg(theme.waiting),
                Line::from(error.clone()),
                Line::from(""),
                Line::from(format!(
                    "Fix the file it names, then {} reads them again.",
                    app.key(muster_core::keys::Action::ReadSettings)
                ))
                .dim(),
            ]),
            None | Some(Settings::Loading) => {
                Text::from(crate::view::loading::rule(app, "Loading settings", width))
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

fn only_mine_text(settings: &ProjectSettings, theme: Theme) -> Text<'static> {
    let toggle = settings.only_mine;
    let (state, does, key) = if toggle.on {
        (
            Span::from("on").fg(theme.done).bold(),
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
    let theme = crate::view::theme::of(app);
    let preferences = app.model.preferences();
    let (state, does, key) = if preferences.notch {
        (
            Span::from("on").fg(theme.done).bold(),
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
    let theme = crate::view::theme::of(app);
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
            Line::from("Muster's own settings cannot be read:").fg(theme.waiting),
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

fn notifications_text(app: &App) -> Text<'static> {
    let how = if app.model.notifier() {
        "Muster's helper tells you with the Mac's own notifications, in any terminal and with \
         every window closed; a click on one opens its session in the Muster window you used \
         last."
    } else if app.person.notch_here {
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

/// What setting the keys does, where they are kept, and which of them do
/// nothing.
fn keys_text(app: &App) -> Text<'static> {
    let theme = crate::view::theme::of(app);
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
        lines.extend([
            Line::from(""),
            Line::from(warning.to_string()).fg(theme.working),
        ]);
    }
    Text::from(lines)
}

fn project_order_text(app: &App) -> Text<'static> {
    Text::from(vec![
        Line::from(vec![
            Span::from(format!("{PROJECT_ORDER}  ")).bold(),
            project_order_value(app),
        ]),
        Line::from(
            "Drag a project's line up or down the list to put it there; every Muster window              lists them so, and the next.",
        ),
        Line::from(""),
        detail(
            "Saved in",
            app.model.preferences().file.display().to_string(),
        ),
        Line::from(""),
        Line::from("⏎ lists them in the order they were added again.").dim(),
    ])
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

fn muster_prefix_text(settings: &ProjectSettings, theme: Theme) -> Text<'static> {
    let Some(toggle) = settings.muster_prefix else {
        return Text::from("The daemon did not say how it names branches.").dim();
    };
    let (state, does, key) = if toggle.on {
        (
            Span::from("on").fg(theme.done).bold(),
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
    let theme = crate::view::theme::of(app);
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
            said.fg(theme.waiting)
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

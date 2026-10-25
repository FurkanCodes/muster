//! The Settings screen: each project with the settings the person can
//! change here, then Muster's own, each with its value; what the selected
//! one does and where it is kept is `details`.

use crossterm::event::KeyCode;
use muster_core::ProjectId;
use muster_core::settings::{Palette, ProjectSettings, SessionLines};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use super::{delegation, list_block, look, naming, plugins, shared_builds, shorten, telling};
use crate::app::{App, SettingRow, Settings};
use crate::cells;

mod details;

pub(super) use details::draw_setting_details;

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
/// What the setting of the projects' order is called.
const PROJECT_ORDER: &str = "Projects' order";
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
        .with_offset(app.settings.offset)
        .with_selected(
            app.settings
                .selected
                .and_then(|selected| rows.iter().position(|row| *row == selected)),
        );
    let list = List::new(items)
        .block(block)
        .highlight_style(look::selected_style(app, app.list_has_keys()));
    frame.render_stateful_widget(list, area, &mut state);
}

/// A project's heading, or a setting with its value at the line's end.
fn row_line(app: &App, row: SettingRow, width: usize) -> Line<'static> {
    let edge = look::edge(app, row.project(), app.settings.selected == Some(row));
    let mut line = row_text(app, row, width.saturating_sub(1));
    line.spans.insert(0, edge);
    line
}

/// A row's line after its edge.
fn row_text(app: &App, row: SettingRow, width: usize) -> Line<'static> {
    let theme = super::theme::of(app);
    let settings = row
        .project()
        .and_then(|project| app.settings.by_project.get(&project));
    match row {
        SettingRow::Muster => Line::from(Span::from("Muster").bold()),
        SettingRow::Notch => labelled(NOTCH, notch_value(app), width),
        SettingRow::Agents => labelled(AGENTS, agents_value(app), width),
        SettingRow::SideBySide => stepped(SIDE_BY_SIDE, &side_by_side_value(app), width),
        SettingRow::Colours => labelled(COLOURS, colours_value(app), width),
        SettingRow::SessionLines => labelled(SESSION_LINES, session_lines_value(app), width),
        SettingRow::Panels => labelled(PANELS, panels_value(app), width),
        SettingRow::ProjectOrder => labelled(PROJECT_ORDER, project_order_value(app), width),
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
        SettingRow::Tell(kind) => stepped(&telling::label(kind), &telling::value(app, kind), width),
        SettingRow::Plugins | SettingRow::Plugin(_) | SettingRow::AddPlugin => {
            plugins::row_text(app, row, width)
        }
        SettingRow::Project(project) => {
            let colour = look::project_colour(app, project);
            let mut spans =
                vec![Span::styled(project_name(app, project), Style::default().fg(colour)).bold()];
            match settings {
                None | Some(Settings::Loading) => spans.push(Span::from("  reading…").dim()),
                Some(Settings::Failed(_)) => {
                    spans.push(Span::from("  cannot be read").fg(theme.waiting));
                }
                Some(Settings::Loaded(_)) => {}
            }
            Line::from(spans)
        }
        SettingRow::OnlyMine(_) => {
            let value = match settings {
                Some(Settings::Loaded(found)) if found.only_mine.on => {
                    Span::from("on").fg(theme.done).bold()
                }
                Some(Settings::Loaded(_)) => Span::from("off").dim(),
                Some(Settings::Failed(_)) => Span::from("?").fg(theme.waiting),
                None | Some(Settings::Loading) => Span::from("…").dim(),
            };
            labelled(ONLY_MINE, value, width)
        }
        SettingRow::TicketsFrom(_) => {
            let value = match settings {
                Some(Settings::Loaded(found)) => tickets_value(app, found),
                Some(Settings::Failed(_)) => Span::from("?").fg(theme.waiting),
                None | Some(Settings::Loading) => Span::from("…").dim(),
            };
            labelled(TICKETS_FROM, value, width)
        }
        SettingRow::MusterPrefix(_) => {
            let value = match settings {
                Some(Settings::Loaded(found)) => match found.muster_prefix {
                    Some(prefix) if prefix.on => Span::from("on").fg(theme.done).bold(),
                    Some(_) => Span::from("off").dim(),
                    None => Span::from("?").fg(theme.waiting),
                },
                Some(Settings::Failed(_)) => Span::from("?").fg(theme.waiting),
                None | Some(Settings::Loading) => Span::from("…").dim(),
            };
            labelled(MUSTER_PREFIX, value, width)
        }
    }
}

/// A setting's line whose value steps through choices: ← and → step it,
/// and so does a click on its arrows (`value_keys`).
fn stepped(label: &str, value: &Span<'static>, width: usize) -> Line<'static> {
    let shown = Span::styled(format!("{LEFT}{}{RIGHT}", value.content), value.style);
    let mut line = labelled(label, shown, width);
    // The arrows quiet, the value as it was.
    if let Some(last) = line.spans.pop() {
        let text = last.content.to_string();
        let inner = text
            .strip_prefix(LEFT)
            .and_then(|rest| rest.strip_suffix(RIGHT))
            .unwrap_or(&text)
            .to_owned();
        line.spans.push(Span::from(LEFT).dim());
        line.spans.push(Span::styled(inner, last.style));
        line.spans.push(Span::from(RIGHT).dim());
    }
    line
}

/// The arrows either side of a value that steps.
const LEFT: &str = "‹ ";
const RIGHT: &str = " ›";

/// Where a click on `row`'s value, at its line's end, presses a key, from
/// the start of the line drawn `width` wide: its arrows ← and →, where it
/// has them, the value itself ⏎, which turns it on or off, goes to its
/// next choice or opens its dialog. None for a heading.
pub(crate) fn value_keys(app: &App, row: SettingRow, width: usize) -> Vec<(u16, u16, KeyCode)> {
    if matches!(
        row,
        SettingRow::Muster | SettingRow::Project(_) | SettingRow::Plugins
    ) {
        return Vec::new();
    }
    let line = row_line(app, row, width);
    let spans = &line.spans;
    let count = spans.len();
    let arrows =
        count >= 3 && spans[count - 1].content == RIGHT && spans[count - 3].content == LEFT;
    let keys: &[KeyCode] = if arrows {
        &[KeyCode::Left, KeyCode::Enter, KeyCode::Right]
    } else {
        &[KeyCode::Enter]
    };
    let mut at = line.width()
        - spans[count - keys.len()..]
            .iter()
            .map(Span::width)
            .sum::<usize>();
    let mut cells = Vec::new();
    for (span, key) in spans[count - keys.len()..].iter().zip(keys) {
        let width = span.width();
        cells.push((
            u16::try_from(at).unwrap_or(0),
            u16::try_from(width).unwrap_or(0),
            *key,
        ));
        at += width;
    }
    cells
}

/// A setting's line: its name, and its value at the line's end.
fn labelled(label: &str, value: Span<'static>, width: usize) -> Line<'static> {
    let label = format!("  {label}");
    let room = width.saturating_sub(cells::width(&value.content) + 1);
    let label = shorten(&label, room);
    let gap = room.saturating_sub(cells::width(&label)) + 1;
    Line::from(vec![Span::raw(label), Span::raw(" ".repeat(gap)), value])
}

/// Whether the notch is on, or being turned on or off.
fn notch_value(app: &App) -> Span<'static> {
    let theme = super::theme::of(app);
    if app.notch_changing() {
        Span::from("…").dim()
    } else if app.model.preferences().notch {
        Span::from("on").fg(theme.done).bold()
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
    let theme = super::theme::of(app);
    let preferences = app.model.preferences();
    if preferences.problem.is_some() {
        return Span::from("cannot be read").fg(theme.waiting);
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
    let theme = super::theme::of(app);
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
        None if settings.tickets_untold => Span::from("choose").fg(theme.waiting).bold(),
        None => Span::from("git remote").dim(),
    }
}

fn project_name(app: &App, project: ProjectId) -> String {
    app.model
        .project(project)
        .map_or_else(|| "?".to_owned(), |found| found.name.clone())
}
/// Whether the person is told of what newly needs them, and how.
fn notifications_value(app: &App) -> Span<'static> {
    let theme = super::theme::of(app);
    match (app.model.preferences().notifications, app.model.notifier()) {
        (false, _) => Span::from("off").dim(),
        (true, true) => Span::from("the Mac's").fg(theme.done).bold(),
        (true, false) => Span::from("on").fg(theme.done).bold(),
    }
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

/// Whether the projects are in the person's order or as they were added.
fn project_order_value(app: &App) -> Span<'static> {
    if app.model.preferences().projects.is_empty() {
        Span::from("as added").dim()
    } else {
        Span::from("yours")
    }
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
    let mut hints = match app.settings.selected {
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
            if let Some(Settings::Loaded(settings)) = app.settings.by_project.get(&project) {
                hints.push(if settings.only_mine.on {
                    "⏎ turn off"
                } else {
                    "⏎ turn on"
                });
            }
        }
        Some(SettingRow::TicketsFrom(project)) => {
            if let Some(Settings::Loaded(_)) = app.settings.by_project.get(&project) {
                hints.push("⏎ choose");
            }
        }
        Some(SettingRow::MusterPrefix(project)) => {
            if let Some(Settings::Loaded(settings)) = app.settings.by_project.get(&project)
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
            hints.extend(["⏎ next width", "← smaller", "→ larger"]);
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
        Some(SettingRow::ProjectOrder) => hints.push("⏎ as they were added"),
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
            hints.extend(["← sound", "→ sound"]);
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

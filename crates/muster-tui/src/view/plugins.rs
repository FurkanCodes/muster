//! The Plugins section of the Settings screen (`app::plugins`): a line
//! for each plugin, saying where it is from, whether it is on, and why it
//! does not run or else what it brings, a change made from the command
//! line marked; and beside it, everything the selected one runs and
//! needs. What comes from a plugin (its names, words and commands) is
//! drawn without control characters (`PRD-07`).

use muster_core::keys::Action as Does;
use muster_core::plugins::{PanelOn, PluginSource};
use muster_core::protocol::plugins::{
    ChangedBy, PluginChange, PluginDetails, PluginInfo, PluginProblem, command_line,
};
use muster_core::text::one_line;
use muster_core::time::{between, span};
use ratatui::style::Stylize;
use ratatui::text::{Line, Span, Text};

use super::settings::detail;
use super::shorten;
use super::theme::Theme;
use crate::app::{App, PluginKey, SettingRow};
use crate::cells;

/// A row of the section after its edge: the heading, a plugin, or *add a
/// plugin*.
pub(super) fn row_text(app: &App, row: SettingRow, width: usize) -> Line<'static> {
    match row {
        SettingRow::Plugin(key) => match app.plugin(key) {
            Some(plugin) => plugin_line(app, plugin, width),
            None => Line::from(format!("  {}", key.id())).dim(),
        },
        SettingRow::AddPlugin => Line::from("  + add a plugin…").dim(),
        _ => Line::from(Span::from("Plugins").bold()),
    }
}

/// A plugin's line: `● zoetrope 0.1.0 git on  panel: graph (Z)`, its
/// columns lined up with the others', and those that do not fit left out,
/// the version first, then where it is from.
fn plugin_line(app: &App, plugin: &PluginInfo, width: usize) -> Line<'static> {
    let theme = super::theme::of(app);
    let plugins = app.model.plugins();
    let widest = |of: &dyn Fn(&PluginInfo) -> String, most: usize| {
        plugins
            .iter()
            .map(|plugin| cells::width(&of(plugin)))
            .max()
            .unwrap_or(0)
            .min(most)
    };
    let name_width = widest(&name, 18);
    let version_width = widest(&version, 10);
    let source_width = widest(&|plugin| source_word(&plugin.source).to_owned(), 8);
    // Its indent, mark, name and state, and room for most of what
    // follows.
    let base = 2 + 2 + name_width + 1 + 3 + 2 + 21;
    let show_source = width >= base + 1 + source_width;
    let show_version = width >= base + 1 + source_width + 1 + version_width;

    let (mark, state) = if app.settings.plugin_changing == PluginKey::of(&plugin.id) {
        (Span::from("○ ").dim(), Span::from("…  ").dim())
    } else if plugin.runs() {
        (
            Span::from("● ").fg(theme.done),
            Span::from("on ").fg(theme.done).bold(),
        )
    } else {
        (Span::from("○ ").dim(), Span::from("off").dim())
    };
    let mut spans = vec![
        Span::raw("  "),
        mark,
        Span::raw(pad(&name(plugin), name_width)),
    ];
    if show_version {
        spans.push(Span::from(format!(" {}", pad(&version(plugin), version_width))).dim());
    }
    if show_source {
        spans.push(
            Span::from(format!(
                " {}",
                pad(source_word(&plugin.source), source_width)
            ))
            .dim(),
        );
    }
    spans.push(Span::raw(" "));
    spans.push(state);
    let used: usize = spans.iter().map(|span| cells::width(&span.content)).sum();
    let room = width.saturating_sub(used + 2);
    if room >= 4 {
        spans.push(Span::raw("  "));
        spans.push(tail(app, plugin, room));
    }
    Line::from(spans)
}

/// What a plugin's line ends with, in `room` characters: why it does not
/// run; else a change made from the command line, with when; else what it
/// brings.
fn tail(app: &App, plugin: &PluginInfo, room: usize) -> Span<'static> {
    let theme = super::theme::of(app);
    if let Some(problem) = &plugin.problem {
        return Span::from(shorten(problem_word(problem), room)).fg(theme.waiting);
    }
    if let Some(change) = plugin
        .changed
        .filter(|change| change.by == ChangedBy::CommandLine)
    {
        let text = [
            change_words(app, &change),
            format!("{} {}", change.what.words(), change.by.words()),
        ]
        .into_iter()
        .find(|text| cells::width(text) <= room)
        .unwrap_or_else(|| change.by.words().to_owned());
        return Span::from(shorten(&text, room)).fg(theme.working);
    }
    Span::from(shorten(&brings(app, plugin), room)).dim()
}

/// A plugin's name as its manifest says, or its id where that does not
/// read.
fn name(plugin: &PluginInfo) -> String {
    plugin
        .details
        .as_ref()
        .map_or_else(|| one_line(&plugin.id), |details| one_line(&details.name))
}

fn version(plugin: &PluginInfo) -> String {
    plugin
        .details
        .as_ref()
        .map_or_else(String::new, |details| one_line(&details.version))
}

/// Where a plugin is from, in a word.
fn source_word(source: &PluginSource) -> &'static str {
    match source {
        PluginSource::Builtin => "built in",
        PluginSource::Git { .. } => "git",
        PluginSource::Folder { .. } => "folder",
    }
}

/// Why a plugin does not run, in a few words for its line.
fn problem_word(problem: &PluginProblem) -> &'static str {
    match problem {
        PluginProblem::Changed => "changed since you approved it",
        PluginProblem::NotApproved => "never approved",
        PluginProblem::Missing { .. } => "its files are gone",
        PluginProblem::Unreadable { .. } => "its manifest does not read",
        PluginProblem::Unknown => "not known",
    }
}

/// What a plugin brings: `panel: graph (Z) · tickets: YouTrack`, each
/// panel with its key as `[keys]` writes it: as bound while its plugin
/// runs (`none` where it has none), else as it suggests.
fn brings(app: &App, plugin: &PluginInfo) -> String {
    let Some(details) = &plugin.details else {
        return String::new();
    };
    let mut brings = Vec::new();
    if !details.panels.is_empty() {
        let panels: Vec<String> = details
            .panels
            .iter()
            .map(|panel| {
                let name = muster_core::keys::panel_action(&plugin.id, &panel.id);
                let bound = app
                    .keymap
                    .panels()
                    .find(|(action, _)| action.name() == name);
                let key = match bound {
                    Some((_, chords)) => Some(
                        chords
                            .first()
                            .map_or_else(|| "none".to_owned(), ToString::to_string),
                    ),
                    None => panel.key.as_deref().map(one_line),
                };
                match key {
                    Some(key) => format!("{} ({key})", one_line(&panel.title)),
                    None => one_line(&panel.title),
                }
            })
            .collect();
        let noun = if panels.len() == 1 { "panel" } else { "panels" };
        brings.push(format!("{noun}: {}", panels.join(", ")));
    }
    if let Some(tickets) = &details.tickets {
        brings.push(format!("tickets: {}", one_line(&tickets.name)));
    }
    if !details.hooks.is_empty() {
        let noun = if details.hooks.len() == 1 {
            "hook"
        } else {
            "hooks"
        };
        let hooks: Vec<String> = details
            .hooks
            .iter()
            .map(|hook| one_line(&hook.id))
            .collect();
        brings.push(format!("{noun}: {}", hooks.join(", ")));
    }
    brings.join(" · ")
}

/// The last change, with when: `turned on 2 min ago from the command
/// line`.
fn change_words(app: &App, change: &PluginChange) -> String {
    match app.now {
        Some(now) => format!(
            "{} {} ago {}",
            change.what.words(),
            span(between(change.at, now)),
            change.by.words()
        ),
        None => format!("{} {}", change.what.words(), change.by.words()),
    }
}

/// `text` padded or cut to `width` characters.
fn pad(text: &str, width: usize) -> String {
    format!("{:<width$}", shorten(text, width))
}

/// The selected row's details: the section's, or a plugin's.
pub(super) fn details(app: &App, row: SettingRow) -> Text<'static> {
    match row {
        SettingRow::Plugin(key) => match app.plugin(key) {
            Some(plugin) => plugin_text(app, plugin),
            None => Text::from("It is gone.").dim(),
        },
        _ => section_text(app),
    }
}

/// What plugins are, and how one is added.
fn section_text(app: &App) -> Text<'static> {
    let theme = super::theme::of(app);
    let mut lines = vec![
        Line::from("Plugins").bold(),
        Line::from(
            "Plugins bring panels beside a session, sources of tickets, and hooks that hear \
             what happens: programs of their own, in any language, which the daemon runs apart \
             from Muster, never inside it.",
        ),
        Line::from(""),
        Line::from(format!(
            "{} adds one from a git repository's address, or links a folder of yours where it \
             is. Before anything is added you see everything it would run, the programs it \
             needs and the secrets it asks for, and say yes or no.",
            app.key(Does::AddPlugin)
        )),
        Line::from(format!(
            "On a plugin, space turns it on or off, {} updates it and {} removes it.",
            app.key(Does::UpdatePlugin),
            app.key(Does::RemovePlugin)
        )),
    ];
    if app.model.plugins().is_empty() {
        lines.extend([Line::from(""), Line::from("None yet.").dim()]);
    }
    if let Some(problem) = &app.model.preferences().problem {
        lines.extend([
            Line::from(""),
            Line::from("Muster's own settings do not read, so no plugin of yours runs:")
                .fg(theme.waiting),
            Line::from(one_line(problem)),
        ]);
    }
    lines.extend([
        Line::from(""),
        Line::from(
            "Muster keeps them in its own settings, [plugins.<id>], and marks here a change \
             made with muster plugin.",
        )
        .dim(),
    ]);
    Text::from(lines)
}

/// Everything about one plugin: what it is, whether it runs and why not,
/// where it is from, everything it runs, what it needs and is given, and
/// its last change.
fn plugin_text(app: &App, plugin: &PluginInfo) -> Text<'static> {
    let theme = super::theme::of(app);
    let mut lines = vec![Line::from(format!("{} {}", name(plugin), version(plugin))).bold()];
    if let Some(description) = plugin.details.as_ref().and_then(|d| d.description.as_ref()) {
        lines.push(Line::from(one_line(description)));
    }
    lines.push(Line::from(""));
    lines.push(match (&plugin.problem, plugin.enabled) {
        (None, true) => Line::from(vec![
            Span::from("On").fg(theme.done).bold(),
            Span::raw(": it runs."),
        ]),
        (None, false) => Line::from(vec![Span::from("Off").bold(), Span::raw(".")]),
        (Some(problem), enabled) => Line::from(vec![
            Span::from(if enabled {
                "Off until you approve it again: "
            } else {
                "Off, and cannot be turned on: "
            })
            .bold(),
            Span::from(format!("{}.", problem.to_string().trim_end_matches('.'))).fg(theme.waiting),
        ]),
    });
    lines.push(Line::from(""));
    lines.push(detail("Id", one_line(&plugin.id)));
    lines.push(detail("From", source_text(&plugin.source)));
    if let Some(details) = &plugin.details {
        let short = details.sha256.get(..12).unwrap_or(&details.sha256);
        lines.push(detail("SHA-256", format!("{short}…")));
        lines.push(detail(
            "Works with",
            format!("Muster {} or later", one_line(&details.muster)),
        ));
        lines.push(Line::from(""));
        lines.extend(runs_lines(details, theme));
    } else {
        lines.push(Line::from(""));
        lines.push(Line::from("Its manifest does not read, so what it runs is not known.").dim());
    }
    if let Some(change) = &plugin.changed {
        let words = change_words(app, change);
        let mut chars = words.chars();
        let words: String = chars
            .next()
            .map(|first| first.to_uppercase().chain(chars).chain(['.']).collect())
            .unwrap_or_default();
        lines.push(Line::from(""));
        lines.push(match change.by {
            ChangedBy::CommandLine => Line::from(words).fg(theme.working),
            ChangedBy::Ui => Line::from(words).dim(),
        });
    }
    Text::from(lines)
}

/// Where a plugin is from, in words.
pub(super) fn source_text(source: &PluginSource) -> String {
    match source {
        PluginSource::Builtin => "built into Muster".to_owned(),
        PluginSource::Git {
            url,
            git_ref,
            commit,
        } => {
            let at = git_ref
                .as_ref()
                .map_or_else(String::new, |git_ref| format!(" ({})", one_line(git_ref)));
            let commit = commit.get(..7).unwrap_or(commit);
            format!("git {} at {}{at}", one_line(url), one_line(commit))
        }
        PluginSource::Folder { path } => format!(
            "the folder {}, read where it is",
            one_line(&path.display().to_string())
        ),
    }
}

/// Everything a plugin runs, the programs it needs and whether each is
/// found, and the variables and secrets it is given: for its details and
/// its preview.
pub(super) fn runs_lines(details: &PluginDetails, theme: Theme) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from("It runs").bold()];
    for panel in &details.panels {
        let beside = match panel.on {
            PanelOn::Session => "beside a session",
            PanelOn::Workspace => "beside a workspace",
        };
        let key = panel
            .key
            .as_ref()
            .map_or_else(String::new, |key| format!(", key {}", one_line(key)));
        lines.push(Line::from(format!(
            "  panel {} ({beside}{key})",
            one_line(&panel.title)
        )));
        lines.push(Line::from(format!("    {}", command_line(&panel.command))).fg(theme.news));
    }
    if let Some(tickets) = &details.tickets {
        lines.push(Line::from(format!(
            "  ticket source {}",
            one_line(&tickets.name)
        )));
        lines.push(Line::from(format!("    {}", command_line(&tickets.command))).fg(theme.news));
    }
    for hook in &details.hooks {
        lines.push(Line::from(format!(
            "  hook {} on {}, at most {} s",
            one_line(&hook.id),
            hook.events(),
            hook.timeout
        )));
        if !hook.when.is_empty() {
            lines.push(Line::from(format!(
                "    only {}",
                one_line(&hook.when.words())
            )));
        }
        lines.push(Line::from(format!("    {}", command_line(&hook.command))).fg(theme.news));
    }
    if !details.requires.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from("It needs on PATH").bold());
        for required in &details.requires {
            lines.push(Line::from(vec![
                Span::raw(format!("  {}  ", one_line(&required.program))),
                if required.found {
                    Span::from("found").fg(theme.done)
                } else {
                    Span::from("not found").fg(theme.waiting)
                },
            ]));
        }
        if details.requires.iter().any(|required| !required.found)
            && let Some(how) = &details.how_to_get
        {
            lines.push(Line::from(format!("  To get them: {}", one_line(how))));
        }
    }
    if !details.env.is_empty() || !details.secrets.is_empty() {
        lines.push(Line::from(""));
    }
    if !details.env.is_empty() {
        lines.push(Line::from(format!(
            "It sees your variables {}.",
            one_line(&details.env.join(", "))
        )));
    }
    if !details.secrets.is_empty() {
        lines.push(Line::from(format!(
            "It is given the secrets {}.",
            one_line(&details.secrets.join(", "))
        )));
    }
    lines
}

/// The keys that do something on the section's selected row.
pub(super) fn hints(app: &App, row: SettingRow) -> Vec<String> {
    let mut hints = Vec::new();
    let hint = |does, what| super::bottom_bar::hint(app, does, what);
    match row {
        SettingRow::Plugin(key) => {
            if let Some(plugin) = app.plugin(key)
                && app.settings.plugin_changing.is_none()
            {
                hints.push(if plugin.runs() {
                    "space turn off".to_owned()
                } else {
                    "space turn on".to_owned()
                });
                if plugin.source != PluginSource::Builtin {
                    hints.extend(hint(Does::UpdatePlugin, "update"));
                    hints.extend(hint(Does::RemovePlugin, "remove"));
                }
            }
            hints.extend(hint(Does::AddPlugin, "add a plugin"));
        }
        SettingRow::AddPlugin => hints.push("⏎ add a plugin".to_owned()),
        _ => hints.extend(hint(Does::AddPlugin, "add a plugin")),
    }
    hints
}

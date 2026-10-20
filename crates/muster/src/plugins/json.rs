//! `muster plugin list --json`: every plugin, in the command line's own
//! shape (`crate::json`), built from the daemon's reply rather than copied
//! from the protocol, so the protocol can change without breaking a
//! script.

use muster_core::plugins::{PanelOn, PluginSource};
use muster_core::protocol::plugins::{
    ChangedBy, PluginAction, PluginDetails, PluginInfo, PluginList, PluginProblem,
};
use serde::Serialize;

use crate::json::FORMAT;

/// Prints `list` as one JSON document.
pub(crate) fn print(list: &PluginList) -> Result<(), serde_json::Error> {
    crate::json::print(&Document {
        format: FORMAT,
        plugins: list.plugins.iter().map(entry).collect(),
        problem: list.problem.clone(),
    })
}

#[derive(Debug, Serialize)]
struct Document {
    format: u32,
    plugins: Vec<PluginEntry>,
    /// Why Muster's own settings do not read, if they do not: then none of
    /// the person's plugins is known.
    problem: Option<String>,
}

#[derive(Debug, Serialize)]
struct PluginEntry {
    id: String,
    /// `builtin`, `git` or `folder`.
    source: &'static str,
    /// For `git`: the repository's address, the branch or tag named, and
    /// the commit installed.
    url: Option<String>,
    #[serde(rename = "ref")]
    git_ref: Option<String>,
    commit: Option<String>,
    /// For `folder`: where it is linked from.
    folder: Option<String>,
    /// Whether the person has it on.
    enabled: bool,
    /// Whether it is on and can run.
    runs: bool,
    /// Why it cannot run: `changed`, `not_approved`, `missing`,
    /// `unreadable` or `unknown`.
    problem: Option<&'static str>,
    /// The same, in words.
    problem_message: Option<String>,
    /// What its manifest says, where it reads.
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    /// The oldest Muster it works with.
    muster: Option<String>,
    panels: Vec<PanelEntry>,
    tickets: Option<TicketsEntry>,
    hooks: Vec<HookEntry>,
    requires: Vec<RequiredEntry>,
    how_to_get: Option<String>,
    env: Vec<String>,
    secrets: Vec<String>,
    manifest_sha256: Option<String>,
    /// The person's last change to it while the daemon runs.
    changed: Option<ChangeEntry>,
}

#[derive(Debug, Serialize)]
struct PanelEntry {
    id: String,
    title: String,
    /// `session` or `workspace`.
    on: &'static str,
    command: Vec<String>,
    key: Option<String>,
}

#[derive(Debug, Serialize)]
struct TicketsEntry {
    name: String,
    noun: String,
    command: Vec<String>,
    settings: Vec<SettingEntry>,
}

#[derive(Debug, Serialize)]
struct SettingEntry {
    key: String,
    label: String,
    required: bool,
}

#[derive(Debug, Serialize)]
struct HookEntry {
    id: String,
    /// The events it runs for, by name: `session.finished`.
    on: Vec<&'static str>,
    command: Vec<String>,
    /// Its deadline, in seconds.
    timeout: u64,
    /// Its filters, each empty or null where it has none.
    when: WhenEntry,
}

#[derive(Debug, Serialize)]
struct WhenEntry {
    project: Vec<String>,
    harness: Vec<String>,
    /// `question`, `permission`, `startup` or `action`.
    reason: Vec<&'static str>,
    away: Option<bool>,
}

#[derive(Debug, Serialize)]
struct RequiredEntry {
    program: String,
    found: bool,
}

#[derive(Debug, Serialize)]
struct ChangeEntry {
    /// `installed`, `linked`, `updated`, `turned_on` or `turned_off`.
    what: &'static str,
    /// `ui` or `command_line`.
    by: &'static str,
    at_ms: i64,
}

fn entry(plugin: &PluginInfo) -> PluginEntry {
    let (source, url, git_ref, commit, folder) = match &plugin.source {
        PluginSource::Builtin => ("builtin", None, None, None, None),
        PluginSource::Git {
            url,
            git_ref,
            commit,
        } => (
            "git",
            Some(url.clone()),
            git_ref.clone(),
            Some(commit.clone()),
            None,
        ),
        PluginSource::Folder { path } => {
            ("folder", None, None, None, Some(path.display().to_string()))
        }
    };
    let details = plugin.details.as_ref();
    PluginEntry {
        id: plugin.id.clone(),
        source,
        url,
        git_ref,
        commit,
        folder,
        enabled: plugin.enabled,
        runs: plugin.runs(),
        problem: plugin.problem.as_ref().map(|problem| match problem {
            PluginProblem::Changed => "changed",
            PluginProblem::NotApproved => "not_approved",
            PluginProblem::Missing { .. } => "missing",
            PluginProblem::Unreadable { .. } => "unreadable",
            PluginProblem::Unknown => "unknown",
        }),
        problem_message: plugin.problem.as_ref().map(ToString::to_string),
        name: details.map(|details| details.name.clone()),
        version: details.map(|details| details.version.clone()),
        description: details.and_then(|details| details.description.clone()),
        muster: details.map(|details| details.muster.clone()),
        panels: details.map(panels).unwrap_or_default(),
        tickets: details.and_then(tickets),
        hooks: details.map(hooks).unwrap_or_default(),
        requires: details.map(requires).unwrap_or_default(),
        how_to_get: details.and_then(|details| details.how_to_get.clone()),
        env: details
            .map(|details| details.env.clone())
            .unwrap_or_default(),
        secrets: details
            .map(|details| details.secrets.clone())
            .unwrap_or_default(),
        manifest_sha256: details.map(|details| details.sha256.clone()),
        changed: plugin.changed.map(|change| ChangeEntry {
            what: match change.what {
                PluginAction::Installed => "installed",
                PluginAction::Linked => "linked",
                PluginAction::Updated => "updated",
                PluginAction::TurnedOn => "turned_on",
                PluginAction::TurnedOff => "turned_off",
            },
            by: match change.by {
                ChangedBy::Ui => "ui",
                ChangedBy::CommandLine => "command_line",
            },
            at_ms: change.at.as_millis(),
        }),
    }
}

fn panels(details: &PluginDetails) -> Vec<PanelEntry> {
    details
        .panels
        .iter()
        .map(|panel| PanelEntry {
            id: panel.id.clone(),
            title: panel.title.clone(),
            on: match panel.on {
                PanelOn::Session => "session",
                PanelOn::Workspace => "workspace",
            },
            command: panel.command.clone(),
            key: panel.key.clone(),
        })
        .collect()
}

fn tickets(details: &PluginDetails) -> Option<TicketsEntry> {
    let tickets = details.tickets.as_ref()?;
    Some(TicketsEntry {
        name: tickets.name.clone(),
        noun: tickets.noun.clone(),
        command: tickets.command.clone(),
        settings: tickets
            .settings
            .iter()
            .map(|setting| SettingEntry {
                key: setting.key.clone(),
                label: setting.label.clone(),
                required: setting.required,
            })
            .collect(),
    })
}

fn hooks(details: &PluginDetails) -> Vec<HookEntry> {
    details
        .hooks
        .iter()
        .map(|hook| HookEntry {
            id: hook.id.clone(),
            on: hook.on.iter().map(|kind| kind.name()).collect(),
            command: hook.command.clone(),
            timeout: hook.timeout,
            when: WhenEntry {
                project: hook.when.project.clone(),
                harness: hook.when.harness.clone(),
                reason: hook
                    .when
                    .reason
                    .iter()
                    .map(|reason| muster_core::plugins::hooks::reason_name(*reason))
                    .collect(),
                away: hook.when.away,
            },
        })
        .collect()
}

fn requires(details: &PluginDetails) -> Vec<RequiredEntry> {
    details
        .requires
        .iter()
        .map(|required| RequiredEntry {
            program: required.program.clone(),
            found: required.found,
        })
        .collect()
}

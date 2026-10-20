//! Plugins as `muster plugin` prints them: a listing, and a preview of
//! everything a plugin would run. What comes from a plugin (its names,
//! words and commands) is shown without the control characters a terminal
//! would act on (`PRD-07`).

use std::fmt::Write as _;

use muster_core::plugins::{PanelOn, PluginSource};
use muster_core::protocol::plugins::{
    PluginDetails, PluginInfo, PluginList, PluginPreview, PreviewKind,
};
use muster_core::time::{Timestamp, between, span};

use crate::commands::shown;

/// Every plugin, a few lines each.
pub(crate) fn list(list: &PluginList) -> String {
    let mut out = String::new();
    if let Some(problem) = &list.problem {
        let _ = writeln!(
            out,
            "Muster's own settings do not read, so no plugin of yours runs: {}",
            shown(problem)
        );
    }
    if list.plugins.is_empty() {
        out.push_str("No plugins. `muster plugin install <address>` adds one.\n");
        return out;
    }
    for plugin in &list.plugins {
        out.push_str(&self::plugin(plugin, list.now));
    }
    out
}

/// One plugin: its line, then what it runs, why it cannot, and its last
/// change, read against the daemon's time `now`.
pub(crate) fn plugin(plugin: &PluginInfo, now: Timestamp) -> String {
    let state = if plugin.runs() {
        "on"
    } else if plugin.enabled {
        "on, not running"
    } else {
        "off"
    };
    let version = plugin.details.as_ref().map_or_else(String::new, |details| {
        format!(" {}", shown(&details.version))
    });
    let mut out = format!(
        "{}{version}: {state}, {}\n",
        shown(&plugin.id),
        source(&plugin.source)
    );
    if let Some(problem) = &plugin.problem {
        let _ = writeln!(out, "  {}", shown(&problem.to_string()));
    }
    if let Some(details) = &plugin.details {
        for line in runs(details) {
            let _ = writeln!(out, "  {line}");
        }
    }
    if let Some(change) = &plugin.changed {
        let _ = writeln!(
            out,
            "  {} {} ago {}",
            change.what.words(),
            span(between(change.at, now)),
            change.by.words()
        );
    }
    out
}

/// Where a plugin is from, in words.
fn source(source: &PluginSource) -> String {
    match source {
        PluginSource::Builtin => "Muster's own".to_owned(),
        PluginSource::Git {
            url,
            git_ref,
            commit,
        } => {
            let at = git_ref
                .as_ref()
                .map_or_else(String::new, |git_ref| format!(" ({})", shown(git_ref)));
            format!("from {} at {}{at}", shown(url), short(commit))
        }
        PluginSource::Folder { path } => {
            format!("linked from {}", shown(&path.display().to_string()))
        }
    }
}

/// A commit as people read it: its first seven digits.
fn short(commit: &str) -> &str {
    commit.get(..7).unwrap_or(commit)
}

/// What a plugin runs and needs, a line each.
fn runs(details: &PluginDetails) -> Vec<String> {
    let mut lines = Vec::new();
    for panel in &details.panels {
        let beside = match panel.on {
            PanelOn::Session => "beside a session",
            PanelOn::Workspace => "beside a workspace",
        };
        let key = panel
            .key
            .as_ref()
            .map_or_else(String::new, |key| format!(", key {}", shown(key)));
        lines.push(format!(
            "panel {} ({beside}{key}) runs: {}",
            shown(&panel.title),
            command(&panel.command)
        ));
    }
    if let Some(tickets) = &details.tickets {
        lines.push(format!(
            "ticket source {} runs: {}",
            shown(&tickets.name),
            command(&tickets.command)
        ));
        for setting in &tickets.settings {
            let required = if setting.required { ", required" } else { "" };
            lines.push(format!(
                "  a project's [tickets] holds {} ({}{required})",
                shown(&setting.key),
                shown(&setting.label)
            ));
        }
    }
    if !details.requires.is_empty() {
        let programs: Vec<String> = details
            .requires
            .iter()
            .map(|required| {
                let found = if required.found { "found" } else { "not found" };
                format!("{} ({found})", shown(&required.program))
            })
            .collect();
        lines.push(format!("needs on PATH: {}", programs.join(", ")));
        if details.requires.iter().any(|required| !required.found)
            && let Some(how) = &details.how_to_get
        {
            lines.push(format!("  to get them: {}", shown(how)));
        }
    }
    if !details.env.is_empty() {
        lines.push(format!(
            "sees your variables: {}",
            shown(&details.env.join(", "))
        ));
    }
    if !details.secrets.is_empty() {
        lines.push(format!(
            "is given the secrets: {}",
            shown(&details.secrets.join(", "))
        ));
    }
    lines
}

/// A command as it would be written in a shell: each argument quoted where
/// it has to be.
fn command(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            let argument = shown(argument);
            let plain = !argument.is_empty()
                && argument
                    .chars()
                    .all(|c| c.is_alphanumeric() || "-_./:=@%+,{}".contains(c));
            if plain {
                argument
            } else {
                format!("'{}'", argument.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A preview: what the plugin is, where it is from, and everything it
/// would run.
pub(crate) fn preview(preview: &PluginPreview) -> String {
    let details = &preview.details;
    let mut out = format!(
        "{} {} ({}), {}\n",
        shown(&details.name),
        shown(&details.version),
        shown(&preview.id),
        source(&preview.source)
    );
    if let Some(description) = &details.description {
        let _ = writeln!(out, "  {}", shown(description));
    }
    if let Some(current) = &preview.current {
        let was = current
            .details
            .as_ref()
            .map_or_else(String::new, |details| {
                format!("{} ", shown(&details.version))
            });
        let manifest = match &current.details {
            Some(now) if now.sha256 == details.sha256 => "the same manifest",
            _ => "a manifest that is not this one",
        };
        let _ = writeln!(
            out,
            "  now {was}{}, with {manifest}",
            source(&current.source)
        );
    }
    let _ = writeln!(
        out,
        "  works with Muster {} or later",
        shown(&details.muster)
    );
    for line in runs(details) {
        let _ = writeln!(out, "  {line}");
    }
    let _ = writeln!(out, "  its manifest's SHA-256: {}", details.sha256);
    if details.requires.iter().any(|required| !required.found) {
        out.push_str("  It needs programs that are not on PATH: it can be installed, but they are yours to get.\n");
    }
    out
}

/// What was done, once the person said yes.
pub(crate) fn approved(preview: &PluginPreview) -> String {
    let id = shown(&preview.id);
    match preview.kind {
        PreviewKind::Install => format!("Installed {id} and turned it on."),
        PreviewKind::Link if preview.current.is_some() => {
            format!("Approved {id} as its folder holds it now.")
        }
        PreviewKind::Link => format!("Linked {id} and turned it on."),
        PreviewKind::Update => format!("Updated {id} to {}.", shown(&preview.details.version)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_command_is_shown_as_a_shell_would_read_it() {
        let arguments =
            ["zoe", "{transcript}", "two words", "it's", "", "--x=1"].map(str::to_owned);
        assert_eq!(
            command(&arguments),
            "zoe {transcript} 'two words' 'it'\\''s' '' --x=1"
        );
        // A terminal's controls never reach it.
        assert_eq!(command(&["a\u{1b}]0;x\u{7}".to_owned()]), "'a]0;x'");
    }
}

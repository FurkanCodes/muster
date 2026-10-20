//! Plugins on the wire (`ARCHITECTURE.md` §7a): what a client asks of the
//! daemon's registry, and what it is told of each plugin. From protocol 41
//! ([`crate::plugins::PLUGINS_PROTOCOL`]).

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::plugins::{Manifest, Panel, PluginSource, TicketSource};
use crate::time::Timestamp;

/// What a client asks of the daemon's plugins.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginCommand {
    /// Every plugin, the registry read again as the person asked to see
    /// it; answered with [`super::Reply::Plugins`].
    List,
    /// What installing or linking `origin` would do: fetched and read,
    /// nothing of it run; answered with [`super::Reply::PluginPreview`],
    /// which the daemon holds for [`PluginCommand::Install`] for ten
    /// minutes.
    Preview { origin: PluginOrigin },
    /// What updating the plugin `id` would do: one from git fetched again
    /// at its ref, a linked folder read again as it is now; answered with
    /// [`super::Reply::PluginPreview`].
    Update { id: String },
    /// Installs, links or updates what the preview `preview` showed, as
    /// the person approved it: its manifest's hash is kept as approved,
    /// and it is turned on (an update keeps whether it was). Answered with
    /// [`super::Reply::Done`].
    Install { preview: String, by: ChangedBy },
    /// Turns the plugin `id` on or off. Turning on one that cannot run (its
    /// manifest changed since it was approved, or is gone) is refused,
    /// saying why. Answered with [`super::Reply::Done`].
    SetEnabled { id: String, on: bool, by: ChangedBy },
    /// Removes the plugin `id`: its entry in Muster's settings and its
    /// files, a linked folder left where it is. Muster's own are refused.
    /// The daemon asks nothing: the client asks the person first. Answered
    /// with [`super::Reply::Done`].
    Remove { id: String },
}

/// Where a plugin to install or link is.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum PluginOrigin {
    /// A git repository, cloned: its address (`https://`, `ssh://`,
    /// `user@host:path`, `file://`, or a repository's folder) and the
    /// branch or tag to fetch, else its default branch.
    Git {
        url: String,
        #[serde(default)]
        git_ref: Option<String>,
    },
    /// A folder linked where it is, for its author; its full path.
    Folder { path: PathBuf },
}

/// Who changed a plugin, as the client says.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangedBy {
    /// The Settings screen's Plugins section.
    Ui,
    /// `muster plugin`.
    CommandLine,
}

impl ChangedBy {
    /// As people read it: `from the command line`.
    pub const fn words(self) -> &'static str {
        match self {
            Self::Ui => "on the Settings screen",
            Self::CommandLine => "from the command line",
        }
    }
}

/// Every plugin, as [`PluginCommand::List`] is answered.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginList {
    pub plugins: Vec<PluginInfo>,
    /// The daemon's time, which each [`PluginChange::at`] is read against.
    pub now: Timestamp,
    /// Why Muster's own settings do not read, if they do not: then no
    /// plugin of the person's is known, and none runs.
    #[serde(default)]
    pub problem: Option<String>,
}

/// One plugin, as the daemon has it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub source: PluginSource,
    /// Whether the person has it on (`enabled` in its `[plugins.<id>]`).
    pub enabled: bool,
    /// Why it cannot run, whether on or off; `None` when it can.
    #[serde(default)]
    pub problem: Option<PluginProblem>,
    /// What its manifest says, where it reads.
    #[serde(default)]
    pub details: Option<PluginDetails>,
    /// The person's last change to it while this daemon runs.
    #[serde(default)]
    pub changed: Option<PluginChange>,
}

impl PluginInfo {
    /// Whether it is on and can run.
    pub fn runs(&self) -> bool {
        self.enabled && self.problem.is_none()
    }
}

/// Why a plugin cannot run (`ARCHITECTURE.md` §7a), said for the person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, thiserror::Error)]
pub enum PluginProblem {
    /// Its manifest is not the one the person approved.
    #[error(
        "its manifest changed since you approved it: update it, or link it again, to see what \
         it runs now and approve it"
    )]
    Changed,
    /// It was written in by hand, never approved.
    #[error("it was never approved: install or link it to see what it runs and approve it")]
    NotApproved,
    /// Its folder or its manifest is not there.
    #[error("{} is gone", path.display())]
    Missing { path: PathBuf },
    /// Its manifest does not read; `message` says why, naming the file.
    #[error("its manifest does not read: {message}")]
    Unreadable { message: String },
    /// Its entry names no repository or folder, and Muster has no plugin
    /// of its own by its id.
    #[error("it names no git repository or folder, and Muster has no plugin of its own by its id")]
    Unknown,
}

/// What a plugin's manifest says it is and runs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginDetails {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: Option<String>,
    /// The oldest Muster it works with.
    pub muster: String,
    /// The programs it needs, and whether each is on the daemon's `PATH`.
    pub requires: Vec<Required>,
    /// How to get them, in the plugin's words.
    #[serde(default)]
    pub how_to_get: Option<String>,
    /// The person's variables it may see, by name.
    pub env: Vec<String>,
    /// The secrets it is given, by name.
    pub secrets: Vec<String>,
    pub panels: Vec<Panel>,
    #[serde(default)]
    pub tickets: Option<TicketSource>,
    /// The SHA-256 of its manifest, which approving it binds.
    pub sha256: String,
}

impl PluginDetails {
    /// What `manifest`, whose bytes hash to `sha256`, says; `found` says
    /// whether a program is on `PATH`.
    pub fn of(manifest: &Manifest, sha256: String, found: impl Fn(&str) -> bool) -> Self {
        Self {
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            description: manifest.description.clone(),
            muster: manifest.muster.clone(),
            requires: manifest
                .requires
                .iter()
                .map(|program| Required {
                    program: program.clone(),
                    found: found(program),
                })
                .collect(),
            how_to_get: manifest.how_to_get.clone(),
            env: manifest.env.clone(),
            secrets: manifest.secrets.clone(),
            panels: manifest.panels.clone(),
            tickets: manifest.tickets.clone(),
            sha256,
        }
    }

    /// Every command it would run, each with where it is written.
    pub fn commands(&self) -> Vec<(String, &[String])> {
        let panels = self
            .panels
            .iter()
            .map(|panel| (format!("panel {}", panel.id), panel.command.as_slice()));
        let tickets = self
            .tickets
            .iter()
            .map(|tickets| ("ticket source".to_owned(), tickets.command.as_slice()));
        panels.chain(tickets).collect()
    }
}

/// A command as it would be written in a shell: each argument quoted where
/// it has to be, without the control characters a terminal would act on
/// (`PRD-07`). As `muster plugin` and the Settings screen show it.
pub fn command_line(arguments: &[String]) -> String {
    arguments
        .iter()
        .map(|argument| {
            let argument = crate::text::one_line(argument);
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

/// A program a plugin needs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Required {
    pub program: String,
    /// Whether it is on the daemon's `PATH`.
    pub found: bool,
}

/// What installing, linking or updating a plugin would do, held by the
/// daemon under `preview` until it is installed or let go.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginPreview {
    /// Its id, for [`PluginCommand::Install`].
    pub preview: String,
    pub kind: PreviewKind,
    /// The plugin's id, as its manifest says.
    pub id: String,
    /// Where it would be from: for git, the commit fetched.
    pub source: PluginSource,
    pub details: PluginDetails,
    /// The plugin as it is now, for an update or a folder linked again.
    #[serde(default)]
    pub current: Option<PluginInfo>,
}

/// What a preview would do once approved.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreviewKind {
    /// Install a plugin from git.
    Install,
    /// Link a folder where it is, or approve a linked one again.
    Link,
    /// Replace a plugin from git with a newer commit.
    Update,
}

/// The person's last change to a plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginChange {
    pub what: PluginAction,
    pub by: ChangedBy,
    pub at: Timestamp,
}

/// A change the person made to a plugin.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginAction {
    Installed,
    Linked,
    Updated,
    TurnedOn,
    TurnedOff,
}

impl PluginAction {
    /// As people read it: `turned on`.
    pub const fn words(self) -> &'static str {
        match self {
            Self::Installed => "installed",
            Self::Linked => "linked",
            Self::Updated => "updated",
            Self::TurnedOn => "turned on",
            Self::TurnedOff => "turned off",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn details_say_what_the_manifest_says_and_which_programs_are_found() {
        let manifest = crate::plugins::Manifest::parse(
            "id = \"p\"\nname = \"P\"\nversion = \"1.0\"\nmuster = \"0.3\"\n\
             requires = [\"zoe\", \"jq\"]\n\n[[panels]]\nid = \"graph\"\n\
             command = [\"zoe\", \"{branch}\"]\n\n[tickets]\nname = \"T\"\nnoun = \"task\"\n\
             command = [\"./t\"]\n",
        )
        .unwrap();
        let details = PluginDetails::of(&manifest, "ab".repeat(32), |program| program == "jq");
        assert_eq!(
            details.requires,
            [
                Required {
                    program: "zoe".to_owned(),
                    found: false
                },
                Required {
                    program: "jq".to_owned(),
                    found: true
                }
            ]
        );
        let commands: Vec<(String, Vec<String>)> = details
            .commands()
            .into_iter()
            .map(|(at, command)| (at, command.to_vec()))
            .collect();
        assert_eq!(
            commands,
            [
                (
                    "panel graph".to_owned(),
                    vec!["zoe".to_owned(), "{branch}".to_owned()]
                ),
                ("ticket source".to_owned(), vec!["./t".to_owned()]),
            ]
        );
    }

    #[test]
    fn a_command_is_shown_as_a_shell_would_read_it() {
        let arguments =
            ["zoe", "{transcript}", "two words", "it's", "", "--x=1"].map(str::to_owned);
        assert_eq!(
            command_line(&arguments),
            "zoe {transcript} 'two words' 'it'\\''s' '' --x=1"
        );
        // A terminal's controls never reach it.
        assert_eq!(command_line(&["a\u{1b}]0;x\u{7}".to_owned()]), "'a]0;x'");
    }

    #[test]
    fn only_a_client_of_41_or_later_is_told_of_the_plugins_arc_09() {
        assert_eq!(
            super::super::Delta::PluginsChanged(Vec::new()).since(),
            crate::plugins::PLUGINS_PROTOCOL
        );
        assert_eq!(crate::plugins::PLUGINS_PROTOCOL, 41);
    }

    #[test]
    fn a_plugin_runs_only_while_on_and_without_a_problem() {
        let mut info = PluginInfo {
            id: "p".to_owned(),
            source: PluginSource::Builtin,
            enabled: true,
            problem: None,
            details: None,
            changed: None,
        };
        assert!(info.runs());
        info.problem = Some(PluginProblem::Changed);
        assert!(!info.runs());
        assert!(
            PluginProblem::Changed
                .to_string()
                .starts_with("its manifest changed since you approved it")
        );
        info.problem = None;
        info.enabled = false;
        assert!(!info.runs());
    }
}

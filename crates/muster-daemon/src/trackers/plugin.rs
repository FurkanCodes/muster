//! Ticket plugins (`ARCHITECTURE.md` §10, *Ticket plugins*): programs the
//! daemon runs out of process and asks for a project's tickets by the
//! ticket protocol (`muster_core::ticket_protocol`), one process for each
//! plugin (`process`). A plugin is a `TicketSource` like any other, so
//! `Hidden`, the prompt's fence and every caller stay as they are, and
//! what it answers is checked and cleaned before anything sees it. Muster's
//! own GitHub, GitLab, Jira and Linear are plugins, built into the `muster`
//! program and run as `muster plugin-run <id>` (`crate::plugin_run`) with
//! the environment sessions get; anyone else's run from their folders,
//! given only what of the environment their manifests name.

pub(crate) mod process;
pub(crate) mod streams;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use muster_core::plugins::{TicketSource as Tickets, own};
use muster_core::ticket_protocol::{self, Call, PluginError, PluginErrorCode, Project};
use muster_core::{Issue, TicketKey};

use self::process::{Limits, Process};
use self::streams::Log;
use super::{BoxFuture, IssueDetail, Noun, ServiceError, TicketSource};
use crate::secrets::Token;

/// How to run a ticket plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginSpec {
    /// Its id: `github`.
    pub(crate) id: String,
    /// Its name, as messages name it and, until a listing says where its
    /// tickets are, the Tickets screen: `GitHub issues`.
    pub(crate) name: String,
    /// How an agent's prompt names one of its tickets: `GitHub issue`.
    pub(crate) noun: Noun,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    /// Where it runs: anyone's plugin in its folder; Muster's own where the
    /// daemon does.
    pub(crate) folder: Option<PathBuf>,
    pub(crate) environment: Environment,
}

/// What a plugin's process is given of the daemon's environment
/// (`ARC-23`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Environment {
    /// What sessions get (`harness::scrub`): Muster's own, whose `gh` and
    /// `glab` read the person's own settings.
    Sessions,
    /// The basics sessions get, then only the person's variables its
    /// manifest names and its secrets, so it gets no credential it did not
    /// name (`PRD-15`).
    Only {
        variables: Vec<(String, Token)>,
        secrets: Vec<(String, Token)>,
    },
}

/// Whether `name` is one of the variables every plugin gets, as sessions
/// do: where programs are, whose they are, and how text reads.
pub(crate) fn is_basic(name: &str) -> bool {
    matches!(
        name,
        "PATH" | "HOME" | "USER" | "LOGNAME" | "SHELL" | "TMPDIR" | "TZ" | "LANG"
    ) || name.starts_with("LC_")
}

/// The basics of `environment` as sessions get it: what `harness::scrub`
/// takes from a session's (a terminal's `LC_TERMINAL`) no plugin gets
/// either.
pub(crate) fn basics(
    environment: impl Iterator<Item = (std::ffi::OsString, std::ffi::OsString)>,
) -> Vec<(String, std::ffi::OsString)> {
    let basic: Vec<(String, std::ffi::OsString)> = environment
        .filter_map(|(key, value)| Some((key.into_string().ok()?, value)))
        .filter(|(key, _)| is_basic(key))
        .collect();
    let scrubbed = crate::harness::scrub::scrubbed(basic.iter().map(|(key, _)| key.clone()));
    basic
        .into_iter()
        .filter(|(key, _)| !scrubbed.contains(key))
        .collect()
}

impl PluginSpec {
    /// The plugin `plugin`, whose ticket source is `tickets`: Muster's own
    /// run as `program`, anyone else's from its folder with what of the
    /// environment it is given.
    pub(crate) fn of(
        plugin: &TicketPlugin,
        tickets: &Tickets,
        program: &Path,
        environment: Environment,
    ) -> Self {
        let (first, args) = tickets
            .command
            .split_first()
            .map_or(("", &[][..]), |(first, args)| (first.as_str(), args));
        let program = match (&plugin.folder, first.strip_prefix("./")) {
            (None, _) if first == own::PROGRAM => program.to_owned(),
            (Some(folder), Some(inside)) => folder.join(inside),
            _ => PathBuf::from(first),
        };
        Self {
            id: plugin.id.clone(),
            name: plugin.name.clone(),
            noun: Noun {
                tracker: tickets.name.clone(),
                word: tickets.noun.clone(),
            },
            program,
            args: args.to_vec(),
            folder: plugin.folder.clone(),
            environment: if plugin.folder.is_none() {
                Environment::Sessions
            } else {
                environment
            },
        }
    }

    /// Its secrets' values, which its log never shows.
    pub(crate) fn secrets(&self) -> Vec<Token> {
        match &self.environment {
            Environment::Sessions => Vec::new(),
            Environment::Only { secrets, .. } => {
                secrets.iter().map(|(_, value)| value.clone()).collect()
            }
        }
    }
}

/// A plugin as the registry last said, for finding a project's tickets:
/// what its ticket source runs, or why it cannot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct TicketPlugin {
    pub(crate) id: String,
    /// Its name on screen: `GitHub issues`.
    pub(crate) name: String,
    /// Where its programs are; `None` for Muster's own.
    pub(crate) folder: Option<PathBuf>,
    /// Why it cannot run, if it cannot: turned off, or why it does not
    /// read as approved.
    pub(crate) off: Option<String>,
    /// Its manifest's ticket source, if it reads and has one.
    pub(crate) tickets: Option<Tickets>,
    /// The person's variables its manifest names.
    pub(crate) env: Vec<String>,
    /// The secrets it names.
    pub(crate) secrets: Vec<String>,
}

impl TicketPlugin {
    /// Muster's own plugins, on: as they are until the registry is read.
    fn own() -> Vec<Self> {
        own::manifests()
            .iter()
            .map(|manifest| Self {
                id: manifest.id.clone(),
                name: manifest.name.clone(),
                folder: None,
                off: None,
                tickets: manifest.tickets.clone(),
                env: manifest.env.clone(),
                secrets: manifest.secrets.clone(),
            })
            .collect()
    }

    /// Its ticket source, if it can run one; else why not, for the
    /// Tickets screen.
    pub(crate) fn runnable(&self) -> Result<&Tickets, ServiceError> {
        if let Some(why) = &self.off {
            return Err(ServiceError::NotConfigured(format!(
                "its tickets come from the {} plugin, which {why}",
                self.name
            )));
        }
        self.tickets.as_ref().ok_or_else(|| {
            ServiceError::NotConfigured(format!(
                "[tickets] tracker names the {} plugin, which is no ticket source; choose \
                 where its tickets are on the Settings screen",
                self.name
            ))
        })
    }
}

/// Every ticket plugin's process, by the plugin's id: as many as there are
/// plugins (`ARC-14`), each kept for the next request; their logs; and the
/// plugins as the registry last said.
#[derive(Clone, Debug)]
pub(crate) struct Plugins {
    /// The `muster` program, which runs Muster's own.
    program: PathBuf,
    limits: Limits,
    held: Arc<Mutex<Held>>,
}

#[derive(Debug)]
struct Held {
    processes: HashMap<String, Arc<Process>>,
    /// Each plugin's log, across its processes; one leaves with its
    /// plugin.
    logs: HashMap<String, Arc<Mutex<Log>>>,
    known: Vec<TicketPlugin>,
}

impl Plugins {
    pub(crate) fn new(program: PathBuf) -> Self {
        Self {
            program,
            limits: Limits::default(),
            held: Arc::new(Mutex::new(Held {
                processes: HashMap::new(),
                logs: HashMap::new(),
                known: TicketPlugin::own(),
            })),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    fn held(&self) -> std::sync::MutexGuard<'_, Held> {
        self.held.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// The `muster` program, which Muster's own plugins' commands name.
    pub(crate) fn program(&self) -> &Path {
        &self.program
    }

    /// The plugin `id`, as the registry last said, or why no such plugin
    /// can give tickets.
    pub(crate) fn find(&self, id: &str) -> Result<TicketPlugin, ServiceError> {
        self.held()
            .known
            .iter()
            .find(|plugin| plugin.id == id)
            .cloned()
            .ok_or_else(|| {
                ServiceError::NotConfigured(format!(
                    "[tickets] tracker = \"{id}\" names a ticket plugin, and none called {id} \
                     is installed: add it on the Settings screen's Plugins section or with \
                     `muster plugin install`, or choose where its tickets are on the \
                     Settings screen"
                ))
            })
    }

    /// Takes `known` as the plugins, as the registry reads now: a plugin
    /// gone takes its log along, and the processes of those gone, turned
    /// off or moved are handed back, to be stopped (`ARC-14`).
    pub(crate) fn publish(&self, known: Vec<TicketPlugin>) -> Vec<Arc<Process>> {
        let mut held = self.held();
        held.logs
            .retain(|id, _| known.iter().any(|plugin| plugin.id == *id));
        let stopping: Vec<String> = held
            .processes
            .iter()
            .filter(|(id, process)| {
                !known.iter().any(|plugin| {
                    plugin.id == **id
                        && plugin.off.is_none()
                        && plugin.folder == process.spec().folder
                })
            })
            .map(|(id, _)| id.clone())
            .collect();
        held.known = known;
        stopping
            .iter()
            .filter_map(|id| held.processes.remove(id))
            .collect()
    }

    /// The process of the plugin `id`, if it has one, handed back to be
    /// stopped as the person changed the plugin: linked, updated or
    /// approved again, turned on or off, removed. The next request starts
    /// it afresh, as the registry says now; its log stays.
    pub(crate) fn changed(&self, id: &str) -> Option<Arc<Process>> {
        self.held().processes.remove(id)
    }

    /// The plugin `id`'s log, which its hooks' runs write to as well
    /// (`plugins::hooks`). One whose plugin is gone leaves as the registry
    /// is next published.
    pub(crate) fn log_of(&self, id: &str) -> Arc<Mutex<Log>> {
        self.held().logs.entry(id.to_owned()).or_default().clone()
    }

    /// The end of the plugin `id`'s log, its secrets hidden: empty where
    /// it has not run.
    pub(crate) fn log(&self, id: &str) -> String {
        self.held()
            .logs
            .get(id)
            .map(|log| log.lock().unwrap_or_else(PoisonError::into_inner).text())
            .unwrap_or_default()
    }

    /// The process of the plugin `spec` describes. One whose plugin is run
    /// another way now is left to stop as its last request ends.
    pub(crate) fn process(&self, spec: PluginSpec) -> Arc<Process> {
        let mut held = self.held();
        if let Some(process) = held
            .processes
            .get(&spec.id)
            .filter(|found| *found.spec() == spec)
        {
            return process.clone();
        }
        let log = held.logs.entry(spec.id.clone()).or_default().clone();
        let process = Arc::new(Process::new(spec.clone(), self.limits, log));
        held.processes.insert(spec.id, process.clone());
        process
    }

    /// Stops every plugin, waiting for each to end: as the daemon stops or
    /// restarts in place. A request after starts its plugin again.
    pub(crate) async fn stop(&self) {
        let stopping: Vec<_> = self
            .held()
            .processes
            .drain()
            .map(|(_, process)| process)
            .collect();
        for process in stopping {
            process.stop().await;
        }
    }
}

/// A plugin's error as Muster's: its code says which kind, its message is
/// the plugin's own.
pub(crate) fn refusal(spec: &PluginSpec, error: PluginError) -> ServiceError {
    let message = if error.message.trim().is_empty() {
        format!("the {} plugin failed and said nothing of why", spec.name)
    } else {
        error.message
    };
    match error.code {
        PluginErrorCode::NotFound => ServiceError::NotFound(message),
        PluginErrorCode::Refused => ServiceError::Refused(message),
        PluginErrorCode::SignedOut => ServiceError::NotSignedIn(message),
        PluginErrorCode::NotConfigured => ServiceError::NotConfigured(message),
        PluginErrorCode::Failed => ServiceError::Unavailable(message),
    }
}

/// One project's tickets, from a plugin.
#[derive(Debug)]
pub(crate) struct PluginTickets {
    process: Arc<Process>,
    project: Project,
    mine: bool,
    /// What the last listing said of where its tickets are: its `where`
    /// and `source`.
    listed: Mutex<(Option<String>, Option<String>)>,
}

impl PluginTickets {
    pub(crate) fn new(process: Arc<Process>, project: Project, mine: bool) -> Self {
        Self {
            process,
            project,
            mine,
            listed: Mutex::default(),
        }
    }

    fn listed(&self) -> std::sync::MutexGuard<'_, (Option<String>, Option<String>)> {
        self.listed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn unreadable(&self, why: &str) -> ServiceError {
        ServiceError::Unreadable(format!("the {} plugin: {why}", self.process.spec().name))
    }
}

impl TicketSource for PluginTickets {
    fn describe(&self) -> String {
        self.listed()
            .0
            .clone()
            .unwrap_or_else(|| self.process.spec().name.clone())
    }

    fn only_mine(&self) -> bool {
        self.mine
    }

    fn source(&self) -> Option<String> {
        self.listed().1.clone()
    }

    fn list(&self) -> BoxFuture<'_, Result<Vec<Issue>, ServiceError>> {
        Box::pin(async move {
            let call = Call::List {
                project: self.project.clone(),
                mine: self.mine,
            };
            let listed = ticket_protocol::listed(self.process.call(call).await?)
                .map_err(|why| self.unreadable(&why))?;
            if listed.dropped > 0 {
                tracing::warn!(
                    plugin = %self.process.spec().id,
                    dropped = listed.dropped,
                    "dropped tickets a plugin listed that Muster cannot take"
                );
            }
            *self.listed() = (listed.tracker, listed.source);
            Ok(listed.tickets)
        })
    }

    fn view<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<IssueDetail, ServiceError>> {
        Box::pin(async move {
            let call = Call::View {
                project: self.project.clone(),
                key: key.clone(),
            };
            let viewed = ticket_protocol::viewed(self.process.call(call).await?, key)
                .map_err(|why| self.unreadable(&why))?;
            Ok(IssueDetail {
                issue: viewed.issue,
                body: viewed.body,
                comments: viewed.comments,
                noun: self.process.spec().noun.clone(),
            })
        })
    }

    fn close<'a>(&'a self, key: &'a TicketKey) -> BoxFuture<'a, Result<(), ServiceError>> {
        Box::pin(async move {
            let call = Call::Close {
                project: self.project.clone(),
                key: key.clone(),
            };
            self.process.call(call).await?;
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests;

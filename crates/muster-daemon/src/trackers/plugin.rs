//! Ticket plugins (`ARCHITECTURE.md` §10, *Ticket plugins*): programs the
//! daemon runs out of process and asks for a project's tickets by the
//! ticket protocol (`muster_core::ticket_protocol`), one process for each
//! plugin (`process`). A plugin is a `TicketSource` like any other, so
//! `Hidden`, the prompt's fence and every caller stay as they are, and
//! what it answers is checked and cleaned before anything sees it. Muster's
//! own GitHub and GitLab are plugins, built into the `muster` program and
//! run as `muster plugin-run github|gitlab` (`crate::plugin_run`).

pub(crate) mod process;
pub(crate) mod streams;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use muster_core::config::TrackerKind;
use muster_core::ticket_protocol::{self, Call, PluginError, PluginErrorCode, Project};
use muster_core::{Issue, TicketKey};

use self::process::{Limits, Process};
use super::{BoxFuture, IssueDetail, ServiceError, TicketSource};

/// How to run a ticket plugin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PluginSpec {
    /// Its id: `github`.
    pub(crate) id: String,
    /// Where its tickets are, for the person, until a listing says:
    /// `GitHub issues`.
    pub(crate) name: String,
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    /// Which tracker its tickets are named for in an agent's prompt
    /// (`trackers::nouns`).
    pub(crate) tracker: TrackerKind,
}

impl PluginSpec {
    /// Muster's own plugin for `tracker`: `program plugin-run <key>`, where
    /// `program` is Muster itself.
    pub(crate) fn builtin(program: &Path, tracker: TrackerKind) -> Self {
        Self {
            id: tracker.key().to_owned(),
            name: tracker.name().to_owned(),
            program: program.to_owned(),
            args: vec!["plugin-run".to_owned(), tracker.key().to_owned()],
            tracker,
        }
    }
}

/// Every ticket plugin's process, by the plugin's id: as many as there are
/// plugins (`ARC-14`), each kept for the next request.
#[derive(Clone, Debug)]
pub(crate) struct Plugins {
    /// The `muster` program, which runs Muster's own.
    program: PathBuf,
    limits: Limits,
    processes: Arc<Mutex<HashMap<String, Arc<Process>>>>,
}

impl Plugins {
    pub(crate) fn new(program: PathBuf) -> Self {
        Self {
            program,
            limits: Limits::default(),
            processes: Arc::default(),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_limits(mut self, limits: Limits) -> Self {
        self.limits = limits;
        self
    }

    /// Muster's own plugin for `tracker`.
    pub(crate) fn builtin(&self, tracker: TrackerKind) -> Arc<Process> {
        self.process(PluginSpec::builtin(&self.program, tracker))
    }

    /// The process of the plugin `spec` describes. One whose plugin is run
    /// another way now is left to stop as its last request ends.
    pub(crate) fn process(&self, spec: PluginSpec) -> Arc<Process> {
        let mut processes = self
            .processes
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if let Some(process) = processes
            .get(&spec.id)
            .filter(|found| *found.spec() == spec)
        {
            return process.clone();
        }
        let process = Arc::new(Process::new(spec.clone(), self.limits));
        processes.insert(spec.id, process.clone());
        process
    }

    /// Stops every plugin, waiting for each to end: as the daemon stops or
    /// restarts in place. A request after starts its plugin again.
    pub(crate) async fn stop(&self) {
        let stopping: Vec<_> = self
            .processes
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
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
                tracker: self.process.spec().tracker,
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

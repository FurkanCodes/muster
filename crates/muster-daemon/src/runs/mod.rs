//! Carrying out runs' work (`ARCHITECTURE.md` §8).
//!
//! The Supervisor steps each run's machine and stores every transition;
//! the work a step asks for (start a container, an agent, the checks; push
//! and open a pull request; a review) is handed to a [`RunWork`], which
//! does it off the Supervisor's loop and reports back. Reports on work name
//! the generation it was asked under, so the machine ignores them once
//! overtaken. Every piece of work must be safe to ask for again: after a
//! restart the Supervisor asks again for whatever was under way.

pub(crate) mod credentials;
pub(crate) mod image;
pub(crate) mod job;
pub(crate) mod mirror;
pub(crate) mod prompts;
pub(crate) mod provision;
pub(crate) mod work;

use std::path::PathBuf;

use muster_core::config::ProjectConfig;
use muster_core::{ProjectId, PullRequest, RunId, SessionId, Ticket};
use muster_state::run::{RunEffect, RunEvent};
use tokio::sync::mpsc;

/// What work on a run needs to know.
#[derive(Clone, Debug)]
pub(crate) struct RunContext {
    pub(crate) run: RunId,
    pub(crate) project: ProjectId,
    /// The project's own checkout on the host.
    pub(crate) root: PathBuf,
    pub(crate) ticket: Ticket,
    pub(crate) branch: String,
    pub(crate) config: ProjectConfig,
    /// The run took over the person's branch: the mirror takes it from
    /// their repository, and the agent carries their work on.
    pub(crate) from_branch: bool,
    /// The building agent's harness session, once it has one.
    pub(crate) agent_session: Option<String>,
    /// The run's container, once started.
    pub(crate) container: Option<String>,
    pub(crate) pr: Option<PullRequest>,
    /// What the run has cost so far, for the work to add to.
    pub(crate) cost_micros: u64,
}

/// What work reports back.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Report {
    /// Something for the run's machine.
    Event(RunEvent),
    /// The run's container, once started.
    Container(String),
    /// The building agent's harness session, once it has one.
    AgentSession(String),
    /// The pull request, once found or opened.
    PullRequest(PullRequest),
}

/// Where work reports to.
pub(crate) type Reports = mpsc::UnboundedSender<(RunId, Report)>;

/// Does runs' work. Each call starts the work and returns at once; what
/// happens comes back through `reports`.
pub(crate) trait RunWork: Send + Sync + std::fmt::Debug {
    /// Carries out one effect other than a transition, which the Supervisor
    /// keeps itself.
    fn start(&self, context: RunContext, effect: RunEffect, reports: Reports);

    /// Looks at the run's pull request once: merged, closed, or changes
    /// asked for by the person become events.
    fn check_pull_request(&self, context: RunContext, reports: Reports);

    /// Stops a session's program in a run's container, where stopping
    /// docker's client on the host does not reach it.
    fn stop_session(&self, container: String, session: SessionId);
}

/// Work that cannot be done: every piece of it fails plainly. For tests
/// that need a Supervisor but no runs.
#[cfg(test)]
#[derive(Debug, Default)]
pub(crate) struct NotYetBuilt;

#[cfg(test)]
impl RunWork for NotYetBuilt {
    fn start(&self, context: RunContext, effect: RunEffect, reports: Reports) {
        let generation = match effect {
            RunEffect::StartContainer { generation }
            | RunEffect::StartBuilder { generation, .. }
            | RunEffect::RunChecks { generation }
            | RunEffect::PublishBranch { generation }
            | RunEffect::StartReviewer { generation } => generation,
            _ => return,
        };
        let _ = reports.send((
            context.run,
            Report::Event(RunEvent::InfraFailed {
                generation,
                reason: "this build of Muster cannot carry out runs yet".to_owned(),
                container_lost: false,
            }),
        ));
    }

    fn check_pull_request(&self, _context: RunContext, _reports: Reports) {}

    fn stop_session(&self, _container: String, _session: SessionId) {}
}

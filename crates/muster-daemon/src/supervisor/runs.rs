//! Runs: picking a ticket up, stepping its machine, storing each
//! transition before its work starts (`ARC-10`), handing the work to
//! [`RunWork`](crate::runs::RunWork) and taking its reports back, and
//! resuming runs after a restart (`PRD-09`).

use std::path::PathBuf;
use std::time::Instant;

use muster_core::config::ProjectConfig;
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::{ProjectId, Run, RunId, RunStage, RunTransition, Ticket};
use muster_state::run::{Limits, RunEffect, RunEvent, RunMachine};
use tokio::sync::oneshot;

use super::{Request, Supervisor, store_closed, store_error};
use crate::github::{self, IssueDetail};
use crate::runs::{Report, RunContext};
use crate::store::{RunProgress, RunUpdate, StoredRun};

/// The harness runs can drive headless, for now.
const HEADLESS: &str = "claude";

/// A run in progress: its machine and what its work needs.
#[derive(Debug)]
pub(super) struct ActiveRun {
    pub(super) machine: RunMachine,
    pub(super) context: RunContext,
}

/// A ticket fetched for a run, on its way back to the Supervisor.
#[derive(Debug)]
pub(super) struct FetchedRun {
    project: ProjectId,
    config: ProjectConfig,
    detail: IssueDetail,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

impl Supervisor {
    /// Reads the project's settings and fetches the issue off the loop,
    /// then continues in [`start_fetched_run`](Self::start_fetched_run).
    pub(super) fn fetch_run_ticket(
        &self,
        project: ProjectId,
        number: u64,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let Some(root) = self.project_root(project) else {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::NotFound,
                format!("no project {project}"),
            )));
            return;
        };
        if let Some(existing) = self.active_run_for(project, number) {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("issue #{number} already has a run going ({existing})"),
            )));
            return;
        }
        let overrides = self
            .config_dir
            .join("projects")
            .join(format!("{project}.toml"));
        let (requests, tickets) = (self.requests.clone(), self.tickets.clone());
        tokio::spawn(async move {
            let prepared = async {
                let config = settings_for_runs(root.clone(), overrides).await?;
                let detail = tickets
                    .view(&root, number)
                    .await
                    .map_err(|error| ErrorBody::new(ErrorCode::BadRequest, error.to_string()))?;
                Ok::<_, ErrorBody>((config, detail))
            };
            match prepared.await {
                Ok((config, detail)) => {
                    let fetched = FetchedRun {
                        project,
                        config,
                        detail,
                        reply,
                    };
                    let _ = requests
                        .send(Request::RunTicketFetched(Box::new(fetched)))
                        .await;
                }
                Err(error) => {
                    tracing::info!(command = "start run", %error, "command refused");
                    let _ = reply.send(Err(error));
                }
            }
        });
    }

    /// Picks the fetched ticket up: stores the run, then gives it room to
    /// run (`M3` adds a scheduler that waits for memory).
    pub(super) async fn start_fetched_run(&mut self, fetched: FetchedRun) {
        let FetchedRun {
            project,
            config,
            detail,
            reply,
        } = fetched;
        let number = detail.issue.number;
        if let Some(existing) = self.active_run_for(project, number) {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("issue #{number} already has a run going ({existing})"),
            )));
            return;
        }
        let Some(root) = self.project_root(project) else {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::NotFound,
                format!("no project {project}"),
            )));
            return;
        };
        let ticket = Ticket {
            number,
            title: detail.issue.title.clone(),
            url: detail.issue.url.clone(),
            body: detail.body.clone(),
            labels: detail.issue.labels.clone(),
            author: detail.issue.author.clone(),
        };
        let branch = self.unused_run_branch(project, &github::issue_branch(&detail.issue));
        let machine = RunMachine::new(Limits {
            attempts: config.limits.attempts,
            run_timeout: config.limits.run_timeout,
            max_cost_micros: config
                .limits
                .max_cost_usd
                .map(|usd| (usd * 1_000_000.0).round() as u64),
        });
        let run = Run {
            id: RunId::new(),
            project,
            ticket: ticket.clone(),
            branch: branch.clone(),
            stage: machine.stage(),
            attempt: 0,
            max_attempts: machine.limits().attempts,
            reason: None,
            pr: None,
            cost_micros: None,
            created_at: self.clock.now(),
            ended_at: None,
        };
        let id = run.id;
        let (stored, saved) = (run.clone(), machine.save(Instant::now()));
        let inserted = self
            .store
            .call(move |store| store.insert_run(&stored, &saved))
            .await
            .map_err(store_closed)
            .and_then(|result| result.map_err(store_error));
        if let Err(error) = inserted {
            let _ = reply.send(Err(error));
            return;
        }
        self.state.runs.push(run.clone());
        self.broadcast(Delta::RunUpserted(run));
        let context = RunContext {
            run: id,
            project,
            root,
            ticket,
            branch,
            config,
            agent_session: None,
            container: None,
            pr: None,
            cost_micros: 0,
        };
        self.active_runs.insert(id, ActiveRun { machine, context });
        tracing::info!(run = %id, issue = number, "picked up a run");
        let _ = reply.send(Ok(Reply::RunStarted { id }));
        self.step_run(id, RunEvent::SlotGranted).await;
    }

    /// The person's commands on a run.
    pub(super) async fn command_run(
        &mut self,
        run: RunId,
        event: RunEvent,
        what: &str,
    ) -> Result<Reply, ErrorBody> {
        let Some(active) = self.active_runs.get(&run) else {
            return Err(
                match self.state.runs.iter().find(|stored| stored.id == run) {
                    Some(ended) => ErrorBody::new(
                        ErrorCode::Conflict,
                        format!("the run on #{} has ended", ended.ticket.number),
                    ),
                    None => ErrorBody::new(ErrorCode::NotFound, format!("no run {run}")),
                },
            );
        };
        let stage = active.machine.stage();
        if !self.step_run(run, event).await {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("a run cannot {what} while it is {}", stage_name(stage)),
            ));
        }
        Ok(Reply::Done)
    }

    pub(super) async fn run_history(&self, run: RunId) -> Result<Reply, ErrorBody> {
        let transitions = self
            .store
            .call(move |store| store.run_history(run))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        if transitions.is_empty() && !self.state.runs.iter().any(|stored| stored.id == run) {
            return Err(ErrorBody::new(ErrorCode::NotFound, format!("no run {run}")));
        }
        Ok(Reply::RunHistory { transitions })
    }

    /// Feeds a run's machine; `false` if the event changed nothing.
    pub(super) async fn step_run(&mut self, run: RunId, event: RunEvent) -> bool {
        let Some(active) = self.active_runs.get_mut(&run) else {
            return false;
        };
        let cost = matches!(event, RunEvent::CostReported { .. });
        let effects = active.machine.step(event, Instant::now());
        let changed = !effects.is_empty();
        if cost && !changed {
            // A cost under the limit moves nothing, but is worth keeping.
            self.save_run_progress(run).await;
        }
        self.apply_run_effects(run, effects).await;
        changed
    }

    /// Stores a transition before anything else (`ARC-10`), then hands the
    /// work it brings to the run's work.
    async fn apply_run_effects(&mut self, run: RunId, effects: Vec<RunEffect>) {
        for effect in effects {
            match effect {
                RunEffect::Transition { from, to, reason } => {
                    self.record_run_transition(run, from, to, reason).await;
                }
                work => {
                    let Some(active) = self.active_runs.get(&run) else {
                        continue;
                    };
                    let mut context = active.context.clone();
                    context.cost_micros = active.machine.cost_micros();
                    self.work.start(context, work, self.run_reports.clone());
                }
            }
        }
        // An ended run's work, if any, has its own copy of what it needs.
        if self
            .active_runs
            .get(&run)
            .is_some_and(|active| active.machine.stage().is_over())
        {
            self.active_runs.remove(&run);
        }
    }

    async fn record_run_transition(
        &mut self,
        run: RunId,
        from: RunStage,
        to: RunStage,
        reason: String,
    ) {
        let Some(active) = self.active_runs.get(&run) else {
            return;
        };
        let at = self.clock.now();
        let ended_at = to.is_over().then_some(at);
        let machine = active.machine.save(Instant::now());
        let pr = active.context.pr.clone();
        let reason_now = active.machine.reason().map(str::to_owned);
        let transition = RunTransition {
            from,
            to,
            reason: reason.clone(),
            at,
        };
        let stored_reason = reason_now.clone();
        let stored_pr = pr.clone();
        self.write(move |store| {
            store.record_run_transition(
                run,
                &transition,
                &RunUpdate {
                    stage: to,
                    reason: stored_reason.as_deref(),
                    pr: stored_pr.as_ref(),
                    machine: &machine,
                    ended_at,
                },
            )
        })
        .await;
        tracing::info!(%run, ?from, ?to, %reason, "run moved");
        self.refresh_run(run, ended_at);
    }

    /// Brings the run as clients see it in line with its machine, and tells
    /// them.
    fn refresh_run(&mut self, run: RunId, ended_at: Option<muster_core::Timestamp>) {
        let Some(active) = self.active_runs.get(&run) else {
            return;
        };
        let Some(shown) = self.state.runs.iter_mut().find(|stored| stored.id == run) else {
            return;
        };
        shown.stage = active.machine.stage();
        shown.attempt = active.machine.attempt();
        shown.max_attempts = active.machine.limits().attempts;
        shown.reason = active.machine.reason().map(str::to_owned);
        shown.pr.clone_from(&active.context.pr);
        shown.cost_micros = Some(active.machine.cost_micros()).filter(|cost| *cost > 0);
        if ended_at.is_some() {
            shown.ended_at = ended_at;
        }
        let shown = shown.clone();
        self.broadcast(Delta::RunUpserted(shown));
    }

    /// Stores what changed in a run between transitions.
    async fn save_run_progress(&mut self, run: RunId) {
        let Some(active) = self.active_runs.get(&run) else {
            return;
        };
        let machine = active.machine.save(Instant::now());
        let context = active.context.clone();
        self.write(move |store| {
            store.save_run(
                run,
                &RunProgress {
                    machine: &machine,
                    pr: context.pr.as_ref(),
                    agent_session: context.agent_session.as_deref(),
                    container: context.container.as_deref(),
                },
            )
        })
        .await;
        self.refresh_run(run, None);
    }

    /// A report from a run's work.
    pub(super) async fn on_run_report(&mut self, run: RunId, report: Report) {
        let Some(active) = self.active_runs.get_mut(&run) else {
            return;
        };
        match report {
            Report::Event(event) => {
                self.step_run(run, event).await;
            }
            Report::Container(container) => {
                active.context.container = Some(container);
                self.save_run_progress(run).await;
            }
            Report::AgentSession(session) => {
                active.context.agent_session = Some(session);
                self.save_run_progress(run).await;
            }
            Report::PullRequest(pr) => {
                active.context.pr = Some(pr);
                self.save_run_progress(run).await;
            }
        }
    }

    /// Asks each run's work to look at its pull request.
    pub(super) fn check_pull_requests(&self) {
        for active in self.active_runs.values() {
            if active.context.pr.is_some() {
                self.work
                    .check_pull_request(active.context.clone(), self.run_reports.clone());
            }
        }
    }

    /// The earliest deadline among runs.
    pub(super) fn next_run_deadline(&self) -> Option<Instant> {
        self.active_runs
            .values()
            .filter_map(|active| active.machine.next_deadline())
            .min()
    }

    /// Steps the runs whose deadline has passed.
    pub(super) async fn tick_runs(&mut self, now: Instant) {
        let due: Vec<RunId> = self
            .active_runs
            .iter()
            .filter(|(_, active)| active.machine.next_deadline().is_some_and(|at| at <= now))
            .map(|(id, _)| *id)
            .collect();
        for run in due {
            self.step_run(run, RunEvent::Tick).await;
        }
    }

    /// Carries on the runs the last daemon left under way: their machines
    /// come back from the store, queued ones get room, the rest are asked
    /// again for the work of their stage (`PRD-09`).
    pub(super) async fn resume_runs(&mut self, stored: Vec<StoredRun>) {
        for StoredRun {
            run,
            machine,
            agent_session,
            container,
        } in stored
        {
            if run.stage.is_over() {
                continue;
            }
            let Some(root) = self.project_root(run.project) else {
                continue;
            };
            let overrides = self
                .config_dir
                .join("projects")
                .join(format!("{}.toml", run.project));
            let config = match settings_for_runs(root.clone(), overrides).await {
                Ok(config) => config,
                Err(error) => {
                    // Its work cannot go on as set up; say so when it is asked for.
                    tracing::warn!(run = %run.id, %error, "a resumed run's settings are not usable");
                    ProjectConfig::default()
                }
            };
            let machine = RunMachine::restore(machine, Instant::now());
            let context = RunContext {
                run: run.id,
                project: run.project,
                root,
                ticket: run.ticket.clone(),
                branch: run.branch.clone(),
                config,
                agent_session,
                container,
                pr: run.pr.clone(),
                cost_micros: machine.cost_micros(),
            };
            let work = machine.resume_work();
            let queued = machine.stage() == RunStage::Queued;
            tracing::info!(run = %run.id, stage = ?machine.stage(), "resumed a run");
            self.active_runs
                .insert(run.id, ActiveRun { machine, context });
            if queued {
                self.step_run(run.id, RunEvent::SlotGranted).await;
            } else {
                self.apply_run_effects(run.id, work).await;
            }
        }
    }

    /// The run going on `number` in `project`, if one is.
    fn active_run_for(&self, project: ProjectId, number: u64) -> Option<RunId> {
        self.active_runs
            .iter()
            .find(|(_, active)| {
                active.context.project == project && active.context.ticket.number == number
            })
            .map(|(id, _)| *id)
    }

    /// `branch`, or `branch-2`, `-3`… if an earlier run took it.
    fn unused_run_branch(&self, project: ProjectId, branch: &str) -> String {
        let taken = |name: &str| {
            self.state
                .runs
                .iter()
                .any(|run| run.project == project && run.branch == name)
        };
        if !taken(branch) {
            return branch.to_owned();
        }
        (2..)
            .map(|n| format!("{branch}-{n}"))
            .find(|candidate| !taken(candidate))
            .unwrap_or_else(|| branch.to_owned())
    }
}

/// A project's settings, checked as runs need them.
async fn settings_for_runs(root: PathBuf, overrides: PathBuf) -> Result<ProjectConfig, ErrorBody> {
    let loaded = tokio::task::spawn_blocking(move || crate::config::load(&root, &overrides))
        .await
        .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?
        .map_err(|error| ErrorBody::new(ErrorCode::BadRequest, error.to_string()))?;
    let config = loaded.config;
    config
        .ready_for_runs()
        .map_err(|reason| ErrorBody::new(ErrorCode::BadRequest, reason))?;
    for (role, harness) in [
        ("builder", &config.agents.builder),
        ("reviewer", &config.agents.reviewer),
    ] {
        if harness != HEADLESS {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("agents.{role} is {harness:?}; runs can use only {HEADLESS:?} for now"),
            ));
        }
    }
    Ok(config)
}

/// A stage as the person reads it.
fn stage_name(stage: RunStage) -> &'static str {
    match stage {
        RunStage::Queued => "queued",
        RunStage::Provisioning => "starting its container",
        RunStage::Building => "building",
        RunStage::Checking => "checking",
        RunStage::OpeningPr => "opening its pull request",
        RunStage::AgentReview => "in review",
        RunStage::ReadyForYou => "ready for you",
        RunStage::TakenOver => "taken over",
        RunStage::NeedsYou => "waiting on you",
        RunStage::Merged => "merged",
        RunStage::Cancelled => "cancelled",
    }
}

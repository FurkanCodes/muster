//! Runs: picking a ticket up, stepping its machine, storing each
//! transition before its work starts (`ARC-10`), handing the work to
//! [`RunWork`](crate::runs::RunWork) and taking its reports back, and
//! resuming runs after a restart (`PRD-09`).

use std::path::{Path, PathBuf};
use std::time::Instant;

use muster_core::config::ProjectConfig;
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::{ProjectId, Run, RunId, RunStage, RunTransition, Ticket, WorkspaceId};
use muster_state::run::{Limits, RunEffect, RunEvent, RunMachine};
use tokio::sync::{oneshot, watch};

use super::{Request, Supervisor, store_closed, store_error};
use crate::runs::{Report, RunContext};
use crate::store::{RunProgress, RunUpdate, StoredRun};
use crate::trackers::IssueDetail;

/// The harness runs can drive headless, for now.
const HEADLESS: &str = "claude";

/// A run in progress: its machine and what its work needs.
#[derive(Debug)]
pub(super) struct ActiveRun {
    pub(super) machine: RunMachine,
    pub(super) context: RunContext,
    /// Tells the run's work its generation as it moves on.
    pub(super) generation: watch::Sender<u64>,
}

impl ActiveRun {
    fn new(machine: RunMachine, mut context: RunContext) -> Self {
        let (generation, current) = watch::channel(machine.generation());
        context.current = current;
        Self {
            machine,
            context,
            generation,
        }
    }
}

/// A ticket fetched for a run, on its way back to the Supervisor.
#[derive(Debug)]
pub(super) struct FetchedRun {
    project: ProjectId,
    config: ProjectConfig,
    detail: IssueDetail,
    /// The person's branch the run takes over, if it was handed one.
    handed: Option<String>,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

/// A workspace whose work a run is to take over: its place and branch.
#[derive(Debug)]
struct HandOff {
    path: PathBuf,
    branch: String,
}

impl Supervisor {
    /// Reads the project's settings and fetches the issue off the loop,
    /// then continues in [`start_fetched_run`](Self::start_fetched_run).
    pub(super) fn fetch_run_ticket(
        &self,
        project: ProjectId,
        number: u64,
        from: Option<WorkspaceId>,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        let root = match self.repository_root(project, "runs") {
            Ok(root) => root,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        if let Some(existing) = self.active_run_for(project, number) {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("issue #{number} already has a run going ({existing})"),
            )));
            return;
        }
        let hand_off = match from.map(|workspace| self.hand_off(project, workspace)) {
            None => None,
            Some(Ok(hand_off)) => Some(hand_off),
            Some(Err(error)) => {
                tracing::info!(command = "start run", %error, "command refused");
                let _ = reply.send(Err(error));
                return;
            }
        };
        let place = match self.tracker_place(project) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        let overrides = place.overrides.clone();
        let (requests, trackers) = (self.requests.clone(), self.trackers.clone());
        self.tasks.spawn(async move {
            let prepared = async {
                let config = settings_for_runs(root.clone(), overrides).await?;
                if let Some(hand_off) = &hand_off {
                    ready_to_hand_off(hand_off, &config).await?;
                }
                let key = muster_core::TicketKey::number(number);
                let detail = async { trackers.for_project(place).await?.view(&key).await }
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
                        handed: hand_off.map(|hand_off| hand_off.branch),
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
    #[expect(
        clippy::too_many_lines,
        reason = "a run's start, step by step, each stored before the next (ARC-10)"
    )]
    pub(super) async fn start_fetched_run(&mut self, fetched: FetchedRun) {
        let FetchedRun {
            project,
            config,
            detail,
            handed,
            reply,
        } = fetched;
        // Runs, set aside, take GitHub's numbered issues only.
        let Some(number) = detail.issue.key.as_number() else {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("runs take numbered issues; {} is not one", detail.issue.key),
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
        if let Some(branch) = &handed
            && let Some(existing) = self.active_run_on(project, branch)
        {
            let _ = reply.send(Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{branch} is already a run's ({existing})"),
            )));
            return;
        }
        let limits = Limits {
            attempts: config.limits.attempts,
            run_timeout: config.limits.run_timeout,
            max_cost_micros: config
                .limits
                .max_cost_usd
                .map(|usd| muster_core::num::round_u64(usd * 1_000_000.0)),
        };
        let (branch, machine) = match handed {
            Some(branch) => (branch, RunMachine::handed_off(limits)),
            None => (
                self.unused_run_branch(
                    project,
                    &root,
                    &muster_core::tickets::ticket_branch(
                        &detail.issue.key,
                        &detail.issue.title,
                        true,
                    ),
                )
                .await,
                RunMachine::new(limits),
            ),
        };
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
        let (stored, saved) = (run.clone(), machine.save(self.clock.instant()));
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
            from_branch: machine.from_branch(),
            agent_session: None,
            container: None,
            pr: None,
            cost_micros: 0,
            current: watch::channel(0).1,
        };
        self.active_runs
            .insert(id, ActiveRun::new(machine, context));
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
        // Sent back to work, the run works with the settings as they are
        // now: the person may have mended them meanwhile, and work with
        // settings that cannot be read is no work at all.
        let rereads = matches!(
            event,
            RunEvent::Retry { .. } | RunEvent::HandBack | RunEvent::PersonRequestedChanges { .. }
        );
        if rereads {
            let (root, project) = (active.context.root.clone(), active.context.project);
            let config = settings_for_runs(root, self.overrides_of(project)).await?;
            if let Some(active) = self.active_runs.get_mut(&run) {
                active.context.config = config;
            }
        }
        if !self.step_run(run, event).await {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("a run cannot {what} while it is {}", stage.phrase()),
            ));
        }
        Ok(Reply::Done)
    }

    /// Where the person's overrides of `project`'s settings live.
    pub(super) fn overrides_of(&self, project: ProjectId) -> PathBuf {
        self.config_dir
            .join("projects")
            .join(format!("{project}.toml"))
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
        let effects = active.machine.step(event, self.clock.instant());
        // The work hears of the new generation before any is asked for
        // under it, and work under the old one stops.
        active.generation.send_replace(active.machine.generation());
        let changed = !effects.is_empty();
        if cost && !changed {
            // A cost under the limit moves nothing, but is worth keeping.
            self.save_run_progress(run).await;
        }
        self.apply_run_effects(run, effects).await;
        changed
    }

    /// Stores a transition before anything else (`ARC-10`), then hands the
    /// work it brings to the run's work. Leaving the person's hands stops
    /// the sessions in the run's container; the end of the run takes its
    /// container's workspace with it.
    async fn apply_run_effects(&mut self, run: RunId, effects: Vec<RunEffect>) {
        for effect in effects {
            match effect {
                RunEffect::Transition { from, to, reason } => {
                    self.record_run_transition(run, from, to, reason).await;
                    if to.is_over() {
                        self.drop_container_workspace(run).await;
                    } else if from == RunStage::TakenOver {
                        self.stop_container_sessions(run).await;
                    }
                }
                // Taking over starts the person's session itself, with the
                // credential it fetched first.
                RunEffect::StartInteractive => {}
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
        let machine = active.machine.save(self.clock.instant());
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
    pub(super) async fn save_run_progress(&mut self, run: RunId) {
        let Some(active) = self.active_runs.get(&run) else {
            return;
        };
        let machine = active.machine.save(self.clock.instant());
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
            if let Report::Container(container) = report {
                // Made by work the run's end overtook: it goes too.
                tracing::info!(%run, %container, "removing the container of a run that ended");
                self.work.remove_containers_of(vec![run]);
            }
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
            let overrides = self.overrides_of(run.project);
            let (config, unusable) = match settings_for_runs(root.clone(), overrides).await {
                Ok(config) => (config, None),
                Err(error) => {
                    tracing::warn!(run = %run.id, %error, "a resumed run's settings are not usable");
                    (ProjectConfig::default(), Some(error.message))
                }
            };
            let machine = RunMachine::restore(machine, self.clock.instant());
            let context = RunContext {
                run: run.id,
                project: run.project,
                root,
                ticket: run.ticket.clone(),
                branch: run.branch.clone(),
                config,
                from_branch: machine.from_branch(),
                agent_session,
                container,
                pr: run.pr.clone(),
                cost_micros: machine.cost_micros(),
                current: watch::channel(0).1,
            };
            let work = machine.resume_work();
            let (stage, generation) = (machine.stage(), machine.generation());
            tracing::info!(run = %run.id, ?stage, "resumed a run");
            self.active_runs
                .insert(run.id, ActiveRun::new(machine, context));
            if let Some(reason) = unusable {
                // Work with settings that cannot be read could pass with no
                // checks at all (`PRD-02`): it stops for the person instead,
                // and goes on once they mend them and send it back.
                if stage.is_active() {
                    let event = RunEvent::InfraFailed {
                        generation,
                        reason: format!("its settings cannot be read: {reason}"),
                        container_lost: false,
                    };
                    self.step_run(run.id, event).await;
                }
                continue;
            }
            if stage == RunStage::Queued {
                self.step_run(run.id, RunEvent::SlotGranted).await;
            } else {
                self.apply_run_effects(run.id, work).await;
            }
        }
    }

    /// Removes what is left of the containers of runs that have ended: the
    /// last daemon may have stopped before it removed one.
    pub(super) fn remove_ended_runs_containers(&self) {
        let ended: Vec<RunId> = self
            .state
            .runs
            .iter()
            .filter(|run| run.stage.is_over())
            .map(|run| run.id)
            .collect();
        self.work.remove_containers_of(ended);
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

    /// The run going on `branch` in `project`, if one is.
    fn active_run_on(&self, project: ProjectId, branch: &str) -> Option<RunId> {
        self.active_runs
            .iter()
            .find(|(_, active)| {
                active.context.project == project && active.context.branch == branch
            })
            .map(|(id, _)| *id)
    }

    /// The workspace whose work a run is to take over: one of the
    /// project's, on the host, on a branch no run has.
    fn hand_off(&self, project: ProjectId, workspace: WorkspaceId) -> Result<HandOff, ErrorBody> {
        let found = self.workspace(workspace)?;
        if found.project != project {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "that workspace belongs to another project",
            ));
        }
        let Some(path) = found.kind.host_path() else {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "that is a run's container; its work is the run's already",
            ));
        };
        let branch = found.kind.branch().to_owned();
        if branch.starts_with("detached at ") || branch == "?" {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!(
                    "{} has no branch checked out ({branch}); a run needs one",
                    path.display()
                ),
            ));
        }
        if let Some(existing) = self.active_run_on(project, &branch) {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{branch} is already a run's ({existing})"),
            ));
        }
        Ok(HandOff {
            path: path.to_owned(),
            branch,
        })
    }

    /// `branch`, or `branch-2`, `-3`… if an earlier run took it, or the
    /// person's repository has it already, on their machine or their
    /// remote as last fetched: a run never takes over a branch someone
    /// works on (and the pull request that may be open from it).
    async fn unused_run_branch(&self, project: ProjectId, root: &Path, branch: &str) -> String {
        for candidate in
            std::iter::once(branch.to_owned()).chain((2..100).map(|n| format!("{branch}-{n}")))
        {
            let run_has_it = self
                .state
                .runs
                .iter()
                .any(|run| run.project == project && run.branch == candidate);
            if !run_has_it && !repository_has(root, &candidate).await {
                return candidate;
            }
        }
        // A hundred taken: the run's id makes it its own.
        format!(
            "{branch}-{}",
            RunId::new().ulid().to_string().to_lowercase()
        )
    }
}

/// Whether the repository at `root` has `branch`, locally or on `origin`
/// as last fetched; a repository that cannot be asked counts as having it.
async fn repository_has(root: &Path, branch: &str) -> bool {
    for reference in [
        format!("refs/heads/{branch}"),
        format!("refs/remotes/origin/{branch}"),
    ] {
        match crate::git::answer(root, &["show-ref", "--verify", "--quiet", &reference]).await {
            Ok(Some(_)) | Err(_) => return true,
            Ok(None) => {}
        }
    }
    false
}

/// Whether the work in `hand_off` can go to a run: all committed (a run
/// takes the branch as committed, and would leave the rest out), and not
/// the branch runs start from.
async fn ready_to_hand_off(hand_off: &HandOff, config: &ProjectConfig) -> Result<(), ErrorBody> {
    if config.base_branch.as_deref() == Some(hand_off.branch.as_str()) {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!(
                "{} is the branch runs start from; hand over a branch of its own",
                hand_off.branch
            ),
        ));
    }
    let uncommitted = crate::git::uncommitted(&hand_off.path)
        .await
        .map_err(|error| ErrorBody::new(ErrorCode::BadRequest, error.to_string()))?;
    if !uncommitted.is_empty() {
        return Err(ErrorBody::new(
            ErrorCode::Conflict,
            format!(
                "{} has {} uncommitted change(s); commit them first, as the run takes the \
                 branch as committed",
                hand_off.path.display(),
                uncommitted.len()
            ),
        ));
    }
    Ok(())
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
        .map_err(|reason| ErrorBody::new(ErrorCode::BadRequest, reason.to_string()))?;
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

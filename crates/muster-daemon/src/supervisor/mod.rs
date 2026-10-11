//! The Supervisor: the one task that owns the daemon's state (`ARC-02`).
//!
//! Every change goes through it: it writes the change to the store first
//! (`ARC-10`), then updates its copy of the state, then broadcasts the delta.
//! Because it alone takes snapshots and hands out subscriptions, a subscriber
//! sees exactly the deltas after its snapshot, with no gap and no repeat.
//!
//! Terminal traffic does not pass through here: a connection asks once for a
//! session's [`PtyHandle`] and then talks to its actor directly.

mod by_hand;
mod containers;
mod issues;
mod projects;
mod runs;
mod sessions;
mod terminals;
mod worktrees;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use muster_core::protocol::{
    Command, Delta, ErrorBody, ErrorCode, Reply, ResourceReport, Snapshot, TermSize,
};
use muster_core::{RunId, SessionId, SessionStatus, WorkspaceId};
use muster_state::run::RunEvent;
use muster_state::session::{Effect, SessionMachine, Signal, Timing};
use tokio::sync::{broadcast, mpsc, oneshot, watch};

pub(crate) use self::terminals::{TerminalLink, WatchGuard};
use crate::clock::Clock;
use crate::githost::GitHost;
use crate::github::TicketSource;
use crate::harness::{Harness, claude};
use crate::hooks::HookDelivery;
use crate::pty::{PtyEvent, PtyHandle};
use crate::runs::credentials::Credentials;
use crate::runs::{Report, Reports, RunWork};
use crate::store::{StoreClosed, StoreError, StoreHandle};

type Subscription = (Snapshot, broadcast::Receiver<Delta>);

enum Request {
    Subscribe(oneshot::Sender<Subscription>),
    Command {
        command: Command,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
    /// A client wants a session's terminal; `watch` when it will show it.
    Terminal {
        session: SessionId,
        watch: bool,
        reply: oneshot::Sender<Option<TerminalLink>>,
    },
    /// An issue fetched off the loop, back to start work on it.
    IssueFetched(Box<issues::FetchedIssue>),
    /// A ticket fetched off the loop, back to start a run on it.
    RunTicketFetched(Box<runs::FetchedRun>),
    /// The runs' credential fetched off the loop, back to start an agent.
    CredentialFetched(Box<containers::CredentialFetched>),
    /// Time to look at runs' pull requests.
    CheckPullRequests,
    /// The sampler asks which sessions run under which process.
    RunningProcesses(oneshot::Sender<Vec<(SessionId, u32)>>),
    /// The branch watcher asks where each workspace is and what it had
    /// checked out.
    Workspaces(oneshot::Sender<Vec<(WorkspaceId, PathBuf, String)>>),
    /// The branch watcher's findings.
    Branches(Vec<(WorkspaceId, String)>),
    /// The worktree sync asks for the projects and their worktrees.
    ProjectsForSync(oneshot::Sender<Vec<crate::worktree_sync::ProjectWorktrees>>),
    /// Git's list of a project's worktrees.
    WorktreesListed {
        project: muster_core::ProjectId,
        listed: Vec<crate::worktree_sync::Listed>,
    },
    /// The sampler's measurement.
    Resources(ResourceReport),
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subscribe(_) => f.write_str("Subscribe"),
            Self::Command { command, .. } => write!(f, "Command({command:?})"),
            Self::Terminal { session, .. } => write!(f, "Terminal({session})"),
            Self::IssueFetched(_) => f.write_str("IssueFetched"),
            Self::RunTicketFetched(_) => f.write_str("RunTicketFetched"),
            Self::CredentialFetched(_) => f.write_str("CredentialFetched"),
            Self::CheckPullRequests => f.write_str("CheckPullRequests"),
            Self::RunningProcesses(_) => f.write_str("RunningProcesses"),
            Self::Workspaces(_) => f.write_str("Workspaces"),
            Self::Branches(_) => f.write_str("Branches"),
            Self::ProjectsForSync(_) => f.write_str("ProjectsForSync"),
            Self::WorktreesListed { .. } => f.write_str("WorktreesListed"),
            Self::Resources(_) => f.write_str("Resources"),
        }
    }
}

/// How connections reach the Supervisor.
#[derive(Clone, Debug)]
pub(crate) struct SupervisorHandle {
    requests: mpsc::Sender<Request>,
}

impl SupervisorHandle {
    /// The current state and every delta after it, or `None` once the
    /// Supervisor has stopped.
    pub(crate) async fn subscribe(&self) -> Option<Subscription> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(Request::Subscribe(reply)).await.ok()?;
        answer.await.ok()
    }

    /// Carries out a client's command.
    pub(crate) async fn command(&self, command: Command) -> Result<Reply, ErrorBody> {
        let stopped = || ErrorBody::new(ErrorCode::Internal, "the daemon is shutting down");
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Command { command, reply })
            .await
            .map_err(|_| stopped())?;
        answer.await.map_err(|_| stopped())?
    }

    /// The running sessions and their programs' process ids, or `None` once
    /// the Supervisor has stopped.
    pub(crate) async fn running_processes(&self) -> Option<Vec<(SessionId, u32)>> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::RunningProcesses(reply))
            .await
            .ok()?;
        answer.await.ok()
    }

    /// Every workspace's place and last known branch, or `None` once the
    /// Supervisor has stopped.
    pub(crate) async fn workspaces(&self) -> Option<Vec<(WorkspaceId, PathBuf, String)>> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(Request::Workspaces(reply)).await.ok()?;
        answer.await.ok()
    }

    /// Hands over branches that changed. `false` once the Supervisor has
    /// stopped.
    pub(crate) async fn record_branches(&self, changes: Vec<(WorkspaceId, String)>) -> bool {
        self.requests.send(Request::Branches(changes)).await.is_ok()
    }

    /// Each project's root and Muster's worktrees of it, or `None` once
    /// the Supervisor has stopped.
    pub(crate) async fn projects_for_sync(
        &self,
    ) -> Option<Vec<crate::worktree_sync::ProjectWorktrees>> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::ProjectsForSync(reply))
            .await
            .ok()?;
        answer.await.ok()
    }

    /// Hands over git's list of a project's worktrees. `false` once the
    /// Supervisor has stopped.
    pub(crate) async fn worktrees_listed(
        &self,
        project: muster_core::ProjectId,
        listed: Vec<crate::worktree_sync::Listed>,
    ) -> bool {
        self.requests
            .send(Request::WorktreesListed { project, listed })
            .await
            .is_ok()
    }

    /// Asks for runs' pull requests to be looked at. `false` once the
    /// Supervisor has stopped.
    pub(crate) async fn check_pull_requests(&self) -> bool {
        self.requests.send(Request::CheckPullRequests).await.is_ok()
    }

    /// Hands over a measurement. `false` once the Supervisor has stopped.
    pub(crate) async fn record_resources(&self, report: ResourceReport) -> bool {
        self.requests.send(Request::Resources(report)).await.is_ok()
    }

    /// A link to a session's terminal, if it still has one. With `watch`,
    /// the session counts as watched until the link is dropped.
    pub(crate) async fn terminal(&self, session: SessionId, watch: bool) -> Option<TerminalLink> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Terminal {
                session,
                watch,
                reply,
            })
            .await
            .ok()?;
        answer.await.ok().flatten()
    }
}

/// What the Supervisor needs to start.
#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) store: StoreHandle,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) worktrees_dir: PathBuf,
    /// Where sessions' hooks report.
    pub(crate) hook_socket: PathBuf,
    /// What the hook receiver accepted.
    pub(crate) hook_queue: mpsc::Receiver<HookDelivery>,
    pub(crate) delta_capacity: usize,
    /// Keep every this-many-th measurement in the store's history.
    pub(crate) history_every: u32,
    /// The session machines' time windows.
    pub(crate) timing: Timing,
    /// Does runs' work.
    pub(crate) work: Arc<dyn RunWork>,
    /// Where issues come from.
    pub(crate) tickets: Arc<dyn TicketSource>,
    /// The person's settings, such as projects' overrides.
    pub(crate) config_dir: PathBuf,
    /// The credential runs' agents use, also when the person drives one.
    pub(crate) credentials: Arc<dyn Credentials>,
    /// The `docker` command, for sessions in runs' containers.
    pub(crate) docker: String,
    /// Where pull requests opened by hand go.
    pub(crate) git_host: Arc<dyn GitHost>,
}

/// Loads the state from the store, resumes or settles sessions the last
/// daemon left running, and starts the Supervisor.
pub(crate) async fn spawn(settings: Settings) -> Result<SupervisorHandle, StoreError> {
    let (state, stored_runs) = settings
        .store
        .call(|store| Ok::<_, StoreError>((store.snapshot()?, store.runs()?)))
        .await
        .map_err(|StoreClosed| StoreError::Invalid("the store stopped".to_owned()))??;
    let (requests, queue) = mpsc::channel(256);
    let (pty_events, pty_queue) = mpsc::unbounded_channel();
    let (unwatched, unwatched_queue) = mpsc::unbounded_channel();
    let (run_reports, run_report_queue) = mpsc::unbounded_channel();
    let mut supervisor = Supervisor {
        requests: requests.clone(),
        store: settings.store,
        clock: settings.clock,
        worktrees_dir: settings.worktrees_dir,
        hook_socket: settings.hook_socket,
        deltas: broadcast::channel(settings.delta_capacity).0,
        state,
        live: HashMap::new(),
        terminals: HashMap::new(),
        watchers: HashMap::new(),
        unwatched,
        hook_tokens: HashMap::new(),
        pty_events,
        history_every: settings.history_every.max(1),
        measurements: 0,
        timing: settings.timing,
        active_runs: HashMap::new(),
        work: settings.work,
        tickets: settings.tickets,
        run_reports,
        config_dir: settings.config_dir,
        credentials: settings.credentials,
        docker: settings.docker,
        git_host: settings.git_host,
    };
    supervisor.ensure_checkouts().await;
    // Runs first: a session in a run's container needs its run.
    supervisor.resume_runs(stored_runs).await;
    supervisor.recover_sessions().await;
    supervisor.drop_stale_container_workspaces().await;
    supervisor.remove_ended_runs_containers();
    tokio::spawn(supervisor.run(
        queue,
        pty_queue,
        settings.hook_queue,
        unwatched_queue,
        run_report_queue,
    ));
    Ok(SupervisorHandle { requests })
}

/// A session whose terminal the daemon holds: running, or exited with its
/// output still readable.
#[derive(Debug)]
struct Live {
    pty: PtyHandle,
    machine: SessionMachine,
    harness: Harness,
    /// The token its hooks carry, while it runs.
    token: Option<String>,
    launched: std::time::Instant,
    /// Started with `--resume`; a quick exit means the resume failed.
    resumed: bool,
    /// Where its output ended and the size its terminal had, once it has
    /// exited; a program started again in the session carries on from there.
    ended: Option<(u64, TermSize)>,
}

#[derive(Debug)]
struct Supervisor {
    /// Its own queue, for work finished off the loop to come back.
    requests: mpsc::Sender<Request>,
    store: StoreHandle,
    clock: Arc<dyn Clock>,
    worktrees_dir: PathBuf,
    hook_socket: PathBuf,
    deltas: broadcast::Sender<Delta>,
    state: Snapshot,
    live: HashMap<SessionId, Live>,
    /// Each session's current terminal, followed by its clients across
    /// relaunches.
    terminals: HashMap<SessionId, watch::Sender<Option<PtyHandle>>>,
    /// How many clients watch each watched session.
    watchers: HashMap<SessionId, usize>,
    /// Where dropped watch guards report.
    unwatched: mpsc::UnboundedSender<SessionId>,
    /// Which session each issued hook token names (`ARC-11`).
    hook_tokens: HashMap<String, SessionId>,
    history_every: u32,
    /// Measurements received, to keep every `history_every`-th.
    measurements: u32,
    timing: Timing,
    /// Runs in progress, with their machines.
    active_runs: HashMap<RunId, runs::ActiveRun>,
    work: Arc<dyn RunWork>,
    tickets: Arc<dyn TicketSource>,
    /// Where runs' work reports.
    run_reports: Reports,
    config_dir: PathBuf,
    credentials: Arc<dyn Credentials>,
    docker: String,
    git_host: Arc<dyn GitHost>,
    /// Handed to every terminal actor so it can report events.
    pty_events: mpsc::UnboundedSender<(SessionId, PtyEvent)>,
}

impl Supervisor {
    async fn run(
        mut self,
        mut queue: mpsc::Receiver<Request>,
        mut pty_queue: mpsc::UnboundedReceiver<(SessionId, PtyEvent)>,
        mut hook_queue: mpsc::Receiver<HookDelivery>,
        mut unwatched: mpsc::UnboundedReceiver<SessionId>,
        mut run_reports: mpsc::UnboundedReceiver<(RunId, Report)>,
    ) {
        loop {
            let deadline = self.next_deadline();
            let wake_at = deadline.map_or_else(tokio::time::Instant::now, Into::into);
            tokio::select! {
                request = queue.recv() => match request {
                    Some(request) => self.on_request(request).await,
                    None => return,
                },
                Some((session, event)) = pty_queue.recv() => self.on_pty_event(session, event).await,
                Some(delivery) = hook_queue.recv() => self.on_hook(delivery).await,
                Some(session) = unwatched.recv() => self.unwatch(session).await,
                Some((run, report)) = run_reports.recv() => self.on_run_report(run, report).await,
                () = tokio::time::sleep_until(wake_at), if deadline.is_some() => self.tick().await,
            }
        }
    }

    async fn on_request(&mut self, request: Request) {
        match request {
            Request::Subscribe(reply) => {
                let _ = reply.send((self.state.clone(), self.deltas.subscribe()));
            }
            Request::Command { command, reply } => match command {
                // These talk to GitHub, so they answer from a task of their own.
                Command::ListIssues { project } => self.list_issues(project, reply),
                Command::ViewIssue { project, number } => self.view_issue(project, number, reply),
                Command::StartRun {
                    project,
                    number,
                    from,
                } => self.fetch_run_ticket(project, number, from, reply),
                Command::OpenPullRequest { workspace } => self.open_pull_request(workspace, reply),
                // These need the runs' credential, fetched off the loop.
                Command::TakeOverRun { run } => self.take_over_run(run, reply),
                Command::RestartSession { session, size }
                    if self.agent_restart(session).is_some() =>
                {
                    match self.agent_restart(session) {
                        Some(Ok(())) => self.fetch_credential(containers::Purpose::Restart {
                            session,
                            size,
                            reply,
                        }),
                        Some(Err(error)) => {
                            tracing::info!(command = "restart session", %error, "command refused");
                            let _ = reply.send(Err(error));
                        }
                        None => {}
                    }
                }
                Command::StartFromIssue {
                    project,
                    number,
                    harness,
                    size,
                    another,
                } => self.fetch_issue(project, number, harness, size, another, reply),
                command => {
                    let name = command_name(&command);
                    let outcome = self.handle(command).await;
                    if let Err(error) = &outcome {
                        // Logged so a refusal can be understood afterwards.
                        tracing::info!(command = name, %error, "command refused");
                    }
                    let _ = reply.send(outcome);
                }
            },
            Request::IssueFetched(fetched) => self.start_fetched_issue(*fetched).await,
            Request::RunTicketFetched(fetched) => self.start_fetched_run(*fetched).await,
            Request::CredentialFetched(fetched) => self.on_credential(*fetched).await,
            Request::CheckPullRequests => self.check_pull_requests(),
            Request::RunningProcesses(reply) => {
                let running = self
                    .live
                    .iter()
                    .filter(|(_, live)| !live.machine.status().is_exited())
                    .filter_map(|(id, live)| live.pty.pid().map(|pid| (*id, pid)))
                    .collect();
                let _ = reply.send(running);
            }
            Request::Resources(report) => self.record_resources(report).await,
            Request::Workspaces(reply) => {
                let _ = reply.send(self.workspace_heads());
            }
            Request::Branches(changes) => self.record_branches(changes).await,
            Request::ProjectsForSync(reply) => {
                let _ = reply.send(self.projects_for_sync());
            }
            Request::WorktreesListed { project, listed } => {
                self.sync_worktrees(project, listed).await;
            }
            Request::Terminal {
                session,
                watch,
                reply,
            } => {
                let link = self.link_terminal(session, watch).await;
                // A client that left already drops the link, which unwatches.
                let _ = reply.send(link);
            }
        }
    }

    async fn handle(&mut self, command: Command) -> Result<Reply, ErrorBody> {
        match command {
            Command::Ping => Ok(Reply::Pong),
            Command::AddProject { root, name } => self.add_project(root, name).await,
            Command::RemoveProject { project } => self.remove_project(project).await,
            Command::CreateWorktree { project, branch } => {
                self.create_worktree(project, branch).await
            }
            Command::RemoveWorktree { workspace, force } => {
                self.remove_worktree(workspace, force).await
            }
            Command::StartSession {
                workspace,
                harness,
                title,
                size,
                prompt,
            } => {
                self.start_session(workspace, harness, title, size, prompt)
                    .await
            }
            Command::StopSession { session } => self.stop_session(session).await,
            Command::CheckWorkspace { workspace, size } => {
                self.check_workspace(workspace, size).await
            }
            Command::ReviewWorkspace { workspace, size } => {
                self.review_workspace(workspace, size).await
            }
            Command::RestartSession { session, size } => self.restart_session(session, size).await,
            Command::RemoveSession { session } => self.remove_session(session).await,
            Command::CancelRun { run } => {
                self.command_run(run, RunEvent::Cancel, "be cancelled")
                    .await
            }
            Command::RetryRun { run, notes } => {
                self.command_run(run, RunEvent::Retry { notes }, "be sent back to work")
                    .await
            }
            Command::HandBackRun { run } => {
                self.command_run(run, RunEvent::HandBack, "be handed back")
                    .await
            }
            Command::RequestChanges { run, comments } => {
                if comments.trim().is_empty() {
                    return Err(ErrorBody::new(
                        ErrorCode::BadRequest,
                        "say what to change: the agent gets nothing else",
                    ));
                }
                let event = RunEvent::PersonRequestedChanges { comments };
                self.command_run(run, event, "take a request for changes")
                    .await
            }
            Command::RunHistory { run } => self.run_history(run).await,
            Command::ListIssues { .. }
            | Command::ViewIssue { .. }
            | Command::StartFromIssue { .. }
            | Command::StartRun { .. }
            | Command::TakeOverRun { .. }
            | Command::OpenPullRequest { .. } => Err(ErrorBody::new(
                ErrorCode::Internal,
                "this command is answered off the Supervisor's loop",
            )),
        }
    }

    /// Keeps the latest measurement, tells clients, and now and then keeps
    /// one in the store's history.
    async fn record_resources(&mut self, report: ResourceReport) {
        self.measurements = self.measurements.wrapping_add(1);
        if self.measurements.is_multiple_of(self.history_every) {
            let at = self.clock.now();
            let samples: Vec<_> = report
                .sessions
                .iter()
                .map(|usage| crate::store::ResourceSample {
                    session: usage.session,
                    at,
                    cpu_permille: usage.cpu_permille,
                    memory_bytes: usage.memory_bytes,
                })
                .collect();
            self.write(move |store| {
                samples
                    .iter()
                    .try_for_each(|sample| store.record_sample(sample))
            })
            .await;
        }
        self.state.resources = Some(report.clone());
        self.broadcast(Delta::Resources(report));
    }

    /// A hook arrived: the token says whose it is, never the payload.
    async fn on_hook(&mut self, delivery: HookDelivery) {
        let Some(&session) = self.hook_tokens.get(&delivery.token) else {
            tracing::debug!("ignored a hook with an unknown token");
            return;
        };
        if let Some((agent_session, event)) = claude::translate(&delivery.payload) {
            self.step_session(
                session,
                Signal::Hook {
                    agent_session,
                    event,
                },
            )
            .await;
        }
    }

    /// The earliest moment a session or run machine wants to be woken.
    fn next_deadline(&self) -> Option<std::time::Instant> {
        let sessions = self
            .live
            .values()
            .filter_map(|live| live.machine.next_deadline())
            .min();
        match (sessions, self.next_run_deadline()) {
            (Some(one), Some(other)) => Some(one.min(other)),
            (one, other) => one.or(other),
        }
    }

    async fn tick(&mut self) {
        let now = std::time::Instant::now();
        let due: Vec<SessionId> = self
            .live
            .iter()
            .filter(|(_, live)| live.machine.next_deadline().is_some_and(|at| at <= now))
            .map(|(id, _)| *id)
            .collect();
        for session in due {
            self.step_session(session, Signal::Tick).await;
        }
        self.tick_runs(now).await;
    }

    /// Feeds a session's machine and carries out what it decides.
    async fn step_session(&mut self, session: SessionId, signal: Signal) {
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        let effects = live.machine.step(signal, std::time::Instant::now());
        for effect in effects {
            self.apply(session, effect).await;
        }
    }

    async fn apply(&mut self, session: SessionId, effect: Effect) {
        match effect {
            Effect::Status { status, unseen } => self.set_status(session, status, unseen).await,
            Effect::SaveResumeId(resume_id) => {
                let id = resume_id.clone();
                self.write(move |store| store.save_resume_id(session, &id))
                    .await;
                if let Some(stored) = self.session_mut(session) {
                    stored.resume_id = Some(resume_id);
                }
            }
        }
    }

    async fn set_status(&mut self, session: SessionId, status: SessionStatus, unseen: bool) {
        let at = self.clock.now();
        self.write(move |store| store.record_status(session, status, unseen, at))
            .await;
        if let Some(stored) = self.session_mut(session) {
            stored.status = status;
            stored.unseen = unseen;
        }
        self.broadcast(Delta::SessionStatus {
            session,
            status,
            unseen,
        });
    }

    fn session_mut(&mut self, session: SessionId) -> Option<&mut muster_core::Session> {
        self.state
            .sessions
            .iter_mut()
            .find(|stored| stored.id == session)
    }

    /// Writes to the store, logging a failure. Used where the change has
    /// already happened in the world (a process exited) and must be shown
    /// even if it cannot be stored.
    async fn write(
        &self,
        work: impl FnOnce(&mut crate::store::Store) -> Result<(), StoreError> + Send + 'static,
    ) {
        match self.store.call(work).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::error!(%error, "could not store a change"),
            Err(StoreClosed) => tracing::error!("could not store a change: the store stopped"),
        }
    }

    fn broadcast(&self, delta: Delta) {
        // Nobody listening is fine; the change is already stored.
        let _ = self.deltas.send(delta);
    }
}

/// A command's name for the log, without its contents (a prompt can be long).
fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Ping => "ping",
        Command::AddProject { .. } => "add project",
        Command::RemoveProject { .. } => "remove project",
        Command::CreateWorktree { .. } => "create worktree",
        Command::RemoveWorktree { .. } => "remove worktree",
        Command::StartSession { .. } => "start session",
        Command::StopSession { .. } => "stop session",
        Command::CheckWorkspace { .. } => "check workspace",
        Command::ReviewWorkspace { .. } => "review workspace",
        Command::OpenPullRequest { .. } => "open pull request",
        Command::RestartSession { .. } => "restart session",
        Command::RemoveSession { .. } => "remove session",
        Command::ListIssues { .. } => "list issues",
        Command::ViewIssue { .. } => "view issue",
        Command::StartRun { .. } => "start run",
        Command::CancelRun { .. } => "cancel run",
        Command::RetryRun { .. } => "retry run",
        Command::TakeOverRun { .. } => "take over run",
        Command::HandBackRun { .. } => "hand back run",
        Command::RequestChanges { .. } => "request changes",
        Command::RunHistory { .. } => "run history",
        Command::StartFromIssue { .. } => "start from issue",
    }
}

fn store_closed(_: StoreClosed) -> ErrorBody {
    ErrorBody::new(ErrorCode::Internal, "the store is not available")
}

fn store_error(error: StoreError) -> ErrorBody {
    let code = match error {
        StoreError::Conflict(_) => ErrorCode::Conflict,
        StoreError::NotFound(_) => ErrorCode::NotFound,
        _ => ErrorCode::Internal,
    };
    ErrorBody::new(code, error.to_string())
}

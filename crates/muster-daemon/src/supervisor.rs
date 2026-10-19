//! The Supervisor: the one task that owns the daemon's state (`ARC-02`).
//!
//! Every change goes through it: it writes the change to the store first
//! (`ARC-10`), then updates its copy of the state, then broadcasts the delta.
//! Because it alone takes snapshots and hands out subscriptions, a subscriber
//! sees exactly the deltas after its snapshot, with no gap and no repeat.
//!
//! Terminal traffic does not pass through here: a connection asks once for a
//! session's [`PtyHandle`] and then talks to its actor directly. Only its
//! resizes come here, as the Supervisor decides which client's size a
//! terminal takes (`sizing`).

mod by_hand;
mod changes;
mod clashes;
mod clients;
mod containers;
mod explain;
mod handle;
mod issues;
mod looking;
mod names;
mod naming;
mod own_settings;
mod projects;
mod pulls;
mod recovery;
mod reload;
mod runs;
mod sessions;
mod settings;
mod shell_names;
mod sizing;
mod spend;
mod stacks;
mod status;
mod terminals;
mod tidy;
mod timeline;
mod worktrees;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use muster_core::protocol::{
    Command, Delta, ErrorBody, ErrorCode, Health, Reply, ResourceReport, Snapshot, TermSize,
};
use muster_core::{RunId, SessionId};
use muster_state::run::RunEvent;
use muster_state::session::{SessionMachine, Signal, Timing};
use tokio::sync::{broadcast, mpsc, watch};

pub(crate) use self::clients::ClientGuard;
use self::handle::Request;
pub(crate) use self::handle::SupervisorHandle;
pub(crate) use self::terminals::{KeysGuard, TerminalLink, WatchGuard};
use crate::clock::Clock;
use crate::githost::choose::Hosts;
use crate::harness::Harness;
use crate::hooks::HookDelivery;
use crate::pty::{PtyEvent, PtyHandle};
use crate::runs::credentials::Credentials;
use crate::runs::{Report, Reports, RunWork};
use crate::store::{StoreClosed, StoreError, StoreHandle};
use crate::tasks::Tasks;
use crate::trackers::Trackers;

/// What the Supervisor needs to start.
#[derive(Debug)]
pub(crate) struct Settings {
    pub(crate) store: StoreHandle,
    pub(crate) clock: Arc<dyn Clock>,
    pub(crate) worktrees_dir: PathBuf,
    /// Where Muster keeps its own data, such as workspaces' timelines.
    pub(crate) data_dir: PathBuf,
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
    /// Where each project's tickets come from.
    pub(crate) trackers: Arc<dyn Trackers>,
    /// The person's settings, such as projects' overrides.
    pub(crate) config_dir: PathBuf,
    /// The credential runs' agents use, also when the person drives one.
    pub(crate) credentials: Arc<dyn Credentials>,
    /// The `docker` command, for sessions in runs' containers.
    pub(crate) docker: String,
    /// Each project's git host, for pull requests opened by hand.
    pub(crate) hosts: Arc<dyn Hosts>,
    /// Sessions the daemon's previous version handed over, to take over
    /// before anything else.
    pub(crate) adopted: Vec<crate::handoff::HandedSession>,
    /// Where its work off the loop runs (`ARC-13`).
    pub(crate) tasks: Tasks,
}

/// Loads the state from the store, resumes or settles sessions the last
/// daemon left running, and starts the Supervisor.
pub(crate) async fn spawn(settings: Settings) -> Result<SupervisorHandle, StoreError> {
    let (mut state, stored_runs) = settings
        .store
        .call(|store| Ok::<_, StoreError>((store.snapshot()?, store.runs()?)))
        .await
        .map_err(|StoreClosed| StoreError::Invalid("the store stopped".to_owned()))??;
    let (preferences, agents) = own_settings::read(settings.config_dir.clone()).await;
    state.preferences = preferences;
    let (requests, queue) = mpsc::channel(256);
    // Unbounded on purpose (`ARC-14`): the Supervisor sends terminals and
    // runs' work requests and waits on some replies, so either waiting on
    // a full queue back to it could leave both waiting forever.
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: terminals' events; a title or progress only as it changes, an exit once"
    )]
    let (pty_events, pty_queue) = mpsc::unbounded_channel();
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: sent once as each watch ends, from Drop, which cannot wait"
    )]
    let (unwatched, unwatched_queue) = mpsc::unbounded_channel();
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: sent once as each client lets go of a session's keys, from Drop, which cannot wait"
    )]
    let (keys_gone, keys_gone_queue) = mpsc::unbounded_channel();
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: sent once as each connection ends, from Drop, which cannot wait"
    )]
    let (client_gone, client_gone_queue) = mpsc::unbounded_channel();
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-14: a few reports per step of a run, and only runs under way take steps"
    )]
    let (run_reports, run_report_queue) = mpsc::unbounded_channel();
    let mut supervisor = Supervisor {
        requests: requests.clone(),
        store: settings.store,
        clock: settings.clock,
        worktrees_dir: settings.worktrees_dir,
        data_dir: settings.data_dir,
        hook_socket: settings.hook_socket,
        deltas: broadcast::channel(settings.delta_capacity).0,
        state,
        live: HashMap::new(),
        terminals: HashMap::new(),
        watchers: HashMap::new(),
        unwatched,
        typing: HashMap::new(),
        keys_gone,
        colours: watch::channel(muster_core::colours::TerminalColours::default()).0,
        lookers: looking::Lookers::default(),
        sizing: sizing::Sizing::default(),
        next_client: 0,
        client_gone,
        hook_tokens: HashMap::new(),
        pty_events,
        history_every: settings.history_every.max(1),
        measurements: 0,
        timing: settings.timing,
        active_runs: HashMap::new(),
        work: settings.work,
        trackers: settings.trackers,
        run_reports,
        config_dir: settings.config_dir,
        credentials: settings.credentials,
        docker: settings.docker,
        hosts: settings.hosts,
        pulls: pulls::Looks::default(),
        changes: changes::Looks::default(),
        timelines: timeline::Timelines::default(),
        clashes: clashes::Clashes::default(),
        tidies: tidy::Tidies::default(),
        resumes: recovery::Resumes::default(),
        tasks: settings.tasks,
        agents,
        asks: Arc::new(tokio::sync::Semaphore::new(naming::AT_ONCE)),
    };
    supervisor.ensure_checkouts().await;
    // Running sessions handed over keep running: taken over before any is
    // taken to have died with the last daemon.
    supervisor.adopt_sessions(settings.adopted);
    // Runs first: a session in a run's container needs its run.
    supervisor.resume_runs(stored_runs).await;
    supervisor.recover_sessions().await;
    supervisor.drop_stale_container_workspaces().await;
    supervisor.remove_ended_runs_containers();
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-13: the Supervisor keeps its own queue open, so it runs as long as the daemon"
    )]
    tokio::spawn(supervisor.run(Queues {
        requests: queue,
        pty: pty_queue,
        hooks: settings.hook_queue,
        unwatched: unwatched_queue,
        keys_gone: keys_gone_queue,
        client_gone: client_gone_queue,
        run_reports: run_report_queue,
    }));
    Ok(SupervisorHandle { requests })
}

/// What the Supervisor listens to.
struct Queues {
    requests: mpsc::Receiver<Request>,
    pty: mpsc::UnboundedReceiver<(SessionId, PtyEvent)>,
    hooks: mpsc::Receiver<HookDelivery>,
    unwatched: mpsc::UnboundedReceiver<(SessionId, u64)>,
    keys_gone: mpsc::UnboundedReceiver<SessionId>,
    client_gone: mpsc::UnboundedReceiver<u64>,
    run_reports: mpsc::UnboundedReceiver<(RunId, Report)>,
}

/// A session whose terminal the daemon holds: running, or exited with its
/// output still readable.
#[derive(Debug)]
struct Live {
    pty: PtyHandle,
    machine: SessionMachine,
    harness: Harness,
    /// The token its hooks carry, while it runs.
    token: Option<crate::secrets::Token>,
    launched: std::time::Instant,
    /// Started with `--resume`; a quick exit means the resume failed.
    resumed: bool,
    /// Where its output ended and the size its terminal had, once it has
    /// exited; a program started again in the session carries on from there.
    ended: Option<(u64, TermSize)>,
    /// What its window title last said, for a harness whose title says how
    /// it is doing.
    title: Option<crate::harness::TitleState>,
    /// The turn under way, by the id the harness's records know it by
    /// (Claude Code's prompt id), once a prompt has named one.
    turn: Option<String>,
    /// The signals its machine last received, for explaining its status.
    signals: explain::Recent,
    /// Its chat being named by an agent.
    naming: Option<naming::Asking>,
    /// The person named it: no name of Muster's replaces theirs.
    named_by_person: bool,
}

#[derive(Debug)]
struct Supervisor {
    /// Its own queue, for work finished off the loop to come back.
    requests: mpsc::Sender<Request>,
    store: StoreHandle,
    clock: Arc<dyn Clock>,
    worktrees_dir: PathBuf,
    data_dir: PathBuf,
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
    unwatched: mpsc::UnboundedSender<(SessionId, u64)>,
    /// How many clients have the person's keys in each session's terminal.
    typing: HashMap<SessionId, usize>,
    /// Where dropped keys guards report.
    keys_gone: mpsc::UnboundedSender<SessionId>,
    /// The colours of the person's terminal, as a UI last told them; every
    /// terminal answers its program's questions about its colours from
    /// them.
    colours: watch::Sender<muster_core::colours::TerminalColours>,
    /// The clients that say whether the person is looking at them.
    lookers: looking::Lookers,
    /// The sizes clients gave sessions' terminals.
    sizing: sizing::Sizing,
    /// The key the last connection was given.
    next_client: u64,
    /// Where dropped client guards report.
    client_gone: mpsc::UnboundedSender<u64>,
    /// Which session each issued hook token names (`ARC-11`).
    hook_tokens: HashMap<crate::secrets::Token, SessionId>,
    history_every: u32,
    /// Measurements received, to keep every `history_every`-th.
    measurements: u32,
    timing: Timing,
    /// Runs in progress, with their machines.
    active_runs: HashMap<RunId, runs::ActiveRun>,
    work: Arc<dyn RunWork>,
    trackers: Arc<dyn Trackers>,
    /// Where runs' work reports.
    run_reports: Reports,
    config_dir: PathBuf,
    credentials: Arc<dyn Credentials>,
    docker: String,
    hosts: Arc<dyn Hosts>,
    /// Handed to every terminal actor so it can report events.
    pty_events: mpsc::UnboundedSender<(SessionId, PtyEvent)>,
    /// Looks at branches' pull requests.
    pulls: pulls::Looks,
    /// Looks at what workspaces' work has changed.
    changes: changes::Looks,
    /// Keeps workspaces' timelines.
    timelines: timeline::Timelines,
    /// Looks at where the work in projects' workspaces meets.
    clashes: clashes::Clashes,
    /// What each tidy of commits under way began from.
    tidies: tidy::Tidies,
    /// Sessions the last daemon left running, waiting their turn to resume.
    resumes: recovery::Resumes,
    /// Its work off the loop (`ARC-13`).
    tasks: Tasks,
    /// The coding agents the person added in Muster's own settings, as
    /// last read.
    agents: Vec<Arc<muster_core::config::agents::AgentDefinition>>,
    /// Turns to ask an agent one question, few at once (`ARC-14`).
    asks: Arc<tokio::sync::Semaphore>,
}

impl Supervisor {
    async fn run(mut self, mut queues: Queues) {
        loop {
            let deadline = self.next_deadline();
            let wake_at = deadline.map_or_else(tokio::time::Instant::now, Into::into);
            tokio::select! {
                request = queues.requests.recv() => match request {
                    Some(request) => {
                        self.take_dropped_guards(&mut queues).await;
                        self.on_request(request).await;
                    }
                    None => return,
                },
                Some((session, event)) = queues.pty.recv() => self.on_pty_event(session, event).await,
                Some(delivery) = queues.hooks.recv() => self.on_hook(delivery).await,
                Some((session, client)) = queues.unwatched.recv() => self.unwatch(session, client).await,
                Some(session) = queues.keys_gone.recv() => self.let_go_keys(session).await,
                Some(client) = queues.client_gone.recv() => self.remove_client(client).await,
                Some((run, report)) = queues.run_reports.recv() => self.on_run_report(run, report).await,
                () = tokio::time::sleep_until(wake_at), if deadline.is_some() => self.tick().await,
            }
        }
    }

    /// Takes the guards dropped before a request came: a client that stops
    /// watching a session and then resizes it dropped its watch first, so
    /// that is taken first, and a connection's steps keep their order.
    async fn take_dropped_guards(&mut self, queues: &mut Queues) {
        while let Ok((session, client)) = queues.unwatched.try_recv() {
            self.unwatch(session, client).await;
        }
        while let Ok(client) = queues.client_gone.try_recv() {
            self.remove_client(client).await;
        }
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one arm for each request the Supervisor serves"
    )]
    async fn on_request(&mut self, request: Request) {
        match request {
            Request::Subscribe(reply) => {
                let _ = reply.send((self.state.clone(), self.deltas.subscribe()));
            }
            Request::Command { command, reply } => match command {
                // These talk to trackers, so they answer from a task of their own.
                Command::ListIssues { project } => self.list_issues(project, reply),
                Command::ViewIssue { project, key } => self.view_issue(project, key, reply),
                Command::CloseIssue { project, key } => {
                    self.close_issue(project, key, reply);
                }
                Command::ProjectSettings { project } => self.project_settings(project, reply),
                // It asks an agent's program.
                Command::ListModels { agent } => self.list_models(&agent, reply),
                Command::ChangeSetting { project, change } => {
                    self.change_setting(project, &change, reply);
                }
                Command::StartRun {
                    project,
                    number,
                    from,
                } => self.fetch_run_ticket(project, number, from, reply),
                Command::OpenPullRequest { workspace } => self.open_pull_request(workspace, reply),
                // These ask git, which can take a while in a large repository.
                Command::WorkspaceChanges { workspace } => self.workspace_changes(workspace, reply),
                Command::FileDiff {
                    workspace,
                    part,
                    path,
                } => self.file_diff(workspace, part, path, reply),
                Command::Timeline { workspace } => self.workspace_timeline(workspace, reply),
                Command::EntryDiff { workspace, number } => {
                    self.entry_diff(workspace, number, reply);
                }
                Command::UndoPlan { workspace, number } => {
                    self.undo_plan(workspace, number, reply);
                }
                Command::Undo {
                    workspace,
                    number,
                    move_branch,
                } => self.undo(workspace, number, move_branch, reply),
                Command::MoveOnto { workspace } => self.move_onto(workspace, reply).await,
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
                    key,
                    harness,
                    size,
                    another,
                    on,
                } => self.fetch_issue(
                    issues::TicketStart {
                        project,
                        key,
                        harness,
                        size,
                        another,
                        on,
                    },
                    reply,
                ),
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
            Request::CheckPullRequests => {
                self.check_pull_requests();
                self.tick_pull_requests();
                self.tick_changes();
                self.tick_timelines();
                self.tick_clashes();
            }
            Request::PullRequestsLooked(looked) => self.pull_requests_looked(*looked),
            Request::LookAtBranch(workspace) => self.look_at_pull_requests(Some(workspace)),
            Request::ChangesLooked(looked) => self.changes_looked(*looked),
            Request::Named(named) => self.named(*named).await,
            Request::TimelineJobDone(done) => self.timeline_job_done(*done),
            Request::ClashesFound(found) => self.clashes_found(*found),
            Request::StackMoved(moved) => {
                let (workspace, stacked) = *moved;
                self.stack_moved(workspace, stacked).await;
            }
            Request::TidyChecked(checked) => {
                let (session, checked) = *checked;
                self.tidy_checked(session, checked);
            }
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
            Request::Shells(reply) => {
                let _ = reply.send(self.shells());
            }
            Request::ShellPrograms(programs) => self.shell_programs(programs).await,
            Request::SpendTargets(reply) => {
                let _ = reply.send(self.spend_targets());
            }
            Request::Spends(spends) => self.record_spends(spends),
            Request::Interrupted(turns) => self.record_interrupted(turns).await,
            Request::HandOver(reply) => {
                let handed = self.hand_over_sessions().await;
                let _ = reply.send(handed);
            }
            Request::Adopt(sessions, reply) => {
                self.adopt_sessions(sessions);
                let _ = reply.send(());
            }
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
            Request::AddClient(reply) => {
                // A client that left already drops the guard, which removes it.
                let _ = reply.send(self.add_client());
            }
            Request::AddLooker { client, outgoing } => self.add_looker(client, outgoing),
            Request::Looking { client, looking } => self.set_looking(client, looking).await,
            Request::Colours(colours) => self.set_colours(colours),
            Request::HoldKeys { session, reply } => {
                let _ = reply.send(self.hold_keys(session).await);
            }
            Request::Terminal {
                session,
                watch,
                client,
                reply,
            } => {
                let link = self.link_terminal(session, watch, client).await;
                // A client that left already drops the link, which unwatches.
                let _ = reply.send(link);
            }
            Request::Resize {
                session,
                client,
                size,
                pixels,
                done,
            } => {
                self.resize_for(session, client, size, pixels).await;
                let _ = done.send(());
            }
            Request::Active {
                session,
                client,
                done,
            } => {
                self.active_on(session, client).await;
                let _ = done.send(());
            }
            Request::Leave { session, client } => self.leave_terminal(session, client).await,
        }
    }

    async fn handle(&mut self, command: Command) -> Result<Reply, ErrorBody> {
        match command {
            Command::Ping => Ok(Reply::Pong),
            Command::Health => Ok(Reply::Health(self.health())),
            Command::ExplainSession { session } => self.explain_session(session),
            Command::Reveal { session } => self.reveal(session),
            Command::ChangePreference(change) => self.change_preference(&change),
            Command::ReadPreferences => Ok(self.read_preferences()),
            Command::AddProject { root, name } => self.add_project(root, name).await,
            Command::RemoveProject { project } => self.remove_project(project).await,
            Command::CreateWorktree {
                project,
                branch,
                on,
            } => self.create_worktree(project, branch, on).await,
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
            Command::TidyCommits {
                workspace,
                harness,
                size,
            } => self.tidy_commits(workspace, &harness, size).await,
            Command::RestartSession { session, size } => self.restart_session(session, size).await,
            Command::RemoveSession { session } => self.remove_session(session).await,
            Command::RenameSession { session, title } => self.rename_session(session, &title).await,
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
            | Command::CloseIssue { .. }
            | Command::ProjectSettings { .. }
            | Command::ListModels { .. }
            | Command::ChangeSetting { .. }
            | Command::StartFromIssue { .. }
            | Command::StartRun { .. }
            | Command::TakeOverRun { .. }
            | Command::OpenPullRequest { .. }
            | Command::WorkspaceChanges { .. }
            | Command::FileDiff { .. }
            | Command::Timeline { .. }
            | Command::EntryDiff { .. }
            | Command::UndoPlan { .. }
            | Command::Undo { .. }
            | Command::MoveOnto { .. } => Err(ErrorBody::new(
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

    /// The earliest moment a session or run machine wants to be woken, or
    /// the next session waiting to resume may start.
    fn next_deadline(&self) -> Option<std::time::Instant> {
        self.live
            .values()
            .filter_map(|live| live.machine.next_deadline())
            .chain(self.next_run_deadline())
            .chain(self.resumes.next_deadline())
            .min()
    }

    async fn tick(&mut self) {
        let now = self.clock.instant();
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
        self.resume_next().await;
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

    fn health(&self) -> Health {
        let counts = self.tasks.counts();
        let count = |n: usize| u64::try_from(n).unwrap_or(u64::MAX);
        Health {
            tasks: count(counts.all),
            work: count(counts.work),
            clients: count(counts.connections),
            terminals: count(self.live.len()),
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
        Command::Health => "health",
        Command::ExplainSession { .. } => "explain session",
        Command::WorkspaceChanges { .. } => "workspace changes",
        Command::FileDiff { .. } => "file diff",
        Command::Timeline { .. } => "timeline",
        Command::EntryDiff { .. } => "entry diff",
        Command::UndoPlan { .. } => "undo plan",
        Command::Undo { .. } => "undo",
        Command::RenameSession { .. } => "rename session",
        Command::MoveOnto { .. } => "move onto",
        Command::Reveal { .. } => "reveal",
        Command::ChangePreference(_) => "change preference",
        Command::ReadPreferences => "read preferences",
        Command::ListModels { .. } => "list models",
        Command::AddProject { .. } => "add project",
        Command::RemoveProject { .. } => "remove project",
        Command::CreateWorktree { .. } => "create worktree",
        Command::RemoveWorktree { .. } => "remove worktree",
        Command::StartSession { .. } => "start session",
        Command::StopSession { .. } => "stop session",
        Command::CheckWorkspace { .. } => "check workspace",
        Command::ReviewWorkspace { .. } => "review workspace",
        Command::TidyCommits { .. } => "tidy commits",
        Command::OpenPullRequest { .. } => "open pull request",
        Command::RestartSession { .. } => "restart session",
        Command::RemoveSession { .. } => "remove session",
        Command::ListIssues { .. } => "list issues",
        Command::ViewIssue { .. } => "view issue",
        Command::CloseIssue { .. } => "close issue",
        Command::ProjectSettings { .. } => "project settings",
        Command::ChangeSetting { .. } => "change setting",
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

#[expect(
    clippy::needless_pass_by_value,
    reason = "a converter for .map_err(f), which hands it the error by value"
)]
fn store_error(error: StoreError) -> ErrorBody {
    let code = match error {
        StoreError::Conflict(_) => ErrorCode::Conflict,
        StoreError::NotFound(_) => ErrorCode::NotFound,
        _ => ErrorCode::Internal,
    };
    ErrorBody::new(code, error.to_string())
}

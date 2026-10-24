//! How the rest of the daemon reaches the Supervisor: requests on its
//! queue, each answered over a channel of its own.

use std::path::PathBuf;

use muster_core::protocol::{
    Command, Delta, ErrorBody, ErrorCode, PixelSize, Reply, ResourceReport, ServerFrame, Snapshot,
    TermSize,
};
use muster_core::{SessionId, WorkspaceId};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{ClientGuard, KeysGuard, TerminalLink, changes, containers, issues, pulls, runs};

pub(super) type Subscription = (Snapshot, broadcast::Receiver<Delta>);

pub(super) enum Request {
    Subscribe(oneshot::Sender<Subscription>),
    Command {
        command: Command,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
    /// A new connection's key.
    AddClient(oneshot::Sender<ClientGuard>),
    /// A client wants a session's terminal; `watch` when it will show it.
    Terminal {
        session: SessionId,
        watch: bool,
        client: u64,
        reply: oneshot::Sender<Option<TerminalLink>>,
    },
    /// A client shows a session's terminal at `size`, and `pixels` where
    /// the person's terminal says.
    Resize {
        session: SessionId,
        client: u64,
        size: TermSize,
        pixels: Option<PixelSize>,
        done: oneshot::Sender<()>,
    },
    /// A client types into a session after another was active on it.
    Active {
        session: SessionId,
        client: u64,
        done: oneshot::Sender<()>,
    },
    /// A client let a session's terminal go.
    Leave {
        session: SessionId,
        client: u64,
    },
    /// A client will say whether the person is looking at it; `outgoing`
    /// takes the sessions to show in it.
    AddLooker {
        client: u64,
        outgoing: mpsc::Sender<ServerFrame>,
    },
    /// A client says whether the person is looking at it.
    Looking {
        client: u64,
        looking: bool,
    },
    /// Muster's helper says it can show notifications, or no longer can.
    Notifier {
        client: u64,
        ready: bool,
    },
    /// A UI says the colours of the person's terminal.
    Colours(muster_core::colours::TerminalColours),
    /// A client has the person's keys in a session's terminal, until it
    /// drops the guard.
    HoldKeys {
        session: SessionId,
        reply: oneshot::Sender<Option<KeysGuard>>,
    },
    /// An issue fetched off the loop, back to start work on it.
    IssueFetched(Box<issues::FetchedIssue>),
    /// A plugin fetched off the loop, back to be held as a preview.
    PluginFetched(Box<crate::plugins::Fetched>),
    /// A ticket fetched off the loop, back to start a run on it.
    RunTicketFetched(Box<runs::FetchedRun>),
    /// The runs' credential fetched off the loop, back to start an agent.
    CredentialFetched(Box<containers::CredentialFetched>),
    /// Time to look at pull requests: runs', and each branch's.
    CheckPullRequests,
    /// A look at branches' pull requests, back from off the loop.
    PullRequestsLooked(Box<pulls::Looked>),
    /// Look at the pull request of one workspace's branch now.
    LookAtBranch(WorkspaceId),
    /// A look at workspaces' changes, back from off the loop.
    ChangesLooked(Box<changes::Looked>),
    /// An agent's answer for a chat's name, back from off the loop.
    Named(Box<super::naming::Named>),
    /// A job on a workspace's timeline came back.
    TimelineJobDone(Box<super::timeline::JobDone>),
    /// A look at where the work in a project's workspaces meets came back.
    ClashesFound(Box<super::clashes::Found>),
    /// A branch moved: what it is stacked on now.
    StackMoved(Box<(WorkspaceId, Option<muster_core::stacks::Stacked>)>),
    /// A check of a tidy of commits came back.
    TidyChecked(Box<(SessionId, Option<muster_core::tidy::TidyCheck>)>),
    /// The sampler asks which sessions run under which process.
    RunningProcesses(oneshot::Sender<Vec<(SessionId, u32)>>),
    /// A watcher asks to follow whether the person looks at any client.
    LookedAt(oneshot::Sender<tokio::sync::watch::Receiver<bool>>),
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
    /// A worktree git made off the loop, or could not.
    WorktreeMade(Box<super::worktrees::Made>),
    /// A worktree git removed off the loop, or could not.
    WorktreeRemoved(Box<super::worktrees::Removed>),
    /// A project cloned off the loop, or not, to be added.
    Cloned(Box<super::cloning::Cloned>),
    /// The sampler's measurement.
    Resources(ResourceReport),
    /// The sampler asks for the running shells' terminals.
    Terminals(oneshot::Sender<super::fronts::Terminals>),
    /// What runs in front in terminals, where it changed (`ARC-28`).
    Fronts(Vec<(SessionId, muster_core::front::Front)>),
    /// The spend reader asks which sessions to read.
    SpendTargets(oneshot::Sender<Vec<crate::spend::Target>>),
    /// The totals the spend reader found changed.
    Spends(Vec<(SessionId, muster_core::Spend)>),
    /// Turns the spend reader found the person cut short.
    Interrupted(Vec<crate::spend::Interrupted>),
    /// Background tasks the spend reader found stopped.
    Stopped(Vec<crate::spend::Stopped>),
    /// Hand every running session over, for the daemon's restart in place.
    HandOver(oneshot::Sender<Vec<crate::handoff::HandedSession>>),
    /// Take sessions over that were handed over.
    Adopt(Vec<crate::handoff::HandedSession>, oneshot::Sender<()>),
    /// Start a plugin's panel, as `Command::OpenPanel` will.
    #[cfg(test)]
    StartPanel {
        workspace: WorkspaceId,
        beside: Option<SessionId>,
        panel: crate::harness::PanelLaunch,
        size: TermSize,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subscribe(_) => f.write_str("Subscribe"),
            Self::Command { command, .. } => write!(f, "Command({command:?})"),
            Self::AddClient(_) => f.write_str("AddClient"),
            Self::Terminal { session, .. } => write!(f, "Terminal({session})"),
            Self::Resize { session, size, .. } => write!(f, "Resize({session}, {size:?})"),
            Self::Active { session, .. } => write!(f, "Active({session})"),
            Self::Leave { session, .. } => write!(f, "Leave({session})"),
            Self::AddLooker { client, .. } => write!(f, "AddLooker({client})"),
            Self::Looking { client, looking } => write!(f, "Looking({client}, {looking})"),
            Self::Colours(_) => f.write_str("Colours"),
            Self::Notifier { client, ready } => write!(f, "Notifier({client}, {ready})"),
            Self::HoldKeys { session, .. } => write!(f, "HoldKeys({session})"),
            Self::IssueFetched(_) => f.write_str("IssueFetched"),
            Self::PluginFetched(_) => f.write_str("PluginFetched"),
            Self::RunTicketFetched(_) => f.write_str("RunTicketFetched"),
            Self::CredentialFetched(_) => f.write_str("CredentialFetched"),
            Self::CheckPullRequests => f.write_str("CheckPullRequests"),
            Self::PullRequestsLooked(_) => f.write_str("PullRequestsLooked"),
            Self::LookAtBranch(workspace) => write!(f, "LookAtBranch({workspace})"),
            Self::ChangesLooked(_) => f.write_str("ChangesLooked"),
            Self::Terminals(_) => f.write_str("Terminals"),
            Self::Fronts(_) => f.write_str("Fronts"),
            Self::Named(_) => f.write_str("Named"),
            Self::TimelineJobDone(_) => f.write_str("TimelineJobDone"),
            Self::ClashesFound(_) => f.write_str("ClashesFound"),
            Self::TidyChecked(_) => f.write_str("TidyChecked"),
            Self::StackMoved(_) => f.write_str("StackMoved"),
            Self::RunningProcesses(_) => f.write_str("RunningProcesses"),
            Self::LookedAt(_) => f.write_str("LookedAt"),
            Self::Workspaces(_) => f.write_str("Workspaces"),
            Self::Branches(_) => f.write_str("Branches"),
            Self::ProjectsForSync(_) => f.write_str("ProjectsForSync"),
            Self::WorktreesListed { .. } => f.write_str("WorktreesListed"),
            Self::WorktreeMade(_) => f.write_str("WorktreeMade"),
            Self::WorktreeRemoved(_) => f.write_str("WorktreeRemoved"),
            Self::Cloned(_) => f.write_str("Cloned"),
            Self::Resources(_) => f.write_str("Resources"),
            Self::SpendTargets(_) => f.write_str("SpendTargets"),
            Self::Spends(spends) => write!(f, "Spends({})", spends.len()),
            Self::Interrupted(turns) => write!(f, "Interrupted({})", turns.len()),
            Self::Stopped(stopped) => write!(f, "Stopped({})", stopped.len()),
            Self::HandOver(_) => f.write_str("HandOver"),
            Self::Adopt(sessions, _) => write!(f, "Adopt({})", sessions.len()),
            #[cfg(test)]
            Self::StartPanel { workspace, .. } => write!(f, "StartPanel({workspace})"),
        }
    }
}

/// How connections reach the Supervisor.
#[derive(Clone, Debug)]
pub(crate) struct SupervisorHandle {
    pub(super) requests: mpsc::Sender<Request>,
}

impl SupervisorHandle {
    /// The current state and every delta after it, or `None` once the
    /// Supervisor has stopped.
    pub(crate) async fn subscribe(&self) -> Option<Subscription> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(Request::Subscribe(reply)).await.ok()?;
        answer.await.ok()
    }

    /// Starts a plugin's panel in `workspace`, beside the session `beside`.
    #[cfg(test)]
    pub(crate) async fn start_panel(
        &self,
        workspace: WorkspaceId,
        beside: Option<SessionId>,
        panel: crate::harness::PanelLaunch,
    ) -> Result<Reply, ErrorBody> {
        let (reply, answer) = oneshot::channel();
        let request = Request::StartPanel {
            workspace,
            beside,
            panel,
            size: TermSize::default(),
            reply,
        };
        self.requests
            .send(request)
            .await
            .expect("the Supervisor runs");
        answer.await.expect("the Supervisor answers")
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

    /// Whether the person looks at any client, now and as it changes, or
    /// `None` once the Supervisor has stopped.
    pub(crate) async fn looked_at(&self) -> Option<tokio::sync::watch::Receiver<bool>> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(Request::LookedAt(reply)).await.ok()?;
        answer.await.ok()
    }

    /// The running terminals, and the agents the person added, or `None`
    /// once the Supervisor has stopped.
    pub(crate) async fn terminals(&self) -> Option<super::fronts::Terminals> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(Request::Terminals(reply)).await.ok()?;
        answer.await.ok()
    }

    /// Tells what runs in front in terminals, where it changed; `false`
    /// once the Supervisor has stopped.
    pub(crate) async fn fronts(&self, fronts: Vec<(SessionId, muster_core::front::Front)>) -> bool {
        self.requests.send(Request::Fronts(fronts)).await.is_ok()
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

    /// Asks for pull requests to be looked at: runs', and each branch's.
    /// `false` once the Supervisor has stopped.
    pub(crate) async fn check_pull_requests(&self) -> bool {
        self.requests.send(Request::CheckPullRequests).await.is_ok()
    }

    /// The sessions whose agents' records to read, or `None` once the
    /// Supervisor has stopped.
    pub(crate) async fn spend_targets(&self) -> Option<Vec<crate::spend::Target>> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::SpendTargets(reply))
            .await
            .ok()?;
        answer.await.ok()
    }

    /// Hands over totals that changed. `false` once the Supervisor has
    /// stopped.
    pub(crate) async fn record_spends(&self, spends: Vec<(SessionId, muster_core::Spend)>) -> bool {
        self.requests.send(Request::Spends(spends)).await.is_ok()
    }

    /// Hands over turns the person cut short. `false` once the Supervisor
    /// has stopped.
    pub(crate) async fn record_interrupted(&self, turns: Vec<crate::spend::Interrupted>) -> bool {
        self.requests
            .send(Request::Interrupted(turns))
            .await
            .is_ok()
    }

    /// Hands over background tasks that stopped. `false` once the
    /// Supervisor has stopped.
    pub(crate) async fn record_stopped(&self, stopped: Vec<crate::spend::Stopped>) -> bool {
        self.requests.send(Request::Stopped(stopped)).await.is_ok()
    }

    /// Hands over a measurement. `false` once the Supervisor has stopped.
    pub(crate) async fn record_resources(&self, report: ResourceReport) -> bool {
        self.requests.send(Request::Resources(report)).await.is_ok()
    }

    /// A key for a new connection, held until the guard is dropped;
    /// `None` once the Supervisor has stopped.
    pub(crate) async fn add_client(&self) -> Option<ClientGuard> {
        let (reply, answer) = oneshot::channel();
        self.requests.send(Request::AddClient(reply)).await.ok()?;
        answer.await.ok()
    }

    /// A client the person looks at, for tests, until the guard is dropped;
    /// its frames are kept on the receiver.
    #[cfg(test)]
    pub(crate) async fn looked_at_client(&self) -> (ClientGuard, mpsc::Receiver<ServerFrame>) {
        let client = self.add_client().await.expect("the Supervisor runs");
        let (outgoing, frames) = mpsc::channel(64);
        assert!(self.add_looker(&client, outgoing).await);
        assert!(self.looking(&client, true).await);
        (client, frames)
    }

    /// Counts a client that will say whether the person is looking at it,
    /// until its connection ends. `false` once the Supervisor has stopped.
    pub(crate) async fn add_looker(
        &self,
        client: &ClientGuard,
        outgoing: mpsc::Sender<ServerFrame>,
    ) -> bool {
        self.requests
            .send(Request::AddLooker {
                client: client.id(),
                outgoing,
            })
            .await
            .is_ok()
    }

    /// Tells whether the person is looking at the client. `false` once the
    /// Supervisor has stopped.
    /// Muster's helper on this connection can show notifications now, or
    /// no longer can.
    pub(crate) async fn notifier(&self, client: &ClientGuard, ready: bool) {
        // Nothing to do once the Supervisor has stopped.
        let _ = self
            .requests
            .send(Request::Notifier {
                client: client.id(),
                ready,
            })
            .await;
    }

    pub(crate) async fn looking(&self, client: &ClientGuard, looking: bool) -> bool {
        self.requests
            .send(Request::Looking {
                client: client.id(),
                looking,
            })
            .await
            .is_ok()
    }

    /// Tells the colours of the person's terminal, which programs in
    /// sessions asking for their terminal's are told from now on.
    pub(crate) async fn colours(&self, colours: muster_core::colours::TerminalColours) {
        // Nothing to do once the Supervisor has stopped.
        let _ = self.requests.send(Request::Colours(colours)).await;
    }

    /// Counts the client among those with the person's keys in `session`'s
    /// terminal until the guard is dropped; `None` for a session that is
    /// not known, or once the Supervisor has stopped.
    pub(crate) async fn hold_keys(&self, session: SessionId) -> Option<KeysGuard> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::HoldKeys { session, reply })
            .await
            .ok()?;
        answer.await.ok().flatten()
    }

    /// A link to a session's terminal for `client`, if it still has one.
    /// With `watch`, the session counts as watched until the link is
    /// dropped.
    pub(crate) async fn terminal(
        &self,
        session: SessionId,
        watch: bool,
        client: &ClientGuard,
    ) -> Option<TerminalLink> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Terminal {
                session,
                watch,
                client: client.id(),
                reply,
            })
            .await
            .ok()?;
        answer.await.ok().flatten()
    }

    /// The client shows the session at `size`, and `pixels` where it
    /// knows them; the session's terminal takes them if the client is the
    /// one in charge of it (`sizing`). Returns once any resize is on its
    /// way to the terminal, so keys sent after it reach the program after
    /// it.
    pub(crate) async fn resize(
        &self,
        session: SessionId,
        client: &ClientGuard,
        size: TermSize,
        pixels: Option<PixelSize>,
    ) {
        let (done, answer) = oneshot::channel();
        let request = Request::Resize {
            session,
            client: client.id(),
            size,
            pixels,
            done,
        };
        if self.requests.send(request).await.is_ok() {
            let _ = answer.await;
        }
    }

    /// The client types into the session after another was active on it.
    /// Returns once any resize that follows is on its way to the terminal.
    pub(crate) async fn active(&self, session: SessionId, client: &ClientGuard) {
        let (done, answer) = oneshot::channel();
        let request = Request::Active {
            session,
            client: client.id(),
            done,
        };
        if self.requests.send(request).await.is_ok() {
            let _ = answer.await;
        }
    }

    /// The client let the session's terminal go.
    pub(crate) async fn leave(&self, session: SessionId, client: &ClientGuard) {
        let client = client.id();
        let _ = self.requests.send(Request::Leave { session, client }).await;
    }
}

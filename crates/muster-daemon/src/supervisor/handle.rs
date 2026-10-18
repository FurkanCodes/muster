//! How the rest of the daemon reaches the Supervisor: requests on its
//! queue, each answered over a channel of its own.

use std::path::PathBuf;

use muster_core::protocol::{
    Command, Delta, ErrorBody, ErrorCode, Reply, ResourceReport, ServerFrame, Snapshot,
};
use muster_core::{SessionId, WorkspaceId};
use tokio::sync::{broadcast, mpsc, oneshot};

use super::{LookGuard, TerminalLink, changes, containers, issues, pulls, runs};

pub(super) type Subscription = (Snapshot, broadcast::Receiver<Delta>);

pub(super) enum Request {
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
    /// A client will say whether the person is looking at it; `outgoing`
    /// takes the sessions to show in it.
    AddLooker {
        outgoing: mpsc::Sender<ServerFrame>,
        reply: oneshot::Sender<LookGuard>,
    },
    /// A client says whether the person is looking at it.
    Looking {
        client: u64,
        looking: bool,
    },
    /// An issue fetched off the loop, back to start work on it.
    IssueFetched(Box<issues::FetchedIssue>),
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
    /// A job on a workspace's timeline came back.
    TimelineJobDone(Box<super::timeline::JobDone>),
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
    /// The spend reader asks which sessions to read.
    SpendTargets(oneshot::Sender<Vec<crate::spend::Target>>),
    /// The totals the spend reader found changed.
    Spends(Vec<(SessionId, muster_core::Spend)>),
    /// Hand every running session over, for the daemon's restart in place.
    HandOver(oneshot::Sender<Vec<crate::handoff::HandedSession>>),
    /// Take sessions over that were handed over.
    Adopt(Vec<crate::handoff::HandedSession>, oneshot::Sender<()>),
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subscribe(_) => f.write_str("Subscribe"),
            Self::Command { command, .. } => write!(f, "Command({command:?})"),
            Self::Terminal { session, .. } => write!(f, "Terminal({session})"),
            Self::AddLooker { .. } => f.write_str("AddLooker"),
            Self::Looking { client, looking } => write!(f, "Looking({client}, {looking})"),
            Self::IssueFetched(_) => f.write_str("IssueFetched"),
            Self::RunTicketFetched(_) => f.write_str("RunTicketFetched"),
            Self::CredentialFetched(_) => f.write_str("CredentialFetched"),
            Self::CheckPullRequests => f.write_str("CheckPullRequests"),
            Self::PullRequestsLooked(_) => f.write_str("PullRequestsLooked"),
            Self::LookAtBranch(workspace) => write!(f, "LookAtBranch({workspace})"),
            Self::ChangesLooked(_) => f.write_str("ChangesLooked"),
            Self::TimelineJobDone(_) => f.write_str("TimelineJobDone"),
            Self::RunningProcesses(_) => f.write_str("RunningProcesses"),
            Self::Workspaces(_) => f.write_str("Workspaces"),
            Self::Branches(_) => f.write_str("Branches"),
            Self::ProjectsForSync(_) => f.write_str("ProjectsForSync"),
            Self::WorktreesListed { .. } => f.write_str("WorktreesListed"),
            Self::Resources(_) => f.write_str("Resources"),
            Self::SpendTargets(_) => f.write_str("SpendTargets"),
            Self::Spends(spends) => write!(f, "Spends({})", spends.len()),
            Self::HandOver(_) => f.write_str("HandOver"),
            Self::Adopt(sessions, _) => write!(f, "Adopt({})", sessions.len()),
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

    /// Hands over a measurement. `false` once the Supervisor has stopped.
    pub(crate) async fn record_resources(&self, report: ResourceReport) -> bool {
        self.requests.send(Request::Resources(report)).await.is_ok()
    }

    /// Counts a client that will say whether the person is looking at it,
    /// until the guard is dropped; `None` once the Supervisor has stopped.
    pub(crate) async fn add_looker(
        &self,
        outgoing: mpsc::Sender<ServerFrame>,
    ) -> Option<LookGuard> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::AddLooker { outgoing, reply })
            .await
            .ok()?;
        answer.await.ok()
    }

    /// Tells whether the person is looking at the client. `false` once the
    /// Supervisor has stopped.
    pub(crate) async fn looking(&self, client: &LookGuard, looking: bool) -> bool {
        self.requests
            .send(Request::Looking {
                client: client.client(),
                looking,
            })
            .await
            .is_ok()
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

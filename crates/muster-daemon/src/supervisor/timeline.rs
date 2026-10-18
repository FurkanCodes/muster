//! Workspaces' timelines in the Supervisor (`PRODUCT.md` §6): an agent's
//! turn beginning and ending, as its session's status tells, is snapshot
//! off the loop (`ARC-13`), one job at a time per workspace so that each
//! turn's start comes before its end; and the person's asks: the timeline,
//! an entry's diff, what putting the files back would do, and doing it,
//! which waits until no agent works there (`PRD-14`).

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply};
use muster_core::{SessionId, SessionStatus, Timestamp, WorkspaceId};
use tokio::sync::oneshot;

use super::{Request, Supervisor};
use crate::harness;
use crate::timeline::{self, Store, TimelineError};

/// What an agent at work holds up, in the person's words.
const PUT_BACK: &str = "put the files back";
pub(super) const MOVE: &str = "move the branch";

/// The most jobs that wait behind one under way in a workspace
/// (`ARC-14`); past these, a turn goes unrecorded.
const MOST_WAITING: usize = 32;

/// A job under way this long is taken as lost, and the next one starts.
const LOST_AFTER_MILLIS: u64 = 30 * 60 * 1000;

/// The Supervisor's part: each workspace with a job under way, since
/// when, and the jobs waiting behind it.
#[derive(Debug, Default)]
pub(super) struct Timelines {
    busy: HashMap<WorkspaceId, (Timestamp, VecDeque<Job>)>,
}

/// Work on a workspace's timeline.
#[derive(Debug)]
pub(super) enum Job {
    /// An agent's turn began; `working` are the sessions working there.
    Began {
        session: SessionId,
        working: Vec<SessionId>,
        at: Timestamp,
    },
    /// An agent's turn ended.
    Ended {
        session: SessionId,
        agent: &'static str,
        at: Timestamp,
    },
    /// The person asked to put the files back from entry `number`.
    Undo {
        number: u64,
        move_branch: bool,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
    /// The person asked to move the branch (`supervisor::stacks`).
    Move {
        plan: timeline::MovePlan,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
}

/// What a job did, back on the Supervisor's queue.
#[derive(Clone, Copy, Debug)]
pub(super) struct JobDone {
    pub(super) workspace: WorkspaceId,
    /// The entry it recorded, if any.
    pub(super) recorded: Option<u64>,
    /// It changed the files, so what changed is looked at again.
    pub(super) undid: bool,
}

/// Whether a session in `status` is at work: on a turn, or waiting on the
/// person in the middle of one.
pub(super) const fn working(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Running | SessionStatus::NeedsYou { .. }
    )
}

impl Supervisor {
    /// A session's status went from `before` to `after`: an agent's turn
    /// beginning or ending is snapshot.
    pub(super) fn timeline_after_status(
        &mut self,
        session: SessionId,
        before: SessionStatus,
        after: SessionStatus,
    ) {
        if working(before) == working(after) {
            return;
        }
        if working(after) {
            self.turn_began(session);
        } else {
            self.turn_ended(session);
        }
    }

    /// An agent's turn began in `session`: also as a session starts with
    /// its first prompt.
    pub(super) fn turn_began(&mut self, session: SessionId) {
        let Some((workspace, _)) = self.agent_session(session) else {
            return;
        };
        let working = self.working_in(workspace);
        let at = self.clock.now();
        self.queue_timeline(
            workspace,
            Job::Began {
                session,
                working,
                at,
            },
        );
    }

    fn turn_ended(&mut self, session: SessionId) {
        let Some((workspace, agent)) = self.agent_session(session) else {
            return;
        };
        let at = self.clock.now();
        self.queue_timeline(workspace, Job::Ended { session, agent, at });
    }

    /// The workspace of `session` and its agent's name, where it is an
    /// agent's in a workspace whose files a timeline can keep.
    fn agent_session(&self, session: SessionId) -> Option<(WorkspaceId, &'static str)> {
        let stored = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)?;
        let found = harness::find(&stored.harness).filter(|found| found.is_agent())?;
        self.timeline_place(stored.workspace).ok()?;
        Some((stored.workspace, found.label))
    }

    /// The sessions at work in `workspace` now.
    fn working_in(&self, workspace: WorkspaceId) -> Vec<SessionId> {
        self.state
            .sessions
            .iter()
            .filter(|session| session.workspace == workspace && working(session.status))
            .map(|session| session.id)
            .collect()
    }

    /// Where a workspace's files and its timeline are: a worktree's or a
    /// checkout's on the host, in a repository.
    pub(super) fn timeline_place(
        &self,
        workspace: WorkspaceId,
    ) -> Result<(PathBuf, PathBuf), ErrorBody> {
        let path = self.repository_path(workspace)?;
        Ok((path, timeline::dir(&self.data_dir, workspace)))
    }

    /// Runs `job` now, or after the one under way in `workspace`.
    pub(super) fn queue_timeline(&mut self, workspace: WorkspaceId, job: Job) {
        if let Some((_, waiting)) = self.timelines.busy.get_mut(&workspace) {
            if waiting.len() < MOST_WAITING {
                waiting.push_back(job);
            } else {
                tracing::warn!(%workspace, "too much waits on a workspace's timeline");
                if let Job::Undo { reply, .. } | Job::Move { reply, .. } = job {
                    let _ = reply.send(Err(ErrorBody::new(
                        ErrorCode::Conflict,
                        "the timeline is busy: try again in a moment",
                    )));
                }
            }
            return;
        }
        let now = self.clock.now();
        self.timelines
            .busy
            .insert(workspace, (now, VecDeque::new()));
        self.run_timeline_job(workspace, job);
    }

    /// Starts `job` off the loop; it reports back with
    /// [`Request::TimelineJobDone`].
    fn run_timeline_job(&mut self, workspace: WorkspaceId, job: Job) {
        let place = self.timeline_place(workspace);
        let (session, working, agent, at) = match job {
            Job::Undo {
                number,
                move_branch,
                reply,
            } => {
                // Asked while nothing worked there; asked again as it starts.
                let checked = place.and_then(|place| match self.busy_agent(workspace, PUT_BACK) {
                    Some(busy) => Err(busy),
                    None => Ok(place),
                });
                match checked {
                    Ok((path, dir)) => {
                        let undoing = Undoing {
                            number,
                            move_branch,
                            reply,
                        };
                        self.run_undo(workspace, path, dir, undoing);
                    }
                    Err(error) => {
                        let _ = reply.send(Err(error));
                        self.next_timeline_job(workspace);
                    }
                }
                return;
            }
            Job::Move { plan, reply } => {
                let checked = place.and_then(|place| match self.busy_agent(workspace, MOVE) {
                    Some(busy) => Err(busy),
                    None => Ok(place),
                });
                match checked {
                    Ok((path, dir)) => self.run_move(workspace, path, dir, plan, reply),
                    Err(error) => {
                        let _ = reply.send(Err(error));
                        self.next_timeline_job(workspace);
                    }
                }
                return;
            }
            Job::Began {
                session,
                working,
                at,
            } => (session, Some(working), None, at),
            Job::Ended { session, agent, at } => (session, None, Some(agent), at),
        };
        let Ok((path, dir)) = place else {
            self.next_timeline_job(workspace);
            return;
        };
        let requests = self.requests.clone();
        self.tasks.spawn(async move {
            let recorded = match Store::open(&dir, &path).await {
                Ok(store) => match (working, agent) {
                    (Some(working), _) => store.began(session, &working, at).await,
                    (None, agent) => store.ended(session, agent.unwrap_or_default(), at).await,
                },
                Err(error) => Err(error),
            };
            let recorded = recorded.unwrap_or_else(|error| {
                tracing::warn!(%error, "could not keep a turn in the timeline");
                None
            });
            let done = JobDone {
                workspace,
                recorded,
                undid: false,
            };
            let _ = requests
                .send(Request::TimelineJobDone(Box::new(done)))
                .await;
        });
    }

    /// The job behind the one just done in `workspace`, if any.
    fn next_timeline_job(&mut self, workspace: WorkspaceId) {
        let next = self
            .timelines
            .busy
            .get_mut(&workspace)
            .and_then(|(_, waiting)| waiting.pop_front());
        match next {
            Some(job) => {
                if let Some((since, _)) = self.timelines.busy.get_mut(&workspace) {
                    *since = self.clock.now();
                }
                self.run_timeline_job(workspace, job);
            }
            None => {
                self.timelines.busy.remove(&workspace);
            }
        }
    }

    /// A job came back: the workspace's newest entry is told to clients,
    /// what changed is looked at again after an undo, and the next job
    /// starts.
    pub(super) fn timeline_job_done(&mut self, done: JobDone) {
        if let Some(recorded) = done.recorded
            && let Some(stored) = self
                .state
                .workspaces
                .iter_mut()
                .find(|candidate| candidate.id == done.workspace)
            && stored.timeline != Some(recorded)
        {
            stored.timeline = Some(recorded);
            let updated = stored.clone();
            self.broadcast(Delta::WorkspaceUpserted(updated));
        }
        if done.undid {
            self.look_at_changes(Some(done.workspace));
        }
        self.next_timeline_job(done.workspace);
    }

    /// On the pull request timer: a job under way too long is taken as
    /// lost, so the workspace's next can start.
    pub(super) fn tick_timelines(&mut self) {
        let now = self.clock.now();
        let lost: Vec<WorkspaceId> = self
            .timelines
            .busy
            .iter()
            .filter(|(_, (since, _))| muster_core::time::between(*since, now) > LOST_AFTER_MILLIS)
            .map(|(workspace, _)| *workspace)
            .collect();
        for workspace in lost {
            tracing::warn!(%workspace, "a timeline job never came back");
            self.next_timeline_job(workspace);
        }
    }

    /// A workspace went: its timeline goes with it (`ARC-14`).
    pub(super) fn forget_timeline(&mut self, workspace: WorkspaceId) {
        self.timelines.busy.remove(&workspace);
        let dir = timeline::dir(&self.data_dir, workspace);
        self.tasks
            .spawn(async move { timeline::remove(&dir).await });
    }

    /// Why the files cannot be put back now: an agent works there.
    pub(super) fn busy_agent(&self, workspace: WorkspaceId, what: &str) -> Option<ErrorBody> {
        let busy = self
            .state
            .sessions
            .iter()
            .find(|session| session.workspace == workspace && working(session.status))?;
        Some(ErrorBody::new(
            ErrorCode::Conflict,
            format!(
                "{} is at work there: {what} once it has stopped",
                busy.title
            ),
        ))
    }

    /// `Timeline`: answered from the store, which is only read; a
    /// workspace with none yet has an empty timeline.
    pub(super) fn workspace_timeline(
        &self,
        workspace: WorkspaceId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        self.read_timeline(workspace, reply, |store| async move {
            match store {
                Some(store) => store.timeline().await.map(Reply::Timeline),
                None => Ok(Reply::Timeline(muster_core::timeline::Timeline::default())),
            }
        });
    }

    /// `EntryDiff`: what one entry changed.
    pub(super) fn entry_diff(
        &self,
        workspace: WorkspaceId,
        number: u64,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        self.read_timeline(workspace, reply, move |store| async move {
            match store {
                Some(store) => store.diff(number).await.map(Reply::FileDiff),
                None => Err(TimelineError::NoEntry(number)),
            }
        });
    }

    /// `UndoPlan`: what putting the files back from an entry would do.
    pub(super) fn undo_plan(
        &self,
        workspace: WorkspaceId,
        number: u64,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        self.read_timeline(workspace, reply, move |store| async move {
            match store {
                Some(store) => store.plan(number).await.map(Reply::UndoPlan),
                None => Err(TimelineError::NoEntry(number)),
            }
        });
    }

    /// `Undo`: refused while an agent works there, else done once the
    /// workspace's timeline is free.
    pub(super) fn undo(
        &mut self,
        workspace: WorkspaceId,
        number: u64,
        move_branch: bool,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        if let Err(error) = self.timeline_place(workspace) {
            let _ = reply.send(Err(error));
            return;
        }
        if let Some(busy) = self.busy_agent(workspace, PUT_BACK) {
            tracing::info!(command = "undo", error = %busy, "command refused");
            let _ = reply.send(Err(busy));
            return;
        }
        self.queue_timeline(
            workspace,
            Job::Undo {
                number,
                move_branch,
                reply,
            },
        );
    }

    fn run_undo(&mut self, workspace: WorkspaceId, path: PathBuf, dir: PathBuf, undo: Undoing) {
        let requests = self.requests.clone();
        let at = self.clock.now();
        self.tasks.spawn(async move {
            let outcome = if dir.join("HEAD").exists() {
                match Store::open(&dir, &path).await {
                    Ok(store) => store.undo(undo.number, undo.move_branch, at).await,
                    Err(error) => Err(error),
                }
            } else {
                Err(TimelineError::NoEntry(undo.number))
            };
            let recorded = outcome.as_ref().ok().map(|(entry, _)| *entry);
            let reply = outcome
                .map(|(entry, moved_branch)| Reply::Undone {
                    entry,
                    moved_branch,
                })
                .map_err(|error| {
                    tracing::info!(command = "undo", %error, "command refused");
                    error_body(&error)
                });
            let _ = undo.reply.send(reply);
            let done = JobDone {
                workspace,
                recorded,
                undid: recorded.is_some(),
            };
            let _ = requests
                .send(Request::TimelineJobDone(Box::new(done)))
                .await;
        });
    }

    /// Answers `reply` with what `ask` makes of the workspace's store, off
    /// the loop: `None` where it has none yet.
    fn read_timeline<F, Fut>(
        &self,
        workspace: WorkspaceId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
        ask: F,
    ) where
        F: FnOnce(Option<Store>) -> Fut + Send + 'static,
        Fut: Future<Output = Result<Reply, TimelineError>> + Send,
    {
        let (path, dir) = match self.timeline_place(workspace) {
            Ok(place) => place,
            Err(error) => {
                let _ = reply.send(Err(error));
                return;
            }
        };
        self.tasks.spawn(async move {
            let outcome = if dir.join("HEAD").exists() {
                match Store::open(&dir, &path).await {
                    Ok(store) => ask(Some(store)).await,
                    Err(error) => Err(error),
                }
            } else {
                ask(None).await
            };
            let _ = reply.send(outcome.map_err(|error| {
                tracing::info!(command = "timeline", %error, "command refused");
                error_body(&error)
            }));
        });
    }
}

/// An undo the person asked for, as it starts.
#[derive(Debug)]
struct Undoing {
    number: u64,
    move_branch: bool,
    reply: oneshot::Sender<Result<Reply, ErrorBody>>,
}

fn error_body(error: &TimelineError) -> ErrorBody {
    let code = match error {
        TimelineError::NoEntry(_) => ErrorCode::NotFound,
        TimelineError::Gone(_)
        | TimelineError::Refused(_)
        | TimelineError::Branch(_)
        | TimelineError::AlreadyThere(_)
        | TimelineError::NotMoved(_) => ErrorCode::Conflict,
        TimelineError::Git(_) | TimelineError::Io(..) => ErrorCode::Internal,
    };
    ErrorBody::new(code, error.to_string())
}

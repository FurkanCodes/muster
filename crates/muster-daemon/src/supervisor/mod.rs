//! The Supervisor: the one task that owns the daemon's state (`ARC-02`).
//!
//! Every change goes through it: it writes the change to the store first
//! (`ARC-10`), then updates its copy of the state, then broadcasts the delta.
//! Because it alone takes snapshots and hands out subscriptions, a subscriber
//! sees exactly the deltas after its snapshot, with no gap and no repeat.
//!
//! Terminal traffic does not pass through here: a connection asks once for a
//! session's [`PtyHandle`] and then talks to its actor directly.

mod projects;
mod sessions;
mod worktrees;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, Snapshot};
use muster_core::{SessionId, SessionStatus};
use muster_state::session::{Effect, SessionMachine, Signal};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::clock::Clock;
use crate::pty::{PtyEvent, PtyHandle};
use crate::store::{StoreClosed, StoreError, StoreHandle};

type Subscription = (Snapshot, broadcast::Receiver<Delta>);

enum Request {
    Subscribe(oneshot::Sender<Subscription>),
    Command {
        command: Command,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
    /// A client wants a session's terminal; `viewed` when it will show it.
    Terminal {
        session: SessionId,
        viewed: bool,
        reply: oneshot::Sender<Option<PtyHandle>>,
    },
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subscribe(_) => f.write_str("Subscribe"),
            Self::Command { command, .. } => write!(f, "Command({command:?})"),
            Self::Terminal { session, .. } => write!(f, "Terminal({session})"),
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

    /// A session's terminal, if it still has one. With `viewed`, the session
    /// counts as seen.
    pub(crate) async fn terminal(&self, session: SessionId, viewed: bool) -> Option<PtyHandle> {
        let (reply, answer) = oneshot::channel();
        self.requests
            .send(Request::Terminal {
                session,
                viewed,
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
    pub(crate) delta_capacity: usize,
}

/// Loads the state from the store, settles sessions the last daemon left
/// running, and starts the Supervisor.
pub(crate) async fn spawn(settings: Settings) -> Result<SupervisorHandle, StoreError> {
    let state = settings
        .store
        .call(|store| store.snapshot())
        .await
        .map_err(|StoreClosed| StoreError::Invalid("the store stopped".to_owned()))??;
    let (requests, queue) = mpsc::channel(256);
    let (pty_events, pty_queue) = mpsc::unbounded_channel();
    let mut supervisor = Supervisor {
        store: settings.store,
        clock: settings.clock,
        worktrees_dir: settings.worktrees_dir,
        deltas: broadcast::channel(settings.delta_capacity).0,
        state,
        live: HashMap::new(),
        pty_events,
    };
    supervisor.settle_orphaned_sessions().await;
    tokio::spawn(supervisor.run(queue, pty_queue));
    Ok(SupervisorHandle { requests })
}

/// A session whose terminal the daemon holds: running, or exited with its
/// output still readable.
#[derive(Debug)]
struct Live {
    pty: PtyHandle,
    machine: SessionMachine,
}

#[derive(Debug)]
struct Supervisor {
    store: StoreHandle,
    clock: Arc<dyn Clock>,
    worktrees_dir: PathBuf,
    deltas: broadcast::Sender<Delta>,
    state: Snapshot,
    live: HashMap<SessionId, Live>,
    /// Handed to every terminal actor so it can report events.
    pty_events: mpsc::UnboundedSender<(SessionId, PtyEvent)>,
}

impl Supervisor {
    async fn run(
        mut self,
        mut queue: mpsc::Receiver<Request>,
        mut pty_queue: mpsc::UnboundedReceiver<(SessionId, PtyEvent)>,
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
                () = tokio::time::sleep_until(wake_at), if deadline.is_some() => self.tick().await,
            }
        }
    }

    async fn on_request(&mut self, request: Request) {
        match request {
            Request::Subscribe(reply) => {
                let _ = reply.send((self.state.clone(), self.deltas.subscribe()));
            }
            Request::Command { command, reply } => {
                let outcome = self.handle(command).await;
                if let Err(error) = &outcome {
                    tracing::debug!(%error, "command refused");
                }
                let _ = reply.send(outcome);
            }
            Request::Terminal {
                session,
                viewed,
                reply,
            } => {
                let pty = self.live.get(&session).map(|live| live.pty.clone());
                let _ = reply.send(pty);
                if viewed {
                    self.step_session(session, Signal::Viewed).await;
                }
            }
        }
    }

    async fn handle(&mut self, command: Command) -> Result<Reply, ErrorBody> {
        match command {
            Command::Ping => Ok(Reply::Pong),
            Command::AddProject { root, name } => self.add_project(root, name).await,
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
            } => self.start_session(workspace, harness, title, size).await,
            Command::StopSession { session } => self.stop_session(session).await,
        }
    }

    /// The earliest moment a session machine wants to be woken.
    fn next_deadline(&self) -> Option<std::time::Instant> {
        self.live
            .values()
            .filter_map(|live| live.machine.next_deadline())
            .min()
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

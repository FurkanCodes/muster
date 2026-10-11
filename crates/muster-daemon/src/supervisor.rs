//! The Supervisor: the one task that owns the daemon's state (`ARC-02`).
//!
//! Every change goes through it: it writes the change to the store first
//! (`ARC-10`), then updates its copy of the state, then broadcasts the delta.
//! Because it alone takes snapshots and hands out subscriptions, a subscriber
//! sees exactly the deltas after its snapshot, with no gap and no repeat.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use muster_core::protocol::{Command, Delta, ErrorBody, ErrorCode, Reply, Snapshot};
use muster_core::{Project, ProjectId};
use tokio::sync::{broadcast, mpsc, oneshot};

use crate::clock::Clock;
use crate::store::{StoreClosed, StoreError, StoreHandle};

type Subscription = (Snapshot, broadcast::Receiver<Delta>);

enum Request {
    Subscribe(oneshot::Sender<Subscription>),
    Command {
        command: Command,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
}

/// How connections reach the Supervisor.
#[derive(Clone, Debug)]
pub(crate) struct SupervisorHandle {
    requests: mpsc::Sender<Request>,
}

impl std::fmt::Debug for Request {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Subscribe(_) => f.write_str("Subscribe"),
            Self::Command { command, .. } => write!(f, "Command({command:?})"),
        }
    }
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
}

/// Loads the state from the store and starts the Supervisor.
pub(crate) async fn spawn(
    store: StoreHandle,
    clock: Arc<dyn Clock>,
    delta_capacity: usize,
) -> Result<SupervisorHandle, StoreError> {
    let state = store
        .call(|store| store.snapshot())
        .await
        .map_err(|StoreClosed| StoreError::Invalid("the store stopped".to_owned()))??;
    let (requests, queue) = mpsc::channel(256);
    let supervisor = Supervisor {
        store,
        clock,
        deltas: broadcast::channel(delta_capacity).0,
        state,
    };
    tokio::spawn(supervisor.run(queue));
    Ok(SupervisorHandle { requests })
}

#[derive(Debug)]
struct Supervisor {
    store: StoreHandle,
    clock: Arc<dyn Clock>,
    deltas: broadcast::Sender<Delta>,
    state: Snapshot,
}

impl Supervisor {
    async fn run(mut self, mut queue: mpsc::Receiver<Request>) {
        while let Some(request) = queue.recv().await {
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
            }
        }
    }

    async fn handle(&mut self, command: Command) -> Result<Reply, ErrorBody> {
        match command {
            Command::Ping => Ok(Reply::Pong),
            Command::AddProject { root, name } => self.add_project(root, name).await,
        }
    }

    async fn add_project(
        &mut self,
        root: PathBuf,
        name: Option<String>,
    ) -> Result<Reply, ErrorBody> {
        let root = repository_root(&root).await?;
        if self
            .state
            .projects
            .iter()
            .any(|project| project.root == root)
        {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{} is already a project", root.display()),
            ));
        }
        let name = name
            .filter(|name| !name.trim().is_empty())
            .or_else(|| {
                root.file_name()
                    .map(|name| name.to_string_lossy().into_owned())
            })
            .unwrap_or_else(|| root.display().to_string());
        let project = Project {
            id: ProjectId::new(),
            name,
            root,
            created_at: self.clock.now(),
        };

        let stored = project.clone();
        self.store
            .call(move |store| store.insert_project(&stored))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;

        self.state.projects.push(project.clone());
        let id = project.id;
        // Nobody listening is fine; the change is already stored.
        let _ = self.deltas.send(Delta::ProjectUpserted(project));
        Ok(Reply::ProjectAdded { id })
    }
}

/// The canonical path of the git repository at `path`.
async fn repository_root(path: &Path) -> Result<PathBuf, ErrorBody> {
    if !path.is_absolute() {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is not an absolute path", path.display()),
        ));
    }
    let root = tokio::fs::canonicalize(path).await.map_err(|error| {
        ErrorBody::new(
            ErrorCode::BadRequest,
            format!("cannot open {}: {error}", path.display()),
        )
    })?;
    let is_repository = tokio::fs::try_exists(root.join(".git"))
        .await
        .unwrap_or(false);
    if !root.is_dir() || !is_repository {
        return Err(ErrorBody::new(
            ErrorCode::BadRequest,
            format!("{} is not a git repository", root.display()),
        ));
    }
    Ok(root)
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

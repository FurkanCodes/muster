//! Sessions in runs' containers (`PRD-08`, `ARCHITECTURE.md` §5, §8).
//!
//! Taking a run over stops Muster's own work on it and starts its building
//! agent for the person, interactively, in the run's container: Claude Code
//! resumes the conversation the run's agent had, so the person carries on
//! where it stopped, and the agent carries on from the person's work when
//! the run is handed back. A shell can be opened there too, to look around.
//! The container is a workspace of the run's project, made the first time a
//! session needs it; handing back stops its sessions, and the workspace
//! goes with the container when the run ends.
//!
//! The agent needs the runs' credential, which is fetched off the loop
//! (reading the Keychain can wait on the person) before it starts.

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{
    RunId, RunStage, Session, SessionId, SessionStatus, Workspace, WorkspaceId, WorkspaceKind,
};
use muster_state::run::RunEvent;
use muster_state::session::Start;
use tokio::sync::oneshot;

use super::sessions::Mode;
use super::{Request, Supervisor, store_closed, store_error};
use crate::harness::{self, Place, claude};
use crate::runs::credentials::{MISSING, Secret};

/// The harness the person drives a run's agent with.
const AGENT: &str = "claude";

/// What the runs' credential was fetched for.
#[derive(Debug)]
pub(super) enum Purpose {
    /// The person takes a run over.
    TakeOver {
        run: RunId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
    /// The person starts a run's agent again while they have the run.
    Restart {
        session: SessionId,
        size: TermSize,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    },
    /// The daemon restarted while the person had the run.
    Recover { session: SessionId },
}

/// The credential, back on the loop with what it was fetched for.
#[derive(Debug)]
pub(super) struct CredentialFetched {
    secret: Option<Secret>,
    purpose: Purpose,
}

impl Supervisor {
    /// Fetches the runs' credential off the loop, then carries on with
    /// `purpose` in [`on_credential`](Self::on_credential).
    pub(super) fn fetch_credential(&self, purpose: Purpose) {
        let (requests, credentials) = (self.requests.clone(), self.credentials.clone());
        self.tasks.spawn(async move {
            let secret = credentials.claude().await;
            let fetched = CredentialFetched { secret, purpose };
            let _ = requests
                .send(Request::CredentialFetched(Box::new(fetched)))
                .await;
        });
    }

    pub(super) async fn on_credential(&mut self, fetched: CredentialFetched) {
        let CredentialFetched { secret, purpose } = fetched;
        let missing = || ErrorBody::new(ErrorCode::BadRequest, MISSING);
        match purpose {
            Purpose::TakeOver { run, reply } => {
                let outcome = match secret {
                    Some(secret) => self.take_over(run, secret).await,
                    None => Err(missing()),
                };
                if let Err(error) = &outcome {
                    tracing::info!(command = "take over run", %error, "command refused");
                }
                let _ = reply.send(outcome);
            }
            Purpose::Restart {
                session,
                size,
                reply,
            } => {
                let outcome = match secret {
                    Some(secret) => self.restart_agent_in_container(session, size, secret).await,
                    None => Err(missing()),
                };
                let _ = reply.send(outcome);
            }
            Purpose::Recover { session } => {
                let resumed = match secret {
                    Some(secret) => {
                        self.restart_agent_in_container(session, TermSize::default(), secret)
                            .await
                    }
                    None => Err(missing()),
                };
                if let Err(error) = resumed {
                    tracing::warn!(%session, %error, "could not resume a run's agent");
                    self.end_session(session, SessionStatus::Exited { success: false }, true)
                        .await;
                }
            }
        }
    }

    /// The person's `TakeOverRun`: refused at once if it cannot apply,
    /// otherwise carried on once the credential is fetched.
    pub(super) fn take_over_run(
        &self,
        run: RunId,
        reply: oneshot::Sender<Result<Reply, ErrorBody>>,
    ) {
        match self.can_take_over(run) {
            Ok(()) => self.fetch_credential(Purpose::TakeOver { run, reply }),
            Err(error) => {
                tracing::info!(command = "take over run", %error, "command refused");
                let _ = reply.send(Err(error));
            }
        }
    }

    /// Whether the run can be taken over now: it is going, past starting
    /// its container, and not the person's already.
    fn can_take_over(&self, run: RunId) -> Result<(), ErrorBody> {
        let active = self.going_run(run)?;
        let stage = active.machine.stage();
        let takes = matches!(
            stage,
            RunStage::Building
                | RunStage::Checking
                | RunStage::AgentReview
                | RunStage::ReadyForYou
                | RunStage::NeedsYou
        );
        if !takes {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("a run cannot be taken over while it is {}", stage.phrase()),
            ));
        }
        if active.context.container.is_none() {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                "the run has no container to take over yet",
            ));
        }
        Ok(())
    }

    /// The run, if it is going.
    fn going_run(&self, run: RunId) -> Result<&super::runs::ActiveRun, ErrorBody> {
        self.active_runs.get(&run).ok_or_else(|| {
            match self.state.runs.iter().find(|stored| stored.id == run) {
                Some(ended) => ErrorBody::new(
                    ErrorCode::Conflict,
                    format!("the run on #{} has ended", ended.ticket.number),
                ),
                None => ErrorBody::new(ErrorCode::NotFound, format!("no run {run}")),
            }
        })
    }

    /// Takes the run over: its transition is stored and Muster's work
    /// stopped first, then the agent starts for the person.
    async fn take_over(&mut self, run: RunId, secret: Secret) -> Result<Reply, ErrorBody> {
        // Things may have moved while the credential was fetched.
        self.can_take_over(run)?;
        if !self.step_run(run, RunEvent::TakeOver).await {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                "the run's container is gone; retry the run for a new one",
            ));
        }
        let (workspace, session) =
            self.start_agent_in_container(run, secret)
                .await
                .map_err(|error| {
                    ErrorBody::new(
                        error.code,
                        format!(
                            "the run is yours, but its agent did not start: {}; \
                         start the session again or hand the run back",
                            error.message
                        ),
                    )
                })?;
        tracing::info!(%run, %session, "the person took a run over");
        Ok(Reply::RunTakenOver { workspace, session })
    }

    /// Starts the run's agent for the person: in the session that drove it
    /// before, if there is one, or a new one. It carries on the run's
    /// conversation; a run whose agent never started gets one now, which
    /// its agent carries on after the hand-back.
    async fn start_agent_in_container(
        &mut self,
        run: RunId,
        secret: Secret,
    ) -> Result<(WorkspaceId, SessionId), ErrorBody> {
        let workspace = self.container_workspace(run).await?;
        let existing = self
            .state
            .sessions
            .iter()
            .find(|session| session.workspace == workspace && session.harness == AGENT)
            .map(|session| session.id);
        if let Some(session) = existing {
            if !self.is_running(session) {
                self.restart_agent_in_container(session, TermSize::default(), secret)
                    .await?;
            }
            return Ok((workspace, session));
        }
        let harness = harness::find(AGENT).ok_or_else(|| {
            ErrorBody::new(ErrorCode::Internal, "Claude Code is not a known harness")
        })?;
        let agent_session = self.run_agent_session(run).await?;
        let place = self.container_place(run, Some(secret))?;
        let session = self
            .add_session(workspace, harness, None, Start::Idle)
            .await?;
        let mode = Mode::Resume { agent_session };
        let size = TermSize::default();
        if let Err(error) = self.launch(session, harness, place, size, mode, Start::Idle, false) {
            self.end_session(session, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start {}: {error}", harness.label),
            ));
        }
        Ok((workspace, session))
    }

    /// The run agent's conversation, made now if it has none.
    async fn run_agent_session(&mut self, run: RunId) -> Result<String, ErrorBody> {
        let known = self
            .active_runs
            .get(&run)
            .and_then(|active| active.context.agent_session.clone());
        if let Some(known) = known {
            return Ok(known);
        }
        let made = claude::new_session_id()
            .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?;
        if let Some(active) = self.active_runs.get_mut(&run) {
            active.context.agent_session = Some(made.clone());
        }
        self.save_run_progress(run).await;
        Ok(made)
    }

    /// Whether `session` is a run's agent in its container, which needs the
    /// credential to start again: `Some` with whether it may, `None` for
    /// any other session.
    pub(super) fn agent_restart(&self, session: SessionId) -> Option<Result<(), ErrorBody>> {
        let stored = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)?;
        let run = self.workspace_run(stored.workspace)?;
        if stored.harness != AGENT {
            return None;
        }
        if self.is_running(session) {
            return Some(Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{} is still running", stored.title),
            )));
        }
        let taken_over = self
            .active_runs
            .get(&run)
            .is_some_and(|active| active.machine.stage() == RunStage::TakenOver);
        Some(if taken_over {
            Ok(())
        } else {
            Err(ErrorBody::new(
                ErrorCode::Conflict,
                "the run's agent is the run's own until you take the run over \
                 (`muster run take-over`)",
            ))
        })
    }

    /// Starts the run's agent again in its session, with the credential.
    async fn restart_agent_in_container(
        &mut self,
        session: SessionId,
        size: TermSize,
        secret: Secret,
    ) -> Result<Reply, ErrorBody> {
        if let Some(Err(error)) = self.agent_restart(session) {
            return Err(error);
        }
        let stored = self.stored_session(session)?;
        let run = self.workspace_run(stored.workspace).ok_or_else(|| {
            ErrorBody::new(ErrorCode::BadRequest, "not a session in a run's container")
        })?;
        let harness = harness::find(AGENT).ok_or_else(|| {
            ErrorBody::new(ErrorCode::Internal, "Claude Code is not a known harness")
        })?;
        let agent_session = self.run_agent_session(run).await?;
        let place = self.container_place(run, Some(secret))?;
        let size = self
            .live
            .get(&session)
            .and_then(|live| live.ended)
            .map_or(size, |(_, size)| size);
        self.reopen_and_launch(
            session,
            harness,
            place,
            size,
            Mode::Resume { agent_session },
            Start::Idle,
        )
        .await?;
        Ok(Reply::SessionStarted { id: session })
    }

    /// A session in a run's container that the last daemon left running.
    /// Its program may still be there, out of reach: the run's agent, while
    /// the person has the run, starts again (stopping it first); anything
    /// else is stopped and recorded as ended.
    pub(super) async fn recover_in_container(&mut self, session: &Session, run: RunId) {
        if self
            .agent_restart(session.id)
            .is_some_and(|may| may.is_ok())
        {
            self.fetch_credential(Purpose::Recover {
                session: session.id,
            });
            return;
        }
        self.stop_in_container(session.id);
        tracing::info!(session = %session.id, %run, "a session in a run's container ended with the daemon");
        self.end_session(session.id, SessionStatus::Exited { success: false }, true)
            .await;
    }

    /// The run's container as a workspace of its project, made now if it is
    /// not one yet.
    async fn container_workspace(&mut self, run: RunId) -> Result<WorkspaceId, ErrorBody> {
        if let Some(found) = self
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.kind.run() == Some(run))
        {
            return Ok(found.id);
        }
        let active = self.going_run(run)?;
        let workspace = Workspace {
            id: WorkspaceId::new(),
            project: active.context.project,
            kind: WorkspaceKind::Container {
                run,
                branch: active.context.branch.clone(),
            },
            created_at: self.clock.now(),
            pull_request: None,
            changes: None,
            timeline: None,
        };
        let stored = workspace.clone();
        self.store
            .call(move |store| store.insert_workspace(&stored))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        let id = workspace.id;
        self.state.workspaces.push(workspace.clone());
        self.broadcast(Delta::WorkspaceUpserted(workspace));
        Ok(id)
    }

    /// Where a session in the run's container runs.
    pub(super) fn container_place(
        &self,
        run: RunId,
        secret: Option<Secret>,
    ) -> Result<Place, ErrorBody> {
        let container = self
            .active_runs
            .get(&run)
            .and_then(|active| active.context.container.clone())
            .ok_or_else(|| ErrorBody::new(ErrorCode::Conflict, "the run's container is gone"))?;
        Ok(Place::Container {
            docker: self.docker.clone(),
            container,
            secret,
        })
    }

    /// The run whose container `workspace` is, if it is one.
    pub(super) fn workspace_run(&self, workspace: WorkspaceId) -> Option<RunId> {
        self.state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .and_then(|found| found.kind.run())
    }

    /// Stops `session`'s program in its run's container, if it runs in one:
    /// stopping docker's client on the host stops nothing there.
    pub(super) fn stop_in_container(&self, session: SessionId) {
        let Some(run) = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .and_then(|stored| self.workspace_run(stored.workspace))
        else {
            return;
        };
        if let Some(container) = self
            .active_runs
            .get(&run)
            .and_then(|active| active.context.container.clone())
        {
            self.work.stop_session(container, session);
        }
    }

    /// The person handed the run back, or it ended while theirs: the
    /// sessions in its container stop.
    pub(super) async fn stop_container_sessions(&mut self, run: RunId) {
        let running: Vec<SessionId> = self
            .state
            .sessions
            .iter()
            .filter(|session| self.workspace_run(session.workspace) == Some(run))
            .map(|session| session.id)
            .filter(|session| self.is_running(*session))
            .collect();
        for session in running {
            self.stop_in_container(session);
            if let Some(live) = self.live.get(&session) {
                let pty = live.pty.clone();
                pty.stop().await;
            }
        }
    }

    /// Containers' workspaces whose run is no longer going, left by a
    /// daemon that stopped between a run's end and its workspace's: they go
    /// now.
    pub(super) async fn drop_stale_container_workspaces(&mut self) {
        let stale: Vec<RunId> = self
            .state
            .workspaces
            .iter()
            .filter_map(|workspace| workspace.kind.run())
            .filter(|run| !self.active_runs.contains_key(run))
            .collect();
        for run in stale {
            self.drop_container_workspace(run).await;
        }
    }

    /// The run ended and its container goes: so do its workspace and the
    /// sessions in it, keeping their history in the store.
    pub(super) async fn drop_container_workspace(&mut self, run: RunId) {
        let Some(workspace) = self
            .state
            .workspaces
            .iter()
            .find(|workspace| workspace.kind.run() == Some(run))
            .map(|workspace| workspace.id)
        else {
            return;
        };
        self.stop_container_sessions(run).await;
        let at = self.clock.now();
        self.write(move |store| store.remove_workspace(workspace, at))
            .await;
        self.drop_workspace(workspace);
        tracing::info!(%run, %workspace, "a run's container left with its run");
    }
}

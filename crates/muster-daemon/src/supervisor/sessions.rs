//! Starting and stopping sessions, and what their terminals report.

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{Session, SessionId, SessionStatus, WorkspaceId, WorkspaceKind};
use muster_state::session::{Launch, SessionMachine, Signal, Timing};

use super::{Live, Supervisor, store_closed, store_error};
use crate::harness;
use crate::pty::{self, PtyEvent};

impl Supervisor {
    pub(super) async fn start_session(
        &mut self,
        workspace: WorkspaceId,
        harness: String,
        title: Option<String>,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        let WorkspaceKind::Worktree { path, .. } = &found.kind;
        let cwd = path.clone();
        let harness = harness::find(&harness).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::BadRequest,
                format!("no harness called {harness:?}"),
            )
        })?;

        let session = Session {
            id: SessionId::new(),
            workspace,
            harness: harness.id.to_owned(),
            title: title
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| harness.label.to_owned()),
            status: SessionStatus::Fresh,
            unseen: false,
            resume_id: None,
            created_at: self.clock.now(),
            ended_at: None,
        };
        let id = session.id;
        let stored = session.clone();
        self.store
            .call(move |store| store.insert_session(&stored))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        self.state.sessions.push(session.clone());
        self.broadcast(Delta::SessionUpserted(session));

        let events = self.pty_events.clone();
        let spec = harness::spawn_spec(harness, id, cwd, size);
        let pty = match pty::spawn(spec, move |event| {
            let _ = events.send((id, event));
        }) {
            Ok(pty) => pty,
            Err(error) => {
                // The session was stored before its process was started
                // (`ARC-10`), so record that it never ran.
                self.end_session(id, SessionStatus::Exited { success: false }, true)
                    .await;
                return Err(ErrorBody::new(
                    ErrorCode::Internal,
                    format!("could not start {}: {error}", harness.label),
                ));
            }
        };
        let machine = SessionMachine::new(Launch::default(), Timing::default());
        self.live.insert(id, Live { pty, machine });
        Ok(Reply::SessionStarted { id })
    }

    pub(super) async fn stop_session(&mut self, session: SessionId) -> Result<Reply, ErrorBody> {
        let live = self.live.get(&session).ok_or_else(|| {
            ErrorBody::new(ErrorCode::NotFound, format!("{session} is not running"))
        })?;
        if live.machine.status().is_exited() {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{session} has already exited"),
            ));
        }
        live.pty.stop().await;
        Ok(Reply::Done)
    }

    pub(super) async fn on_pty_event(&mut self, session: SessionId, event: PtyEvent) {
        match event {
            PtyEvent::Progress { busy } => {
                self.step_session(session, Signal::Progress { busy }).await;
            }
            PtyEvent::Exited { success, requested } => {
                self.step_session(session, Signal::Exited { success, requested })
                    .await;
                let at = self.clock.now();
                self.write(move |store| store.end_session(session, at))
                    .await;
                if let Some(stored) = self.session_mut(session) {
                    stored.ended_at = Some(at);
                    let stored = stored.clone();
                    self.broadcast(Delta::SessionUpserted(stored));
                }
            }
        }
    }

    /// Records that a session ended without its terminal reporting it.
    async fn end_session(&mut self, session: SessionId, status: SessionStatus, unseen: bool) {
        self.set_status(session, status, unseen).await;
        let at = self.clock.now();
        self.write(move |store| store.end_session(session, at))
            .await;
        if let Some(stored) = self.session_mut(session) {
            stored.ended_at = Some(at);
            let stored = stored.clone();
            self.broadcast(Delta::SessionUpserted(stored));
        }
    }

    /// Sessions the last daemon left running died with it, because it held
    /// their terminals. Records them as exited, for the person to see.
    pub(super) async fn settle_orphaned_sessions(&mut self) {
        let orphaned: Vec<SessionId> = self
            .state
            .sessions
            .iter()
            .filter(|session| session.ended_at.is_none())
            .map(|session| session.id)
            .collect();
        for session in orphaned {
            tracing::info!(%session, "a session ended when the daemon stopped");
            self.end_session(session, SessionStatus::Exited { success: false }, true)
                .await;
        }
    }

    /// How many of a workspace's sessions are still running.
    pub(super) fn running_sessions_in(&self, workspace: WorkspaceId) -> usize {
        self.state
            .sessions
            .iter()
            .filter(|session| session.workspace == workspace)
            .filter(|session| {
                self.live
                    .get(&session.id)
                    .is_some_and(|live| !live.machine.status().is_exited())
            })
            .count()
    }
}

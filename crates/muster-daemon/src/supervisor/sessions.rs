//! Starting, stopping and resuming sessions, and what their terminals report.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{Session, SessionId, SessionStatus, WorkspaceId, WorkspaceKind};
use muster_state::session::{self, SessionMachine, Signal, Start, Timing};

use super::{Live, Supervisor, store_closed, store_error};
use crate::harness::{self, Harness, HookEndpoint, claude, transcript};
use crate::pty::{self, PtyEvent};

/// A resumed program that exits sooner than this, unasked, could not
/// resume; the session starts fresh instead (as Nebula does).
const RESUME_GRACE: Duration = Duration::from_secs(10);

/// How a session's program is started.
#[derive(Debug)]
enum Mode {
    Fresh { prompt: Option<String> },
    Resume { agent_session: String },
}

impl Supervisor {
    pub(super) async fn start_session(
        &mut self,
        workspace: WorkspaceId,
        harness: String,
        title: Option<String>,
        size: TermSize,
        prompt: Option<String>,
    ) -> Result<Reply, ErrorBody> {
        let cwd = self.workspace_path(workspace).ok_or_else(|| {
            ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
        })?;
        let harness = harness::find(&harness).ok_or_else(|| {
            ErrorBody::new(
                ErrorCode::BadRequest,
                format!("no harness called {harness:?}"),
            )
        })?;
        let prompt = prompt.filter(|prompt| !prompt.trim().is_empty());
        if prompt.is_some() && !harness.takes_prompt() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{} takes no prompt", harness.label),
            ));
        }

        let session = Session {
            id: SessionId::new(),
            workspace,
            harness: harness.id.to_owned(),
            title: title
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| harness.label.to_owned()),
            status: if prompt.is_some() {
                SessionStatus::Running
            } else {
                SessionStatus::Fresh
            },
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

        let start = if prompt.is_some() {
            Start::WithPrompt
        } else {
            Start::Idle
        };
        if let Err(error) =
            self.launch(id, harness, cwd, size, Mode::Fresh { prompt }, start, false)
        {
            // The session was stored before its program was started
            // (`ARC-10`), so record that it never ran.
            self.end_session(id, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start {}: {error}", harness.label),
            ));
        }
        Ok(Reply::SessionStarted { id })
    }

    /// Starts a session's program and begins tracking it. A program started
    /// again in a session carries on from its last one: the same terminal
    /// size, and output numbered on from where the last one's ended, past a
    /// gap of one so that no client's position from before is mistaken for
    /// one after and every client starts the new program on a clean screen.
    /// `unseen` starts the session with news for the person, unless someone
    /// is already watching it.
    #[allow(clippy::too_many_arguments)]
    fn launch(
        &mut self,
        session: SessionId,
        harness: Harness,
        cwd: PathBuf,
        size: TermSize,
        mode: Mode,
        start: Start,
        unseen: bool,
    ) -> std::io::Result<()> {
        let previous = self.live.get(&session).and_then(|live| live.ended);
        let size = previous.map_or(size, |(_, size)| size);
        let first_seq = previous.map_or(0, |(end, _)| end + 1);
        let (agent_session, resume, prompt) = match mode {
            Mode::Fresh { prompt } => {
                let id = harness
                    .has_hooks()
                    .then(claude::new_session_id)
                    .transpose()?;
                (id, false, prompt)
            }
            Mode::Resume { agent_session } => (Some(agent_session), true, None),
        };
        let hooks = if harness.has_hooks() {
            Some(HookEndpoint {
                socket: self.hook_socket.clone(),
                token: harness::new_token()?,
            })
        } else {
            None
        };
        let spec = harness::spawn_spec(
            harness,
            harness::Launch {
                session,
                agent_session: agent_session.clone(),
                resume,
                prompt,
                hooks: hooks.clone(),
                cwd,
                size,
            },
        );
        let events = self.pty_events.clone();
        let pty = pty::spawn(spec, first_seq, move |event| {
            let _ = events.send((session, event));
        })?;

        self.forget_token(session);
        let token = hooks.map(|hooks| hooks.token);
        if let Some(token) = &token {
            self.hook_tokens.insert(token.clone(), session);
        }
        let machine = SessionMachine::new(
            session::Launch {
                agent_session,
                start,
                unseen,
            },
            Timing::default(),
        );
        self.live.insert(
            session,
            Live {
                pty: pty.clone(),
                machine,
                harness,
                token,
                launched: Instant::now(),
                resumed: resume,
                ended: None,
            },
        );
        self.carry_watch(session);
        self.publish_terminal(session, pty);
        Ok(())
    }

    /// The status a just-launched session's machine starts in, as the person
    /// should see it.
    fn launched_status(&self, session: SessionId) -> Option<(SessionStatus, bool)> {
        self.live
            .get(&session)
            .map(|live| (live.machine.status(), live.machine.unseen()))
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
            PtyEvent::Exited {
                success,
                requested,
                end,
                size,
            } => {
                if let Some(live) = self.live.get_mut(&session) {
                    live.ended = Some((end, size));
                }
                self.forget_token(session);
                let failed_resume = self.live.get(&session).is_some_and(|live| {
                    live.resumed && !requested && live.launched.elapsed() < RESUME_GRACE
                });
                if failed_resume && self.start_fresh_after_failed_resume(session).await {
                    return;
                }
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

    /// A resumed program exited at once: start the harness fresh in the same
    /// session rather than leave a dead terminal. `false` if that failed too.
    async fn start_fresh_after_failed_resume(&mut self, session: SessionId) -> bool {
        let Some(harness) = self.live.get(&session).map(|live| live.harness) else {
            return false;
        };
        let Some(cwd) = self.session_path(session) else {
            return false;
        };
        tracing::warn!(%session, "resume failed; starting the session fresh");
        let size = TermSize::default();
        let mode = Mode::Fresh { prompt: None };
        match self.launch(session, harness, cwd, size, mode, Start::Idle, true) {
            Ok(()) => {
                if let Some((status, unseen)) = self.launched_status(session) {
                    self.set_status(session, status, unseen).await;
                }
                true
            }
            Err(error) => {
                tracing::error!(%session, %error, "could not start the session fresh");
                false
            }
        }
    }

    /// Sessions the last daemon left running died with it, because it held
    /// their terminals. A harness that can resume is resumed where its
    /// transcript survives; the rest are recorded as exited, for the person
    /// to see.
    pub(super) async fn recover_sessions(&mut self) {
        let orphaned: Vec<Session> = self
            .state
            .sessions
            .iter()
            .filter(|session| session.ended_at.is_none())
            .cloned()
            .collect();
        for session in orphaned {
            if self.resume(&session).await {
                continue;
            }
            tracing::info!(session = %session.id, "a session ended when the daemon stopped");
            self.end_session(session.id, SessionStatus::Exited { success: false }, true)
                .await;
        }
    }

    /// Resumes `session` if it can be. A turn the restart cut short counts
    /// as unseen.
    async fn resume(&mut self, session: &Session) -> bool {
        let harness = harness::find(&session.harness).filter(|harness| harness.can_resume());
        let (Some(harness), Some(agent_session), Some(cwd)) = (
            harness,
            session.resume_id.clone(),
            self.workspace_path(session.workspace),
        ) else {
            return false;
        };
        let transcript = transcript::projects_dir()
            .and_then(|projects| transcript::exists(&projects, &agent_session));
        if transcript == Some(false) {
            tracing::info!(session = %session.id, "no transcript to resume");
            return false;
        }
        let unseen = session.unseen
            || matches!(
                session.status,
                SessionStatus::Running | SessionStatus::NeedsYou { .. }
            );
        let mode = Mode::Resume { agent_session };
        let size = TermSize::default();
        match self.launch(session.id, harness, cwd, size, mode, Start::Resumed, unseen) {
            Ok(()) => {
                tracing::info!(session = %session.id, "resumed");
                if let Some((status, unseen)) = self.launched_status(session.id) {
                    self.set_status(session.id, status, unseen).await;
                }
                true
            }
            Err(error) => {
                tracing::warn!(session = %session.id, %error, "could not resume");
                false
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

    /// Stops accepting hooks for `session`.
    fn forget_token(&mut self, session: SessionId) {
        if let Some(token) = self
            .live
            .get_mut(&session)
            .and_then(|live| live.token.take())
        {
            self.hook_tokens.remove(&token);
        }
    }

    fn workspace_path(&self, workspace: WorkspaceId) -> Option<PathBuf> {
        self.state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .map(|found| {
                let WorkspaceKind::Worktree { path, .. } = &found.kind;
                path.clone()
            })
    }

    fn session_path(&self, session: SessionId) -> Option<PathBuf> {
        let workspace = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)?
            .workspace;
        self.workspace_path(workspace)
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

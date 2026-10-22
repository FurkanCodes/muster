//! Starting, stopping and restarting sessions, and what their terminals
//! report. Bringing them back after a daemon restart is `recovery`'s.

use std::path::PathBuf;
use std::time::Duration;

use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reading, Reply, TermSize};
use muster_core::text::one_line;
use muster_core::{Session, SessionId, SessionStatus, WorkspaceId};
use muster_state::session::{self, SessionMachine, Signal, Start};

use super::recovery::resumable;
use super::{Live, Supervisor, store_closed, store_error};
use crate::harness::{self, CheckCommands, Harness, HookEndpoint, Place, TitleState, claude};
use crate::pty::{self, PtyEvent};

/// A resumed program that exits sooner than this, unasked, could not
/// resume; the session starts fresh instead (as Nebula does).
const RESUME_GRACE: Duration = Duration::from_secs(10);

/// What starting a session asks for, beside where and with what.
#[derive(Debug, Default)]
pub(super) struct Asked {
    pub(super) title: Option<String>,
    pub(super) prompt: Option<String>,
    /// The agent's session asking, through the command line (`PRD-01`).
    pub(super) started_by: Option<SessionId>,
}

impl Asked {
    pub(super) fn new(
        title: Option<String>,
        prompt: Option<String>,
        started_by: Option<SessionId>,
    ) -> Self {
        Self {
            title,
            prompt,
            started_by,
        }
    }
}

/// The sessions a new one belongs with.
#[derive(Debug, Default)]
pub(super) struct Ties {
    /// The chat a plugin's panel is opened beside.
    pub(super) beside: Option<SessionId>,
    /// The agent's session that started it (`PRD-01`).
    pub(super) started_by: Option<SessionId>,
}

impl Ties {
    pub(super) fn beside(beside: Option<SessionId>) -> Self {
        Self {
            beside,
            started_by: None,
        }
    }

    fn started_by(started_by: Option<SessionId>) -> Self {
        Self {
            beside: None,
            started_by,
        }
    }
}

/// How a session's program is started.
#[derive(Debug)]
pub(super) enum Mode {
    Fresh {
        prompt: Option<String>,
    },
    Resume {
        agent_session: String,
    },
    /// The project's checks, for the harness that runs them.
    Checks(CheckCommands),
}

impl Supervisor {
    pub(super) async fn start_session(
        &mut self,
        workspace: WorkspaceId,
        harness: String,
        size: TermSize,
        asked: Asked,
    ) -> Result<Reply, ErrorBody> {
        let Asked {
            title,
            prompt,
            started_by,
        } = asked;
        let started_by = self.starter(started_by)?;
        let harness = self.known_harness(&harness)?;
        let place = self.place_for(workspace, &harness)?;
        let prompt = prompt.filter(|prompt| !prompt.trim().is_empty());
        if prompt.is_some() && !harness.takes_prompt() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{} takes no prompt", harness.label()),
            ));
        }
        if harness.is_checks() {
            // They need the project's settings: see `check_workspace`.
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "the checks start with the check command",
            ));
        }
        let start = if prompt.is_some() {
            Start::WithPrompt
        } else {
            Start::Idle
        };
        // A chat with no name of its own is named by its first prompt, and
        // then by an agent.
        let named_by_prompt = title.is_none().then(|| prompt.clone()).flatten();
        let title = title.or_else(|| prompt.as_deref().and_then(muster_core::names::from_prompt));
        let id = self
            .add_session_tied(
                workspace,
                &harness,
                title,
                start,
                Ties::started_by(started_by),
            )
            .await?;
        if let Err(error) = self.launch(
            id,
            harness.clone(),
            place,
            size,
            Mode::Fresh { prompt },
            start,
            false,
        ) {
            // The session was stored before its program was started
            // (`ARC-10`), so record that it never ran.
            self.end_session(id, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start {}: {error}", harness.label()),
            ));
        }
        if let Some(prompt) = named_by_prompt {
            self.ask_name(id, &prompt);
        }
        Ok(Reply::SessionStarted { id })
    }

    /// What `session` shows, or its agent last answered, as text: for
    /// `muster logs` and `muster wait`.
    pub(super) async fn read_session(&self, session: SessionId, what: Reading) -> Reply {
        let live = self.live.get(&session);
        let text = match what {
            Reading::LastReply => live.and_then(|live| live.last_reply.clone()),
            Reading::Screen => match live {
                Some(live) => live.pty.text().await,
                None => None,
            },
        };
        Reply::SessionText { text }
    }

    /// Stores a new session in `workspace` and tells clients, before its
    /// program starts (`ARC-10`).
    pub(super) async fn add_session(
        &mut self,
        workspace: WorkspaceId,
        harness: &Harness,
        title: Option<String>,
        start: Start,
    ) -> Result<SessionId, ErrorBody> {
        self.add_session_tied(workspace, harness, title, start, Ties::default())
            .await
    }

    /// The session an agent asking to start one runs in, where its limits
    /// let it (`PRD-01`); `None` for the person. A shell is the person's,
    /// and a session this daemon does not know (one of another daemon's) is
    /// no agent of its.
    fn starter(&self, asking: Option<SessionId>) -> Result<Option<SessionId>, ErrorBody> {
        let Some(asking) = asking else {
            return Ok(None);
        };
        let Some(session) = self.state.sessions.iter().find(|s| s.id == asking) else {
            return Ok(None);
        };
        if session.kind().is_shell() {
            return Ok(None);
        }
        match self
            .state
            .preferences
            .delegation
            .refusal(&self.state.sessions, asking)
        {
            Some(why) => Err(ErrorBody::new(ErrorCode::Conflict, why)),
            None => Ok(Some(asking)),
        }
    }

    /// [`add_session`](Self::add_session), for a plugin's panel opened
    /// beside a session, or an agent's started by another.
    pub(super) async fn add_session_tied(
        &mut self,
        workspace: WorkspaceId,
        harness: &Harness,
        title: Option<String>,
        start: Start,
        ties: Ties,
    ) -> Result<SessionId, ErrorBody> {
        let Ties { beside, started_by } = ties;
        let now = self.clock.now();
        let session = Session {
            id: SessionId::new(),
            workspace,
            harness: harness.id().to_owned(),
            title: title
                .map(|title| one_line(&title).trim().to_owned())
                .filter(|title| !title.is_empty())
                .unwrap_or_else(|| harness.label().to_owned()),
            status: if start == Start::WithPrompt {
                SessionStatus::Running
            } else {
                SessionStatus::Fresh
            },
            unseen: false,
            resume_id: None,
            created_at: now,
            ended_at: None,
            status_since: now,
            activity: None,
            spend: None,
            tidy: None,
            beside,
            started_by,
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
        self.session_added(id);
        if start == Start::WithPrompt {
            // Its first turn begins with it.
            self.turn_began(id);
        }
        Ok(id)
    }

    /// Where a new session in `workspace` runs: on the host in a worktree
    /// or checkout; in a run's container through docker, where Claude Code
    /// runs only for the person taking the run over.
    pub(super) fn place_for(
        &self,
        workspace: WorkspaceId,
        harness: &Harness,
    ) -> Result<Place, ErrorBody> {
        let found = self
            .state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .ok_or_else(|| {
                ErrorBody::new(ErrorCode::NotFound, format!("no workspace {workspace}"))
            })?;
        if self.worktree_jobs.is_removing(workspace) {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                "its worktree is being removed",
            ));
        }
        if let Some(path) = found.kind.host_path() {
            return Ok(Place::Host(path.to_owned()));
        }
        let Some(run) = found.kind.run() else {
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                "a workspace with no place",
            ));
        };
        if harness.is_agent() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!(
                    "{} runs in a run's container only for the person taking the run over \
                     (`muster run take-over`); a shell can look around",
                    harness.label()
                ),
            ));
        }
        if harness.is_checks() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                "a run checks its own work; hand it back to have it checked",
            ));
        }
        self.container_place(run, None)
    }

    /// Starts a session's program at `size` and begins tracking it. A
    /// program started again in a session numbers its output on from where
    /// the last one's ended, past a gap of one, so that no client's position
    /// from before is mistaken for one after and every client starts the
    /// new program on a clean screen. `unseen` starts the session with news
    /// for the person, unless someone is already watching it.
    #[expect(
        clippy::too_many_arguments,
        reason = "each of its callers chooses every one of these differently"
    )]
    pub(super) fn launch(
        &mut self,
        session: SessionId,
        harness: Harness,
        place: Place,
        size: TermSize,
        mode: Mode,
        start: Start,
        unseen: bool,
    ) -> std::io::Result<()> {
        let first_seq = self
            .live
            .get(&session)
            .and_then(|live| live.ended)
            .map_or(0, |(end, _)| end + 1);
        // Hooks reach the daemon only from the host; in a container the
        // status comes from the terminal and the process.
        let hooked = harness.has_hooks() && place.hears_hooks();
        let on_host = matches!(place, Place::Host(_));
        let (agent_session, resume, prompt, checks) = match mode {
            Mode::Fresh { prompt } => {
                let chosen = hooked && harness.chooses_session_id();
                let id = chosen.then(claude::new_session_id).transpose()?;
                (id, false, prompt, None)
            }
            Mode::Resume { agent_session } => (Some(agent_session), true, None, None),
            Mode::Checks(checks) => (None, false, None, Some(checks)),
        };
        let hooks = if hooked {
            Some(HookEndpoint {
                socket: self.hook_socket.clone(),
                token: crate::secrets::Token::new(harness::new_token()?),
            })
        } else {
            None
        };
        // The files of Muster's own it hears some harnesses through. If they
        // cannot be written the session still starts, with less status.
        let support = self.hook_socket.parent().map(|dir| dir.join("harness"));
        let support = support.filter(|dir| match harness.prepare_support(dir) {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, harness = harness.id(), "could not write the files its hooks need");
                false
            }
        });
        let spec = harness::spawn_spec(
            &harness,
            &harness::Launch {
                session,
                agent_session: agent_session.clone(),
                resume,
                prompt,
                hooks: hooks.clone(),
                place,
                size,
                checks,
                support: hooked.then_some(support).flatten(),
            },
        );
        let events = self.pty_events.clone();
        let pty = pty::spawn(&spec, first_seq, self.colours(), move |event| {
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
                reports_start: hooked && harness.reports_start(),
            },
            self.timing,
            self.clock.instant(),
        );
        // A program started again in the session keeps what came before,
        // and the person's name for it.
        let named_by_person = self
            .live
            .get(&session)
            .is_some_and(|live| live.named_by_person);
        let signals = self
            .live
            .get_mut(&session)
            .map(|live| std::mem::take(&mut live.signals))
            .unwrap_or_default();
        self.live.insert(
            session,
            Live {
                pty: pty.clone(),
                machine,
                harness,
                token,
                launched: self.clock.instant(),
                // In a container the program itself falls back to a fresh
                // conversation when it has none to resume.
                resumed: resume && on_host,
                ended: None,
                title: None,
                turn: None,
                signals,
                naming: None,
                named_by_person,
                last_reply: None,
            },
        );
        self.carry_watch(session);
        self.publish_terminal(session, pty, size);
        Ok(())
    }

    /// The status a just-launched session's machine starts in, as the person
    /// should see it.
    pub(super) fn launched_status(&self, session: SessionId) -> Option<(SessionStatus, bool)> {
        self.live
            .get(&session)
            .map(|live| (live.machine.status(), live.machine.unseen()))
    }

    /// Starts an ended session's program again in the same session. Claude
    /// Code resumes the conversation it had, if its transcript is still
    /// there; otherwise, and for a shell, the program starts afresh, and a
    /// plugin's panel is opened afresh (`restart_panel`). (The
    /// agent in a run's container is started again by
    /// [`restart_agent_in_container`](Self::restart_agent_in_container),
    /// once its credential is fetched.)
    pub(super) async fn restart_session(
        &mut self,
        session: SessionId,
        size: TermSize,
    ) -> Result<Reply, ErrorBody> {
        let stored = self.stored_session(session)?;
        if self.is_running(session) {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{} is still running", stored.title),
            ));
        }
        if stored.kind().is_panel() {
            return self.restart_panel(session, size).await;
        }
        let harness = self.known_harness(&stored.harness)?;
        let place = self.place_for(stored.workspace, &harness)?;
        let (mode, start) = match (&place, resumable(&harness, &stored)) {
            // The checks run again, as the workspace has them now.
            (Place::Host(path), _) if harness.is_checks() => (
                Mode::Checks(self.checks_in(stored.workspace, path).await?),
                Start::WithPrompt,
            ),
            (Place::Host(_), Some(agent_session)) => {
                // A conversation is resumed in one session only.
                if let Some(holder) = self.conversation_holder(&harness, &agent_session, session) {
                    let holder = self.stored_session(holder)?.title;
                    return Err(ErrorBody::new(
                        ErrorCode::Conflict,
                        format!(
                            "{holder} has this conversation open; stop it there to resume it here"
                        ),
                    ));
                }
                (Mode::Resume { agent_session }, Start::Resumed)
            }
            _ => (Mode::Fresh { prompt: None }, Start::Idle),
        };
        self.reopen_and_launch(session, harness, place, size, mode, start)
            .await?;
        Ok(Reply::SessionStarted { id: session })
    }

    /// The stored session `session`, if there is one.
    pub(super) fn stored_session(&self, session: SessionId) -> Result<Session, ErrorBody> {
        self.state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .cloned()
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no session {session}")))
    }

    /// Opens an ended session again and starts its program, telling
    /// clients.
    pub(super) async fn reopen_and_launch(
        &mut self,
        session: SessionId,
        harness: Harness,
        place: Place,
        size: TermSize,
        mode: Mode,
        start: Start,
    ) -> Result<(), ErrorBody> {
        // The session is open again before its program starts (`ARC-10`).
        self.store
            .call(move |store| store.reopen_session(session))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        if let Some(reopened) = self.session_mut(session) {
            reopened.ended_at = None;
        }
        let label = harness.label().to_owned();
        if let Err(error) = self.launch(session, harness, place, size, mode, start, false) {
            self.end_session(session, SessionStatus::Exited { success: false }, true)
                .await;
            return Err(ErrorBody::new(
                ErrorCode::Internal,
                format!("could not start {label}: {error}"),
            ));
        }
        tracing::info!(%session, "restarted");
        if let Some((status, unseen)) = self.launched_status(session) {
            self.set_status(session, status, unseen).await;
        }
        if let Some(reopened) = self.session_mut(session) {
            let reopened = reopened.clone();
            self.broadcast(Delta::SessionUpserted(reopened));
        }
        Ok(())
    }

    /// Takes an ended session off the list, keeping its history.
    pub(super) async fn remove_session(&mut self, session: SessionId) -> Result<Reply, ErrorBody> {
        let title = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .map(|stored| stored.title.clone())
            .ok_or_else(|| ErrorBody::new(ErrorCode::NotFound, format!("no session {session}")))?;
        if self.is_running(session) {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{title} is still running; stop it first"),
            ));
        }
        let at = self.clock.now();
        self.store
            .call(move |store| store.remove_session(session, at))
            .await
            .map_err(store_closed)?
            .map_err(store_error)?;
        self.release_terminal(session);
        self.state.sessions.retain(|stored| stored.id != session);
        self.broadcast(Delta::SessionRemoved { id: session });
        Ok(Reply::Done)
    }

    /// Whether the session's program is running, or waits its turn to be
    /// started again after a restart.
    pub(super) fn is_running(&self, session: SessionId) -> bool {
        self.resumes.is_waiting(session)
            || self
                .live
                .get(&session)
                .is_some_and(|live| !live.machine.status().is_exited())
    }

    pub(super) async fn stop_session(&mut self, session: SessionId) -> Result<Reply, ErrorBody> {
        if self.stop_waiting(session).await {
            return Ok(Reply::Done);
        }
        let live = self.live.get(&session).ok_or_else(|| {
            ErrorBody::new(ErrorCode::NotFound, format!("{session} is not running"))
        })?;
        if live.machine.status().is_exited() {
            return Err(ErrorBody::new(
                ErrorCode::Conflict,
                format!("{session} has already exited"),
            ));
        }
        let pty = live.pty.clone();
        self.stop_in_container(session);
        pty.stop().await;
        Ok(Reply::Done)
    }

    /// A session's program set its window title. For a harness whose title
    /// says how it is doing, a change of what it says is a signal: working
    /// or ready as the progress sequence would say it, and, for one whose
    /// title is its only word of waiting on the person, the wait and its
    /// end; its first title also says it has started.
    async fn on_title(&mut self, session: SessionId, title: &str) {
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        let Some(state) = live.harness.read_title(title) else {
            return;
        };
        let before = live.title.replace(state);
        if before == Some(state) {
            return;
        }
        let waiting = state == TitleState::Waiting;
        let mut signals = Vec::new();
        if live.harness.title_reports_waits()
            && (before.is_none() || (before == Some(TitleState::Waiting)) != waiting)
        {
            signals.push(Signal::Waiting { waiting });
        }
        match state {
            TitleState::Working => signals.push(Signal::Progress { busy: true }),
            TitleState::Ready => signals.push(Signal::Progress { busy: false }),
            TitleState::Waiting => {}
        }
        for signal in signals {
            self.step_session(session, signal).await;
        }
    }

    pub(super) async fn on_pty_event(&mut self, session: SessionId, event: PtyEvent) {
        match event {
            PtyEvent::Progress { busy } => {
                self.step_session(session, Signal::Progress { busy }).await;
            }
            PtyEvent::Title(title) => self.on_title(session, &title).await,
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
                    live.resumed
                        && !requested
                        && self
                            .clock
                            .instant()
                            .saturating_duration_since(live.launched)
                            < RESUME_GRACE
                });
                if failed_resume && self.start_fresh_after_failed_resume(session).await {
                    return;
                }
                self.events.ending(session, requested);
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
                self.reopen_panel(session, size).await;
            }
        }
    }

    /// A resumed program exited at once: start the harness fresh in the same
    /// session rather than leave a dead terminal. `false` if that failed too.
    async fn start_fresh_after_failed_resume(&mut self, session: SessionId) -> bool {
        let Some(harness) = self.live.get(&session).map(|live| live.harness.clone()) else {
            return false;
        };
        let Some(cwd) = self.session_path(session) else {
            return false;
        };
        tracing::warn!(%session, "resume failed; starting the session fresh");
        // In the size the failed program's terminal had: its clients are
        // still looking at it, and will not say again.
        let size = self
            .live
            .get(&session)
            .and_then(|live| live.ended)
            .map_or_else(TermSize::default, |(_, size)| size);
        let mode = Mode::Fresh { prompt: None };
        let place = Place::Host(cwd);
        match self.launch(session, harness, place, size, mode, Start::Idle, true) {
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

    /// Records that a session ended without its terminal reporting it.
    pub(super) async fn end_session(
        &mut self,
        session: SessionId,
        status: SessionStatus,
        unseen: bool,
    ) {
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
    pub(super) fn forget_token(&mut self, session: SessionId) {
        if let Some(token) = self
            .live
            .get_mut(&session)
            .and_then(|live| live.token.take())
        {
            self.hook_tokens.remove(&token);
        }
    }

    /// Where a workspace is on the host; a run's container has no such place.
    pub(super) fn workspace_path(&self, workspace: WorkspaceId) -> Option<PathBuf> {
        self.state
            .workspaces
            .iter()
            .find(|candidate| candidate.id == workspace)
            .and_then(|found| found.kind.host_path().map(std::path::Path::to_owned))
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

    /// The sessions running in `workspace`, each with whether it ends
    /// with the workspace, as a shell or a plugin's panel does.
    pub(super) fn running_in(&self, workspace: WorkspaceId) -> Vec<(SessionId, bool)> {
        self.state
            .sessions
            .iter()
            .filter(|session| session.workspace == workspace)
            .filter(|session| self.is_running(session.id))
            .map(|session| {
                let ends = session.kind().ends_with_workspace();
                (session.id, ends)
            })
            .collect()
    }

    /// How many of a workspace's sessions are still running.
    pub(super) fn running_sessions_in(&self, workspace: WorkspaceId) -> usize {
        self.state
            .sessions
            .iter()
            .filter(|session| session.workspace == workspace)
            .filter(|session| self.is_running(session.id))
            .count()
    }
}

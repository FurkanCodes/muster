//! What runs in front in each terminal (`PRODUCT.md` §6, `ARC-28`), as
//! the watcher finds it (`crate::fronts`): a terminal is called by the
//! program in its foreground, `zsh` at its prompt, `cargo` or `aider`
//! while one runs, unless the person named it; and while a coding agent
//! runs there, the session is that agent's chat: its logo, its status
//! (a machine of its own, fed what the agent's terminal shows) and its
//! place among the agents, until it leaves and the terminal is a
//! terminal again.

use muster_core::front::{AGENT_VARIABLE, Front};
use muster_core::harness::SessionKind;
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, TermSize};
use muster_core::{AgentNow, SessionId};
use muster_state::session::{self, SessionMachine, Start};

use super::Supervisor;
use crate::harness::{Harness, HookEndpoint, Place, claude};
use crate::pty::PtyHandle;

/// What the watcher looks at: the running terminals, and the agents the
/// person added, each `(name, program)`, to know them by.
#[derive(Debug)]
pub(crate) struct Terminals {
    pub(crate) list: Vec<(SessionId, PtyHandle)>,
    pub(crate) added: Vec<(String, String)>,
}

impl Supervisor {
    /// The running terminals, for the watcher to ask what runs in each.
    pub(super) fn terminals(&self) -> Terminals {
        let list = self
            .live
            .iter()
            .filter(|(_, live)| SessionKind::of(live.harness.id()).named_by_foreground())
            .filter(|(_, live)| !live.machine.status().is_exited())
            .map(|(id, live)| (*id, live.pty.clone()))
            .collect();
        let added = self
            .agents
            .iter()
            .map(|agent| (agent.name.clone(), agent.program.clone()))
            .collect();
        Terminals { list, added }
    }

    /// What runs in front in terminals now, where it changed.
    pub(super) async fn fronts(&mut self, fronts: Vec<(SessionId, Front)>) {
        for (session, front) in fronts {
            // An agent still in front keeps the name it has (its chat's,
            // from its first prompt): only one arriving is named by it.
            let changed = self.agent_in_front(session, front.agent.as_deref()).await;
            if changed || front.agent.is_none() {
                self.name_by_front(session, &front.program).await;
            }
        }
    }

    /// Calls `session` by `program`, unless the person named it.
    async fn name_by_front(&mut self, session: SessionId, program: &str) {
        let program = muster_core::text::one_line(program).trim().to_owned();
        if program.is_empty() {
            return;
        }
        let ours = self
            .live
            .get(&session)
            .is_some_and(|live| !live.named_by_person);
        let renamed = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .is_some_and(|stored| stored.title != program);
        if ours
            && renamed
            && let Err(error) = self.name_session(session, program).await
        {
            tracing::warn!(%error, "could not name a terminal by what runs in it");
        }
    }

    /// `agent` (a harness's id) runs in front in `session` now, or none
    /// does, as the watcher found. Arriving or leaving, the session's
    /// status starts again: the agent's from its prompt, or the
    /// terminal's, which keeps news the agent left unseen.
    /// Whether the agent in front changed.
    async fn agent_in_front(&mut self, session: SessionId, agent: Option<&str>) -> bool {
        let harness = agent
            .and_then(|id| self.harness(id))
            .filter(Harness::is_agent);
        let Some(live) = self.live.get(&session) else {
            return false;
        };
        if live.agent.as_ref().map(Harness::id) == harness.as_ref().map(Harness::id) {
            return false;
        }
        let unseen = harness.is_none() && live.machine.unseen();
        let launch = session::Launch {
            unseen,
            ..session::Launch::default()
        };
        self.put_agent(session, harness, launch).await;
        true
    }

    /// The agent `muster exec` ran in `session` ended: the terminal is a
    /// terminal again at once, its ending news unless the person watched.
    pub(super) async fn agent_ended(&mut self, session: SessionId, success: bool) {
        let Some(live) = self.live.get(&session) else {
            return;
        };
        if live.agent.is_none() {
            return;
        }
        let unseen = live.machine.unseen() || !success || !self.is_watched(session);
        let launch = session::Launch {
            unseen,
            ..session::Launch::default()
        };
        self.put_agent(session, None, launch).await;
    }

    /// Puts `harness` in front in terminal `session`, or no agent, its
    /// status starting again as `launch` says, and tells clients.
    pub(super) async fn put_agent(
        &mut self,
        session: SessionId,
        harness: Option<Harness>,
        launch: session::Launch,
    ) {
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        if live.machine.status().is_exited() {
            return;
        }
        live.agent.clone_from(&harness);
        live.title = None;
        live.turn = None;
        live.machine = SessionMachine::new(launch, self.timing, self.clock.instant());
        self.carry_watch(session);
        let since = self.clock.now();
        if let Some(stored) = self.session_mut(session) {
            stored.agent = harness.map(|harness| AgentNow {
                harness: harness.id().to_owned(),
                since,
            });
            let stored = stored.clone();
            self.broadcast(Delta::SessionUpserted(stored));
        }
        if let Some((status, unseen)) = self.launched_status(session) {
            self.set_status(session, status, unseen).await;
        }
    }

    /// Runs coding agent `harness` in terminal `session`'s foreground, as
    /// `muster exec` asks from inside it: with the terminal's hooks and a
    /// session id of Muster's choosing, heard from now as the terminal's
    /// agent. The command line it is run with goes back to the asker.
    pub(super) async fn exec_agent(
        &mut self,
        session: SessionId,
        harness: &str,
        prompt: Option<String>,
    ) -> Result<Reply, ErrorBody> {
        let harness = self.known_harness(harness)?;
        if !harness.is_agent() || harness.program().is_none() {
            return Err(ErrorBody::new(
                ErrorCode::BadRequest,
                format!("{} is no coding agent", harness.id()),
            ));
        }
        let token = self
            .live
            .get(&session)
            .filter(|live| SessionKind::of(live.harness.id()).is_shell())
            .filter(|live| !live.machine.status().is_exited())
            .map(|live| live.token.clone())
            .ok_or_else(|| {
                ErrorBody::new(
                    ErrorCode::NotFound,
                    format!("{session} is no terminal of Muster's that runs"),
                )
            })?;
        let cwd = self.session_path(session).ok_or_else(|| {
            ErrorBody::new(ErrorCode::NotFound, format!("{session} has no folder"))
        })?;
        let hooks = token
            .filter(|_| harness.has_hooks())
            .map(|token| HookEndpoint {
                socket: self.hook_socket.clone(),
                token,
            });
        let agent_session = (hooks.is_some() && harness.chooses_session_id())
            .then(claude::new_session_id)
            .transpose()
            .map_err(|error| ErrorBody::new(ErrorCode::Internal, error.to_string()))?;
        let support = self.support_for(&harness, hooks.is_some());
        let spec = crate::harness::spawn_spec(
            &harness,
            &crate::harness::Launch {
                session,
                agent_session: agent_session.clone(),
                resume: false,
                prompt: prompt.clone(),
                hooks: hooks.clone(),
                place: Place::Host(cwd),
                size: TermSize::default(),
                checks: None,
                support,
                ssh_agent: self.ssh_agent(true),
            },
        );
        let launch = session::Launch {
            agent_session,
            start: if prompt.is_some() {
                Start::WithPrompt
            } else {
                Start::Idle
            },
            unseen: false,
            reports_start: hooks.is_some() && harness.reports_start(),
        };
        let id = harness.id().to_owned();
        self.put_agent(session, Some(harness), launch).await;
        // Named by its program until its first prompt names it.
        if let Some(program) = spawn_program_name(&spec.program) {
            self.name_by_front(session, &program).await;
        }
        if let Some(prompt) = &prompt {
            self.name_from_prompt(session, prompt).await;
        }
        let mut env = spec.env;
        env.push((AGENT_VARIABLE.to_owned(), id));
        Ok(Reply::AgentCommand {
            program: spec.program,
            args: spec.args,
            env,
        })
    }
}

/// What a program at `path` is called by in a terminal: its file's name.
fn spawn_program_name(path: &str) -> Option<String> {
    std::path::Path::new(path)
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
}

//! What runs in front in each terminal (`PRODUCT.md` §6, `ARC-28`), as
//! the watcher finds it (`crate::fronts`): a terminal is called by the
//! program in its foreground, `zsh` at its prompt, `cargo` or `aider`
//! while one runs, unless the person named it; and while a coding agent
//! runs there, the session is that agent's chat: its logo, its status
//! (a machine of its own, fed what the agent's terminal shows) and its
//! place among the agents, until it leaves and the terminal is a
//! terminal again.

use muster_core::front::Front;
use muster_core::harness::SessionKind;
use muster_core::protocol::Delta;
use muster_core::{AgentNow, SessionId};
use muster_state::session::{self, SessionMachine};

use super::Supervisor;
use crate::harness::Harness;
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
            self.name_by_front(session, &front.program).await;
            self.agent_in_front(session, front.agent.as_deref()).await;
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
    /// does. Arriving or leaving, the session's status starts again: the
    /// agent's from its prompt, or the terminal's, which keeps news the
    /// agent left unseen.
    async fn agent_in_front(&mut self, session: SessionId, agent: Option<&str>) {
        let harness = agent
            .and_then(|id| self.harness(id))
            .filter(Harness::is_agent);
        let Some(live) = self.live.get_mut(&session) else {
            return;
        };
        if live.agent.as_ref().map(Harness::id) == harness.as_ref().map(Harness::id)
            || live.machine.status().is_exited()
        {
            return;
        }
        let unseen = harness.is_none() && live.machine.unseen();
        live.agent.clone_from(&harness);
        live.title = None;
        live.machine = SessionMachine::new(
            session::Launch {
                unseen,
                ..session::Launch::default()
            },
            self.timing,
            self.clock.instant(),
        );
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
}

//! Shells named by what runs in them (`PRODUCT.md` §6): a shell session is
//! called by the program in its terminal's foreground, `zsh` at its
//! prompt, `bun` or `cargo` while one runs, as the sampler finds it every
//! few seconds (`resources`). A name the person gives it stays.

use muster_core::SessionId;

use super::Supervisor;
use crate::pty::PtyHandle;

impl Supervisor {
    /// The running shells and their terminals, for the sampler to ask what
    /// runs in each.
    pub(super) fn shells(&self) -> Vec<(SessionId, PtyHandle)> {
        self.live
            .iter()
            .filter(|(_, live)| live.harness.id() == "shell")
            .filter(|(_, live)| !live.machine.status().is_exited())
            .map(|(id, live)| (*id, live.pty.clone()))
            .collect()
    }

    /// What runs in each shell now: each is called by it, unless the
    /// person named it.
    pub(super) async fn shell_programs(&mut self, programs: Vec<(SessionId, String)>) {
        for (session, program) in programs {
            // A login shell's name starts with a dash.
            let program = muster_core::text::one_line(program.trim_start_matches('-'))
                .trim()
                .to_owned();
            if program.is_empty() {
                continue;
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
                tracing::warn!(%error, "could not name a shell by what runs in it");
            }
        }
    }
}

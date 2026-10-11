//! Which terminal each session has now, and who is watching it.
//!
//! A session keeps its id when its program is started again (a resume that
//! failed and started fresh, or a restart), but gets a new terminal. Clients
//! follow it through a [`watch`] channel that always holds the session's
//! current terminal, so their streams and keystrokes move to the new program
//! on their own. The channel closes when the session is released.
//!
//! Watching is counted per session: while anyone watches, nothing that
//! happens in the session is news to the person (`Signal::Watched`). Each
//! watcher holds a [`WatchGuard`]; dropping it, however the watcher ends,
//! stops the watch.

use std::time::Instant;

use muster_core::SessionId;
use muster_state::session::Signal;
use tokio::sync::{mpsc, watch};

use super::Supervisor;
use crate::pty::PtyHandle;

/// A session's terminal as a client holds it.
#[derive(Debug)]
pub(crate) struct TerminalLink {
    /// The session's current terminal; `None` or closed once it is released.
    pub(crate) current: watch::Receiver<Option<PtyHandle>>,
    /// Held while the client watches the session.
    pub(crate) watching: Option<WatchGuard>,
}

/// Counts one watcher of a session until dropped.
#[derive(Debug)]
pub(crate) struct WatchGuard {
    session: SessionId,
    unwatched: mpsc::UnboundedSender<SessionId>,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        // The Supervisor has stopped if this fails; nothing is left to count.
        let _ = self.unwatched.send(self.session);
    }
}

impl Supervisor {
    /// Makes `pty` the session's current terminal, moving its clients to it.
    pub(super) fn publish_terminal(&mut self, session: SessionId, pty: PtyHandle) {
        match self.terminals.get(&session) {
            Some(current) => {
                current.send_replace(Some(pty));
            }
            None => {
                self.terminals.insert(session, watch::channel(Some(pty)).0);
            }
        }
    }

    /// Lets go of the session's terminal: its hooks are no longer accepted,
    /// its clients are told it is gone, and its output is freed once they
    /// have let go too.
    pub(super) fn release_terminal(&mut self, session: SessionId) {
        self.forget_token(session);
        self.live.remove(&session);
        // Dropping the sender closes every client's link.
        self.terminals.remove(&session);
    }

    /// A link to the session's terminal, if it has one. With `watch`, the
    /// link counts as watching the session until it is dropped.
    pub(super) async fn link_terminal(
        &mut self,
        session: SessionId,
        watch: bool,
    ) -> Option<TerminalLink> {
        let current = self.terminals.get(&session)?.subscribe();
        let watching = if watch {
            self.watch(session).await;
            Some(WatchGuard {
                session,
                unwatched: self.unwatched.clone(),
            })
        } else {
            None
        };
        Some(TerminalLink { current, watching })
    }

    async fn watch(&mut self, session: SessionId) {
        let watchers = self.watchers.entry(session).or_insert(0);
        *watchers += 1;
        if let Some(live) = self.live.get(&session)
            && live.harness.suspends_itself()
        {
            // Looking at a suspended agent brings it back.
            live.pty.resume_if_suspended().await;
        }
        if *self.watchers.get(&session).unwrap_or(&0) == 1 {
            self.step_session(session, Signal::Watched { watching: true })
                .await;
        }
    }

    /// One watcher of `session` stopped watching.
    pub(super) async fn unwatch(&mut self, session: SessionId) {
        let Some(watchers) = self.watchers.get_mut(&session) else {
            return;
        };
        *watchers = watchers.saturating_sub(1);
        if *watchers == 0 {
            self.watchers.remove(&session);
            self.step_session(session, Signal::Watched { watching: false })
                .await;
        }
    }

    /// Whether anyone is watching `session`.
    pub(super) fn is_watched(&self, session: SessionId) -> bool {
        self.watchers.contains_key(&session)
    }

    /// Tells a newly started machine that its session is being watched, so
    /// it starts seen. Its status is published by whoever started it.
    pub(super) fn carry_watch(&mut self, session: SessionId) {
        if self.is_watched(session)
            && let Some(live) = self.live.get_mut(&session)
        {
            live.machine
                .step(Signal::Watched { watching: true }, Instant::now());
        }
    }
}

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
//! stops the watch. A connection holds its watches only while its client
//! says the person is looking at it, as its
//! [`ClientGuard`](super::ClientGuard) keeps (`server`), so a session is
//! watched only while a window showing it is looked at. A client that
//! links to a terminal is counted among its clients, whose sizes it takes
//! (`sizing`).
//!
//! Focus is counted the same way: while a client shows a session's
//! terminal (beside the list or filling the window, in a window the
//! person looks at; the guards keep their first name, the keys), its
//! program is in focus, and it is told so as that starts and
//! ends (`CSI I`, `CSI O`) if it asked; but Claude Code is never told it is
//! out of focus (`Harness::stays_in_focus`). Each such client holds a
//! [`KeysGuard`], beside its connection's
//! [`ClientGuard`](super::ClientGuard).
//!
//! Every terminal answers its program's questions about its colours from
//! the person's terminal's, the last a UI told: a terminal reads them as
//! it is asked, so one told later answers with them too.

use muster_core::SessionId;
use muster_core::colours::TerminalColours;
use muster_core::protocol::TermSize;
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
    /// The client active on the session last (`sizing`).
    pub(crate) latest: watch::Receiver<u64>,
}

/// Counts one watcher of a session until dropped.
#[derive(Debug)]
pub(crate) struct WatchGuard {
    session: SessionId,
    /// The connection watching.
    client: u64,
    unwatched: mpsc::UnboundedSender<(SessionId, u64)>,
}

impl Drop for WatchGuard {
    fn drop(&mut self) {
        // The Supervisor has stopped if this fails; nothing is left to count.
        let _ = self.unwatched.send((self.session, self.client));
    }
}

/// Counts one client with the person's keys in a session's terminal until
/// dropped.
#[derive(Debug)]
pub(crate) struct KeysGuard {
    session: SessionId,
    gone: mpsc::UnboundedSender<SessionId>,
}

impl Drop for KeysGuard {
    fn drop(&mut self) {
        // The Supervisor has stopped if this fails; nothing is left to count.
        let _ = self.gone.send(self.session);
    }
}

impl Supervisor {
    /// The person's terminal's colours, as a UI found them. A UI whose
    /// terminal said none changes nothing, and a palette is kept to the
    /// colours that are asked for.
    pub(super) fn set_colours(&mut self, mut colours: TerminalColours) {
        if colours.is_empty() {
            return;
        }
        colours.palette.truncate(muster_core::colours::PALETTE);
        self.colours.send_replace(colours);
    }

    /// What a new terminal answers its program's questions about its
    /// colours from.
    pub(super) fn colours(&self) -> crate::pty::Colours {
        self.colours.subscribe()
    }

    /// Makes `pty`, started at `size`, the session's current terminal,
    /// moving its clients to it; it takes the size of the client in charge
    /// of the session.
    pub(super) fn publish_terminal(&mut self, session: SessionId, pty: PtyHandle, size: TermSize) {
        match self.terminals.get(&session) {
            Some(current) => {
                current.send_replace(Some(pty));
            }
            None => {
                self.terminals.insert(session, watch::channel(Some(pty)).0);
            }
        }
        self.fit_new_terminal(session, size);
    }

    /// Lets go of the session's terminal: its hooks are no longer accepted,
    /// its clients are told it is gone, and its output is freed once they
    /// have let go too.
    pub(super) fn release_terminal(&mut self, session: SessionId) {
        self.forget_token(session);
        self.live.remove(&session);
        self.sizing.released(session);
        // A closed channel keeps its last value for receivers still held,
        // which would keep the terminal's output in memory; empty it, then
        // close it, which tells every client's stream it is gone.
        if let Some(current) = self.terminals.remove(&session) {
            current.send_replace(None);
        }
    }

    /// A link to the session's terminal for `client`, if it has one,
    /// counting the client among the terminal's. With `watch`, the link
    /// counts as watching the session until it is dropped.
    pub(super) async fn link_terminal(
        &mut self,
        session: SessionId,
        watch: bool,
        client: u64,
    ) -> Option<TerminalLink> {
        let Some(current) = self.terminals.get(&session).map(watch::Sender::subscribe) else {
            if watch {
                self.seen_without_terminal(session).await;
            }
            return None;
        };
        let latest = self.sizing.join(session, client);
        let watching = if watch {
            self.watch(session, client).await;
            Some(WatchGuard {
                session,
                client,
                unwatched: self.unwatched.clone(),
            })
        } else {
            None
        };
        Some(TerminalLink {
            current,
            watching,
            latest,
        })
    }

    async fn watch(&mut self, session: SessionId, client: u64) {
        // Beginning to watch is being active, so its size may now apply.
        self.sizing.watching(session, client, true);
        self.fit(session).await;
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

    /// One watch of `session` by `client` ended.
    pub(super) async fn unwatch(&mut self, session: SessionId, client: u64) {
        self.sizing.watching(session, client, false);
        self.fit(session).await;
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

    /// A session whose terminal is no longer held (it ended with an earlier
    /// daemon) has nothing to watch, but being looked at still clears its
    /// news.
    async fn seen_without_terminal(&mut self, session: SessionId) {
        let unseen = self
            .state
            .sessions
            .iter()
            .find(|stored| stored.id == session)
            .filter(|stored| stored.unseen)
            .map(|stored| stored.status);
        if let Some(status) = unseen {
            self.set_status(session, status, false).await;
        }
    }

    /// A client has the person's keys in `session`'s terminal: the first
    /// tells its program it is in focus.
    pub(super) async fn hold_keys(&mut self, session: SessionId) -> Option<KeysGuard> {
        if !self
            .state
            .sessions
            .iter()
            .any(|stored| stored.id == session)
        {
            return None;
        }
        let typing = self.typing.entry(session).or_insert(0);
        *typing += 1;
        if *typing == 1
            && let Some(live) = self.live.get(&session)
        {
            live.pty.focus(true).await;
        }
        Some(KeysGuard {
            session,
            gone: self.keys_gone.clone(),
        })
    }

    /// One client let go of the person's keys in `session`'s terminal: the
    /// last tells its program it is out of focus, unless it stays in focus.
    pub(super) async fn let_go_keys(&mut self, session: SessionId) {
        let Some(typing) = self.typing.get_mut(&session) else {
            return;
        };
        *typing = typing.saturating_sub(1);
        if *typing == 0 {
            self.typing.remove(&session);
            if let Some(live) = self.live.get(&session)
                && !live.harness.stays_in_focus()
            {
                live.pty.focus(false).await;
            }
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
                .step(Signal::Watched { watching: true }, self.clock.instant());
        }
    }
}

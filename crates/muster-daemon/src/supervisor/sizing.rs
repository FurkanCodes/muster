//! Which client's size each session's terminal takes (`ARC-02`).
//!
//! A program draws for one size, so clients showing a session at different
//! sizes cannot each give its terminal theirs: two windows would resize it
//! back and forth. The Supervisor keeps every client's size for each
//! session and gives the terminal that of the client most recently active
//! on it: the one that last typed into it, resized it, or began to watch
//! it. A client watching the session comes before one that is not, and
//! among those one whose window the person is looking at
//! (`supervisor::looking`), so a window out of focus never takes the size
//! from one in front of the person. The other clients' sizes are kept, not
//! applied; when the client in charge lets the session go or stops
//! watching it, the next one's size applies.
//!
//! A size is in cells and, where the person's terminal says, in pixels
//! too (`Resize::pixels`). The pixels are kept per client with its cells
//! and travel with them: the terminal takes the pixels of the client in
//! charge, or none if that client gave none, never one client's cells
//! with another's pixels.
//!
//! What is kept for a client leaves with it: when it lets the session go,
//! when its connection ends ([`ClientGuard`](super::ClientGuard)), and with
//! the session's terminal (`ARC-14`).

use std::collections::HashMap;

use muster_core::SessionId;
use muster_core::protocol::{PixelSize, TermSize};
use tokio::sync::watch;

use super::Supervisor;

/// A size a session's terminal is shown at: cells, and pixels where known.
type Shown = (TermSize, Option<PixelSize>);

/// The sizes clients gave each session's terminal.
#[derive(Debug, Default)]
pub(super) struct Sizing {
    sessions: HashMap<SessionId, Sizers>,
    /// Counts each time a client was active, so the latest is known.
    stamps: u64,
}

/// The clients of one session's terminal.
#[derive(Debug)]
struct Sizers {
    clients: HashMap<u64, Sizer>,
    /// The client active on the session last. A connection reads it to
    /// tell the Supervisor of a keystroke only when another client was
    /// active since, so typing never waits on the Supervisor.
    latest: watch::Sender<u64>,
    /// The size the terminal has, as far as it was told.
    applied: Option<Shown>,
}

/// One client of a session's terminal.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Sizer {
    /// The size it shows the session at, once it has said.
    size: Option<TermSize>,
    /// That size in pixels, where its terminal says.
    pixels: Option<PixelSize>,
    /// How many of its watches of the session are held: two for a moment
    /// while it attaches again.
    watching: usize,
    /// When it was last active, in [`Sizing::stamps`]; 0 if never.
    active: u64,
}

impl Sizing {
    /// Counts `client` among the session's clients, if it is not already;
    /// the latest client active on the session follows.
    pub(super) fn join(&mut self, session: SessionId, client: u64) -> watch::Receiver<u64> {
        let sizers = self.sessions.entry(session).or_insert_with(|| Sizers {
            clients: HashMap::new(),
            latest: watch::channel(0).0,
            applied: None,
        });
        sizers.clients.entry(client).or_default();
        sizers.latest.subscribe()
    }

    /// The client shows the session at `size`, and `pixels` where it
    /// knows them, which is being active.
    pub(super) fn resized(
        &mut self,
        session: SessionId,
        client: u64,
        size: TermSize,
        pixels: Option<PixelSize>,
    ) {
        if let Some(sizer) = self.sizer(session, client) {
            sizer.size = Some(size);
            sizer.pixels = pixels;
        }
        self.active(session, client);
    }

    /// The client typed into the session, resized it or began to watch it.
    pub(super) fn active(&mut self, session: SessionId, client: u64) {
        self.stamps += 1;
        let stamp = self.stamps;
        let Some(sizers) = self.sessions.get_mut(&session) else {
            return;
        };
        if let Some(sizer) = sizers.clients.get_mut(&client) {
            sizer.active = stamp;
            sizers.latest.send_replace(client);
        }
    }

    /// The client began or stopped one watch of the session; beginning is
    /// being active.
    pub(super) fn watching(&mut self, session: SessionId, client: u64, watching: bool) {
        let Some(sizer) = self.sizer(session, client) else {
            return;
        };
        if watching {
            sizer.watching += 1;
            self.active(session, client);
        } else {
            sizer.watching = sizer.watching.saturating_sub(1);
        }
    }

    /// The client let the session go.
    pub(super) fn leave(&mut self, session: SessionId, client: u64) {
        if let Some(sizers) = self.sessions.get_mut(&session) {
            sizers.clients.remove(&client);
            if sizers.clients.is_empty() {
                self.sessions.remove(&session);
            }
        }
    }

    /// The client's connection ended; returns the sessions it was a client
    /// of, whose size may now change.
    pub(super) fn gone(&mut self, client: u64) -> Vec<SessionId> {
        let sessions = self.sessions_of(client);
        for session in &sessions {
            self.leave(*session, client);
        }
        sessions
    }

    /// The clients watching the session now: those showing it to the
    /// person.
    pub(super) fn watchers(&self, session: SessionId) -> Vec<u64> {
        self.sessions.get(&session).map_or_else(Vec::new, |sizers| {
            sizers
                .clients
                .iter()
                .filter(|(_, sizer)| sizer.watching > 0)
                .map(|(client, _)| *client)
                .collect()
        })
    }

    /// The sessions `client` is a client of.
    pub(super) fn sessions_of(&self, client: u64) -> Vec<SessionId> {
        self.sessions
            .iter()
            .filter(|(_, sizers)| sizers.clients.contains_key(&client))
            .map(|(session, _)| *session)
            .collect()
    }

    /// The session has a new terminal, started at `size`, with no pixels
    /// known.
    pub(super) fn published(&mut self, session: SessionId, size: TermSize) {
        if let Some(sizers) = self.sessions.get_mut(&session) {
            sizers.applied = Some((size, None));
        }
    }

    /// The session's terminal was released: its clients are let go.
    pub(super) fn released(&mut self, session: SessionId) {
        self.sessions.remove(&session);
    }

    /// The size the session's terminal is to take, with its pixels, if it
    /// is to change; taken as applied. `looked_at` says whether the person
    /// is looking at a client's window.
    pub(super) fn due(
        &mut self,
        session: SessionId,
        looked_at: impl Fn(u64) -> bool,
    ) -> Option<Shown> {
        let sizers = self.sessions.get_mut(&session)?;
        let chosen = sizers
            .clients
            .iter()
            .filter_map(|(client, sizer)| {
                let rank = (
                    sizer.watching > 0,
                    looked_at(*client),
                    sizer.active,
                    *client,
                );
                Some(((sizer.size?, sizer.pixels), rank))
            })
            .max_by_key(|(_, rank)| *rank)
            .map(|(shown, _)| shown)?;
        if sizers.applied == Some(chosen) {
            return None;
        }
        sizers.applied = Some(chosen);
        Some(chosen)
    }

    fn sizer(&mut self, session: SessionId, client: u64) -> Option<&mut Sizer> {
        self.sessions.get_mut(&session)?.clients.get_mut(&client)
    }

    /// How many sessions have clients, for tests of `ARC-14`.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.sessions.len()
    }
}

impl Supervisor {
    /// The client shows the session at `size`, and `pixels` where it
    /// knows them.
    pub(super) async fn resize_for(
        &mut self,
        session: SessionId,
        client: u64,
        size: TermSize,
        pixels: Option<PixelSize>,
    ) {
        self.clients.sizing.resized(session, client, size, pixels);
        self.fit(session).await;
    }

    /// The client typed into the session after another was active on it.
    pub(super) async fn active_on(&mut self, session: SessionId, client: u64) {
        self.clients.sizing.active(session, client);
        self.fit(session).await;
    }

    /// The client let the session's terminal go.
    pub(super) async fn leave_terminal(&mut self, session: SessionId, client: u64) {
        self.clients.sizing.leave(session, client);
        self.fit(session).await;
    }

    /// The person began or stopped looking at the client's window, which
    /// changes which client's size its sessions take; beginning is being
    /// active on each.
    pub(super) async fn looked_changed(&mut self, client: u64, looking: bool) {
        for session in self.clients.sizing.sessions_of(client) {
            if looking {
                self.clients.sizing.active(session, client);
            }
            self.fit(session).await;
        }
    }

    /// Gives the session's terminal the size of the client in charge of
    /// it, pixels and all, if that changed.
    pub(super) async fn fit(&mut self, session: SessionId) {
        let lookers = &self.clients.lookers;
        let Some((size, pixels)) = self
            .clients
            .sizing
            .due(session, |client| lookers.is_looking(client))
        else {
            return;
        };
        if let Some(live) = self.live.get(&session) {
            live.pty.resize(size, pixels).await;
        }
    }

    /// Gives a terminal just started or taken over the size of the client
    /// in charge of the session, if it was started at another.
    pub(super) fn fit_new_terminal(&mut self, session: SessionId, size: TermSize) {
        self.clients.sizing.published(session, size);
        let lookers = &self.clients.lookers;
        if let Some((size, pixels)) = self
            .clients
            .sizing
            .due(session, |client| lookers.is_looking(client))
            && let Some(live) = self.live.get(&session)
        {
            live.pty.resize_now(size, pixels);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: u64 = 1;
    const TWO: u64 = 2;

    fn size(cols: u16, rows: u16) -> TermSize {
        TermSize { cols, rows }
    }

    fn nobody_looks(_: u64) -> bool {
        false
    }

    /// Two clients joined to one session, each watching it.
    fn watched_by_two() -> (Sizing, SessionId) {
        let (mut sizing, session) = (Sizing::default(), SessionId::new());
        for client in [ONE, TWO] {
            sizing.join(session, client);
            sizing.watching(session, client, true);
        }
        (sizing, session)
    }

    #[test]
    fn the_terminal_takes_the_size_of_the_client_active_last_arc_02() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), None))
        );
        sizing.resized(session, TWO, size(80, 20), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(80, 20), None))
        );
        // Typing makes a client the one in charge again; nothing changes
        // twice.
        sizing.active(session, ONE);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), None))
        );
        assert_eq!(sizing.due(session, nobody_looks), None);
    }

    #[test]
    fn a_client_not_watching_is_remembered_not_applied_until_the_watcher_goes() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), None))
        );
        // The second's window went out of focus; its resize comes later.
        sizing.watching(session, TWO, false);
        sizing.resized(session, TWO, size(80, 20), None);
        assert_eq!(sizing.due(session, nobody_looks), None);
        // The first lets the session go: the second's size applies.
        sizing.leave(session, ONE);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(80, 20), None))
        );
    }

    #[test]
    fn a_window_looked_at_comes_before_one_active_later() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), None);
        sizing.resized(session, TWO, size(80, 20), None);
        let looked_at = |client: u64| client == ONE;
        assert_eq!(sizing.due(session, looked_at), Some((size(100, 30), None)));
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(80, 20), None))
        );
    }

    #[test]
    fn a_lone_client_out_of_focus_still_sizes_its_session() {
        let (mut sizing, session) = (Sizing::default(), SessionId::new());
        sizing.join(session, ONE);
        sizing.resized(session, ONE, size(90, 25), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(90, 25), None))
        );
        // Its watch ended: nobody else is there, so nothing changes.
        sizing.watching(session, ONE, true);
        sizing.watching(session, ONE, false);
        assert_eq!(sizing.due(session, nobody_looks), None);
    }

    #[test]
    fn a_client_that_never_gave_a_size_changes_nothing() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), None))
        );
        sizing.active(session, TWO);
        assert_eq!(sizing.due(session, nobody_looks), None);
    }

    fn pixels(width: u16, height: u16) -> PixelSize {
        PixelSize { width, height }
    }

    #[test]
    fn the_terminal_takes_the_pixels_of_the_client_in_charge_kept_with_its_cells() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), Some(pixels(1000, 600)));
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), Some(pixels(1000, 600))))
        );
        // The second's terminal says no pixels: none go with its cells.
        sizing.resized(session, TWO, size(80, 20), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(80, 20), None))
        );
        // The first is in charge again, with the pixels it gave.
        sizing.active(session, ONE);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), Some(pixels(1000, 600))))
        );
        // Pixels alone changing is a change.
        sizing.resized(session, ONE, size(100, 30), Some(pixels(2000, 1200)));
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), Some(pixels(2000, 1200))))
        );
        assert_eq!(sizing.due(session, nobody_looks), None);
    }

    #[test]
    fn a_new_terminal_is_given_the_pixels_of_the_client_in_charge() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), Some(pixels(1000, 600)));
        sizing.due(session, nobody_looks);
        // Started at the client's cells but with no pixels.
        sizing.published(session, size(100, 30));
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), Some(pixels(1000, 600))))
        );
    }

    #[test]
    fn a_new_terminal_is_given_the_size_only_if_it_started_at_another() {
        let (mut sizing, session) = watched_by_two();
        sizing.resized(session, ONE, size(100, 30), None);
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), None))
        );
        sizing.published(session, size(100, 30));
        assert_eq!(sizing.due(session, nobody_looks), None);
        sizing.published(session, size(80, 24));
        assert_eq!(
            sizing.due(session, nobody_looks),
            Some((size(100, 30), None))
        );
    }

    #[test]
    fn the_latest_client_is_told_to_connections() {
        let (mut sizing, session) = watched_by_two();
        let latest = sizing.join(session, ONE);
        sizing.active(session, ONE);
        assert_eq!(*latest.borrow(), ONE);
        sizing.resized(session, TWO, size(80, 20), None);
        assert_eq!(*latest.borrow(), TWO);
    }

    #[test]
    fn what_is_kept_for_a_client_leaves_with_it_arc_14() {
        let (mut sizing, session) = watched_by_two();
        let other = SessionId::new();
        sizing.join(other, ONE);
        assert_eq!(sizing.len(), 2);
        let mut left = sizing.gone(ONE);
        left.sort();
        let mut both = vec![session, other];
        both.sort();
        assert_eq!(left, both);
        // The other session had no client left; this one still has one.
        assert_eq!(sizing.len(), 1);
        sizing.released(session);
        assert_eq!(sizing.len(), 0);
    }
}

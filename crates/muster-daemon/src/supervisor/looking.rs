//! Who looks at Muster's windows, for the notch (`ARC-22`): each client
//! that says whether the person is looking at it. While any is looked at,
//! the notch keeps out of the way; a session the person picks in the
//! notch is shown in the window they looked at last. Muster's own
//! settings, which turn the notch on and off, are kept here too.
//!
//! A client says so first through [`Supervisor::add_looker`], under its
//! connection's key; it leaves the list as its connection ends
//! ([`ClientGuard`](super::ClientGuard)).

use std::collections::HashMap;

use muster_core::SessionId;
use muster_core::protocol::{Delta, ErrorBody, ErrorCode, Reply, ServerFrame};
use muster_core::settings::PreferenceChange;
use tokio::sync::mpsc;

use super::Supervisor;

/// One client that says whether the person is looking at it.
#[derive(Debug)]
struct Looker {
    looking: bool,
    /// When it was last looked at, in [`Lookers::looks`]; 0 if never.
    looked: u64,
    /// Its frames, to show a session in it.
    outgoing: mpsc::Sender<ServerFrame>,
}

/// The clients that say whether the person is looking at them, by their
/// connections' keys.
#[derive(Debug, Default)]
pub(super) struct Lookers {
    clients: HashMap<u64, Looker>,
    /// Counts each time a client came to be looked at, so the one looked at
    /// last is known.
    looks: u64,
    /// Muster's helper that shows notifications, while it can.
    notifier: Option<u64>,
}

impl Lookers {
    /// Whether the person is looking at the client's window.
    pub(super) fn is_looking(&self, client: u64) -> bool {
        self.clients
            .get(&client)
            .is_some_and(|looker| looker.looking)
    }
}

impl Supervisor {
    /// Counts a client that will say whether the person is looking at it,
    /// at first not looked at.
    pub(super) fn add_looker(&mut self, client: u64, outgoing: mpsc::Sender<ServerFrame>) {
        self.lookers.clients.entry(client).or_insert(Looker {
            looking: false,
            looked: 0,
            outgoing,
        });
    }

    /// A client says whether the person is looking at it, which may change
    /// which client's size its sessions take (`sizing`).
    pub(super) async fn set_looking(&mut self, client: u64, looking: bool) {
        let lookers = &mut self.lookers;
        let Some(looker) = lookers.clients.get_mut(&client) else {
            return;
        };
        let changed = looker.looking != looking;
        if looking && !looker.looking {
            lookers.looks += 1;
            looker.looked = lookers.looks;
        }
        looker.looking = looking;
        self.publish_looked_at();
        if changed {
            self.looked_changed(client, looking).await;
        }
    }

    /// A client left.
    pub(super) fn remove_looker(&mut self, client: u64) {
        self.lookers.clients.remove(&client);
        self.publish_looked_at();
        if self.lookers.notifier == Some(client) {
            self.set_notifier(client, false);
        }
    }

    /// Muster's helper, as `client`, can show notifications now, or no
    /// longer can: the UIs send their terminals none while it can.
    pub(super) fn set_notifier(&mut self, client: u64, ready: bool) {
        if ready {
            self.lookers.notifier = Some(client);
        } else if self.lookers.notifier == Some(client) {
            self.lookers.notifier = None;
        }
        let notifier = self.lookers.notifier.is_some();
        if notifier != self.state.notifier {
            self.state.notifier = notifier;
            self.broadcast(Delta::Notifier { ready: notifier });
        }
    }

    /// Tells clients when the person began or stopped looking at every
    /// window.
    fn publish_looked_at(&mut self) {
        let looked_at = self.lookers.clients.values().any(|looker| looker.looking);
        if looked_at != self.state.looked_at {
            self.state.looked_at = looked_at;
            self.looked_at.send_replace(looked_at);
            self.broadcast(Delta::LookedAt { looked_at });
            if looked_at {
                self.catch_up_changes();
                self.catch_up_pull_requests();
            }
        }
    }

    /// Shows `session` in the window the person looked at last, or the
    /// newest if they have looked at none.
    pub(super) fn reveal(&self, session: SessionId) -> Result<Reply, ErrorBody> {
        if !self
            .state
            .sessions
            .iter()
            .any(|stored| stored.id == session)
        {
            return Err(ErrorBody::new(ErrorCode::NotFound, "no such session"));
        }
        let Some((_, window)) = self
            .lookers
            .clients
            .iter()
            .max_by_key(|(client, looker)| (looker.looked, **client))
        else {
            return Err(ErrorBody::new(
                ErrorCode::NotFound,
                "no Muster window is open to show it in",
            ));
        };
        window
            .outgoing
            .try_send(ServerFrame::Reveal { session })
            .map_err(|_| ErrorBody::new(ErrorCode::Conflict, "the Muster window is busy"))?;
        Ok(Reply::Done)
    }

    /// Shows `to` in every client that shows the person `from` now: a
    /// terminal tab attached to it (`muster attach`), or a window with it
    /// in view. Refused where nothing shows `from`, or `to` is no session.
    pub(super) fn switch(&self, from: SessionId, to: SessionId) -> Result<Reply, ErrorBody> {
        if !self.state.sessions.iter().any(|stored| stored.id == to) {
            return Err(ErrorBody::new(ErrorCode::NotFound, "no such session"));
        }
        let mut shown = false;
        for client in self.sizing.watchers(from) {
            if let Some(looker) = self.lookers.clients.get(&client) {
                // A client too busy to take it goes on showing `from`.
                shown |= looker
                    .outgoing
                    .try_send(ServerFrame::Reveal { session: to })
                    .is_ok();
            }
        }
        if shown {
            Ok(Reply::Done)
        } else {
            Err(ErrorBody::new(
                ErrorCode::NotFound,
                "nothing shows that session to you now",
            ))
        }
    }

    /// Makes a change in Muster's own settings and tells every client.
    pub(super) fn change_preference(
        &mut self,
        change: &PreferenceChange,
    ) -> Result<Reply, ErrorBody> {
        let refused = |error: muster_core::config::ConfigError| {
            ErrorBody::new(ErrorCode::BadRequest, error.to_string())
        };
        crate::config::change_preference(&self.config_dir, change).map_err(refused)?;
        let own = crate::config::own_settings(&self.config_dir).map_err(refused)?;
        tracing::info!(?change, "changed one of Muster's own settings");
        let agents = own.agents.into_iter().map(std::sync::Arc::new).collect();
        self.take_own_settings((own.preferences, agents));
        Ok(Reply::Done)
    }
}

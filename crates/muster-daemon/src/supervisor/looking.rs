//! Who looks at Muster's windows, for the notch (`ARC-22`): each client
//! that says whether the person is looking at it. While any is looked at,
//! the notch keeps out of the way; a session the person picks in the
//! notch is shown in the window they looked at last. Muster's own
//! settings, which turn the notch on and off, are kept here too.
//!
//! A client says so first through [`Supervisor::add_looker`], which hands
//! it a [`LookGuard`]; dropping the guard, however the client ends, takes
//! it off the list.

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

/// The clients that say whether the person is looking at them.
#[derive(Debug, Default)]
pub(super) struct Lookers {
    clients: HashMap<u64, Looker>,
    /// The key the next client is given.
    next: u64,
    /// Counts each time a client came to be looked at, so the one looked at
    /// last is known.
    looks: u64,
}

/// Counts one client among the lookers until dropped.
#[derive(Debug)]
pub(crate) struct LookGuard {
    client: u64,
    gone: mpsc::UnboundedSender<u64>,
}

impl LookGuard {
    /// The client's key, for [`SupervisorHandle::looking`](super::SupervisorHandle::looking).
    pub(crate) fn client(&self) -> u64 {
        self.client
    }
}

impl Drop for LookGuard {
    fn drop(&mut self) {
        // The Supervisor has stopped if this fails; nothing is left to count.
        let _ = self.gone.send(self.client);
    }
}

impl Supervisor {
    /// Counts a client that will say whether the person is looking at it,
    /// at first not looked at.
    pub(super) fn add_looker(&mut self, outgoing: mpsc::Sender<ServerFrame>) -> LookGuard {
        let lookers = &mut self.lookers;
        lookers.next += 1;
        let client = lookers.next;
        lookers.clients.insert(
            client,
            Looker {
                looking: false,
                looked: 0,
                outgoing,
            },
        );
        LookGuard {
            client,
            gone: self.looker_gone.clone(),
        }
    }

    /// A client says whether the person is looking at it.
    pub(super) fn set_looking(&mut self, client: u64, looking: bool) {
        let lookers = &mut self.lookers;
        let Some(looker) = lookers.clients.get_mut(&client) else {
            return;
        };
        if looking && !looker.looking {
            lookers.looks += 1;
            looker.looked = lookers.looks;
        }
        looker.looking = looking;
        self.publish_looked_at();
    }

    /// A client left.
    pub(super) fn remove_looker(&mut self, client: u64) {
        self.lookers.clients.remove(&client);
        self.publish_looked_at();
    }

    /// Tells clients when the person began or stopped looking at every
    /// window.
    fn publish_looked_at(&mut self) {
        let looked_at = self.lookers.clients.values().any(|looker| looker.looking);
        if looked_at != self.state.looked_at {
            self.state.looked_at = looked_at;
            self.broadcast(Delta::LookedAt { looked_at });
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

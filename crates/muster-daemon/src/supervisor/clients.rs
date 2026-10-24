//! Each connection as the Supervisor knows it: a key, under which it is
//! counted among the windows the person may look at (`looking`) and among
//! the clients of sessions' terminals (`sizing`), and a guard that takes
//! all of that away when the connection ends, however it ends (`ARC-14`).
//!
//! The guard also keeps, for its connection, whether the person is looking
//! at the client's window as the client last said: what a connection
//! shows is watched only while it is (`terminals`), and one that never
//! says counts as looked at.

use tokio::sync::mpsc;

use super::Supervisor;

/// Names one connection to the Supervisor until dropped.
#[derive(Debug)]
pub(crate) struct ClientGuard {
    client: u64,
    /// Whether the person is looking at the client, as it last said;
    /// `None` until it says.
    looking: Option<bool>,
    gone: mpsc::UnboundedSender<u64>,
}

impl ClientGuard {
    /// The connection's key.
    pub(crate) fn id(&self) -> u64 {
        self.client
    }

    /// Whether the client has said whether the person is looking at it,
    /// which counts it among the windows they may look at.
    pub(crate) fn has_said(&self) -> bool {
        self.looking.is_some()
    }

    /// Whether the person is looking at the client, as it last said; one
    /// that never says counts as looked at.
    pub(crate) fn looked_at(&self) -> bool {
        self.looking.unwrap_or(true)
    }

    /// The client says whether the person is looking at it.
    pub(crate) fn said(&mut self, looking: bool) {
        self.looking = Some(looking);
    }
}

impl Drop for ClientGuard {
    fn drop(&mut self) {
        // The Supervisor has stopped if this fails; nothing is left to count.
        let _ = self.gone.send(self.client);
    }
}

impl Supervisor {
    /// A key for a new connection, and its guard.
    pub(super) fn add_client(&mut self) -> ClientGuard {
        self.clients.next_client += 1;
        ClientGuard {
            client: self.clients.next_client,
            looking: None,
            gone: self.clients.client_gone.clone(),
        }
    }

    /// A connection ended: it is no longer a window the person may look
    /// at, nor a client of any session's terminal.
    pub(super) async fn remove_client(&mut self, client: u64) {
        self.remove_looker(client);
        for session in self.clients.sizing.gone(client) {
            self.fit(session).await;
        }
    }
}

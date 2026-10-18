//! Each connection as the Supervisor knows it: a key, under which it is
//! counted among the windows the person may look at (`looking`) and among
//! the clients of sessions' terminals (`sizing`), and a guard that takes
//! all of that away when the connection ends, however it ends (`ARC-14`).

use tokio::sync::mpsc;

use super::Supervisor;

/// Names one connection to the Supervisor until dropped.
#[derive(Debug)]
pub(crate) struct ClientGuard {
    client: u64,
    gone: mpsc::UnboundedSender<u64>,
}

impl ClientGuard {
    /// The connection's key.
    pub(crate) fn id(&self) -> u64 {
        self.client
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
        self.next_client += 1;
        ClientGuard {
            client: self.next_client,
            gone: self.client_gone.clone(),
        }
    }

    /// A connection ended: it is no longer a window the person may look
    /// at, nor a client of any session's terminal.
    pub(super) async fn remove_client(&mut self, client: u64) {
        self.remove_looker(client);
        for session in self.sizing.gone(client) {
            self.fit(session).await;
        }
    }
}

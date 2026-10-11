//! The socket server: one reader loop and one writer task per connection, so
//! replies and pushed frames never interleave mid-frame (`ARCHITECTURE.md` §6).

use std::collections::HashMap;
use std::future::Future;

use muster_client::{FramedRead, FramedWrite, TransportError};
use muster_core::SessionId;
use muster_core::protocol::{
    ClientFrame, DetachReason, ErrorBody, ErrorCode, Incompatible, MIN_PROTOCOL_VERSION,
    PROTOCOL_VERSION, ServerFrame, Welcome, negotiate,
};
use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, watch};
use tokio::task::JoinHandle;

use crate::pty::PtyHandle;
use crate::supervisor::{SupervisorHandle, TerminalLink, WatchGuard};
use crate::terminal::forward_output;

/// Frames queued for one client before the sender waits. A subscriber that
/// stops reading makes its delta pump wait, falls behind the broadcast, and
/// is sent a fresh snapshot when it catches up.
const OUTGOING_FRAMES: usize = 256;

/// Accepts connections until `shutdown` completes.
pub(crate) async fn serve(
    listener: UnixListener,
    supervisor: SupervisorHandle,
    shutdown: impl Future<Output = ()>,
) {
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => return,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    tokio::spawn(connection(stream, supervisor.clone()));
                }
                Err(error) => tracing::warn!(%error, "could not accept a connection"),
            },
        }
    }
}

async fn connection(stream: UnixStream, supervisor: SupervisorHandle) {
    let (read, write) = stream.into_split();
    let (outgoing, queue) = mpsc::channel(OUTGOING_FRAMES);
    let writer = tokio::spawn(write_frames(FramedWrite::new(write), queue));

    let mut session = Connection {
        reader: FramedRead::new(read),
        outgoing,
        supervisor,
        pump: None,
        terminals: HashMap::new(),
        forwarders: HashMap::new(),
        watching: HashMap::new(),
    };
    if let Err(error) = session.run().await {
        // The writer sends this last, then closes.
        let _ = session.outgoing.send(ServerFrame::Error(error)).await;
    }
    if let Some(pump) = session.pump.take() {
        pump.abort();
    }
    session.watching.clear();
    for (_, forwarder) in session.forwarders.drain() {
        forwarder.abort();
    }
    drop(session);
    let _ = writer.await;
}

async fn write_frames(
    mut writer: FramedWrite<OwnedWriteHalf>,
    mut queue: mpsc::Receiver<ServerFrame>,
) {
    while let Some(frame) = queue.recv().await {
        let last = matches!(frame, ServerFrame::Error(_) | ServerFrame::Incompatible(_));
        if writer.send(&frame).await.is_err() || last {
            break;
        }
    }
    let _ = writer.shutdown().await;
}

struct Connection {
    reader: FramedRead<OwnedReadHalf>,
    outgoing: mpsc::Sender<ServerFrame>,
    supervisor: SupervisorHandle,
    pump: Option<JoinHandle<()>>,
    /// Terminals this client has used, so keystrokes skip the Supervisor:
    /// each session's current one, which follows it to a new program.
    terminals: HashMap<SessionId, watch::Receiver<Option<PtyHandle>>>,
    /// One output stream per attached session.
    forwarders: HashMap<SessionId, JoinHandle<()>>,
    /// The attached sessions this client's person is looking at; dropping
    /// a guard ends its watch.
    watching: HashMap<SessionId, WatchGuard>,
}

impl Connection {
    /// Serves the client until it leaves. An error is a protocol violation
    /// to report before closing.
    async fn run(&mut self) -> Result<(), ErrorBody> {
        if !self.greet().await? {
            return Ok(());
        }
        while let Some(frame) = self.receive().await? {
            match frame {
                ClientFrame::Hello(_) => {
                    return Err(ErrorBody::new(
                        ErrorCode::OutOfOrder,
                        "hello was already sent",
                    ));
                }
                ClientFrame::Subscribe => {
                    if let Some(old) = self.pump.take() {
                        old.abort();
                    }
                    let pump = pump_state(self.supervisor.clone(), self.outgoing.clone());
                    self.pump = Some(tokio::spawn(pump));
                }
                ClientFrame::Command { req_id, command } => {
                    let (supervisor, outgoing) = (self.supervisor.clone(), self.outgoing.clone());
                    tokio::spawn(async move {
                        let outcome = supervisor.command(command).await;
                        let _ = outgoing.send(ServerFrame::Reply { req_id, outcome }).await;
                    });
                }
                ClientFrame::Attach {
                    session,
                    from_seq,
                    watching,
                } => self.attach(session, from_seq, watching).await,
                ClientFrame::Detach { session } => {
                    self.stop_forwarding(session).await;
                    self.terminals.remove(&session);
                }
                ClientFrame::Watch { session, watching } => self.watch(session, watching).await,
                ClientFrame::Input { session, data } => {
                    if let Some(pty) = self.terminal(session).await
                        && !pty.input(data).await
                    {
                        self.forget(session).await;
                    }
                }
                ClientFrame::Resize { session, size } => {
                    if let Some(pty) = self.terminal(session).await
                        && !pty.resize(size).await
                    {
                        self.forget(session).await;
                    }
                }
            }
        }
        Ok(())
    }

    async fn attach(&mut self, session: SessionId, from_seq: Option<u64>, watch: bool) {
        let Some(TerminalLink { current, watching }) =
            self.supervisor.terminal(session, watch).await
        else {
            self.stop_forwarding(session).await;
            self.detached(session).await;
            return;
        };
        // The new watch is counted before the old one ends, so a re-attach
        // never looks away for a moment.
        self.stop_forwarding(session).await;
        if let Some(guard) = watching {
            self.watching.insert(session, guard);
        }
        self.terminals.insert(session, current.clone());
        let forwarder = forward_output(session, current, from_seq, self.outgoing.clone());
        self.forwarders.insert(session, tokio::spawn(forwarder));
    }

    /// Stops streaming `session` to this client, and watching it.
    async fn stop_forwarding(&mut self, session: SessionId) {
        self.watching.remove(&session);
        if let Some(forwarder) = self.forwarders.remove(&session) {
            forwarder.abort();
            let _ = forwarder.await;
        }
    }

    /// Starts or stops watching an attached session.
    async fn watch(&mut self, session: SessionId, watching: bool) {
        if !watching {
            self.watching.remove(&session);
            return;
        }
        if !self.forwarders.contains_key(&session) || self.watching.contains_key(&session) {
            return;
        }
        if let Some(guard) = self
            .supervisor
            .terminal(session, true)
            .await
            .and_then(|link| link.watching)
        {
            self.watching.insert(session, guard);
        }
    }

    /// The session's current terminal, asking the Supervisor only the first
    /// time.
    async fn terminal(&mut self, session: SessionId) -> Option<PtyHandle> {
        if !self.terminals.contains_key(&session) {
            match self.supervisor.terminal(session, false).await {
                Some(link) => {
                    self.terminals.insert(session, link.current);
                }
                None => {
                    self.detached(session).await;
                    return None;
                }
            }
        }
        let current = self.terminals.get(&session)?;
        // A closed link keeps its last value; the terminal is released.
        let pty = match current.has_changed() {
            Ok(_) => current.borrow().clone(),
            Err(_) => None,
        };
        if pty.is_none() {
            self.forget(session).await;
        }
        pty
    }

    /// The terminal was released: tell the client and stop using it.
    async fn forget(&mut self, session: SessionId) {
        self.terminals.remove(&session);
        self.detached(session).await;
    }

    async fn detached(&self, session: SessionId) {
        let _ = self
            .outgoing
            .send(ServerFrame::Detached {
                session,
                reason: DetachReason::NotRunning,
            })
            .await;
    }

    /// Handles the hello. `false` means the client was turned away politely.
    async fn greet(&mut self) -> Result<bool, ErrorBody> {
        let hello = match self.receive().await? {
            Some(ClientFrame::Hello(hello)) => hello,
            Some(_) => {
                return Err(ErrorBody::new(ErrorCode::OutOfOrder, "send hello first"));
            }
            None => return Ok(false),
        };
        let daemon_version = env!("CARGO_PKG_VERSION").to_owned();
        let Some(version) = negotiate(&hello) else {
            tracing::info!(client = %hello.client, "turned away an incompatible client");
            let _ = self
                .outgoing
                .send(ServerFrame::Incompatible(Incompatible {
                    min_version: MIN_PROTOCOL_VERSION,
                    max_version: PROTOCOL_VERSION,
                    daemon_version,
                    daemon_pid: std::process::id(),
                }))
                .await;
            return Ok(false);
        };
        tracing::debug!(client = %hello.client, version, "client connected");
        let _ = self
            .outgoing
            .send(ServerFrame::Welcome(Welcome {
                version,
                daemon_pid: std::process::id(),
                daemon_version,
            }))
            .await;
        Ok(true)
    }

    async fn receive(&mut self) -> Result<Option<ClientFrame>, ErrorBody> {
        self.reader.next().await.map_err(|error| match error {
            TransportError::Frame(error) => {
                ErrorBody::new(ErrorCode::BadRequest, error.to_string())
            }
            error => ErrorBody::new(ErrorCode::BadRequest, error.to_string()),
        })
    }
}

/// Sends the client a snapshot and then every delta. A client that falls
/// behind the broadcast is sent a fresh snapshot instead of the deltas it
/// missed.
pub(crate) async fn pump_state(supervisor: SupervisorHandle, outgoing: mpsc::Sender<ServerFrame>) {
    while let Some((snapshot, mut deltas)) = supervisor.subscribe().await {
        if outgoing
            .send(ServerFrame::Snapshot(snapshot))
            .await
            .is_err()
        {
            return;
        }
        loop {
            match deltas.recv().await {
                Ok(delta) => {
                    if outgoing.send(ServerFrame::Delta(delta)).await.is_err() {
                        return;
                    }
                }
                Err(RecvError::Lagged(missed)) => {
                    tracing::debug!(missed, "a subscriber fell behind; resending a snapshot");
                    break;
                }
                Err(RecvError::Closed) => return,
            }
        }
    }
}

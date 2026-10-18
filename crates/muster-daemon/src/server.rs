//! The socket server: one reader loop and one writer task per connection, so
//! replies and pushed frames never interleave mid-frame (`ARCHITECTURE.md` §6).
//! What the writer sends, and in which order, is its outbox's (`outbox`).

pub(crate) mod outbox;

use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

use muster_client::{FramedRead, FramedWrite, TransportError};
use muster_core::SessionId;
use muster_core::protocol::{
    ClientFrame, DetachReason, ErrorBody, ErrorCode, Incompatible, MIN_PROTOCOL_VERSION,
    PROTOCOL_VERSION, ServerFrame, Welcome, negotiate,
};
use tokio::net::unix::OwnedReadHalf;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, watch};

use self::outbox::{LANE_BUDGET, Outbox};
use crate::pty::PtyHandle;
use crate::supervisor::{ClientGuard, SupervisorHandle, TerminalLink, WatchGuard};
use crate::tasks::{self, Owned, Tasks};
use crate::terminal::forward_output;

/// Frames queued for one client before the sender waits: everything but
/// terminal output, which waits in its session's lane (`outbox`). A
/// subscriber that stops reading makes its delta pump wait, falls behind
/// the broadcast, and is sent a fresh snapshot when it catches up.
const OUTGOING_FRAMES: usize = 256;

/// Accepts connections until `shutdown` completes. Each is counted in
/// `tasks` and served until its client leaves or the daemon stops.
pub(crate) async fn serve(
    listener: UnixListener,
    supervisor: SupervisorHandle,
    tasks: &Tasks,
    shutdown: impl Future<Output = ()>,
) {
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            () = &mut shutdown => return,
            accepted = listener.accept() => match accepted {
                Ok((stream, _)) => {
                    tasks.connection(connection(stream, supervisor.clone(), tasks.clone()));
                }
                Err(error) => tracing::warn!(%error, "could not accept a connection"),
            },
        }
    }
}

async fn connection(stream: UnixStream, supervisor: SupervisorHandle, tasks: Tasks) {
    let (read, write) = stream.into_split();
    let (outgoing, queue) = mpsc::channel(OUTGOING_FRAMES);
    let streams = Outbox::new(LANE_BUDGET);
    let writer = tasks::owned(outbox::write_frames(
        FramedWrite::new(write),
        queue,
        streams.clone(),
    ));

    let mut session = Connection {
        reader: FramedRead::new(read),
        outgoing,
        streams,
        supervisor,
        tasks,
        pump: None,
        terminals: HashMap::new(),
        forwarders: HashMap::new(),
        watching: HashMap::new(),
        client: None,
        looker: false,
        version: MIN_PROTOCOL_VERSION,
    };
    if let Err(error) = session.run().await {
        // The writer sends this last, then closes.
        let _ = session.outgoing.send(ServerFrame::Error(error)).await;
    }
    // Its pump and forwarders end as they are dropped, here or, should
    // this task end early, with it (`ARC-13`).
    drop(session);
    let _ = writer.await;
}

struct Connection {
    reader: FramedRead<OwnedReadHalf>,
    /// Frames that must all arrive, in order.
    outgoing: mpsc::Sender<ServerFrame>,
    /// Each attached session's stream, which a client that cannot keep up
    /// resumes from the ring rather than queues.
    streams: Arc<Outbox>,
    supervisor: SupervisorHandle,
    tasks: Tasks,
    /// Streams state to the client once it subscribes.
    pump: Option<Owned>,
    /// Terminals this client has used, so keystrokes skip the Supervisor:
    /// each session's current one, which follows it to a new program.
    terminals: HashMap<SessionId, Linked>,
    /// One output stream per attached session.
    forwarders: HashMap<SessionId, Owned>,
    /// The attached sessions this client's person is looking at; dropping
    /// a guard ends its watch.
    watching: HashMap<SessionId, WatchGuard>,
    /// The connection's key with the Supervisor, once greeted.
    client: Option<ClientGuard>,
    /// The client has said whether the person is looking at it.
    looker: bool,
    /// The protocol agreed with the client.
    version: u32,
}

/// A session's terminal as this connection uses it.
struct Linked {
    /// Its current terminal.
    current: watch::Receiver<Option<PtyHandle>>,
    /// The client active on it last, so a keystroke tells the Supervisor
    /// only when another client was active since (`sizing`).
    latest: watch::Receiver<u64>,
}

impl Connection {
    /// Serves the client until it leaves. An error is a protocol violation
    /// to report before closing.
    async fn run(&mut self) -> Result<(), ErrorBody> {
        if !self.greet().await? {
            return Ok(());
        }
        // `None` only while the daemon stops, when no terminal is linked.
        self.client = self.supervisor.add_client().await;
        while let Some(frame) = self.receive().await? {
            match frame {
                ClientFrame::Hello(_) => {
                    return Err(ErrorBody::new(
                        ErrorCode::OutOfOrder,
                        "hello was already sent",
                    ));
                }
                ClientFrame::Subscribe => {
                    let pump =
                        pump_state(self.supervisor.clone(), self.outgoing.clone(), self.version);
                    // Dropping the old one ends it.
                    self.pump = Some(tasks::owned(pump));
                }
                ClientFrame::Command { req_id, command } => {
                    let (supervisor, outgoing) = (self.supervisor.clone(), self.outgoing.clone());
                    let version = self.version;
                    self.tasks.spawn(async move {
                        // As the client's protocol can read it (`ARC-09`).
                        let outcome = supervisor
                            .command(command)
                            .await
                            .map(|reply| reply.for_version(version));
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
                    self.let_go(session).await;
                }
                ClientFrame::Watch { session, watching } => self.watch(session, watching).await,
                ClientFrame::Looking { looking } => self.looking(looking).await,
                ClientFrame::Input { session, data } => {
                    if let Some(pty) = self.terminal(session).await {
                        self.typed(session).await;
                        if !pty.input(data).await {
                            self.forget(session).await;
                        }
                    }
                }
                // The Supervisor decides whose size the terminal takes.
                ClientFrame::Resize { session, size } => {
                    if self.terminal(session).await.is_some()
                        && let Some(client) = &self.client
                    {
                        self.supervisor.resize(session, client, size).await;
                    }
                }
            }
        }
        Ok(())
    }

    async fn attach(&mut self, session: SessionId, from_seq: Option<u64>, watch: bool) {
        let link = match &self.client {
            Some(client) => self.supervisor.terminal(session, watch, client).await,
            None => None,
        };
        let Some(TerminalLink {
            current,
            watching,
            latest,
        }) = link
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
        let linked = Linked {
            current: current.clone(),
            latest,
        };
        self.terminals.insert(session, linked);
        let forwarder = forward_output(session, current, from_seq, self.streams.clone());
        self.forwarders.insert(session, tasks::owned(forwarder));
    }

    /// Stops streaming `session` to this client, and watching it. What of
    /// its stream still waits is dropped: the client attaches again from
    /// what it has, or not at all.
    async fn stop_forwarding(&mut self, session: SessionId) {
        self.watching.remove(&session);
        if let Some(forwarder) = self.forwarders.remove(&session) {
            forwarder.abort();
            let _ = forwarder.await;
        }
        self.streams.clear(session);
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
        let Some(client) = &self.client else {
            return;
        };
        if let Some(guard) = self
            .supervisor
            .terminal(session, true, client)
            .await
            .and_then(|link| link.watching)
        {
            self.watching.insert(session, guard);
        }
    }

    /// The client says whether the person is looking at it; the first
    /// time, it is counted among the clients that say so.
    async fn looking(&mut self, looking: bool) {
        let Some(client) = &self.client else {
            return;
        };
        if !self.looker {
            self.looker = self
                .supervisor
                .add_looker(client, self.outgoing.clone())
                .await;
        }
        self.supervisor.looking(client, looking).await;
    }

    /// The session's current terminal, asking the Supervisor only the first
    /// time.
    async fn terminal(&mut self, session: SessionId) -> Option<PtyHandle> {
        if !self.terminals.contains_key(&session) {
            let link = match &self.client {
                Some(client) => self.supervisor.terminal(session, false, client).await,
                None => None,
            };
            let Some(link) = link else {
                self.detached(session).await;
                return None;
            };
            let linked = Linked {
                current: link.current,
                latest: link.latest,
            };
            self.terminals.insert(session, linked);
        }
        let current = &self.terminals.get(&session)?.current;
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

    /// The client typed into the session: if another client was active on
    /// it since this one, the Supervisor is told, and the terminal may take
    /// this client's size before the keys reach it.
    async fn typed(&mut self, session: SessionId) {
        let (Some(client), Some(linked)) = (&self.client, self.terminals.get(&session)) else {
            return;
        };
        if *linked.latest.borrow() != client.id() {
            self.supervisor.active(session, client).await;
        }
    }

    /// Stops using the session's terminal, and no longer counts among the
    /// clients whose size it may take.
    async fn let_go(&mut self, session: SessionId) {
        if self.terminals.remove(&session).is_some()
            && let Some(client) = &self.client
        {
            self.supervisor.leave(session, client).await;
        }
    }

    /// The terminal was released: tell the client and stop using it.
    async fn forget(&mut self, session: SessionId) {
        self.let_go(session).await;
        self.detached(session).await;
    }

    /// Tells the client the session's stream ended: after what of it still
    /// waits, if it is streamed; otherwise among the frames that wait their
    /// turn, so a client asking after sessions it never reads of is held
    /// back like any other (`ARC-14`).
    async fn detached(&self, session: SessionId) {
        let reason = DetachReason::NotRunning;
        let frame = ServerFrame::Detached { session, reason };
        if self.forwarders.contains_key(&session) {
            let _ = self.streams.push(session, frame);
        } else {
            let _ = self.outgoing.send(frame).await;
        }
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
        self.version = version;
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

/// Sends the client a snapshot and then every delta its protocol `version`
/// knows. A client that falls behind the broadcast is sent a fresh snapshot
/// instead of the deltas it missed.
pub(crate) async fn pump_state(
    supervisor: SupervisorHandle,
    outgoing: mpsc::Sender<ServerFrame>,
    version: u32,
) {
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
                Ok(delta) if delta.since() > version => {}
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

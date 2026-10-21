//! The link's own task: one loop that owns the connection's two ends, the
//! model, what the webview was last sent and the terminal streams, and
//! takes the webview's requests, the daemon's frames and its own deadline
//! (the next patch) in turn. Nothing here is shared; the webview reaches
//! it only through requests (`ARC-02` in spirit).
//!
//! As the terminal UI's link does (`muster_tui::runtime::link`), a task of
//! its own connects, starting the daemon if none answers, and reads the
//! daemon's frames into a bounded queue, so a link that falls behind stops
//! reading and the daemon holds back as for any slow client (`ARC-14`);
//! frames for the daemon go to a writer of their own, so the loop never
//! waits on a daemon that may itself wait for this client to read.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use muster_client::{ClientError, Connection, FrameReceiver, FrameSender};
use muster_core::protocol::{ClientFrame, Command, Reply, ServerFrame, Welcome};
use muster_core::{SessionId, SessionStatus};
use muster_model::Model;
use tokio::sync::{Notify, mpsc, oneshot};
use tokio::task::JoinHandle;
use tokio::time::Instant;

use super::patches::Patcher;
use super::streams::Streams;
use super::{
    Looking, Options, PATCH_EVERY, RETRY_EVERY, Request, START_AGAIN_AFTER, START_WAIT, Start,
    StateSink, now,
};
use crate::contract::{BridgeError, LinkState, StateMessage};

/// Frames from the daemon waiting for the loop before the link stops
/// reading the socket, as the terminal UI's (`LINK_EVENTS`).
const LINK_EVENTS: usize = 64;

/// How the app names itself to the daemon.
const CLIENT: &str = concat!("muster-desktop ", env!("CARGO_PKG_VERSION"));

/// The first protocol that knows `ClientFrame::Looking`.
const LOOKING_PROTOCOL: u32 = muster_core::settings::NOTCH_PROTOCOL;

/// A task that ends when its owner drops it (`ARC-13`).
struct Owned(JoinHandle<()>);

impl Drop for Owned {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// What the task keeping the connection reports.
enum LinkEvent {
    Connected {
        outgoing: mpsc::UnboundedSender<ClientFrame>,
        welcome: Welcome,
    },
    Frame(Box<ServerFrame>),
    Lost(String),
}

/// The link's loop, until every handle has gone.
pub(super) async fn run(
    options: Options,
    mut requests: mpsc::UnboundedReceiver<Request>,
    connected: Arc<AtomicBool>,
    again: Arc<Notify>,
) {
    let (events, mut linked) = mpsc::channel(LINK_EVENTS);
    // Ends as the loop does.
    #[expect(
        clippy::disallowed_methods,
        reason = "ARC-13: the Owned handle holds it, for as long as the link's loop"
    )]
    let _keeper = Owned(tokio::spawn(keep_linked(options, events, again)));
    let mut actor = Actor::new(connected);
    loop {
        let flush = actor.flush_at;
        tokio::select! {
            request = requests.recv() => match request {
                Some(request) => actor.on_request(request),
                None => return,
            },
            Some(event) = linked.recv() => actor.on_event(event),
            () = sleep_until(flush) => actor.flush(),
        }
    }
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// The connection, while there is one.
struct Linked {
    outgoing: mpsc::UnboundedSender<ClientFrame>,
    version: u32,
    daemon_version: String,
}

struct Actor {
    /// Where the webview's state goes, once it has opened the link.
    sink: Option<Box<dyn StateSink>>,
    link: LinkState,
    linked: Option<Linked>,
    /// Shared with the handles, so a keystroke while out of reach is
    /// refused at once.
    connected: Arc<AtomicBool>,
    /// Connections so far, numbering each.
    connections: u64,
    model: Model,
    patcher: Patcher,
    streams: Streams,
    /// Commands sent and not yet answered, by request id; they leave as
    /// answered or as the connection goes (`ARC-14`).
    pending: HashMap<u64, oneshot::Sender<Result<Reply, BridgeError>>>,
    next_request: u64,
    /// How the person looks at the window, as the webview last said.
    looking: Looking,
    /// What the daemon was last told of it on this connection.
    told: Option<(bool, Option<SessionId>)>,
    /// When the next patch is due, if one is.
    flush_at: Option<Instant>,
    last_flush: Option<Instant>,
}

impl Actor {
    fn new(connected: Arc<AtomicBool>) -> Self {
        Self {
            sink: None,
            link: LinkState::Connecting,
            linked: None,
            connected,
            connections: 0,
            model: Model::default(),
            patcher: Patcher::default(),
            streams: Streams::default(),
            pending: HashMap::new(),
            next_request: 1,
            looking: Looking::default(),
            told: None,
            flush_at: None,
            last_flush: None,
        }
    }

    fn tell(&self, message: StateMessage) {
        if let Some(sink) = &self.sink {
            sink.send(message);
        }
    }

    fn send(&self, frame: ClientFrame) {
        if let Some(linked) = &self.linked {
            let _ = linked.outgoing.send(frame);
        }
    }

    /// Sends what the streams have for the daemon.
    fn send_streams(&mut self) {
        for frame in self.streams.drain() {
            self.send(frame);
        }
    }

    fn set_link(&mut self, link: LinkState) {
        if self.link != link {
            self.link = link.clone();
            self.tell(StateMessage::Link { link });
        }
    }

    // The webview's requests.

    fn on_request(&mut self, request: Request) {
        match request {
            Request::Open(sink) => {
                self.sink = Some(sink);
                // A webview loaded again holds nothing: its terminals are
                // gone, and it is told everything afresh.
                self.streams.drop_views();
                self.send_streams();
                self.tell(StateMessage::Link {
                    link: self.link.clone(),
                });
                self.patcher.reset();
                self.schedule_flush();
            }
            Request::Command { command, reply } => self.command(*command, reply),
            Request::Attach {
                session,
                from_seq,
                watching,
                sink,
            } => {
                self.streams.view_attach(session, from_seq, watching, sink);
                self.send_streams();
            }
            Request::Detach { session } => {
                self.streams.view_detach(session);
                self.send_streams();
            }
            Request::Input { session, data } => self.send(ClientFrame::Input { session, data }),
            Request::Resize {
                session,
                size,
                pixels,
            } => {
                self.streams.resized(session, size);
                self.send(ClientFrame::Resize {
                    session,
                    size,
                    pixels,
                });
                self.schedule_flush_if_questions_changed();
            }
            Request::Watch { session, watching } => {
                self.streams.view_watch(session, watching);
                self.send_streams();
            }
            Request::Looking(looking) => {
                let was_seen = self.looking.seen;
                self.looking = looking;
                self.tell_looking();
                if looking.seen != was_seen {
                    self.settle_questions();
                    self.schedule_flush();
                }
            }
        }
    }

    fn command(&mut self, command: Command, reply: oneshot::Sender<Result<Reply, BridgeError>>) {
        if self.linked.is_none() {
            let _ = reply.send(Err(BridgeError::Unreachable));
            return;
        }
        let req_id = self.next_request;
        self.next_request += 1;
        self.pending.insert(req_id, reply);
        self.send(ClientFrame::Command { req_id, command });
    }

    /// Tells the daemon whether the person looks at the window, and which
    /// terminal is in focus in it, when either changed: each only to a
    /// daemon that knows it (`ARC-09`).
    fn tell_looking(&mut self) {
        let Some(version) = self.linked.as_ref().map(|linked| linked.version) else {
            return;
        };
        let now = (self.looking.looking, self.looking.focused);
        let before = self.told.replace(now);
        if version >= LOOKING_PROTOCOL && before.map(|(looking, _)| looking) != Some(now.0) {
            self.send(ClientFrame::Looking { looking: now.0 });
        }
        if version >= muster_core::colours::TERMINAL_PROTOCOL
            && before.map(|(_, focused)| focused) != Some(now.1)
        {
            self.send(ClientFrame::Keys { session: now.1 });
        }
    }

    // The daemon's frames, and the link coming and going.

    fn on_event(&mut self, event: LinkEvent) {
        match event {
            LinkEvent::Connected { outgoing, welcome } => {
                self.connections += 1;
                self.linked = Some(Linked {
                    outgoing,
                    version: welcome.version,
                    daemon_version: welcome.daemon_version,
                });
                self.connected.store(true, Ordering::Relaxed);
                self.told = None;
                self.streams.connected(self.connections);
                self.send(ClientFrame::Subscribe);
                self.tell_looking();
            }
            LinkEvent::Frame(frame) => self.on_frame(*frame),
            LinkEvent::Lost(reason) => self.lost(reason),
        }
    }

    fn lost(&mut self, reason: String) {
        self.linked = None;
        self.connected.store(false, Ordering::Relaxed);
        self.told = None;
        for (_, reply) in self.pending.drain() {
            let _ = reply.send(Err(BridgeError::Unreachable));
        }
        self.streams.disconnected();
        let since = match &self.link {
            LinkState::Down { since, .. } => *since,
            LinkState::Connecting | LinkState::Up { .. } => now(),
        };
        self.set_link(LinkState::Down { reason, since });
    }

    fn on_frame(&mut self, frame: ServerFrame) {
        match frame {
            ServerFrame::Snapshot(snapshot) => {
                self.model.replace(snapshot);
                if let Some(linked) = &self.linked {
                    let up = LinkState::Up {
                        version: linked.daemon_version.clone(),
                        protocol: linked.version,
                    };
                    self.set_link(up);
                }
                // Every (re)subscription is told whole.
                self.patcher.reset();
                self.state_changed();
            }
            ServerFrame::Delta(delta) => {
                self.model.apply(delta);
                self.state_changed();
            }
            ServerFrame::Reply { req_id, outcome } => {
                if let Some(reply) = self.pending.remove(&req_id) {
                    let _ = reply.send(outcome.map_err(|error| BridgeError::Refused {
                        code: error.code,
                        message: error.message,
                    }));
                }
            }
            ServerFrame::Reveal { session } => self.tell(StateMessage::Reveal { session }),
            frame @ (ServerFrame::Replay { .. }
            | ServerFrame::Output { .. }
            | ServerFrame::Modes { .. }
            | ServerFrame::Detached { .. }) => {
                self.streams.on_frame(frame);
                self.send_streams();
                self.schedule_flush_if_questions_changed();
            }
            ServerFrame::Welcome(_) | ServerFrame::Incompatible(_) | ServerFrame::Error(_) => {}
        }
    }

    /// The model changed: streams settle to it and a patch is due.
    fn state_changed(&mut self) {
        let model = &self.model;
        self.streams
            .prune(|session| model.session(session).is_some());
        self.streams.settle(|session| {
            model
                .session(session)
                .is_some_and(|stored| stored.ended_at.is_none() && !stored.status.is_exited())
        });
        self.settle_questions();
        self.schedule_flush();
    }

    /// Reads the screens of the sessions asking the person, while the
    /// window is seen and the state is the daemon's own.
    fn settle_questions(&mut self) {
        let asking: HashSet<SessionId> = if self.looking.seen && self.linked.is_some() {
            self.model
                .sessions()
                .iter()
                .filter(|session| matches!(session.status, SessionStatus::NeedsYou { .. }))
                .map(|session| session.id)
                .collect()
        } else {
            HashSet::new()
        };
        self.streams.ask(&asking);
        self.send_streams();
    }

    // Patches.

    fn schedule_flush_if_questions_changed(&mut self) {
        if self.streams.take_changed() {
            self.schedule_flush();
        }
    }

    /// A patch is due: now, or a frame after the last (`PATCH_EVERY`).
    /// Not while the window cannot be seen: the link then wakes for
    /// nothing, and the patch waits until it is seen again (`ARC-25`).
    fn schedule_flush(&mut self) {
        if self.flush_at.is_some() || !self.looking.seen {
            return;
        }
        let now = Instant::now();
        let at = self
            .last_flush
            .map_or(now, |last| (last + PATCH_EVERY).max(now));
        self.flush_at = Some(at);
    }

    /// Sends the webview what changed since the last patch.
    fn flush(&mut self) {
        self.flush_at = None;
        if self.sink.is_none() || !self.model.loaded() || !self.looking.seen {
            return;
        }
        self.last_flush = Some(Instant::now());
        let questions = self.streams.questions();
        if let Some(patch) = self.patcher.patch(&self.model, questions) {
            self.tell(StateMessage::Patch {
                patch: Box::new(patch),
            });
        }
    }
}

// Keeping the connection, off the loop.

/// Connects, starting the daemon if need be, and reconnects every half
/// second whenever the connection is lost, until the loop goes. Woken by
/// `again` (the person's *Try now*), it tries at once and starts the
/// daemon again whenever it last did.
async fn keep_linked(options: Options, events: mpsc::Sender<LinkEvent>, again: Arc<Notify>) {
    let start = options.start_daemon.map(Arc::new);
    let mut last_start: Option<Instant> = None;
    loop {
        match connect(&options.socket, start.as_ref(), &mut last_start, &again).await {
            Ok(connection) => {
                let (sender, receiver, welcome) = connection.into_split();
                // Unbounded on purpose: the loop must never wait on a
                // daemon that may be waiting for this client to read.
                #[expect(
                    clippy::disallowed_methods,
                    reason = "ARC-14: what the person does and the webview asks of the daemon, which takes it as soon as this client reads again"
                )]
                let (outgoing, frames) = mpsc::unbounded_channel();
                // Ends as the connection does.
                #[expect(
                    clippy::disallowed_methods,
                    reason = "ARC-13: the Owned handle holds it, for as long as the connection"
                )]
                let _writer = Owned(tokio::spawn(write_frames(sender, frames)));
                if events
                    .send(LinkEvent::Connected { outgoing, welcome })
                    .await
                    .is_err()
                {
                    return;
                }
                let Some(reason) = read_frames(receiver, &events).await else {
                    return;
                };
                if events.send(LinkEvent::Lost(reason)).await.is_err() {
                    return;
                }
            }
            Err(reason) => {
                if events.send(LinkEvent::Lost(reason)).await.is_err() {
                    return;
                }
            }
        }
        tokio::select! {
            () = tokio::time::sleep(RETRY_EVERY) => {}
            () = again.notified() => last_start = None,
        }
    }
}

/// Hands the loop each frame from the daemon, waiting for room: why the
/// connection ended, or `None` once the loop has.
async fn read_frames(
    mut receiver: FrameReceiver,
    events: &mpsc::Sender<LinkEvent>,
) -> Option<String> {
    loop {
        match receiver.next().await {
            Ok(Some(frame)) => {
                if events
                    .send(LinkEvent::Frame(Box::new(frame)))
                    .await
                    .is_err()
                {
                    return None;
                }
            }
            Ok(None) => return Some("the daemon closed the connection".to_owned()),
            Err(error) => return Some(error.to_string()),
        }
    }
}

/// Writes the loop's frames in order until the connection or the loop
/// goes.
async fn write_frames(mut sender: FrameSender, mut frames: mpsc::UnboundedReceiver<ClientFrame>) {
    while let Some(frame) = frames.recv().await {
        if sender.send(&frame).await.is_err() {
            // The reader notices too, and the link connects again.
            return;
        }
    }
}

/// Connects to the daemon, starting it if nothing answers and it was not
/// started in the last [`START_AGAIN_AFTER`]; why not, in words. Woken by
/// `again` while it waits for a daemon it started, it starts it again.
async fn connect(
    socket: &std::path::Path,
    start: Option<&Arc<Start>>,
    last_start: &mut Option<Instant>,
    again: &Notify,
) -> Result<Connection, String> {
    loop {
        match Connection::connect(socket, CLIENT).await {
            Err(ClientError::Unreachable { .. }) => {}
            other => return other.map_err(|error| error.to_string()),
        }
        let Some(start) = start else {
            return Connection::connect(socket, CLIENT)
                .await
                .map_err(|error| error.to_string());
        };
        if last_start.is_none_or(|at| at.elapsed() > START_AGAIN_AFTER) {
            *last_start = Some(Instant::now());
            let start = Arc::clone(start);
            // Starting asks the person's login shell for their PATH: off the
            // loop's threads.
            let started = tokio::task::spawn_blocking(move || start())
                .await
                .map_err(|error| error.to_string())?;
            if let Err(error) = started {
                return Err(format!("could not start Muster's daemon: {error}"));
            }
        }
        let deadline = Instant::now() + START_WAIT;
        loop {
            match Connection::connect(socket, CLIENT).await {
                Err(ClientError::Unreachable { .. }) if Instant::now() < deadline => {
                    tokio::select! {
                        () = tokio::time::sleep(Duration::from_millis(50)) => {}
                        () = again.notified() => break,
                    }
                }
                other => return other.map_err(|error| error.to_string()),
            }
        }
        // The person asked to try now: the daemon is started again at once.
        *last_start = None;
    }
}

#[cfg(test)]
#[path = "actor_tests.rs"]
mod tests;

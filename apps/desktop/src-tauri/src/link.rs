//! The link to the daemon (`ARCHITECTURE.md` §5c): the app's one
//! connection, found or started as the terminal UI's is and kept up every
//! half second; the daemon's state kept in the model the terminal UI keeps
//! (`muster-model`) and sent to the webview as patches of whole entities
//! with what the model derives; commands passed through as the daemon's
//! own; terminals streamed as raw frames.
//!
//! The link is a task of its own ([`Link::new`] hands it back to spawn);
//! the webview's commands reach it through the [`Link`] handle. What it
//! sends goes to sinks ([`StateSink`], [`FrameSink`]): Tauri's channels in
//! the app, plain channels in tests, so all of it is tested without a
//! window (`tests/link.rs` against the real daemon).

use std::fmt;
use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use muster_core::SessionId;
use muster_core::protocol::{Command, PixelSize, Reply, TermSize};
use tokio::sync::{Notify, mpsc, oneshot};

use crate::contract::{BridgeError, StateMessage};

mod actor;
mod daemon;
mod patches;
mod questions;
mod streams;

pub use daemon::{find_muster, starter};

/// How often a lost link tries again (`ARCHITECTURE.md` §5a, *Link*).
pub const RETRY_EVERY: Duration = Duration::from_millis(500);

/// How long the link waits for a daemon it started to answer before saying
/// it cannot reach it: the terminal UI's `START_WAIT`.
pub const START_WAIT: Duration = Duration::from_secs(5);

/// How long after starting the daemon the link may start it again, should
/// it still not answer: the terminal UI's `START_AGAIN_AFTER`, so one start
/// never overlaps the next (`muster daemon --detach` gives up on launchd
/// well within it).
pub const START_AGAIN_AFTER: Duration = Duration::from_secs(10);

/// The fewest milliseconds between two patches: one a frame (`ARC-25`).
pub const PATCH_EVERY: Duration = Duration::from_millis(16);

/// Where the webview's state goes: the channel `link_open` opened, in
/// order.
pub trait StateSink: Send + 'static {
    fn send(&self, message: StateMessage);
}

/// Where one terminal's frames go (`contract::frame`): the channel
/// `attach` opened. `false` once nobody reads it.
pub trait FrameSink: Send + 'static {
    fn send(&self, frame: Vec<u8>) -> bool;
}

/// Starts the daemon when none answers, as `muster daemon --detach` does.
pub type Start = Box<dyn Fn() -> std::io::Result<()> + Send + Sync>;

/// Where the link finds the daemon, and how it starts one.
pub struct Options {
    /// The daemon's socket (`muster_core::Paths::socket`).
    pub socket: PathBuf,
    /// Starts a daemon when none answers; `None` never starts one.
    pub start_daemon: Option<Start>,
}

impl fmt::Debug for Options {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Options")
            .field("socket", &self.socket)
            .field("starts_daemon", &self.start_daemon.is_some())
            .finish()
    }
}

/// What the webview asks of the link, in the order it asked.
enum Request {
    Open(Box<dyn StateSink>),
    Command {
        command: Box<Command>,
        reply: oneshot::Sender<Result<Reply, BridgeError>>,
    },
    Attach {
        session: SessionId,
        from_seq: Option<u64>,
        watching: bool,
        sink: Box<dyn FrameSink>,
    },
    Detach {
        session: SessionId,
    },
    Input {
        session: SessionId,
        data: Vec<u8>,
    },
    Resize {
        session: SessionId,
        size: TermSize,
        pixels: Option<PixelSize>,
    },
    Watch {
        session: SessionId,
        watching: bool,
    },
    Looking(Looking),
}

/// How the person looks at the window, as the webview last said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Looking {
    /// The window can be seen: not hidden, minimised or covered. While it
    /// cannot, the link sends the webview nothing and streams no question.
    pub seen: bool,
    /// The person looks at it: seen and in focus (`ClientFrame::Looking`).
    pub looking: bool,
    /// The session whose terminal is in focus in it (`ClientFrame::Keys`).
    pub focused: Option<SessionId>,
}

impl Default for Looking {
    /// A window opens seen and looked at, with no terminal in focus.
    fn default() -> Self {
        Self {
            seen: true,
            looking: true,
            focused: None,
        }
    }
}

/// The webview's way to the link. Cheap to clone; the link ends once every
/// handle has gone.
#[derive(Clone)]
pub struct Link {
    requests: mpsc::UnboundedSender<Request>,
    connected: Arc<AtomicBool>,
    /// Wakes the loop that keeps the connection: *Try now*.
    again: Arc<Notify>,
}

impl fmt::Debug for Link {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Link")
            .field("connected", &self.connected.load(Ordering::Relaxed))
            .finish_non_exhaustive()
    }
}

impl Link {
    /// The link's handle, and the task that is the link, for the caller to
    /// spawn and own (`ARC-13`). It connects at once and ends once every
    /// handle has gone, taking its connection with it.
    pub fn new(options: Options) -> (Self, impl Future<Output = ()> + Send + 'static) {
        #[expect(
            clippy::disallowed_methods,
            reason = "ARC-14: what the person does in the window, which the link takes at once"
        )]
        let (requests, received) = mpsc::unbounded_channel();
        let connected = Arc::new(AtomicBool::new(false));
        let again = Arc::new(Notify::new());
        let task = actor::run(options, received, connected.clone(), again.clone());
        (
            Self {
                requests,
                connected,
                again,
            },
            task,
        )
    }

    fn ask(&self, request: Request) -> Result<(), BridgeError> {
        self.requests
            .send(request)
            .map_err(|_| BridgeError::Unreachable)
    }

    /// Tries to reach the daemon now rather than in half a second, and
    /// starts it again if nothing answers, however lately it was started:
    /// the person's *Try now* while it is out of reach.
    pub fn try_now(&self) {
        self.again.notify_one();
    }

    /// Whether the link is connected to a daemon just now.
    pub fn is_connected(&self) -> bool {
        self.connected.load(Ordering::Relaxed)
    }

    fn connected_or_unreachable(&self) -> Result<(), BridgeError> {
        if self.is_connected() {
            Ok(())
        } else {
            Err(BridgeError::Unreachable)
        }
    }

    /// The webview's state goes to `sink` from now on: first how the link
    /// stands, then, once the state has come, a patch with `reset`.
    pub fn open(&self, sink: impl StateSink) -> Result<(), BridgeError> {
        self.ask(Request::Open(Box::new(sink)))
    }

    /// Sends `command` to the daemon and waits for its reply.
    pub async fn command(&self, command: Command) -> Result<Reply, BridgeError> {
        let (reply, answer) = oneshot::channel();
        self.ask(Request::Command {
            command: Box::new(command),
            reply,
        })?;
        answer.await.map_err(|_| BridgeError::Unreachable)?
    }

    /// Streams `session`'s terminal to `sink`, from `from_seq` (the next
    /// byte the webview's screen lacks) or from all the daemon holds. Kept
    /// while the daemon is out of reach, and streamed again once it is back.
    pub fn attach(
        &self,
        session: SessionId,
        from_seq: Option<u64>,
        watching: bool,
        sink: impl FrameSink,
    ) -> Result<(), BridgeError> {
        self.ask(Request::Attach {
            session,
            from_seq,
            watching,
            sink: Box::new(sink),
        })
    }

    /// Stops streaming `session`'s terminal to the webview.
    pub fn detach(&self, session: SessionId) -> Result<(), BridgeError> {
        self.ask(Request::Detach { session })
    }

    /// Keystrokes or pasted text for `session`'s program.
    pub fn input(&self, session: SessionId, data: Vec<u8>) -> Result<(), BridgeError> {
        self.connected_or_unreachable()?;
        self.ask(Request::Input { session, data })
    }

    /// The webview's view of `session`'s terminal changed size.
    pub fn resize(
        &self,
        session: SessionId,
        size: TermSize,
        pixels: Option<PixelSize>,
    ) -> Result<(), BridgeError> {
        self.connected_or_unreachable()?;
        self.ask(Request::Resize {
            session,
            size,
            pixels,
        })
    }

    /// Whether the person is looking at an attached session.
    pub fn watch(&self, session: SessionId, watching: bool) -> Result<(), BridgeError> {
        self.ask(Request::Watch { session, watching })
    }

    /// How the person looks at the window now.
    pub fn looking(&self, looking: Looking) -> Result<(), BridgeError> {
        self.ask(Request::Looking(looking))
    }
}

/// The wall clock, for when the link went down.
#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: the app reads the clock here and in `monotonic` only"
)]
fn now() -> muster_core::Timestamp {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| {
            i64::try_from(since.as_millis()).unwrap_or(i64::MAX)
        });
    muster_core::Timestamp::from_millis(millis)
}

/// The monotonic clock, for a deadline waited for off the link's loop.
#[expect(
    clippy::disallowed_methods,
    reason = "ARC-03: the app reads the clock here and in `now` only"
)]
fn monotonic() -> std::time::Instant {
    std::time::Instant::now()
}

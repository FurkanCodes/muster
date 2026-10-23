//! The link to the daemon, off the UI's loop: connecting, starting the
//! daemon if none answers, and carrying frames both ways.
//!
//! Frames from the daemon wait for the loop in a bounded queue
//! ([`LINK_EVENTS`]). A loop that falls behind fills it, the link stops
//! reading the socket, and the daemon's writer waits on this client as on
//! any slow one: its replies and state wait their turn, and a session's
//! terminal output is dropped and resumed from the ring (`ARC-14`).
//! Nothing that must arrive is lost: every frame, and the link's end, waits
//! for room. Frames for the daemon go out through a writer of their own, so
//! the loop never waits on a daemon that may itself be waiting for this
//! client to read.

use std::time::{Duration, Instant};

use muster_client::{ClientError, Connection, FrameReceiver, FrameSender};
use muster_core::protocol::{ClientFrame, ServerFrame, Welcome};
use tokio::sync::mpsc;
use tokio_util::task::AbortOnDropHandle;

use super::{Options, monotonic};

/// Events from the link waiting for the UI's loop before the link stops
/// reading the daemon.
pub(super) const LINK_EVENTS: usize = 64;

/// How long the UI tries to reach a daemon it started before saying it
/// cannot.
pub const START_WAIT: Duration = Duration::from_secs(5);

/// How long after starting the daemon the UI may start it again, should it
/// still not answer. `muster daemon --detach` gives up on launchd and
/// starts the daemon itself well within it, so one start never overlaps
/// the next.
pub const START_AGAIN_AFTER: Duration = Duration::from_secs(10);

const CLIENT: &str = concat!("muster-tui ", env!("CARGO_PKG_VERSION"));

/// What the link reports.
pub(super) enum LinkEvent {
    Connected { sender: Outgoing, welcome: Welcome },
    Frame(Box<ServerFrame>),
    Lost(String),
}

/// Frames for the daemon on one connection, written as the daemon reads
/// them.
#[derive(Clone, Debug)]
pub(super) struct Outgoing(mpsc::UnboundedSender<ClientFrame>);

impl Outgoing {
    /// Hands `frame` to the writer, at once; `false` once the connection
    /// has gone.
    pub(super) fn send(&self, frame: ClientFrame) -> bool {
        self.0.send(frame).is_ok()
    }
}

/// Connects, starting the daemon if needed, and reconnects whenever the
/// connection is lost.
pub(super) async fn keep_linked(options: Options, events: mpsc::Sender<LinkEvent>) {
    let mut last_start: Option<Instant> = None;
    // Tries in a row that failed: another machine is tried again less and
    // less often, up to [`FARTHEST_RETRY`], not every half second.
    let mut failed: u32 = 0;
    loop {
        match connect(&options, &mut last_start).await {
            Ok(connection) => {
                failed = 0;
                let (sender, receiver, welcome) = connection.into_split();
                // Unbounded on purpose: the loop must never wait on a daemon
                // that may be waiting for this client to read.
                #[expect(
                    clippy::disallowed_methods,
                    reason = "ARC-14: what the person does and the UI asks of the daemon, which takes it as soon as this client reads again"
                )]
                let (outgoing, frames) = mpsc::unbounded_channel();
                // Ends as the connection does (`ARC-13`).
                #[expect(
                    clippy::disallowed_methods,
                    reason = "ARC-13: the AbortOnDropHandle owns it, for as long as the connection"
                )]
                let _writer = AbortOnDropHandle::new(tokio::spawn(write_frames(sender, frames)));
                let sender = Outgoing(outgoing);
                if events
                    .send(LinkEvent::Connected { sender, welcome })
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
            Err(error) => {
                failed = failed.saturating_add(1);
                if events
                    .send(LinkEvent::Lost(error.to_string()))
                    .await
                    .is_err()
                {
                    return;
                }
            }
        }
        tokio::time::sleep(retry_after(options.connect.is_some(), failed)).await;
    }
}

/// The longest another machine's link waits before it is tried again.
const FARTHEST_RETRY: Duration = Duration::from_secs(8);

/// How long the link waits before connecting again, `failed` tries in a
/// row having failed: half a second here; for another machine, twice as
/// long after each failure, up to [`FARTHEST_RETRY`].
fn retry_after(remote: bool, failed: u32) -> Duration {
    let first = Duration::from_millis(500);
    if !remote {
        return first;
    }
    first
        .saturating_mul(2_u32.saturating_pow(failed.saturating_sub(1)))
        .min(FARTHEST_RETRY)
}

/// Hands the loop each frame from the daemon, waiting for room: why the
/// connection ended, or `None` once the UI has.
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

/// Writes the UI's frames in order until the connection or the UI goes.
async fn write_frames(mut sender: FrameSender, mut frames: mpsc::UnboundedReceiver<ClientFrame>) {
    while let Some(frame) = frames.recv().await {
        if sender.send(&frame).await.is_err() {
            // The reader notices too, and the link connects again.
            return;
        }
    }
}

/// Connects to the daemon, starting it if nothing answers and it was not
/// started in the last [`START_AGAIN_AFTER`].
async fn connect(
    options: &Options,
    last_start: &mut Option<Instant>,
) -> Result<Connection, ClientError> {
    if let Some(connect) = &options.connect {
        return connect().await;
    }
    match Connection::connect(&options.socket, CLIENT).await {
        Err(ClientError::Unreachable { .. }) => {}
        other => return other,
    }
    let Some(start) = &options.start_daemon else {
        return Connection::connect(&options.socket, CLIENT).await;
    };
    if last_start.is_none_or(|at| monotonic().saturating_duration_since(at) > START_AGAIN_AFTER) {
        *last_start = Some(monotonic());
        if let Err(error) = start() {
            return Err(ClientError::Unreachable {
                path: options.socket.display().to_string(),
                source: error,
            });
        }
    }
    let deadline = monotonic() + START_WAIT;
    loop {
        match Connection::connect(&options.socket, CLIENT).await {
            Err(ClientError::Unreachable { .. }) if monotonic() < deadline => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
            other => return other,
        }
    }
}

#[cfg(test)]
#[path = "link_tests.rs"]
mod tests;

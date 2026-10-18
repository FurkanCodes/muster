//! Streaming one session's terminal to one client.

use std::sync::Arc;

use muster_core::SessionId;
use muster_core::protocol::{DetachReason, ServerFrame, TerminalModes};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;

use crate::pty::PtyHandle;
use crate::server::outbox::{Outbox, Pushed};

/// How streaming one program's terminal ended.
enum Ended {
    /// The client is gone.
    ClientGone,
    /// The program exited and everything it wrote was sent.
    Exited,
    /// The session moved to a new program.
    Replaced,
    /// The session's terminal was released.
    Released,
}

/// Sends a replay from `from_seq` and the input modes, then live output and
/// any change of modes, then `Detached` when the program exits. A client
/// that falls behind the live stream is sent a new replay from where it
/// was, which is marked `reset` if the ring has since dropped those bytes;
/// so is one that cannot take the output as fast as it comes, once it has
/// caught up (`server::outbox`).
///
/// The stream follows the session: `current` always holds its terminal,
/// and when a new program starts in it the client is sent that program's
/// terminal from the start, with a reset. It ends when the session's
/// terminal is released, or the client goes.
///
/// The first replay is history; each later one says where the output
/// written since it begins (`live_from`), so a program's copy the client
/// missed live still reaches it, and one in history never does.
pub(crate) async fn forward_output(
    session: SessionId,
    mut current: watch::Receiver<Option<PtyHandle>>,
    from_seq: Option<u64>,
    outbox: Arc<Outbox>,
) {
    let stream = Stream { session, outbox };
    let mut from = from_seq;
    // The end of the first replay: what was written before the client
    // attached.
    let mut history = None;
    loop {
        let pty = current.borrow_and_update().clone();
        let Some(pty) = pty else {
            stream.ended(DetachReason::NotRunning);
            return;
        };
        match forward_program(&stream, &pty, &mut from, &mut history, &mut current).await {
            Ended::ClientGone => return,
            Ended::Released => {
                stream.ended(DetachReason::NotRunning);
                return;
            }
            Ended::Replaced => {}
            Ended::Exited => {
                if !stream.ended(DetachReason::Exited) {
                    return;
                }
                // Wait for a new program in the session, or its release.
                if current.changed().await.is_err() {
                    return;
                }
            }
        }
    }
}

/// One session's stream to one client, through its connection's outbox.
struct Stream {
    session: SessionId,
    outbox: Arc<Outbox>,
}

impl Stream {
    /// Offers a frame; `None` once the client is gone.
    fn send(&self, frame: ServerFrame) -> Option<Pushed> {
        self.outbox.push(self.session, frame).ok()
    }

    /// Tells the client the stream ended; `false` once the client is gone.
    fn ended(&self, reason: DetachReason) -> bool {
        let session = self.session;
        self.send(ServerFrame::Detached { session, reason })
            .is_some()
    }

    /// Tells the client the modes; `false` once it is gone.
    fn modes(&self, modes: TerminalModes) -> bool {
        let session = self.session;
        self.send(ServerFrame::Modes { session, modes }).is_some()
    }
}

/// Streams one program's terminal from `from`, which is kept at the next
/// byte the client expects; `history` is where the stream's first replay
/// ended, once there was one.
async fn forward_program(
    stream: &Stream,
    pty: &PtyHandle,
    from: &mut Option<u64>,
    history: &mut Option<u64>,
    current: &mut watch::Receiver<Option<PtyHandle>>,
) -> Ended {
    let session = stream.session;
    loop {
        // A client that could not keep up is sent more once it has caught
        // up, from the first byte it missed, as the ring holds it.
        if stream.outbox.is_behind(session) {
            tokio::select! {
                biased;
                changed = current.changed() => {
                    return match changed {
                        Ok(()) => Ended::Replaced,
                        Err(_) => Ended::Released,
                    };
                }
                resume = stream.outbox.caught_up(session) => match resume {
                    // Starting over, the client is sent all the ring holds.
                    Ok(resume) => *from = (!resume.reset).then_some(resume.seq),
                    Err(_) => return Ended::ClientGone,
                },
            }
        }
        let Some(attachment) = pty.attach(*from).await else {
            return Ended::Released;
        };
        let replay = attachment.replay;
        let mut next = replay.seq + replay.data.len() as u64;
        *from = Some(next);
        let live_from = *history;
        history.get_or_insert(next);
        let sent = stream.send(ServerFrame::Replay {
            session,
            seq: replay.seq,
            data: replay.data,
            reset: replay.reset,
            live_from,
        });
        match sent {
            Some(Pushed::Queued) => {}
            // It did not fit behind what waits: resume once caught up.
            Some(Pushed::Behind) => continue,
            None => return Ended::ClientGone,
        }
        let mut modes = attachment.modes;
        if !stream.modes(modes) {
            return Ended::ClientGone;
        }
        let Some(mut live) = attachment.live else {
            return Ended::Exited;
        };
        loop {
            tokio::select! {
                // What the old program wrote goes out before the switch.
                biased;
                chunk = live.recv() => match chunk {
                    Ok(chunk) => {
                        let end = chunk.seq + chunk.data.len() as u64;
                        if end <= next {
                            continue;
                        }
                        // Never resend bytes the replay already carried.
                        let skip = usize::try_from(next.saturating_sub(chunk.seq)).unwrap_or(0);
                        let frame = ServerFrame::Output {
                            session,
                            seq: chunk.seq + skip as u64,
                            data: chunk.data[skip..].to_vec(),
                        };
                        match stream.send(frame) {
                            Some(Pushed::Queued) => {}
                            // It cannot keep up: resume once it has.
                            Some(Pushed::Behind) => break,
                            None => return Ended::ClientGone,
                        }
                        next = end;
                        *from = Some(next);
                        if chunk.modes != modes {
                            modes = chunk.modes;
                            if !stream.modes(modes) {
                                return Ended::ClientGone;
                            }
                        }
                    }
                    // Fell behind: replay from where the client is.
                    Err(RecvError::Lagged(_)) => break,
                    Err(RecvError::Closed) => return Ended::Exited,
                },
                changed = current.changed() => {
                    return match changed {
                        Ok(()) => Ended::Replaced,
                        Err(_) => Ended::Released,
                    };
                }
            }
        }
    }
}

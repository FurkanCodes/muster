//! Streaming one session's terminal to one client.

use muster_core::SessionId;
use muster_core::protocol::{DetachReason, ServerFrame, TerminalModes};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{mpsc, watch};

use crate::pty::PtyHandle;

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
/// was, which is marked `reset` if the ring has since dropped those bytes.
///
/// The stream follows the session: `current` always holds its terminal,
/// and when a new program starts in it the client is sent that program's
/// terminal from the start, with a reset. It ends when the session's
/// terminal is released, or the client goes.
pub(crate) async fn forward_output(
    session: SessionId,
    mut current: watch::Receiver<Option<PtyHandle>>,
    from_seq: Option<u64>,
    outgoing: mpsc::Sender<ServerFrame>,
) {
    let mut from = from_seq;
    loop {
        let pty = current.borrow_and_update().clone();
        let Some(pty) = pty else {
            detached(&outgoing, session, DetachReason::NotRunning).await;
            return;
        };
        match forward_program(session, &pty, &mut from, &mut current, &outgoing).await {
            Ended::ClientGone => return,
            Ended::Released => {
                detached(&outgoing, session, DetachReason::NotRunning).await;
                return;
            }
            Ended::Replaced => {}
            Ended::Exited => {
                if !detached(&outgoing, session, DetachReason::Exited).await {
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

/// Streams one program's terminal from `from`, which is kept at the next
/// byte the client expects.
async fn forward_program(
    session: SessionId,
    pty: &PtyHandle,
    from: &mut Option<u64>,
    current: &mut watch::Receiver<Option<PtyHandle>>,
    outgoing: &mpsc::Sender<ServerFrame>,
) -> Ended {
    loop {
        let Some(attachment) = pty.attach(*from).await else {
            return Ended::Released;
        };
        let replay = attachment.replay;
        let mut next = replay.seq + replay.data.len() as u64;
        *from = Some(next);
        let sent = outgoing
            .send(ServerFrame::Replay {
                session,
                seq: replay.seq,
                data: replay.data,
                reset: replay.reset,
            })
            .await;
        if sent.is_err() {
            return Ended::ClientGone;
        }
        let mut modes = attachment.modes;
        if !send_modes(outgoing, session, modes).await {
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
                        if outgoing.send(frame).await.is_err() {
                            return Ended::ClientGone;
                        }
                        next = end;
                        *from = Some(next);
                        if chunk.modes != modes {
                            modes = chunk.modes;
                            if !send_modes(outgoing, session, modes).await {
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

/// Tells the client the stream ended; `false` once the client is gone.
async fn detached(
    outgoing: &mpsc::Sender<ServerFrame>,
    session: SessionId,
    reason: DetachReason,
) -> bool {
    outgoing
        .send(ServerFrame::Detached { session, reason })
        .await
        .is_ok()
}

/// Tells the client the modes; `false` once it is gone.
async fn send_modes(
    outgoing: &mpsc::Sender<ServerFrame>,
    session: SessionId,
    modes: TerminalModes,
) -> bool {
    outgoing
        .send(ServerFrame::Modes { session, modes })
        .await
        .is_ok()
}

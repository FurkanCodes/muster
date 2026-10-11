//! Streaming one session's terminal to one client.

use muster_core::SessionId;
use muster_core::protocol::{DetachReason, ServerFrame};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::mpsc;

use crate::pty::PtyHandle;

/// Sends a replay from `from_seq`, then live output, then `Detached` when the
/// process exits. A client that falls behind the live stream is sent a new
/// replay from where it was, which is marked `reset` if the ring has since
/// dropped those bytes.
pub(crate) async fn forward_output(
    session: SessionId,
    pty: PtyHandle,
    from_seq: Option<u64>,
    outgoing: mpsc::Sender<ServerFrame>,
) {
    let mut from = from_seq;
    loop {
        let Some(attachment) = pty.attach(from).await else {
            let _ = outgoing
                .send(ServerFrame::Detached {
                    session,
                    reason: DetachReason::NotRunning,
                })
                .await;
            return;
        };
        let replay = attachment.replay;
        let mut next = replay.seq + replay.data.len() as u64;
        let sent = outgoing
            .send(ServerFrame::Replay {
                session,
                seq: replay.seq,
                data: replay.data,
                reset: replay.reset,
            })
            .await;
        if sent.is_err() {
            return;
        }
        let Some(mut live) = attachment.live else {
            let _ = outgoing
                .send(ServerFrame::Detached {
                    session,
                    reason: DetachReason::Exited,
                })
                .await;
            return;
        };
        loop {
            match live.recv().await {
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
                        return;
                    }
                    next = end;
                }
                Err(RecvError::Lagged(_)) => {
                    from = Some(next);
                    break;
                }
                Err(RecvError::Closed) => {
                    let _ = outgoing
                        .send(ServerFrame::Detached {
                            session,
                            reason: DetachReason::Exited,
                        })
                        .await;
                    return;
                }
            }
        }
    }
}

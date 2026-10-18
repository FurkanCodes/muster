//! What one connection has to send, and the order it goes in.
//!
//! Two kinds of frame leave for a client. Replies, the state and its
//! deltas, and the like must all arrive, in order: they wait in a bounded
//! queue, and whoever sends one waits while it is full (a delta pump that
//! falls behind is sent a fresh snapshot instead). A session's terminal
//! output need not: the session's ring holds it, numbered by byte, so a
//! client that cannot keep up is better sent where the output is now than
//! everything it missed, in the order it was written, long after. Each
//! attached session's stream frames (replay, output, modes, the end) so
//! wait in a lane of their own, ahead of nothing else, and a lane holds at
//! most [`LANE_BUDGET`] bytes of output, but for the one replay a client
//! resumes with. Past that the client is behind:
//! what waits in the lane is dropped, its stream marked to resume from the
//! first byte dropped, and nothing more is queued for it. Once the writer
//! has written a frame since, so the client is reading again, the session's
//! forwarder sends a replay from that byte, which the ring still holds, or
//! the whole ring marked `reset` if it fell further behind than the ring
//! holds (`ARC-14`). Should the stream end while its client is behind, so
//! nothing is ever resent, the end follows an empty replay marked `reset`
//! from the end of what was dropped. No byte reaches the client twice, and
//! a gap is always a reset.
//!
//! The writer takes queued frames first, then each session's lanes in turn,
//! one frame each.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use muster_client::FramedWrite;
use muster_core::SessionId;
use muster_core::protocol::ServerFrame;
use tokio::io::AsyncWrite;
use tokio::sync::{Notify, mpsc};

/// The most terminal output kept waiting for one session before its
/// client counts as behind.
pub(crate) const LANE_BUDGET: usize = 256 * 1024;

/// Where a client that fell behind continues a session's stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resume {
    /// The first byte it was not sent.
    pub(crate) seq: u64,
    /// What was dropped began a stream anew (a replay that started over,
    /// or a program's end), so the client starts over too.
    pub(crate) reset: bool,
}

/// What became of a stream frame offered to a lane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Pushed {
    Queued,
    /// The client is behind; the frame, and what waited, was dropped.
    Behind,
}

/// The connection's writer has ended; the client is gone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Gone;

/// Each attached session's stream frames waiting for the writer: the
/// policy, with no IO, so it is tested on its own.
#[derive(Debug)]
pub(crate) struct Lanes {
    sessions: HashMap<SessionId, Lane>,
    /// Sessions with frames waiting, in the order they are served.
    turns: VecDeque<SessionId>,
    budget: usize,
}

#[derive(Debug, Default)]
struct Lane {
    /// Each frame, with the bytes of output it counts for.
    frames: VecDeque<(ServerFrame, usize)>,
    /// Bytes of output waiting.
    bytes: usize,
    /// Output was dropped; the client resumes once the writer catches up.
    behind: Option<Behind>,
}

#[derive(Debug)]
struct Behind {
    resume: Resume,
    /// The end of the output dropped, or refused since.
    dropped_to: u64,
    /// The writer has written a frame since the output was dropped.
    caught_up: bool,
}

impl Lanes {
    pub(crate) fn new(budget: usize) -> Self {
        Self {
            sessions: HashMap::new(),
            turns: VecDeque::new(),
            budget,
        }
    }

    /// Offers one of `session`'s stream frames. Output past the budget
    /// drops what waits and puts the session behind, as does a replay that
    /// does not fit behind what waits. A replay into an empty lane, as a
    /// client resumes, is not counted, so it is never dropped for its own
    /// size; it and the stream's end begin the stream afresh. The same end
    /// twice in a row is said once, and an end while the client is behind
    /// follows a reset, as what it missed is never resent.
    pub(crate) fn push(&mut self, session: SessionId, frame: ServerFrame) -> Pushed {
        let lane = self.sessions.entry(session).or_default();
        let mut cost = 0;
        let mut first = None;
        match &frame {
            ServerFrame::Output { seq, data, .. } | ServerFrame::Replay { seq, data, .. } => {
                let replay = matches!(frame, ServerFrame::Replay { .. });
                let end = seq + data.len() as u64;
                if let Some(behind) = lane.behind.as_mut().filter(|_| !replay) {
                    behind.dropped_to = behind.dropped_to.max(end);
                    return Pushed::Behind;
                }
                if !(replay && lane.frames.is_empty()) {
                    cost = data.len();
                }
                if lane.bytes + cost > self.budget {
                    let (resume, dropped_to) = lane.drop_waiting(*seq);
                    lane.behind = Some(Behind {
                        resume,
                        dropped_to: dropped_to.max(end),
                        caught_up: false,
                    });
                    self.turns.retain(|waiting| *waiting != session);
                    return Pushed::Behind;
                }
                lane.behind = None;
            }
            ServerFrame::Detached { .. } => {
                if lane.frames.back().is_some_and(|(last, _)| *last == frame) {
                    return Pushed::Queued;
                }
                // The stream ends with the client behind: what it missed is
                // never resent, so it starts over, with nothing, where the
                // output dropped ends (`ARC-14`).
                first = lane.behind.take().map(|behind| ServerFrame::Replay {
                    session,
                    seq: behind.dropped_to,
                    data: Vec::new(),
                    reset: true,
                });
            }
            _ => {}
        }
        if lane.frames.is_empty() {
            self.turns.push_back(session);
        }
        lane.frames.extend(first.map(|reset| (reset, 0)));
        lane.bytes += cost;
        lane.frames.push_back((frame, cost));
        Pushed::Queued
    }

    /// The next stream frame to write, each session in turn.
    pub(crate) fn pop(&mut self) -> Option<ServerFrame> {
        while let Some(session) = self.turns.pop_front() {
            let Some(lane) = self.sessions.get_mut(&session) else {
                continue;
            };
            let Some((frame, cost)) = lane.frames.pop_front() else {
                continue;
            };
            lane.bytes -= cost;
            if !lane.frames.is_empty() {
                self.turns.push_back(session);
            } else if lane.behind.is_none() {
                self.sessions.remove(&session);
            }
            return Some(frame);
        }
        None
    }

    /// The writer wrote a frame, or has nothing to write: every session
    /// behind may resume. Returns whether any now may.
    pub(crate) fn catch_up(&mut self) -> bool {
        let mut any = false;
        for behind in self
            .sessions
            .values_mut()
            .filter_map(|lane| lane.behind.as_mut())
        {
            any |= !behind.caught_up;
            behind.caught_up = true;
        }
        any
    }

    /// Whether `session`'s client is behind its stream.
    pub(crate) fn is_behind(&self, session: SessionId) -> bool {
        self.sessions
            .get(&session)
            .is_some_and(|lane| lane.behind.is_some())
    }

    /// Where `session`'s client resumes, once the writer has caught up.
    pub(crate) fn resume(&self, session: SessionId) -> Option<Resume> {
        let behind = self.sessions.get(&session)?.behind.as_ref()?;
        behind.caught_up.then_some(behind.resume)
    }

    /// Drops everything waiting for `session`: its stream stopped.
    pub(crate) fn clear(&mut self, session: SessionId) {
        self.sessions.remove(&session);
        self.turns.retain(|waiting| *waiting != session);
    }

    /// How many sessions have a lane, for tests of `ARC-14`.
    #[cfg(test)]
    fn len(&self) -> usize {
        self.sessions.len()
    }
}

impl Lane {
    /// Drops every frame waiting; returns where the client resumes, given
    /// that `pushed` is the position of the output that did not fit, and
    /// the end of the output dropped.
    fn drop_waiting(&mut self, pushed: u64) -> (Resume, u64) {
        let output = |frame: &ServerFrame| match frame {
            ServerFrame::Replay { seq, data, .. } | ServerFrame::Output { seq, data, .. } => {
                Some((*seq, seq + data.len() as u64))
            }
            _ => None,
        };
        let first = self.frames.iter().find_map(|(frame, _)| output(frame));
        let end = self
            .frames
            .iter()
            .filter_map(|(frame, _)| output(frame))
            .map(|(_, end)| end)
            .max()
            .unwrap_or(pushed);
        let reset = self.frames.iter().any(|(frame, _)| {
            matches!(
                frame,
                ServerFrame::Replay { reset: true, .. } | ServerFrame::Detached { .. }
            )
        });
        self.frames.clear();
        self.bytes = 0;
        let resume = Resume {
            seq: first.map_or(pushed, |(seq, _)| seq),
            reset,
        };
        (resume, end)
    }
}

/// One connection's lanes, shared by its forwarders and its writer: a leaf
/// of the connection's own, never held across a wait (`ARC-02`).
#[derive(Debug)]
pub(crate) struct Outbox {
    state: Mutex<State>,
    /// Wakes the writer when a frame is waiting.
    ready: Notify,
    /// Wakes forwarders when their client caught up, or is gone.
    caught_up: Notify,
}

#[derive(Debug)]
struct State {
    lanes: Lanes,
    /// The writer has ended.
    closed: bool,
}

impl Outbox {
    pub(crate) fn new(budget: usize) -> Arc<Self> {
        Arc::new(Self {
            state: Mutex::new(State {
                lanes: Lanes::new(budget),
                closed: false,
            }),
            ready: Notify::new(),
            caught_up: Notify::new(),
        })
    }

    fn state(&self) -> MutexGuard<'_, State> {
        // Nothing holding it can panic mid-change; carry on with it.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Offers one of `session`'s stream frames to the writer.
    pub(crate) fn push(&self, session: SessionId, frame: ServerFrame) -> Result<Pushed, Gone> {
        let pushed = {
            let mut state = self.state();
            if state.closed {
                return Err(Gone);
            }
            state.lanes.push(session, frame)
        };
        // Either way the writer looks: a client behind with nothing left
        // to write may resume at once.
        self.ready.notify_one();
        Ok(pushed)
    }

    /// Whether `session`'s client is behind its stream.
    pub(crate) fn is_behind(&self, session: SessionId) -> bool {
        self.state().lanes.is_behind(session)
    }

    /// Waits until `session`'s client, behind, has caught up: where it
    /// resumes.
    pub(crate) async fn caught_up(&self, session: SessionId) -> Result<Resume, Gone> {
        loop {
            let woken = self.caught_up.notified();
            tokio::pin!(woken);
            // Counted as waiting before looking, so no wake is missed.
            woken.as_mut().enable();
            {
                let state = self.state();
                if state.closed {
                    return Err(Gone);
                }
                if let Some(resume) = state.lanes.resume(session) {
                    return Ok(resume);
                }
            }
            woken.await;
        }
    }

    /// Drops what waits for `session`: its stream stopped.
    pub(crate) fn clear(&self, session: SessionId) {
        self.state().lanes.clear(session);
    }

    /// The next stream frame, waiting for one. With nothing to write, the
    /// client is reading: whoever fell behind may resume.
    async fn next(&self) -> ServerFrame {
        loop {
            let popped = self.state().lanes.pop();
            if let Some(frame) = popped {
                return frame;
            }
            self.catch_up();
            // A frame offered meanwhile left its wake behind.
            self.ready.notified().await;
        }
    }

    /// The writer wrote a frame or has nothing to write: clients behind
    /// may resume.
    fn catch_up(&self) {
        if self.state().lanes.catch_up() {
            self.caught_up.notify_waiters();
        }
    }

    /// The writer ended: nothing more is taken, and forwarders waiting
    /// learn the client is gone.
    fn close(&self) {
        self.state().closed = true;
        self.caught_up.notify_waiters();
    }
}

/// Closes the outbox however the writer ends.
struct Closing(Arc<Outbox>);

impl Drop for Closing {
    fn drop(&mut self) {
        self.0.close();
    }
}

/// Writes the connection's frames until the queue closes, the client goes,
/// or a frame that ends the connection has been sent: queued frames first,
/// then the sessions' streams in turn.
pub(crate) async fn write_frames<W: AsyncWrite + Unpin>(
    mut writer: FramedWrite<W>,
    mut queue: mpsc::Receiver<ServerFrame>,
    outbox: Arc<Outbox>,
) {
    let closing = Closing(outbox);
    loop {
        // Whoever fell behind while the last frame was written may resume.
        closing.0.catch_up();
        let frame = tokio::select! {
            biased;
            frame = queue.recv() => match frame {
                Some(frame) => frame,
                None => break,
            },
            frame = closing.0.next() => frame,
        };
        let last = matches!(frame, ServerFrame::Error(_) | ServerFrame::Incompatible(_));
        if writer.send(&frame).await.is_err() || last {
            break;
        }
    }
    drop(closing);
    let _ = writer.shutdown().await;
}

#[cfg(test)]
#[path = "outbox_tests.rs"]
mod tests;

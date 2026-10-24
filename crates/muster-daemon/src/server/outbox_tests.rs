//! The writer's policy (`ARC-14`): replies and the state arrive, in order;
//! a session's output that a client cannot take is dropped and resumed
//! from the ring, never shown twice and never with a silent gap.

use std::time::Duration;

use muster_client::FramedRead;
use muster_core::protocol::{DetachReason, Reply, TerminalModes};

use super::*;
use crate::waiting;

fn output(session: SessionId, seq: u64, len: usize) -> ServerFrame {
    ServerFrame::Output {
        session,
        seq,
        data: vec![b'x'; len],
    }
}

fn replay(session: SessionId, seq: u64, len: usize, reset: bool) -> ServerFrame {
    ServerFrame::Replay {
        session,
        seq,
        data: vec![b'r'; len],
        reset,
        live_from: None,
        screen: None,
    }
}

/// Where a stream frame starts, and whether it is a replay.
fn position(frame: &ServerFrame) -> Option<(u64, bool)> {
    match frame {
        ServerFrame::Replay { seq, .. } => Some((*seq, true)),
        ServerFrame::Output { seq, .. } => Some((*seq, false)),
        _ => None,
    }
}

#[test]
fn a_session_s_frames_keep_their_order_and_sessions_take_turns() {
    let (one, two) = (SessionId::new(), SessionId::new());
    let mut lanes = Lanes::new(1024);
    for frame in [
        replay(one, 0, 4, true),
        output(one, 4, 2),
        output(one, 6, 2),
    ] {
        assert_eq!(lanes.push(one, frame), Pushed::Queued);
    }
    assert_eq!(lanes.push(two, output(two, 10, 1)), Pushed::Queued);
    let order: Vec<_> = std::iter::from_fn(|| lanes.pop())
        .map(|frame| position(&frame))
        .collect();
    assert_eq!(
        order,
        [
            Some((0, true)),
            Some((10, false)),
            Some((4, false)),
            Some((6, false))
        ]
    );
}

#[test]
fn output_past_the_budget_is_dropped_and_resumed_from_its_first_byte_arc_14() {
    let session = SessionId::new();
    let mut lanes = Lanes::new(16);
    assert_eq!(lanes.push(session, output(session, 0, 4)), Pushed::Queued);
    // Written: never dropped.
    assert!(lanes.pop().is_some());
    assert_eq!(lanes.push(session, output(session, 4, 8)), Pushed::Queued);
    assert_eq!(lanes.push(session, output(session, 12, 8)), Pushed::Queued);
    let modes = ServerFrame::Modes {
        session,
        modes: TerminalModes::default(),
    };
    assert_eq!(lanes.push(session, modes), Pushed::Queued);
    // Past the budget: what waited goes, and nothing more is taken.
    assert_eq!(lanes.push(session, output(session, 20, 1)), Pushed::Behind);
    assert!(lanes.pop().is_none());
    assert_eq!(lanes.push(session, output(session, 21, 1)), Pushed::Behind);
    assert!(lanes.is_behind(session));
    // It resumes only once the writer has caught up.
    assert_eq!(lanes.resume(session), None);
    assert!(lanes.catch_up());
    assert!(!lanes.catch_up(), "said once");
    assert_eq!(
        lanes.resume(session),
        Some(Resume {
            seq: 4,
            reset: false
        })
    );
    // The replay from there is taken, and the stream goes on.
    assert_eq!(
        lanes.push(session, replay(session, 4, 30, false)),
        Pushed::Queued
    );
    assert!(!lanes.is_behind(session));
    assert_eq!(lanes.push(session, output(session, 34, 2)), Pushed::Queued);
}

#[test]
fn a_dropped_stream_that_began_anew_resumes_from_the_start() {
    let session = SessionId::new();
    let mut lanes = Lanes::new(8);
    lanes.push(session, replay(session, 100, 50, true));
    lanes.push(session, output(session, 150, 8));
    assert_eq!(lanes.push(session, output(session, 158, 8)), Pushed::Behind);
    lanes.catch_up();
    assert_eq!(
        lanes.resume(session),
        Some(Resume {
            seq: 100,
            reset: true
        })
    );

    // A program's end, and the next one's start, dropped alike.
    let mut lanes = Lanes::new(16);
    lanes.push(session, output(session, 0, 4));
    let reason = DetachReason::Exited;
    lanes.push(session, ServerFrame::Detached { session, reason });
    assert_eq!(lanes.push(session, output(session, 5, 8)), Pushed::Queued);
    assert_eq!(lanes.push(session, output(session, 13, 8)), Pushed::Behind);
    lanes.catch_up();
    assert_eq!(lanes.resume(session).map(|resume| resume.reset), Some(true));
}

#[test]
fn a_replay_counts_unless_a_client_resumes_with_it_arc_14() {
    let session = SessionId::new();
    let mut lanes = Lanes::new(16);
    // Resuming, into an empty lane: taken whatever its size, uncounted.
    assert_eq!(
        lanes.push(session, replay(session, 0, 100, true)),
        Pushed::Queued
    );
    assert_eq!(
        lanes.push(session, output(session, 100, 16)),
        Pushed::Queued
    );
    // Behind what waits, as after the live stream lagged, it counts.
    assert_eq!(
        lanes.push(session, replay(session, 116, 4, false)),
        Pushed::Behind
    );
    lanes.catch_up();
    assert_eq!(
        lanes.resume(session),
        Some(Resume {
            seq: 0,
            reset: true
        })
    );
}

#[test]
fn the_end_of_a_stream_is_said_once_however_often_it_is_asked_arc_14() {
    let session = SessionId::new();
    let mut lanes = Lanes::new(16);
    let reason = DetachReason::NotRunning;
    for _ in 0..1000 {
        lanes.push(session, ServerFrame::Detached { session, reason });
    }
    assert!(lanes.pop().is_some());
    assert!(lanes.pop().is_none());
}

#[test]
fn a_stream_that_ends_while_its_client_is_behind_ends_after_a_reset_arc_14() {
    let session = SessionId::new();
    let mut lanes = Lanes::new(16);
    assert_eq!(lanes.push(session, output(session, 0, 4)), Pushed::Queued);
    assert!(lanes.pop().is_some());
    lanes.push(session, output(session, 4, 8));
    assert_eq!(lanes.push(session, output(session, 12, 8)), Pushed::Queued);
    assert_eq!(lanes.push(session, output(session, 20, 8)), Pushed::Behind);
    // Refused while behind: dropped too.
    assert_eq!(lanes.push(session, output(session, 28, 2)), Pushed::Behind);
    // The terminal is released before the client caught up, so nothing
    // it missed is ever resent.
    let reason = DetachReason::NotRunning;
    let end = ServerFrame::Detached { session, reason };
    assert_eq!(lanes.push(session, end.clone()), Pushed::Queued);
    assert!(!lanes.is_behind(session));
    let sent: Vec<_> = std::iter::from_fn(|| lanes.pop()).collect();
    assert_eq!(
        sent,
        [
            ServerFrame::Replay {
                session,
                seq: 30,
                data: Vec::new(),
                reset: true,
                live_from: None,
                screen: None,
            },
            end.clone(),
        ],
        "the gap from 4 to 30 is a reset, before the end"
    );
    assert_eq!(lanes.len(), 0);
    // Said again, the end is not said twice, nor the reset.
    lanes.push(session, end.clone());
    lanes.push(session, end.clone());
    assert_eq!(
        std::iter::from_fn(|| lanes.pop()).collect::<Vec<_>>(),
        [end]
    );
}

#[test]
fn a_lane_leaves_once_empty_or_cleared_arc_14() {
    let (one, two) = (SessionId::new(), SessionId::new());
    let mut lanes = Lanes::new(4);
    lanes.push(one, output(one, 0, 2));
    assert_eq!(lanes.len(), 1);
    assert!(lanes.pop().is_some());
    assert_eq!(lanes.len(), 0);
    // One behind stays until it resumes, or its stream stops.
    lanes.push(two, output(two, 0, 8));
    assert!(lanes.is_behind(two));
    assert_eq!(lanes.len(), 1);
    lanes.clear(two);
    assert_eq!(lanes.len(), 0);
    assert!(!lanes.is_behind(two));
}

/// What a client reading slowly makes of what reaches it.
#[derive(Debug, Default)]
struct Seen {
    replies: Vec<u64>,
    /// The output shown, from `base` on.
    shown: Vec<u8>,
    base: u64,
    replays: usize,
    resets: usize,
    ended: bool,
}

const TOTAL: usize = 256 * 1024;
const CHUNK: usize = 1024;
const RING: usize = 16 * 1024;
const REPLIES: u64 = 64;

/// The session's output, byte by byte.
fn written() -> Vec<u8> {
    (0..TOTAL)
        .map(|at| u8::try_from(at % 251).unwrap_or(0))
        .collect()
}

/// Stands in for a forwarder: a program writes `TOTAL` bytes, its ring
/// holding the last `RING`; offered as output, and resumed from the ring
/// whenever the client fell behind. Every other time it is behind, the
/// program writes a ring's worth more before the client catches up, so it
/// falls further behind than the ring holds.
async fn forward(outbox: Arc<Outbox>, session: SessionId) {
    let stream = written();
    let (mut next, mut end, mut times_behind) = (0, 0, 0);
    loop {
        if outbox.is_behind(session) {
            times_behind += 1;
            if times_behind % 2 == 0 {
                end = (end + RING).min(TOTAL);
            }
            let resume = outbox.caught_up(session).await.unwrap();
            let start = end.saturating_sub(RING);
            let at = usize::try_from(resume.seq).unwrap();
            let (seq, reset) = if resume.reset || at < start {
                (start, true)
            } else {
                (at, false)
            };
            let frame = ServerFrame::Replay {
                session,
                seq: seq as u64,
                data: stream[seq..end].to_vec(),
                reset,
                live_from: None,
                screen: None,
            };
            assert_eq!(outbox.push(session, frame), Ok(Pushed::Queued));
            next = end;
            continue;
        }
        if next == TOTAL {
            break;
        }
        end = (end + CHUNK).min(TOTAL);
        let frame = ServerFrame::Output {
            session,
            seq: next as u64,
            data: stream[next..end].to_vec(),
        };
        if outbox.push(session, frame) == Ok(Pushed::Queued) {
            next = end;
        }
        tokio::task::yield_now().await;
    }
    let reason = DetachReason::Exited;
    let _ = outbox.push(session, ServerFrame::Detached { session, reason });
}

/// Reads like a client, slowly at first, until the stream has ended and
/// every reply has come.
async fn read_slowly(reader: tokio::io::DuplexStream, session: SessionId) -> Seen {
    let mut reader = FramedRead::new(reader);
    let mut seen = Seen::default();
    for read in 0.. {
        if seen.ended && seen.replies.len() as u64 == REPLIES {
            break;
        }
        if read < 300 {
            waiting::passes(Duration::from_millis(1), "a client slow to read").await;
        }
        let frame: ServerFrame = reader.next().await.unwrap().expect("the stream's end");
        match frame {
            ServerFrame::Reply { req_id, .. } => seen.replies.push(req_id),
            ServerFrame::Replay {
                session: s,
                seq,
                data,
                reset,
                ..
            } if s == session => {
                seen.replays += 1;
                if reset {
                    seen.resets += 1;
                    (seen.base, seen.shown) = (seq, data);
                } else {
                    let next = seen.base + seen.shown.len() as u64;
                    assert_eq!(seq, next, "a replay goes on from what was shown");
                    seen.shown.extend(data);
                }
            }
            ServerFrame::Output {
                session: s,
                seq,
                data,
            } if s == session => {
                let next = seen.base + seen.shown.len() as u64;
                assert_eq!(seq, next, "no byte twice, and no gap without a reset");
                seen.shown.extend(data);
            }
            ServerFrame::Detached { .. } => seen.ended = true,
            other => panic!("unexpected {other:?}"),
        }
    }
    seen
}

#[tokio::test]
async fn a_client_reading_slowly_gets_every_reply_in_order_and_its_output_from_the_ring_arc_14() {
    let session = SessionId::new();
    let (client, daemon) = tokio::io::duplex(4 * 1024);
    let (queue, queued) = mpsc::channel(256);
    let outbox = Outbox::new(8 * 1024);
    let writer = crate::tasks::owned(write_frames(
        FramedWrite::new(daemon),
        queued,
        outbox.clone(),
    ));
    let forwarder = crate::tasks::owned(forward(outbox.clone(), session));
    let replies = queue.clone();
    let replier = crate::tasks::owned(async move {
        for req_id in 1..=REPLIES {
            let outcome = Ok(Reply::Pong);
            replies
                .send(ServerFrame::Reply { req_id, outcome })
                .await
                .unwrap();
            waiting::passes(Duration::from_millis(2), "replies apart in time").await;
        }
    });

    let seen = tokio::time::timeout(Duration::from_secs(20), read_slowly(client, session))
        .await
        .expect("the client caught up in time");
    forwarder.await.unwrap();
    replier.await.unwrap();
    drop(queue);
    let _ = writer.await;

    assert_eq!(seen.replies, (1..=REPLIES).collect::<Vec<_>>());
    // It ends with the newest output, as the session wrote it.
    let stream = written();
    let base = usize::try_from(seen.base).unwrap();
    assert_eq!(base + seen.shown.len(), TOTAL);
    assert!(seen.shown == stream[base..], "the screen is the session's");
    // It fell behind, caught up from the ring, and once further than it.
    assert!(seen.replays >= 2, "{seen:?}");
    assert!(seen.resets >= 1, "{seen:?}");
}

#[tokio::test]
async fn once_the_client_is_gone_its_streams_are_told() {
    let session = SessionId::new();
    let (client, daemon) = tokio::io::duplex(64);
    let (queue, queued) = mpsc::channel(4);
    let outbox = Outbox::new(1024);
    let writer = crate::tasks::owned(write_frames(
        FramedWrite::new(daemon),
        queued,
        outbox.clone(),
    ));
    drop(client);
    let outcome = Ok(Reply::Pong);
    let _ = queue.send(ServerFrame::Reply { req_id: 1, outcome }).await;
    let _ = writer.await;
    assert_eq!(outbox.push(session, output(session, 0, 1)), Err(Gone));
    assert_eq!(outbox.caught_up(session).await, Err(Gone));
}

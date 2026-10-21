use std::sync::{Arc, Mutex};

use muster_core::protocol::MouseTracking;

use super::*;

/// A frame as sent: its kind, its place and its payload.
type Sent = (u8, u64, Vec<u8>);

/// What a terminal channel was sent.
#[derive(Clone, Default)]
struct Frames {
    sent: Arc<Mutex<Vec<Sent>>>,
    closed: Arc<Mutex<bool>>,
}

impl Frames {
    fn take(&self) -> Vec<(u8, u64, String)> {
        std::mem::take(&mut *self.sent.lock().unwrap())
            .into_iter()
            .map(|(kind, seq, payload)| (kind, seq, String::from_utf8_lossy(&payload).into_owned()))
            .collect()
    }

    fn close(&self) {
        *self.closed.lock().unwrap() = true;
    }
}

impl FrameSink for Frames {
    fn send(&self, bytes: Vec<u8>) -> bool {
        if *self.closed.lock().unwrap() {
            return false;
        }
        let seq = u64::from_le_bytes(bytes[1..9].try_into().unwrap());
        self.sent
            .lock()
            .unwrap()
            .push((bytes[0], seq, bytes[frame::HEADER..].to_vec()));
        true
    }
}

fn replay(session: SessionId, seq: u64, data: &str, reset: bool) -> ServerFrame {
    ServerFrame::Replay {
        session,
        seq,
        data: data.as_bytes().to_vec(),
        reset,
        live_from: None,
    }
}

fn output(session: SessionId, seq: u64, data: &str) -> ServerFrame {
    ServerFrame::Output {
        session,
        seq,
        data: data.as_bytes().to_vec(),
    }
}

fn attach(session: SessionId, from_seq: Option<u64>, watching: bool) -> ClientFrame {
    ClientFrame::Attach {
        session,
        from_seq,
        watching,
    }
}

fn connected() -> Streams {
    let mut streams = Streams::default();
    streams.connected(1);
    streams
}

fn asking(sessions: &[SessionId]) -> HashSet<SessionId> {
    sessions.iter().copied().collect()
}

#[test]
fn a_cursor_takes_each_byte_once_in_order() {
    let mut cursor = Cursor::default();
    assert_eq!(
        cursor.output(0, b"early"),
        Piece::Nothing,
        "output before the replay is an earlier attach's"
    );
    assert_eq!(
        cursor.replay(10, b"held", false, false),
        Piece::More {
            seq: 10,
            data: b"held"
        }
    );
    assert_eq!(
        cursor.output(14, b"new"),
        Piece::More {
            seq: 14,
            data: b"new"
        }
    );
    assert_eq!(cursor.output(14, b"new"), Piece::Nothing, "had it");
    assert_eq!(
        cursor.output(15, b"ew more"),
        Piece::More {
            seq: 17,
            data: b" more"
        },
        "only what it lacks"
    );
}

#[test]
fn a_cursor_starts_over_at_a_reset_unless_it_asked_for_all_and_the_replay_covers_it() {
    let mut cursor = Cursor::at(Some(4));
    assert_eq!(
        cursor.replay(0, b"0123456789", true, false),
        Piece::Reset {
            seq: 0,
            data: b"0123456789"
        },
        "a new program, or bytes no longer held"
    );
    let mut cursor = Cursor::at(Some(4));
    assert_eq!(
        cursor.replay(0, b"0123456789", true, true),
        Piece::More {
            seq: 4,
            data: b"456789"
        },
        "everything held, which the link asked for, from where it is"
    );
    let mut cursor = Cursor::at(Some(40));
    assert_eq!(
        cursor.replay(0, b"0123", true, true),
        Piece::Reset {
            seq: 0,
            data: b"0123"
        },
        "a replay that does not reach it"
    );
}

#[test]
fn a_stale_cursor_waits_for_the_replay_and_starts_over() {
    let mut cursor = Cursor::at(Some(100));
    cursor.go_stale();
    assert_eq!(cursor.output(100, b"x"), Piece::Nothing);
    assert_eq!(
        cursor.replay(0, b"fresh", false, true),
        Piece::Reset {
            seq: 0,
            data: b"fresh"
        }
    );
    assert_eq!(cursor.output(5, b"!"), Piece::More { seq: 5, data: b"!" });
}

#[test]
fn the_webview_s_terminal_is_attached_and_streamed_as_raw_frames() {
    let session = SessionId::new();
    let mut streams = connected();
    let frames = Frames::default();
    streams.view_attach(session, None, true, Box::new(frames.clone()));
    assert_eq!(streams.drain(), [attach(session, None, true)]);

    streams.on_frame(replay(session, 0, "$ ", false));
    streams.on_frame(output(session, 2, "ls\r\n"));
    let modes = TerminalModes {
        bracketed_paste: true,
        mouse: MouseTracking::Press,
        ..TerminalModes::default()
    };
    streams.on_frame(ServerFrame::Modes { session, modes });
    streams.on_frame(ServerFrame::Detached {
        session,
        reason: DetachReason::Exited,
    });
    let sent = frames.take();
    assert_eq!(sent[0], (frame::REPLAY, 0, "$ ".to_owned()));
    assert_eq!(sent[1], (frame::OUTPUT, 2, "ls\r\n".to_owned()));
    assert_eq!(sent[2].0, frame::EVENT);
    let event: serde_json::Value = serde_json::from_str(&sent[2].2).unwrap();
    assert_eq!(event["kind"], "modes");
    assert_eq!(event["modes"]["bracketed_paste"], true);
    assert_eq!(event["modes"]["mouse"], "press");
    let ended: serde_json::Value = serde_json::from_str(&sent[3].2).unwrap();
    assert_eq!(
        ended,
        serde_json::json!({"kind": "ended", "reason": "exited"})
    );
}

#[test]
fn watching_passes_through_and_detaching_lets_the_stream_go() {
    let session = SessionId::new();
    let mut streams = connected();
    streams.view_attach(session, None, true, Box::new(Frames::default()));
    streams.drain();
    streams.view_watch(session, false);
    assert_eq!(
        streams.drain(),
        [ClientFrame::Watch {
            session,
            watching: false
        }]
    );
    streams.view_detach(session);
    assert_eq!(streams.drain(), [ClientFrame::Detach { session }]);
    streams.on_frame(output(session, 0, "late"));
    streams.view_detach(session);
    assert_eq!(streams.drain(), [], "nothing left to let go");
}

#[test]
fn a_question_joining_the_terminal_shown_reads_from_the_start_and_the_terminal_carries_on() {
    let session = SessionId::new();
    let mut streams = connected();
    let frames = Frames::default();
    streams.view_attach(session, None, true, Box::new(frames.clone()));
    streams.drain();
    streams.on_frame(replay(session, 0, "Working\r\n", false));
    frames.take();

    streams.ask(&asking(&[session]));
    assert_eq!(
        streams.drain(),
        [attach(session, None, true)],
        "from all the daemon holds, still watched"
    );
    // The daemon starts the stream over with everything it holds, marked as
    // a reset since it was asked for no place.
    streams.on_frame(replay(session, 0, "Working\r\nProceed?\r\n", true));
    assert_eq!(
        frames.take(),
        [(frame::REPLAY, 9, "Proceed?\r\n".to_owned())],
        "the terminal is handed only what it lacks, and not cleared"
    );
    assert!(streams.take_changed());
    assert_eq!(
        streams.questions().get(&session).map(Vec::as_slice),
        Some(&["Working".to_owned(), "Proceed?".to_owned()][..])
    );
}

#[test]
fn the_terminal_joining_a_question_streams_from_where_it_is_and_the_question_carries_on() {
    let session = SessionId::new();
    let mut streams = connected();
    streams.ask(&asking(&[session]));
    assert_eq!(
        streams.drain(),
        [attach(session, None, false)],
        "a question is read by nobody looking"
    );
    streams.on_frame(replay(session, 0, "Which?\r\n", false));
    assert_eq!(streams.questions()[&session], ["Which?"]);

    // The webview shows it, holding nothing yet: everything, watched.
    let frames = Frames::default();
    streams.view_attach(session, None, true, Box::new(frames.clone()));
    assert_eq!(streams.drain(), [attach(session, None, true)]);
    streams.on_frame(replay(session, 0, "Which?\r\n", true));
    assert_eq!(frames.take(), [(frame::REPLAY, 0, "Which?\r\n".to_owned())]);
    streams.on_frame(output(session, 8, "A or B\r\n"));
    assert_eq!(streams.questions()[&session], ["Which?", "A or B"]);
    assert_eq!(frames.take(), [(frame::OUTPUT, 8, "A or B\r\n".to_owned())]);

    // The webview lets it go; the question is still read, unwatched.
    streams.view_detach(session);
    assert_eq!(
        streams.drain(),
        [ClientFrame::Watch {
            session,
            watching: false
        }]
    );
    // It stops asking: nobody reads the stream.
    streams.ask(&asking(&[]));
    assert_eq!(streams.drain(), [ClientFrame::Detach { session }]);
    assert!(streams.questions().is_empty());
}

#[test]
fn a_terminal_shown_again_streams_from_the_byte_it_lacks() {
    let session = SessionId::new();
    let mut streams = connected();
    let frames = Frames::default();
    streams.view_attach(session, None, true, Box::new(frames.clone()));
    streams.on_frame(replay(session, 0, "abc", false));
    streams.view_detach(session);
    streams.drain();
    // Hidden and seen again: the webview says where its screen is.
    streams.view_attach(session, Some(3), true, Box::new(frames.clone()));
    assert_eq!(streams.drain(), [attach(session, Some(3), true)]);
    streams.on_frame(replay(session, 3, "def", false));
    assert_eq!(
        frames.take(),
        [
            (frame::REPLAY, 0, "abc".to_owned()),
            (frame::REPLAY, 3, "def".to_owned()),
        ]
    );
}

#[test]
fn after_the_link_is_lost_streams_attach_again_from_the_start_and_screens_start_over() {
    let session = SessionId::new();
    let mut streams = connected();
    let frames = Frames::default();
    streams.view_attach(session, None, true, Box::new(frames.clone()));
    streams.on_frame(replay(session, 0, "before", false));
    streams.drain();
    frames.take();

    streams.disconnected();
    streams.settle(|_| true);
    assert_eq!(streams.drain(), [], "nothing to send while out of reach");

    streams.connected(2);
    streams.settle(|_| true);
    assert_eq!(
        streams.drain(),
        [attach(session, None, true)],
        "positions from the old daemon mean nothing to the new one"
    );
    streams.on_frame(output(session, 6, "stale"));
    streams.on_frame(replay(session, 0, "after", false));
    assert_eq!(
        frames.take(),
        [(frame::REPLAY_RESET, 0, "after".to_owned())]
    );

    // A terminal the webview shows again from a place an older connection
    // gave it starts over too.
    streams.view_detach(session);
    streams.connected(3);
    streams.view_attach(session, Some(5), true, Box::new(frames.clone()));
    assert_eq!(streams.drain()[1], attach(session, None, true));
    streams.on_frame(replay(session, 0, "third", false));
    assert_eq!(
        frames.take(),
        [(frame::REPLAY_RESET, 0, "third".to_owned())]
    );
}

#[test]
fn a_session_holding_no_terminal_is_streamed_again_once_it_runs_again() {
    let session = SessionId::new();
    let mut streams = connected();
    streams.view_attach(session, None, true, Box::new(Frames::default()));
    streams.drain();
    streams.on_frame(ServerFrame::Detached {
        session,
        reason: DetachReason::NotRunning,
    });
    streams.settle(|_| false);
    assert_eq!(streams.drain(), [], "not while it does not run");
    streams.settle(|_| true);
    assert_eq!(streams.drain(), [attach(session, None, true)]);
    streams.settle(|_| true);
    assert_eq!(streams.drain(), [], "once");
}

#[test]
fn a_terminal_nobody_reads_is_let_go() {
    let session = SessionId::new();
    let mut streams = connected();
    let frames = Frames::default();
    streams.view_attach(session, None, true, Box::new(frames.clone()));
    streams.drain();
    frames.close();
    streams.on_frame(replay(session, 0, "x", false));
    assert_eq!(streams.drain(), [ClientFrame::Detach { session }]);
}

#[test]
fn a_loaded_webview_s_terminals_all_go() {
    let (one, two) = (SessionId::new(), SessionId::new());
    let mut streams = connected();
    streams.view_attach(one, None, true, Box::new(Frames::default()));
    streams.view_attach(two, None, false, Box::new(Frames::default()));
    streams.drain();
    streams.drop_views();
    let mut sent = streams.drain();
    sent.sort_by_key(|frame| format!("{frame:?}"));
    let mut expected = vec![
        ClientFrame::Detach { session: one },
        ClientFrame::Detach { session: two },
    ];
    expected.sort_by_key(|frame| format!("{frame:?}"));
    assert_eq!(sent, expected);
}

#[test]
fn a_question_is_read_at_the_size_its_terminal_was_given() {
    let session = SessionId::new();
    let mut streams = connected();
    streams.resized(session, TermSize { cols: 10, rows: 3 });
    streams.ask(&asking(&[session]));
    streams.on_frame(replay(
        session,
        0,
        "0123456789abc\r\none\r\ntwo\r\nthree",
        false,
    ));
    // Ten columns wrap the long line; three rows keep the last three.
    assert_eq!(streams.questions()[&session], ["one", "two", "three"]);
    streams.prune(|_| false);
    assert!(streams.sizes.is_empty() && streams.fed_by.is_empty());
}

#[test]
fn nothing_is_asked_of_the_daemon_while_out_of_reach() {
    let session = SessionId::new();
    let mut streams = Streams::default();
    streams.view_attach(session, None, true, Box::new(Frames::default()));
    streams.ask(&asking(&[session]));
    assert_eq!(streams.drain(), []);
    streams.connected(1);
    streams.settle(|_| true);
    assert_eq!(streams.drain(), [attach(session, None, true)]);
}

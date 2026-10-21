//! End to end (`ARC-14`): a client that cannot keep up with a session's
//! output is not queued for. Its replies come first, not behind output
//! long out of date; it catches up from the session's ring, no byte of
//! output shown twice and no gap but one marked `reset`; and its screen
//! ends as a client's that attaches then. What it was sent late is marked
//! live, the history it attached to never. A client starting over, sent
//! bytes cut from the middle of the output, is sent the program's screen
//! after them, which they cannot draw.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own threads and tasks; ARC-03, ARC-13 and ARC-14 bind Muster's own code"
)]

mod support;

use std::time::Duration;

use muster_client::FrameReceiver;
use muster_core::SessionId;
use muster_core::protocol::{ClientFrame, Command, Reply, ServerFrame, TermSize};

use support::Sandbox;

const REPLIES: u64 = 20;
/// The most output a client may read before replies asked for while it
/// was behind: what the socket holds (some 200 KiB on Linux) and a frame
/// on its way, not a queue of output out of date.
const AHEAD_OF_REPLIES: usize = 512 * 1024;
const SIZE: TermSize = TermSize { cols: 80, rows: 24 };

/// A session's screen as a client draws it, and where its stream is.
struct Screen {
    parser: vt100::Parser,
    next: Option<u64>,
    replays: usize,
    resets: usize,
    /// Where the first replay, the history attached to, ended.
    history: Option<u64>,
    /// The newest output, to look for a marker in.
    tail: String,
}

impl Screen {
    fn new() -> Self {
        Self {
            parser: vt100::Parser::new(SIZE.rows, SIZE.cols, 0),
            next: None,
            replays: 0,
            resets: 0,
            history: None,
            tail: String::new(),
        }
    }

    /// Takes one of the session's stream frames, checking it follows on
    /// from what was shown: nothing twice, and no gap but a reset.
    fn take(&mut self, frame: &ServerFrame) {
        match frame {
            ServerFrame::Replay {
                seq,
                data,
                reset,
                live_from,
                screen,
                ..
            } => {
                // History first; then, sent late, what was written live.
                assert_eq!(*live_from, self.history, "replay {}", self.replays);
                self.history.get_or_insert(seq + data.len() as u64);
                self.replays += 1;
                if *reset {
                    self.resets += 1;
                    self.parser = vt100::Parser::new(SIZE.rows, SIZE.cols, 0);
                } else if let Some(next) = self.next {
                    assert_eq!(*seq, next, "a replay goes on from what was shown");
                }
                self.show(*seq, data);
                // Starting over, the program's screen, drawn after.
                if let (true, Some(screen)) = (*reset, screen) {
                    self.parser.process(screen);
                }
            }
            ServerFrame::Output { seq, data, .. } => {
                assert_eq!(Some(*seq), self.next, "no byte twice, no gap unmarked");
                self.show(*seq, data);
            }
            _ => {}
        }
    }

    fn show(&mut self, seq: u64, data: &[u8]) {
        self.parser.process(data);
        self.next = Some(seq + data.len() as u64);
        self.tail.push_str(&String::from_utf8_lossy(data));
        let keep = self.tail.len().saturating_sub(256);
        let cut = (keep..=self.tail.len())
            .find(|at| self.tail.is_char_boundary(*at))
            .unwrap_or(0);
        self.tail.drain(..cut);
    }
}

/// How many bytes of output a stream frame carries.
fn stream_len(frame: &ServerFrame) -> usize {
    match frame {
        ServerFrame::Replay { data, .. } | ServerFrame::Output { data, .. } => data.len(),
        _ => 0,
    }
}

fn of_session(frame: &ServerFrame, session: SessionId) -> bool {
    matches!(
        frame,
        ServerFrame::Replay { session: s, .. } | ServerFrame::Output { session: s, .. } if *s == session
    )
}

/// The next frame, or `None` once nothing has come for a while.
async fn next_frame(receiver: &mut FrameReceiver, quiet: Duration) -> Option<ServerFrame> {
    match tokio::time::timeout(quiet, receiver.next()).await {
        Ok(frame) => Some(frame.unwrap().expect("the daemon is there")),
        Err(_) => None,
    }
}

#[tokio::test]
async fn a_client_that_reads_slowly_catches_up_from_the_ring_arc_14() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap();
    let workspace = connection.subscribe().await.unwrap().workspaces[0].id;
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: None,
            size: SIZE,
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("no session");
    };
    connection.attach(session, None).await.unwrap();
    let (mut sender, mut receiver, _) = connection.into_split();

    // Output far past what the daemon keeps waiting for a client, and
    // past what the ring holds, in large pieces (`cat` writes blocks),
    // while the client reads nothing; it asks things once it is behind.
    let flood = b"seq 1 400000 | cat; echo flood-$((2+3))-done\n".to_vec();
    let input = ClientFrame::Input {
        session,
        data: flood,
    };
    sender.send(&input).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;
    for req_id in 1..=REPLIES {
        let command = Command::Ping;
        sender
            .send(&ClientFrame::Command { req_id, command })
            .await
            .unwrap();
    }
    tokio::time::sleep(Duration::from_secs(1)).await;

    let mut screen = Screen::new();
    let mut replies = Vec::new();
    let (mut output, mut ahead_of_replies) = (0, 0);
    let caught_up = tokio::time::timeout(Duration::from_secs(30), async {
        while !(screen.tail.contains("flood-5-done") && replies.len() as u64 == REPLIES) {
            let frame = receiver.next().await.unwrap().expect("the daemon is there");
            match &frame {
                ServerFrame::Reply { req_id, outcome } => {
                    assert_eq!(outcome, &Ok(Reply::Pong));
                    replies.push(*req_id);
                    ahead_of_replies = output;
                }
                frame if of_session(frame, session) => {
                    output += stream_len(frame);
                    screen.take(frame);
                }
                _ => {}
            }
        }
    })
    .await;
    caught_up.expect("the client caught up");
    // Whatever the shell writes after, such as its prompt.
    while let Some(frame) = next_frame(&mut receiver, Duration::from_millis(500)).await {
        if of_session(&frame, session) {
            screen.take(&frame);
        }
    }

    replies.sort_unstable();
    assert_eq!(replies, (1..=REPLIES).collect::<Vec<_>>(), "each once");
    assert!(
        ahead_of_replies < AHEAD_OF_REPLIES,
        "{ahead_of_replies} bytes of output came before the replies"
    );
    assert!(screen.replays >= 2, "it fell behind and caught up");
    assert!(screen.resets >= 1, "further behind than the ring holds");

    // The same screen as a client that attaches now.
    let fresh = daemon.connect().await;
    let (mut fresh_sender, mut fresh_receiver, _) = fresh.into_split();
    let attach = ClientFrame::Attach {
        session,
        from_seq: None,
        watching: true,
    };
    fresh_sender.send(&attach).await.unwrap();
    let mut fresh_screen = Screen::new();
    while let Some(frame) = next_frame(&mut fresh_receiver, Duration::from_millis(500)).await {
        if of_session(&frame, session) {
            fresh_screen.take(&frame);
        }
    }
    assert_eq!(
        fresh_screen.next, screen.next,
        "both are at the newest byte"
    );
    assert_eq!(
        screen.parser.screen().contents(),
        fresh_screen.parser.screen().contents()
    );
}

#[tokio::test]
async fn a_client_starting_over_sees_the_program_s_screen_not_the_output_s_middle() {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap();
    let workspace = connection.subscribe().await.unwrap().workspaces[0].id;
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
            title: None,
            size: SIZE,
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("no session");
    };
    // A full-screen program draws its screen, then writes more than the
    // ring holds that draws nothing, so the ring keeps none of what drew
    // it, then its last line.
    let run = b"stty -echo; printf '\\033[?1049h\\033[5;10Hmarker'; \
        awk 'BEGIN { for (i = 0; i < 200000; i++) printf \"\\033[s\\033[u\" }'; \
        printf '\\033[1;1Hdrawn-%d' $((6*7)); sleep 600\n";
    connection.attach(session, None).await.unwrap();
    let (mut sender, mut receiver, _) = connection.into_split();
    let input = ClientFrame::Input {
        session,
        data: run.to_vec(),
    };
    sender.send(&input).await.unwrap();
    let mut watching = Screen::new();
    tokio::time::timeout(Duration::from_secs(30), async {
        while !watching.tail.contains("drawn-42") {
            let frame = receiver.next().await.unwrap().expect("the daemon is there");
            if of_session(&frame, session) {
                watching.take(&frame);
            }
        }
    })
    .await
    .expect("the program drew its last line");

    // A client that opens on it now is sent the ring, cut partway, and
    // the program's screen.
    let fresh = daemon.connect().await;
    let (mut fresh_sender, mut fresh_receiver, _) = fresh.into_split();
    let attach = ClientFrame::Attach {
        session,
        from_seq: None,
        watching: true,
    };
    fresh_sender.send(&attach).await.unwrap();
    let mut opened = Screen::new();
    let mut cut = vt100::Parser::new(SIZE.rows, SIZE.cols, 0);
    while let Some(frame) = next_frame(&mut fresh_receiver, Duration::from_millis(500)).await {
        if let ServerFrame::Replay { data, reset, .. } = &frame
            && *reset
        {
            cut.process(data);
        }
        if of_session(&frame, session) {
            opened.take(&frame);
        }
    }
    assert_eq!(opened.resets, 1, "it starts over");
    assert!(
        !cut.screen().contents().contains("marker"),
        "the ring's bytes alone lost the program's screen"
    );
    let (shown, program) = (opened.parser.screen(), watching.parser.screen());
    assert!(shown.alternate_screen());
    assert!(shown.contents().contains("marker"), "{}", shown.contents());
    assert_eq!(shown.contents(), program.contents());
    assert_eq!(shown.cursor_position(), program.cursor_position());
}

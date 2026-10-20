//! The real terminal forwarder against a shell and a deliberately stalled
//! in-memory transport: no socket or external service is needed (ARC-14).

use std::sync::Arc;
use std::time::Duration;

use muster_client::{FramedRead, FramedWrite};
use muster_core::SessionId;
use muster_core::protocol::{Reply, ServerFrame, TermSize};
use tokio::sync::{mpsc, watch};

use crate::pty::{PtyEvent, PtyHandle, SpawnSpec, spawn};
use crate::server::outbox::{Outbox, write_frames};

#[tokio::test]
async fn a_stalled_client_catches_up_to_the_final_screen_and_modes_arc_14() {
    let session = SessionId::new();
    let size = TermSize { cols: 80, rows: 24 };
    let (pty, mut exit) = flood(size).unwrap();
    let (_current, current) = watch::channel(Some(pty.clone()));
    let (client, daemon) = tokio::io::duplex(512);
    let (commands, queued) = mpsc::channel(4);
    let outbox = Outbox::new(8 * 1024);
    let _writer = crate::tasks::owned(write_frames(
        FramedWrite::new(daemon),
        queued,
        Arc::clone(&outbox),
    ));
    let _forwarder = crate::tasks::owned(super::forward_output(session, current, None, outbox));
    let mut reader = FramedRead::new(client);
    let first: ServerFrame = reader.next().await.unwrap().unwrap();
    assert!(matches!(first, ServerFrame::Replay { .. }));
    assert!(pty.input(b"go\n".to_vec()).await);
    tokio::time::timeout(Duration::from_secs(10), exit.wait_for(|ended| *ended))
        .await
        .unwrap()
        .unwrap();
    // The program has ended while its client has read no output. A reply
    // asked for now must still get past the stale output.
    commands
        .send(ServerFrame::Reply {
            req_id: 7,
            outcome: Ok(Reply::Pong),
        })
        .await
        .unwrap();
    let mut screen = vt100::Parser::new(size.rows, size.cols, 0);
    let (mut next, mut reset, mut replied, mut modes) = (None, false, false, false);
    let mut ahead = 0;
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let frame: ServerFrame = reader.next().await.unwrap().unwrap();
            match frame {
                ServerFrame::Replay {
                    seq,
                    data,
                    reset: fresh,
                    ..
                } => {
                    if fresh {
                        screen = vt100::Parser::new(size.rows, size.cols, 0);
                        reset = true;
                    } else if let Some(next) = next {
                        assert_eq!(seq, next);
                    }
                    screen.process(&data);
                    next = Some(seq + data.len() as u64);
                    if !replied {
                        ahead += data.len();
                    }
                }
                ServerFrame::Output { seq, data, .. } => {
                    // The first replay was consumed above; no other
                    // output was consumed before this loop.
                    if let Some(next) = next {
                        assert_eq!(seq, next, "no duplicate or unmarked gap");
                    }
                    screen.process(&data);
                    next = Some(seq + data.len() as u64);
                    if !replied {
                        ahead += data.len();
                    }
                }
                ServerFrame::Modes { modes: current, .. } => modes |= current.bracketed_paste,
                ServerFrame::Reply { req_id: 7, outcome } => {
                    assert_eq!(outcome, Ok(Reply::Pong));
                    replied = true;
                }
                ServerFrame::Detached { .. } => break,
                other => panic!("unexpected {other:?}"),
            }
        }
    })
    .await
    .unwrap();
    assert!(reset && replied && modes);
    assert!(ahead < 64 * 1024, "{ahead} output bytes ahead of the reply");
    let fresh = pty.attach(None).await.unwrap();
    let mut expected = vt100::Parser::new(size.rows, size.cols, 0);
    expected.process(&fresh.replay.data);
    assert_eq!(screen.screen().contents(), expected.screen().contents());
    assert_eq!(
        next,
        Some(fresh.replay.seq + fresh.replay.data.len() as u64)
    );
    assert!(fresh.modes.bracketed_paste);
}

fn flood(size: TermSize) -> std::io::Result<(PtyHandle, watch::Receiver<bool>)> {
    let (exited, exit) = watch::channel(false);
    let pty = spawn(
        &SpawnSpec {
            program: "/bin/sh".to_owned(),
            args: vec![
                "-c".to_owned(),
                r"stty -echo; read trigger; seq 1 400000; printf '\033[?2004hfinal-done'"
                    .to_owned(),
            ],
            cwd: std::env::temp_dir(),
            env: Vec::new(),
            env_remove: Vec::new(),
            size,
        },
        0,
        watch::channel(muster_core::colours::TerminalColours::default()).1,
        move |event| {
            if matches!(event, PtyEvent::Exited { .. }) {
                let _ = exited.send(true);
            }
        },
    )?;
    Ok((pty, exit))
}

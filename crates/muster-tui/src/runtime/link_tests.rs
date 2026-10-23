//! The link to the daemon (`ARC-14`): a UI that stops reading holds the
//! daemon back, rather than queueing all it sends, and loses nothing.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use muster_client::{FramedRead, FramedWrite};
use muster_core::SessionId;
use tokio::net::UnixListener;

use super::*;

const CHUNK: usize = 16 * 1024;
const FRAMES: usize = 400;

fn options(socket: std::path::PathBuf) -> Options {
    Options {
        socket,
        connect: None,
        machine: None,
        start_daemon: None,
        cwd: std::path::PathBuf::from("/"),
        notify: false,
        motion: false,
        start_notch: None,
        terminal_app: None,
        open: None,
    }
}

/// Waits until `written` has not moved for a while: what it stopped at.
async fn settled(written: &AtomicUsize) -> usize {
    let mut last = written.load(Ordering::SeqCst);
    for _ in 0..100 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        let now = written.load(Ordering::SeqCst);
        if now == last {
            return now;
        }
        last = now;
    }
    last
}

#[tokio::test]
async fn a_ui_that_stops_reading_stops_the_daemon_writing_and_loses_nothing_arc_14() {
    let dir = tempfile::tempdir().unwrap();
    let socket = dir.path().join("d.sock");
    let listener = UnixListener::bind(&socket).unwrap();
    let (events, mut link) = mpsc::channel(LINK_EVENTS);
    #[expect(clippy::disallowed_methods, reason = "ARC-13: the test owns it")]
    let _link = AbortOnDropHandle::new(tokio::spawn(keep_linked(options(socket), events)));

    // A daemon that says welcome, then writes output as fast as it is read.
    let (stream, _) = listener.accept().await.unwrap();
    let (read, write) = stream.into_split();
    let (mut reader, mut writer) = (FramedRead::new(read), FramedWrite::new(write));
    let hello = reader.next::<ClientFrame>().await.unwrap();
    assert!(matches!(hello, Some(ClientFrame::Hello(_))), "{hello:?}");
    let welcome = ServerFrame::Welcome(Welcome {
        version: muster_core::protocol::PROTOCOL_VERSION,
        daemon_pid: 1,
        daemon_version: "test".to_owned(),
    });
    writer.send(&welcome).await.unwrap();
    let session = SessionId::new();
    let written = Arc::new(AtomicUsize::new(0));
    let counted = written.clone();
    #[expect(clippy::disallowed_methods, reason = "ARC-13: the test owns it")]
    let daemon = AbortOnDropHandle::new(tokio::spawn(async move {
        for n in 0..FRAMES {
            let frame = ServerFrame::Output {
                session,
                seq: (n * CHUNK) as u64,
                data: vec![b'x'; CHUNK],
            };
            writer.send(&frame).await.unwrap();
            counted.fetch_add(1, Ordering::SeqCst);
        }
        // The daemon goes: the link's end must reach the UI too.
        drop(writer);
    }));

    // The UI's loop reads nothing: the daemon's writes stop, a queue's
    // worth and the socket's buffers on.
    let stalled = settled(&written).await;
    assert!(
        stalled < FRAMES / 2,
        "{stalled} of {FRAMES} frames written to a UI that reads nothing"
    );

    // Its frames for the daemon go out meanwhile, without waiting.
    let Some(LinkEvent::Connected { sender, .. }) = link.recv().await else {
        panic!("not connected first");
    };
    assert!(sender.send(ClientFrame::Subscribe));
    let subscribe = reader.next::<ClientFrame>().await.unwrap();
    assert!(
        matches!(subscribe, Some(ClientFrame::Subscribe)),
        "{subscribe:?}"
    );

    // Reading again, it gets every frame, in order, then the link's end.
    for n in 0..FRAMES {
        match link.recv().await {
            Some(LinkEvent::Frame(frame)) => match *frame {
                ServerFrame::Output { seq, .. } => assert_eq!(seq, (n * CHUNK) as u64),
                other => panic!("{other:?}"),
            },
            _ => panic!("frame {n} did not arrive"),
        }
    }
    assert!(matches!(link.recv().await, Some(LinkEvent::Lost(_))));
    daemon.await.unwrap();
}

#[test]
fn another_machine_is_tried_again_less_and_less_often() {
    assert_eq!(retry_after(false, 9), Duration::from_millis(500));
    assert_eq!(retry_after(true, 1), Duration::from_millis(500));
    assert_eq!(retry_after(true, 2), Duration::from_secs(1));
    assert_eq!(retry_after(true, 4), Duration::from_secs(4));
    assert_eq!(retry_after(true, 30), FARTHEST_RETRY);
}

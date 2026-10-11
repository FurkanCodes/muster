use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::protocol::TermSize;
use tokio::time::timeout;

use super::progress::ProgressScanner;
use super::ring::{Replay, Ring};
use super::{PtyEvent, PtyHandle, SpawnSpec, spawn};

// Ring

#[test]
fn the_ring_numbers_every_byte_ever_written() {
    let mut ring = Ring::new(8);
    assert_eq!(ring.push(b"abc"), 0);
    assert_eq!(ring.push(b"def"), 3);
    assert_eq!(ring.end(), 6);
    assert_eq!(
        ring.replay_from(Some(4)),
        Replay {
            seq: 4,
            data: b"ef".to_vec(),
            reset: false,
        }
    );
    assert_eq!(ring.replay_from(Some(6)).data, b"");
}

#[test]
fn the_ring_drops_the_oldest_bytes_and_says_so() {
    let mut ring = Ring::new(4);
    ring.push(b"abcdef");
    assert_eq!(
        ring.replay_from(Some(1)),
        Replay {
            seq: 2,
            data: b"cdef".to_vec(),
            reset: true,
        }
    );
    ring.push(b"gh");
    assert_eq!(ring.end(), 8);
    assert_eq!(ring.replay_from(Some(5)).data, b"fgh");
}

#[test]
fn a_position_from_nowhere_gets_everything_with_a_reset() {
    let mut ring = Ring::new(4);
    ring.push(b"ab");
    for from in [None, Some(99)] {
        let replay = ring.replay_from(from);
        assert_eq!(
            (replay.seq, replay.data.as_slice(), replay.reset),
            (0, &b"ab"[..], true)
        );
    }
}

// Progress

fn scan(scanner: &mut ProgressScanner, bytes: &[u8]) -> Vec<bool> {
    scanner.scan(bytes)
}

#[test]
fn progress_reports_are_found_with_either_terminator() {
    let mut scanner = ProgressScanner::default();
    assert_eq!(scan(&mut scanner, b"text\x1b]9;4;3\x07more"), [true]);
    assert_eq!(scan(&mut scanner, b"\x1b]9;4;0\x1b\\"), [false]);
    assert_eq!(scan(&mut scanner, b"\x1b]9;4;1;50\x07"), [true]);
}

#[test]
fn only_changes_are_reported() {
    let mut scanner = ProgressScanner::default();
    assert_eq!(
        scan(&mut scanner, b"\x1b]9;4;3\x07\x1b]9;4;1;20\x07"),
        [true]
    );
    assert!(scan(&mut scanner, b"\x1b]9;4;2\x07").is_empty());
}

#[test]
fn a_report_split_across_reads_is_still_found() {
    let mut scanner = ProgressScanner::default();
    let report = b"\x1b]9;4;3\x07";
    let mut changes = Vec::new();
    for byte in report {
        changes.extend(scan(&mut scanner, &[*byte]));
    }
    assert_eq!(changes, [true]);
}

#[test]
fn other_sequences_are_ignored() {
    let mut scanner = ProgressScanner::default();
    // A window title, a colour, a long hyperlink, and a malformed report.
    let noise = b"\x1b]0;title\x07\x1b[31mred\x1b]8;;https://example.com/a/very/long/path\x07\x1b]9;4;x\x07";
    assert!(scan(&mut scanner, noise).is_empty());
    // An escape sequence that interrupts an OSC ends it.
    assert!(scan(&mut scanner, b"\x1b]9;4;3\x1b[0m\x07").is_empty());
}

// The actor, with a real shell

#[derive(Clone, Default)]
struct Events(Arc<Mutex<Vec<PtyEvent>>>);

impl Events {
    fn sink(&self) -> impl Fn(PtyEvent) + Send + 'static {
        let events = self.0.clone();
        move |event| events.lock().unwrap().push(event)
    }

    fn all(&self) -> Vec<PtyEvent> {
        self.0.lock().unwrap().clone()
    }

    async fn exit(&self) -> PtyEvent {
        timeout(Duration::from_secs(10), async {
            loop {
                if let Some(exit) = self
                    .all()
                    .into_iter()
                    .find(|event| matches!(event, PtyEvent::Exited { .. }))
                {
                    return exit;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("the process exited")
    }
}

fn shell(env: Vec<(&str, &str)>, env_remove: Vec<&str>) -> (PtyHandle, Events) {
    let events = Events::default();
    let spec = SpawnSpec {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
        cwd: std::env::temp_dir(),
        env: env
            .into_iter()
            .map(|(key, value)| (key.to_owned(), value.to_owned()))
            .collect(),
        env_remove: env_remove.into_iter().map(str::to_owned).collect(),
        size: TermSize::default(),
    };
    (spawn(spec, events.sink()).unwrap(), events)
}

/// Collects a session's output from `from` until it contains `marker`.
async fn read_until(pty: &PtyHandle, from: Option<u64>, marker: &str) -> (String, u64) {
    let attachment = pty.attach(from).await.unwrap();
    let mut text = String::from_utf8_lossy(&attachment.replay.data).into_owned();
    let mut next = attachment.replay.seq + attachment.replay.data.len() as u64;
    let mut live = attachment.live.expect("still running");
    timeout(Duration::from_secs(10), async {
        while !text.contains(marker) {
            let chunk = live.recv().await.unwrap();
            text.push_str(&String::from_utf8_lossy(&chunk.data));
            next = chunk.seq + chunk.data.len() as u64;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("never saw {marker:?} in {text:?}"));
    (text, next)
}

#[tokio::test]
async fn input_reaches_the_shell_and_its_output_comes_back() {
    let (pty, _) = shell(vec![], vec![]);
    pty.input(b"echo out-$((6*7))\n".to_vec()).await;
    read_until(&pty, None, "out-42").await;
}

#[tokio::test]
async fn re_attaching_from_a_position_replays_only_what_followed() {
    let (pty, _) = shell(vec![], vec![]);
    pty.input(b"echo first-$((1+1))\n".to_vec()).await;
    let (_, next) = read_until(&pty, None, "first-2").await;
    pty.input(b"echo second-$((2+2))\n".to_vec()).await;
    let (text, _) = read_until(&pty, Some(next), "second-4").await;
    assert!(!text.contains("first-2"), "{text:?}");
}

#[tokio::test]
async fn the_terminal_takes_the_size_it_is_given() {
    let (pty, _) = shell(vec![], vec![]);
    pty.resize(TermSize {
        cols: 132,
        rows: 41,
    })
    .await;
    pty.input(b"stty size\n".to_vec()).await;
    read_until(&pty, None, "41 132").await;
}

#[tokio::test]
async fn the_environment_is_set_and_scrubbed() {
    let (pty, _) = shell(vec![("MUSTER_PROBE", "present")], vec!["HOME"]);
    pty.input(b"echo \"probe-$MUSTER_PROBE home-${HOME:-gone}\"\n".to_vec())
        .await;
    read_until(&pty, None, "probe-present home-gone").await;
}

#[tokio::test]
async fn the_child_inherits_no_descriptors_beyond_stdio() {
    // Something this process holds that a careless spawn would pass on.
    let _held_open = tempfile::tempfile().unwrap();
    let (pty, _) = shell(vec![], vec![]);
    // Asks lsof what the idle shell holds, writing to a file so the probe
    // itself opens nothing in the shell. Descriptor 255 is excluded: bash
    // keeps its own copy of the terminal there. (Probing with `>&$fd` would
    // not do either: bash opens descriptors 10 and up to carry it out.)
    let probe = "lsof -a -p $$ -d 3-254 -F f > fds.$$ 2>/dev/null; \
                 echo \"fds-$(grep -c '^f' fds.$$)-done-$((3*3))\"; rm -f fds.$$\n";
    pty.input(probe.as_bytes().to_vec()).await;
    let (text, _) = read_until(&pty, None, "done-9").await;
    assert!(text.contains("fds-0-done-9"), "{text:?}");
}

#[tokio::test]
async fn the_progress_sequence_is_reported() {
    let (pty, events) = shell(vec![], vec![]);
    pty.input(
        b"printf '\\033]9;4;3\\007'; printf '\\033]9;4;0\\007'; echo done-$((5*5))\n".to_vec(),
    )
    .await;
    read_until(&pty, None, "done-25").await;
    let progress: Vec<_> = events
        .all()
        .into_iter()
        .filter(|event| matches!(event, PtyEvent::Progress { .. }))
        .collect();
    assert_eq!(
        progress,
        [
            PtyEvent::Progress { busy: true },
            PtyEvent::Progress { busy: false }
        ]
    );
}

#[tokio::test]
async fn an_exit_is_reported_after_its_output_and_the_output_stays_readable() {
    let (pty, events) = shell(vec![], vec![]);
    pty.input(b"echo bye-$((2*4)); exit 3\n".to_vec()).await;
    assert_eq!(
        events.exit().await,
        PtyEvent::Exited {
            success: false,
            requested: false,
        }
    );
    let after = pty.attach(None).await.unwrap();
    assert!(String::from_utf8_lossy(&after.replay.data).contains("bye-8"));
    assert!(after.live.is_none());
}

#[tokio::test]
async fn a_stop_hangs_up_and_then_kills_a_process_that_ignores_it() {
    let (pty, events) = shell(vec![], vec![]);
    pty.input(b"trap '' HUP; echo armed-$((3+4))\n".to_vec())
        .await;
    read_until(&pty, None, "armed-7").await;
    pty.stop().await;
    let PtyEvent::Exited { requested, .. } = events.exit().await else {
        unreachable!();
    };
    assert!(requested);
}

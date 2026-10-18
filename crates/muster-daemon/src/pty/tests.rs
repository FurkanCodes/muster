use std::sync::{Arc, Mutex};
use std::time::Duration;

use muster_core::colours::{Rgb, TerminalColours};
use muster_core::protocol::TermSize;
use tokio::time::timeout;

use super::osc::{OscScanner, Report};
use super::ring::{Replay, Ring};
use super::{PtyEvent, PtyHandle, SpawnSpec, adopt, spawn};

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
fn a_ring_can_continue_an_earlier_numbering() {
    let mut ring = Ring::starting_at(8, 100);
    assert_eq!(ring.end(), 100);
    assert_eq!(ring.push(b"ab"), 100);
    // A position from before the ring's start is not held.
    assert!(ring.replay_from(Some(99)).reset);
    let replay = ring.replay_from(Some(101));
    assert_eq!((replay.seq, replay.data.as_slice()), (101, &b"b"[..]));
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

fn scan(scanner: &mut OscScanner, bytes: &[u8]) -> Vec<bool> {
    scanner
        .scan(bytes)
        .into_iter()
        .filter_map(|report| match report {
            Report::Progress { busy } => Some(busy),
            Report::Title(_) => None,
        })
        .collect()
}

#[test]
fn progress_reports_are_found_with_either_terminator() {
    let mut scanner = OscScanner::default();
    assert_eq!(scan(&mut scanner, b"text\x1b]9;4;3\x07more"), [true]);
    assert_eq!(scan(&mut scanner, b"\x1b]9;4;0\x1b\\"), [false]);
    assert_eq!(scan(&mut scanner, b"\x1b]9;4;1;50\x07"), [true]);
}

#[test]
fn only_changes_are_reported() {
    let mut scanner = OscScanner::default();
    assert_eq!(
        scan(&mut scanner, b"\x1b]9;4;3\x07\x1b]9;4;1;20\x07"),
        [true]
    );
    assert!(scan(&mut scanner, b"\x1b]9;4;2\x07").is_empty());
}

#[test]
fn a_report_split_across_reads_is_still_found() {
    let mut scanner = OscScanner::default();
    let report = b"\x1b]9;4;3\x07";
    let mut changes = Vec::new();
    for byte in report {
        changes.extend(scan(&mut scanner, &[*byte]));
    }
    assert_eq!(changes, [true]);
}

#[test]
fn other_sequences_are_ignored() {
    let mut scanner = OscScanner::default();
    // A colour, a long hyperlink, and a malformed report.
    let noise = b"\x1b[31mred\x1b]8;;https://example.com/a/very/long/path\x07\x1b]9;4;x\x07";
    assert!(scan(&mut scanner, noise).is_empty());
    // An escape sequence that interrupts an OSC ends it.
    assert!(scan(&mut scanner, b"\x1b]9;4;3\x1b[0m\x07").is_empty());
}

#[test]
fn titles_are_reported_as_they_change() {
    let mut scanner = OscScanner::default();
    assert_eq!(
        scanner.scan(b"\x1b]0;\xe2\xa0\x8b Working\x07text\x1b]2;Ready\x1b\\"),
        [
            Report::Title("⠋ Working".to_owned()),
            Report::Title("Ready".to_owned())
        ]
    );
    // The same title again is no change; progress is still found beside it.
    assert_eq!(
        scanner.scan(b"\x1b]2;Ready\x07\x1b]9;4;3\x07"),
        [Report::Progress { busy: true }]
    );
    // An empty title is a change, and one too long to be a status is not read.
    assert_eq!(scanner.scan(b"\x1b]0;\x07"), [Report::Title(String::new())]);
    let long = format!("\x1b]0;{}\x07", "x".repeat(600));
    assert!(scanner.scan(long.as_bytes()).is_empty());
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

/// No UI has told the person's terminal's colours.
fn no_colours() -> super::Colours {
    tokio::sync::watch::channel(TerminalColours::default()).1
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
    (
        spawn(&spec, 0, no_colours(), events.sink()).unwrap(),
        events,
    )
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
async fn a_terminal_handed_over_is_adopted_with_its_program_running() {
    let (pty, _) = shell(vec![], vec![]);
    // State only the running shell has, and an input mode it set.
    pty.input(b"MARK=kept; printf '\\033[?2004h'; echo armed-$((2+3))\n".to_vec())
        .await;
    let (_, end) = read_until(&pty, None, "armed-5").await;

    let handed = pty.hand_over().await.expect("its program still runs");
    assert!(pty.attach(None).await.is_none(), "the old actor has gone");
    // Everything read so far, the prompt after the marker too.
    let carried_end = handed.terminal.seq + handed.terminal.ring.len() as u64;
    assert!(carried_end >= end, "{carried_end} < {end}");
    let events = Events::default();
    let adopted = adopt(handed.master, handed.terminal, no_colours(), events.sink()).unwrap();

    // Its output so far, numbered as before, and its mode, carried.
    let attachment = adopted.attach(None).await.unwrap();
    assert!(String::from_utf8_lossy(&attachment.replay.data).contains("armed-5"));
    assert!(attachment.modes.bracketed_paste);
    // The same shell, still running: it remembers what it was told.
    adopted.input(b"echo still-$MARK-$((6*7))\n".to_vec()).await;
    read_until(&adopted, Some(carried_end), "still-kept-42").await;
}

#[tokio::test]
async fn a_terminal_flooded_with_output_is_still_handed_over() {
    let (pty, _) = shell(vec![], vec![]);
    // Output faster than the actor takes it, so the reader's queue fills
    // and the reader waits on it.
    pty.input(b"echo flood-$((4*4)); yes muster-flood\n".to_vec())
        .await;
    read_until(&pty, None, "flood-16").await;
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let handed = tokio::time::timeout(std::time::Duration::from_secs(10), pty.hand_over())
        .await
        .expect("handed over while its program floods it")
        .expect("its program still runs");
    // Closing the terminal hangs `yes` up.
    drop(handed);
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
async fn a_session_s_program_is_told_its_terminal_is_muster() {
    use crate::harness::{Launch, Place, find, spawn_spec};
    let launch = Launch {
        session: muster_core::SessionId::new(),
        agent_session: None,
        resume: false,
        prompt: None,
        hooks: None,
        place: Place::Host(std::env::temp_dir()),
        size: TermSize::default(),
        checks: None,
        support: None,
    };
    let spec = SpawnSpec {
        // The person's own shell would read their startup files.
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
        ..spawn_spec(find("shell").unwrap(), &launch)
    };
    let pty = spawn(&spec, 0, no_colours(), Events::default().sink()).unwrap();
    // Whatever terminal ran these tests, the shell sees Muster as its own,
    // and nothing that addresses the other one.
    pty.input(
        b"echo \"is-$TERM_PROGRAM-$TERM_PROGRAM_VERSION $COLORTERM tmux-${TMUX:-none} lc-${LC_TERMINAL:-none}\"\n"
            .to_vec(),
    )
    .await;
    let wanted = format!(
        "is-muster-{} truecolor tmux-none lc-none",
        env!("CARGO_PKG_VERSION")
    );
    read_until(&pty, None, &wanted).await;
}

#[tokio::test]
async fn the_child_inherits_no_descriptors_beyond_stdio() {
    // Something this process holds that a careless spawn would pass on.
    let _held_open = tempfile::tempfile().unwrap();
    let (pty, _) = shell(vec![], vec![]);
    // Asks lsof what the idle shell holds, writing to a file so the probe
    // itself opens nothing in the shell, and counts the descriptors past
    // stdio that are not a terminal. A shell keeps copies of its terminal
    // for itself (bash at 255, dash at 10), and opens them after it starts;
    // a file, a socket or a pipe there can only have been inherited, as
    // the listening socket this guards against once was. (Probing with
    // `>&$fd` would not do: bash opens descriptors 10 and up to carry it
    // out.)
    let probe = "lsof -a -p $$ -d 3-255 -F ftn > fds.$$ 2>/dev/null; \
                 n=$(awk '/^f/ {fd = substr($0, 2)} \
                 /^t/ {if (fd ~ /^[0-9]+$/ && $0 != \"tCHR\") n++} \
                 END {print n + 0}' fds.$$); \
                 echo \"inherited-$n-done-$((3*3))\"; rm -f fds.$$\n";
    pty.input(probe.as_bytes().to_vec()).await;
    let (text, _) = read_until(&pty, None, "done-9").await;
    assert!(text.contains("inherited-0-done-9"), "{text:?}");
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
    let PtyEvent::Exited {
        success,
        requested,
        end,
        ..
    } = events.exit().await
    else {
        unreachable!();
    };
    assert!(!success && !requested);
    let after = pty.attach(None).await.unwrap();
    assert_eq!(after.replay.seq + after.replay.data.len() as u64, end);
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

#[tokio::test]
async fn questions_to_the_terminal_are_answered() {
    let (pty, _) = shell(vec![], vec![]);
    // Turns echo off, asks for device attributes and the cursor position,
    // and prints the 11 bytes that come back, with ESC made visible.
    let probe = "stty -echo -icanon min 1; printf '\\033[c\\033[6n'; \
                 r=$(dd bs=1 count=11 2>/dev/null | tr '\\033' 'E'); stty echo icanon; \
                 echo \"answer:$r:$((2*21))\"\n";
    pty.input(probe.as_bytes().to_vec()).await;
    let (text, _) = read_until(&pty, None, ":42").await;
    // The last `answer:` is the output; the first is the echoed command.
    let answer = text
        .rsplit("answer:")
        .next()
        .and_then(|rest| rest.split(':').next())
        .unwrap_or_default();
    // A VT102 claim, then a cursor position: a row and a column. Where the
    // cursor is depends on the shell (dash, Linux's `sh`, prints its prompt
    // before it runs the line), so only the answer's form is checked.
    assert!(answer.starts_with("E[?6cE["), "{answer:?} in {text:?}");
    let position = answer
        .trim_start_matches("E[?6cE[")
        .strip_suffix('R')
        .and_then(|rest| rest.split_once(';'));
    assert!(
        position.is_some_and(
            |(row, column)| row.parse::<u16>().is_ok() && column.parse::<u16>().is_ok()
        ),
        "{answer:?} in {text:?}"
    );
}

#[tokio::test]
async fn a_program_learns_that_synchronized_output_is_supported() {
    let (pty, _) = shell(vec![], vec![]);
    // DECRQM for mode 2026, as Claude Code asks it; the 11 bytes back.
    let probe = "stty -echo -icanon min 1; printf '\\033[?2026$p'; \
                 r=$(dd bs=1 count=11 2>/dev/null | tr '\\033' 'E'); stty echo icanon; \
                 echo \"answer:$r:$((2*21))\"\n";
    pty.input(probe.as_bytes().to_vec()).await;
    let (text, _) = read_until(&pty, None, ":42").await;
    let answer = text
        .rsplit("answer:")
        .next()
        .and_then(|rest| rest.split(':').next())
        .unwrap_or_default();
    // Known, and not in a frame now.
    assert_eq!(answer, "E[?2026;2$y", "{text:?}");
}

#[tokio::test]
async fn a_program_asking_for_its_terminal_s_background_is_told_the_person_s() {
    let (told, colours) = tokio::sync::watch::channel(TerminalColours::default());
    let spec = SpawnSpec {
        program: "/bin/sh".to_owned(),
        args: Vec::new(),
        cwd: std::env::temp_dir(),
        env: Vec::new(),
        env_remove: Vec::new(),
        size: TermSize::default(),
    };
    let pty = spawn(&spec, 0, colours, Events::default().sink()).unwrap();
    // Told after the terminal started, as a UI connects after it.
    told.send_replace(TerminalColours {
        background: Some(Rgb {
            red: 0x1e1e,
            green: 0x1e1e,
            blue: 0x2e2e,
        }),
        ..TerminalColours::default()
    });
    // OSC 11 as programs ask it, ended with ST; the 25 bytes back.
    let probe = "stty -echo -icanon min 1; printf '\\033]11;?\\033\\\\'; \
                 r=$(dd bs=1 count=25 2>/dev/null | tr '\\033\\\\' 'EB'); stty echo icanon; \
                 echo \"answer=$r=$((2*21))\"\n";
    pty.input(probe.as_bytes().to_vec()).await;
    // The answer holds colons, so `=` marks it off.
    let (text, _) = read_until(&pty, None, "=42").await;
    let answer = text
        .rsplit("answer=")
        .next()
        .and_then(|rest| rest.split('=').next())
        .unwrap_or_default();
    // ESC made E, and the backslash of its ST B.
    assert_eq!(answer, "E]11;rgb:1e1e/1e1e/2e2eEB", "{text:?}");
}

#[tokio::test]
async fn a_program_that_asked_is_told_as_its_terminal_gains_and_loses_focus() {
    let (pty, _) = shell(vec![], vec![]);
    // Not asked yet: nothing is sent, but the terminal knows it has focus.
    pty.focus(true).await;
    // Asks (`CSI ? 1004 h`), then prints the 6 bytes it is sent.
    let probe = "stty -echo -icanon min 1; printf '\\033[?1004h'; echo ready-$((1+2)); \
                 r=$(dd bs=1 count=6 2>/dev/null | tr '\\033' 'E'); stty echo icanon; \
                 echo \"focus:$r:$((2*21))\"\n";
    pty.input(probe.as_bytes().to_vec()).await;
    let (_, next) = read_until(&pty, None, "ready-3").await;
    // Told again what it knows: nothing; then out, and back in.
    pty.focus(true).await;
    pty.focus(false).await;
    pty.focus(true).await;
    let (text, _) = read_until(&pty, Some(next), ":42").await;
    let told = text
        .rsplit("focus:")
        .next()
        .and_then(|rest| rest.split(':').next())
        .unwrap_or_default();
    assert_eq!(told, "E[OE[I", "{text:?}");
}

#[tokio::test]
async fn a_program_that_suspended_itself_is_continued_and_no_other_is() {
    let (pty, _) = shell(vec![], vec![]);
    // A loop that reports continues, like a program waiting for one. The
    // shell runs the trap between the loop's commands.
    let waiting = "trap 'echo got-cont-$((2+5))' CONT; echo ready-$((1+1)); \
                   while :; do sleep 0.1; done\n";
    pty.input(waiting.as_bytes().to_vec()).await;
    let (_, next) = read_until(&pty, None, "ready-2").await;

    // Not continued: it never handed the terminal back.
    pty.resume_if_suspended().await;
    tokio::time::sleep(Duration::from_millis(400)).await;
    let quiet = pty.attach(Some(next)).await.unwrap().replay.data;
    assert!(!String::from_utf8_lossy(&quiet).contains("got-cont-7"));

    // Turning bracketed paste on and then off while still running is what
    // Claude Code's suspend looks like.
    pty.input(b"\x03".to_vec()).await;
    let suspend = "printf '\\033[?2004h\\033[?2004l'; echo handed-$((3*3)); \
                   while :; do sleep 0.1; done\n";
    pty.input(suspend.as_bytes().to_vec()).await;
    let (_, next) = read_until(&pty, Some(next), "handed-9").await;
    pty.resume_if_suspended().await;
    read_until(&pty, Some(next), "got-cont-7").await;
}

//! The desktop app's link against the real daemon (`ARCHITECTURE.md` §5c,
//! §13): it connects, patches the webview's state as the daemon's changes,
//! passes commands through, streams a terminal both ways, reads a session
//! asking the person for its question, and comes back with the daemon.
//! Tauri plays no part: the link sends to the tests' own sinks.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves and start their own tasks; ARC-03 and ARC-13 bind the app's own code"
)]

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use muster_core::protocol::{Command, Reply, TermSize};
use muster_core::{ProjectId, SessionId, SessionStatus, WorkspaceId};
use muster_desktop::contract::{BridgeError, LinkState, Row, frame};
use muster_desktop::link::{Link, Looking, Options};
use support::{Sandbox, Screen, Webview};

/// A link to `sandbox`'s daemon that never starts one, its webview open.
fn link_to(sandbox: &Sandbox) -> (Link, Webview) {
    let (link, task) = Link::new(Options {
        socket: sandbox.paths().socket(),
        start_daemon: None,
    });
    tokio::spawn(task);
    let (webview, states) = Webview::new();
    link.open(states).unwrap();
    (link, webview)
}

/// A project of a fresh repository, and its own checkout.
async fn project(
    sandbox: &Sandbox,
    link: &Link,
    webview: &mut Webview,
) -> (ProjectId, WorkspaceId) {
    let root = sandbox.repository("app");
    let Reply::ProjectAdded { id } = link
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    webview
        .until("the project and its checkout", |webview| {
            webview.projects.contains_key(&id)
                && webview.workspaces.values().any(|w| w.project == id)
        })
        .await;
    let checkout = webview
        .workspaces
        .values()
        .find(|workspace| workspace.project == id)
        .unwrap()
        .id;
    (id, checkout)
}

async fn start(link: &Link, workspace: WorkspaceId, harness: &str) -> SessionId {
    let Reply::SessionStarted { id } = link
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
            title: None,
            size: TermSize { cols: 80, rows: 24 },
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    id
}

#[tokio::test(flavor = "multi_thread")]
async fn the_link_connects_and_patches_the_state_as_the_daemon_changes_it_arc_24() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let (link, mut webview) = link_to(&sandbox);
    webview
        .until("the state", |w| w.up() && w.resets == 1)
        .await;
    assert!(matches!(
        webview.links[..],
        [LinkState::Connecting, LinkState::Up { .. }]
    ));
    assert!(link.is_connected());

    let (project, checkout) = project(&sandbox, &link, &mut webview).await;
    let shell = start(&link, checkout, "shell").await;
    webview
        .until("the shell, listed under its checkout", |webview| {
            webview.derived.rows.ends_with(&[
                Row::Project(project),
                Row::Workspace(checkout),
                Row::Session(shell),
            ])
        })
        .await;
    assert_eq!(webview.resets, 1, "one reset, then changes only");
    assert_eq!(
        webview.derived.counts.working, 0,
        "a shell is the person's own, never counted"
    );
    // The webview holds what the daemon holds.
    let snapshot = support::snapshot(&sandbox.paths()).await;
    webview
        .until("the shell as the daemon has it", |webview| {
            webview.sessions.get(&shell).map(|s| s.status)
                == snapshot
                    .sessions
                    .iter()
                    .find(|s| s.id == shell)
                    .map(|s| s.status)
        })
        .await;

    // A refusal keeps the daemon's own code and words.
    let refused = link
        .command(Command::StartSession {
            workspace: WorkspaceId::new(),
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await;
    assert!(
        matches!(&refused, Err(BridgeError::Refused { message, .. }) if !message.is_empty()),
        "{refused:?}"
    );

    // Ended, then gone from the daemon: gone from the webview.
    link.command(Command::StopSession { session: shell })
        .await
        .unwrap();
    webview
        .until("the shell ended", |webview| {
            webview
                .sessions
                .get(&shell)
                .is_some_and(|s| s.status.is_exited())
        })
        .await;
    link.command(Command::RemoveProject { project })
        .await
        .unwrap();
    webview
        .until(
            "the project gone, its checkout and shell with it",
            |webview| {
                webview.projects.is_empty()
                    && webview.workspaces.is_empty()
                    && webview.sessions.is_empty()
                    && webview.derived.rows.is_empty()
            },
        )
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn typing_into_a_terminal_reaches_its_program_and_its_output_comes_back_as_raw_frames() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let (link, mut webview) = link_to(&sandbox);
    let (_, checkout) = project(&sandbox, &link, &mut webview).await;
    let shell = start(&link, checkout, "shell").await;

    let (mut screen, frames) = Screen::new();
    link.attach(shell, None, true, frames).unwrap();
    link.input(shell, b"echo round-$((40+2))-trip\r".to_vec())
        .unwrap();
    screen.until_shows("round-42-trip").await;
    assert_eq!(
        screen.kinds.first(),
        Some(&frame::REPLAY),
        "a replay first, then output"
    );
    assert!(screen.kinds.contains(&frame::EVENT), "its modes");

    // The program draws for the size the window gives it.
    link.resize(
        shell,
        TermSize {
            cols: 101,
            rows: 33,
        },
        None,
    )
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    link.input(shell, b"stty size\r".to_vec()).unwrap();
    screen.until_shows("33 101").await;

    // Let go and shown again from the byte it lacks: nothing twice.
    link.detach(shell).unwrap();
    let held = screen.next;
    let (mut again, frames) = Screen::new();
    link.attach(shell, Some(held), true, frames).unwrap();
    link.input(shell, b"echo again-$((1+1))\r".to_vec())
        .unwrap();
    again.until_shows("again-2").await;
    assert!(
        !again.text.contains("round-42-trip"),
        "only what came after {held}: {:?}",
        again.text
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn a_session_asking_the_person_shows_its_question_while_the_window_is_seen() {
    let sandbox = Sandbox::new();
    let _daemon = sandbox.start();
    let (link, mut webview) = link_to(&sandbox);
    let (_, checkout) = project(&sandbox, &link, &mut webview).await;
    let codex = start(&link, checkout, "codex").await;

    // Codex says in its title that it waits on the person, and asks.
    link.input(
        codex,
        b"printf '\\033]0;[ ! ] Action Required\\007'; echo 'Allow Codex to run this command?'\r"
            .to_vec(),
    )
    .unwrap();
    webview
        .until("the question in Needs you", |webview| {
            webview.derived.needs_you.contains(&codex)
                && webview.derived.questions.get(&codex).is_some_and(|lines| {
                    lines.len() <= 4
                        && lines
                            .iter()
                            .any(|line| line == "Allow Codex to run this command?")
                })
        })
        .await;
    assert!(matches!(
        webview.sessions[&codex].status,
        SessionStatus::NeedsYou { .. }
    ));

    // Hidden, the window is sent nothing; seen again, all of it at once.
    link.looking(Looking {
        seen: false,
        looking: false,
        focused: None,
    })
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    webview.take_waiting();
    let patches = webview.patches;
    link.input(codex, b"echo more-$((1+1))\r".to_vec()).unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(webview.take_waiting(), 0, "nothing while hidden");
    assert_eq!(webview.patches, patches);
    link.looking(Looking::default()).unwrap();
    webview
        .until("the question as it is now", |webview| {
            webview
                .derived
                .questions
                .get(&codex)
                .is_some_and(|lines| lines.iter().any(|line| line == "more-2"))
        })
        .await;

    // Answered, it asks no more and its question goes.
    link.input(codex, b"printf '\\033]0;Ready\\007'\r".to_vec())
        .unwrap();
    webview
        .until("no question", |webview| {
            !webview.derived.needs_you.contains(&codex)
                && !webview.derived.questions.contains_key(&codex)
        })
        .await;
}

#[tokio::test(flavor = "multi_thread")]
async fn the_link_says_the_daemon_is_out_of_reach_and_comes_back_with_it() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let (link, mut webview) = link_to(&sandbox);
    webview
        .until("the state", |w| w.up() && w.resets == 1)
        .await;

    daemon.stop();
    webview
        .until("out of reach", |webview| {
            matches!(webview.link, Some(LinkState::Down { .. }))
        })
        .await;
    assert!(!link.is_connected());
    assert!(matches!(
        link.command(Command::Ping).await,
        Err(BridgeError::Unreachable)
    ));
    assert!(matches!(
        link.input(SessionId::new(), b"x".to_vec()),
        Err(BridgeError::Unreachable)
    ));
    let Some(LinkState::Down { since, .. }) = webview.link.clone() else {
        unreachable!()
    };

    // Retried every half second: once a daemon answers, the state comes
    // whole again.
    tokio::time::sleep(Duration::from_millis(1200)).await;
    webview.take_waiting();
    assert!(
        matches!(&webview.link, Some(LinkState::Down { since: still, .. }) if *still == since),
        "down since it was first lost"
    );
    let _again = sandbox.start();
    webview.until("back", |w| w.up() && w.resets == 2).await;
    assert!(link.is_connected());
    assert!(link.command(Command::Ping).await.is_ok());
}

#[tokio::test(flavor = "multi_thread")]
async fn where_nothing_answers_the_link_starts_the_daemon_once() {
    let sandbox = Arc::new(Sandbox::new());
    let starts = Arc::new(AtomicUsize::new(0));
    let start = {
        let (sandbox, starts) = (Arc::clone(&sandbox), Arc::clone(&starts));
        Box::new(move || {
            starts.fetch_add(1, Ordering::SeqCst);
            // As the app does, `muster daemon --detach`; directly rather
            // than through launchd, as over SSH, so the test owns no job.
            sandbox
                .command()
                .args(["daemon", "--detach"])
                .env("SSH_CONNECTION", "test")
                .stderr(std::process::Stdio::null())
                .status()
                .map(|_| ())
        })
    };
    let (link, task) = Link::new(Options {
        socket: sandbox.paths().socket(),
        start_daemon: Some(start),
    });
    tokio::spawn(task);
    let (mut webview, states) = Webview::new();
    link.open(states).unwrap();
    webview
        .until("the state", |w| w.up() && w.resets == 1)
        .await;
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert_eq!(
        webview.links,
        [LinkState::Connecting, webview.link.clone().unwrap()],
        "connecting while it starts, then up: never down"
    );
    drop(link);
    sandbox.stop_detached();
}

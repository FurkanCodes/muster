//! End-to-end tests for a session's life after its program ends: starting
//! it again in the same session, taking it off the list, and stopping the
//! daemon itself, also one from another build.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use std::time::Duration;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use serde_json::json;

use support::{Daemon, Sandbox, expect_event, read_terminal};

struct Setup {
    sandbox: Sandbox,
    daemon: Daemon,
    workspace: WorkspaceId,
}

async fn setup() -> Setup {
    let sandbox = Sandbox::new();
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::WorktreeCreated { id: workspace } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    Setup {
        sandbox,
        daemon,
        workspace,
    }
}

async fn start(connection: &mut Connection, workspace: WorkspaceId, harness: &str) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    {
        Reply::SessionStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

async fn restart(connection: &mut Connection, session: SessionId) -> Result<Reply, ClientError> {
    connection
        .command(Command::RestartSession {
            session,
            size: TermSize { cols: 90, rows: 30 },
        })
        .await
}

/// Waits until `session`'s stored state satisfies `done`.
async fn wait_for(
    connection: &mut Connection,
    session: SessionId,
    done: impl Fn(&muster_core::Session) -> bool,
) -> muster_core::Session {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        if let Some(found) = snapshot.sessions.iter().find(|s| s.id == session)
            && done(found)
        {
            return found.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{:?}",
            snapshot.sessions
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Makes the session's shell exit and waits until the daemon says so.
async fn exit(connection: &mut Connection, session: SessionId) {
    connection.input(session, b"exit\n").await.unwrap();
    wait_for(connection, session, |s| s.ended_at.is_some()).await;
}

fn refused(outcome: Result<Reply, ClientError>) -> ErrorCode {
    match outcome {
        Err(ClientError::Daemon(error)) => error.code,
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[tokio::test]
async fn an_ended_shell_starts_afresh_in_the_same_session_and_watchers_follow() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace, "shell").await;
    let mut viewer = setup.daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    viewer
        .input(session, b"echo first-$((2*3)); exit\n")
        .await
        .unwrap();
    read_terminal(&mut viewer, session, "first-6").await;
    wait_for(&mut connection, session, |s| s.ended_at.is_some()).await;

    assert_eq!(
        restart(&mut connection, session).await.unwrap(),
        Reply::SessionStarted { id: session }
    );
    let stored = wait_for(&mut connection, session, |s| s.ended_at.is_none()).await;
    // Watched while it restarted, so not news.
    assert_eq!(
        (stored.status, stored.unseen),
        (SessionStatus::Fresh, false)
    );

    // The viewer's stream moved to the new shell, on a clean screen.
    let reset = expect_event(&mut viewer, |frame| match frame {
        ServerFrame::Replay {
            session: s, reset, ..
        } if *s == session => Some(*reset),
        _ => None,
    })
    .await;
    assert!(reset);
    viewer
        .input(session, b"echo second-$((3*3))\n")
        .await
        .unwrap();
    read_terminal(&mut viewer, session, "second-9").await;
}

#[tokio::test]
async fn a_session_that_ended_with_the_daemon_can_be_started_again() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace, "shell").await;
    drop(connection);
    let Setup {
        sandbox, daemon, ..
    } = setup;
    assert!(daemon.stop().success());

    // A shell cannot resume, so it ended with the daemon, as news.
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let stored = wait_for(&mut connection, session, |s| s.ended_at.is_some()).await;
    assert_eq!(
        (stored.status, stored.unseen),
        (SessionStatus::Exited { success: false }, true)
    );
    // Looking at it clears that, though it has no terminal to show.
    let mut viewer = daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    wait_for(&mut connection, session, |s| !s.unseen).await;
    drop(viewer);

    // Nothing holds its terminal any more; restarting gives it a new one,
    // at the size the client asked for.
    restart(&mut connection, session).await.unwrap();
    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"echo again-$((4*5)); stty size\n")
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "again-20").await;
    let (text, _) = if text.contains("30 90") {
        (text, 0)
    } else {
        read_terminal(&mut connection, session, "30 90").await
    };
    assert!(text.contains("30 90"), "{text}");
}

#[tokio::test]
async fn a_restarted_program_gets_the_size_the_client_asks_for() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    // Started, and ended, at the default size.
    let session = start(&mut connection, setup.workspace, "shell").await;
    exit(&mut connection, session).await;
    restart(&mut connection, session).await.unwrap();
    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"stty size; echo sized-$((5*5))\n")
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "sized-25").await;
    assert!(text.contains("30 90"), "{text}");
}

#[tokio::test]
async fn claude_resumes_its_conversation_and_starts_fresh_without_one() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();

    // One session ran a turn and has a transcript: it resumes.
    let talked = start(&mut connection, setup.workspace, "claude").await;
    let launch = setup.sandbox.wait_for_launches(talked, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let payload = json!({ "session_id": id, "hook_event_name": "UserPromptSubmit" });
    launch.fire("UserPromptSubmit", &payload);
    wait_for(&mut connection, talked, |s| s.resume_id.is_some()).await;
    let transcripts = setup.sandbox.claude_projects().join("-work");
    std::fs::create_dir_all(&transcripts).unwrap();
    std::fs::write(transcripts.join(format!("{id}.jsonl")), "{}\n").unwrap();
    exit(&mut connection, talked).await;
    restart(&mut connection, talked).await.unwrap();
    let launches = setup.sandbox.wait_for_launches(talked, 2);
    assert_eq!(launches[1].value("--resume"), Some(id.as_str()));
    assert_ne!(launches[1].token, launches[0].token);
    let stored = wait_for(&mut connection, talked, |s| s.ended_at.is_none()).await;
    assert_eq!(stored.status, SessionStatus::Finished);

    // Its hooks reach the daemon with the new token.
    launches[1].fire("UserPromptSubmit", &payload);
    wait_for(&mut connection, talked, |s| {
        s.status == SessionStatus::Running
    })
    .await;

    // One never ran a turn: it starts fresh with a new id.
    let silent = start(&mut connection, setup.workspace, "claude").await;
    let first = setup.sandbox.wait_for_launches(silent, 1).remove(0);
    exit(&mut connection, silent).await;
    restart(&mut connection, silent).await.unwrap();
    let launches = setup.sandbox.wait_for_launches(silent, 2);
    assert_eq!(launches[1].value("--resume"), None);
    let fresh = launches[1].value("--session-id").expect("a fresh start");
    assert_ne!(Some(fresh), first.value("--session-id"));
    let stored = wait_for(&mut connection, silent, |s| s.ended_at.is_none()).await;
    assert_eq!(stored.status, SessionStatus::Fresh);
}

#[tokio::test]
async fn running_or_unknown_sessions_are_neither_restarted_nor_removed() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace, "shell").await;
    assert_eq!(
        refused(restart(&mut connection, session).await),
        ErrorCode::Conflict
    );
    assert_eq!(
        refused(connection.command(Command::RemoveSession { session }).await),
        ErrorCode::Conflict
    );
    let unknown = SessionId::new();
    assert_eq!(
        refused(restart(&mut connection, unknown).await),
        ErrorCode::NotFound
    );
    assert_eq!(
        refused(
            connection
                .command(Command::RemoveSession { session: unknown })
                .await
        ),
        ErrorCode::NotFound
    );
}

#[tokio::test]
async fn a_removed_session_leaves_the_list_for_good() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace, "shell").await;
    let kept = start(&mut connection, setup.workspace, "shell").await;
    exit(&mut connection, session).await;
    let mut watcher = setup.daemon.connect().await;
    watcher.subscribe().await.unwrap();
    watcher.attach(session, None).await.unwrap();

    assert_eq!(
        connection
            .command(Command::RemoveSession { session })
            .await
            .unwrap(),
        Reply::Done
    );
    let (mut removed, mut detached) = (false, false);
    expect_event(&mut watcher, |frame| {
        match frame {
            ServerFrame::Delta(Delta::SessionRemoved { id }) if *id == session => removed = true,
            ServerFrame::Detached { session: s, .. } if *s == session => detached = true,
            _ => {}
        }
        (removed && detached).then_some(())
    })
    .await;
    // Removing it twice finds nothing.
    assert_eq!(
        refused(connection.command(Command::RemoveSession { session }).await),
        ErrorCode::NotFound
    );

    drop((connection, watcher));
    let Setup {
        sandbox, daemon, ..
    } = setup;
    assert!(daemon.stop().success());
    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    let ids: Vec<_> = snapshot.sessions.iter().map(|s| s.id).collect();
    assert_eq!(ids, [kept]);
}

#[test]
fn daemon_stop_stops_the_running_daemon() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let pid = daemon.pid();
    let output = sandbox.run(&["daemon", "stop"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "{output:?}");
    assert!(stdout.contains(&format!("pid {pid}")), "{stdout}");
    assert!(!support::answers_hello(daemon.socket()));

    let again = sandbox.run(&["daemon", "stop"]);
    assert!(again.status.success());
    assert!(String::from_utf8_lossy(&again.stdout).contains("not running"));
}

#[test]
fn daemon_stop_reaches_a_daemon_from_an_older_build() {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixListener;

    use muster_core::frame::{self, FrameDecoder};
    use muster_core::protocol::{ClientFrame, Welcome};

    let sandbox = Sandbox::new();
    let socket = sandbox.paths().socket();
    std::fs::create_dir_all(socket.parent().unwrap()).unwrap();
    // The "daemon": a process to stop, and a socket that greets as protocol
    // 1 did, answering until that process is gone.
    let mut process = std::process::Command::new("sleep")
        .arg("30")
        .spawn()
        .unwrap();
    let pid = process.id();
    let listener = UnixListener::bind(&socket).unwrap();
    listener.set_nonblocking(true).unwrap();
    #[expect(
        clippy::disallowed_methods,
        reason = "a test's thread ends with the test"
    )]
    let old = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while std::time::Instant::now() < deadline {
            if process.try_wait().unwrap().is_some() {
                return true;
            }
            let Ok((mut stream, _)) = listener.accept() else {
                std::thread::sleep(Duration::from_millis(10));
                continue;
            };
            stream.set_nonblocking(false).unwrap();
            let (mut decoder, mut chunk) = (FrameDecoder::new(), [0; 4096]);
            let hello = loop {
                let read = stream.read(&mut chunk).unwrap();
                decoder.push(&chunk[..read]);
                if let Some(ClientFrame::Hello(hello)) = decoder.next_frame().unwrap() {
                    break hello;
                }
            };
            assert_eq!(hello.min_version, 1, "the hello offers every version");
            let welcome = ServerFrame::Welcome(Welcome {
                version: 1,
                daemon_pid: pid,
                daemon_version: "0.0.9".to_owned(),
            });
            stream.write_all(&frame::encode(&welcome).unwrap()).unwrap();
        }
        let _ = process.kill();
        let _ = process.wait();
        false
    });

    let output = sandbox.run(&["daemon", "stop"]);
    assert!(output.status.success(), "{output:?}");
    assert!(old.join().unwrap(), "the old daemon's process was stopped");
}

//! End-to-end tests for hands-on sessions (M1 step 5): a real shell on a real
//! terminal in a real worktree, driven through the protocol.

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

use std::path::PathBuf;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{
    Command, Delta, DetachReason, ErrorCode, Reply, ServerFrame, TermSize,
};
use muster_core::{SessionId, SessionStatus, WorkspaceId};

use support::{Daemon, Sandbox, expect_event, read_terminal};

/// A daemon with one project and one worktree.
struct Setup {
    _sandbox: Sandbox,
    daemon: Daemon,
    workspace: WorkspaceId,
    path: PathBuf,
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
    let snapshot = connection.subscribe().await.unwrap();
    let path = snapshot
        .workspaces
        .iter()
        .find(|found| found.id == workspace)
        .map(|found| found.kind.host_path().unwrap().to_owned())
        .unwrap();
    Setup {
        _sandbox: sandbox,
        daemon,
        workspace,
        path,
    }
}

async fn start(connection: &mut Connection, workspace: WorkspaceId) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: "shell".to_owned(),
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

/// Waits for a status change of `session` that `wanted` accepts.
async fn expect_status(
    connection: &mut Connection,
    session: SessionId,
    wanted: impl Fn(SessionStatus, bool) -> bool,
) -> (SessionStatus, bool) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            unseen,
            ..
        }) if *changed == session && wanted(*status, *unseen) => Some((*status, *unseen)),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn a_shell_session_runs_in_its_worktree() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace).await;

    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"pwd; echo here-$((2+3))\n")
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "here-5").await;
    assert!(text.contains(setup.path.to_str().unwrap()), "{text:?}");

    let snapshot = connection.subscribe().await.unwrap();
    let stored = &snapshot.sessions[0];
    assert_eq!((stored.id, stored.title.as_str()), (session, "Shell"));
    assert_eq!(stored.status, SessionStatus::Fresh);
}

#[tokio::test]
async fn output_waits_in_the_daemon_for_a_client_to_come_back() {
    let setup = setup().await;
    let mut first = setup.daemon.connect().await;
    let session = start(&mut first, setup.workspace).await;
    first.attach(session, None).await.unwrap();
    first.input(session, b"echo one-$((1*1))\n").await.unwrap();
    let (_, next) = read_terminal(&mut first, session, "one-1").await;
    drop(first);

    // Typed by another client while nobody watches.
    let mut typist = setup.daemon.connect().await;
    typist.input(session, b"echo two-$((1+1))\n").await.unwrap();

    let mut second = setup.daemon.connect().await;
    second.attach(session, Some(next)).await.unwrap();
    let (text, _) = read_terminal(&mut second, session, "two-2").await;
    assert!(!text.contains("one-1"), "only what followed: {text:?}");
}

#[tokio::test]
async fn the_terminal_size_follows_the_client() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace).await;
    connection.attach(session, None).await.unwrap();
    connection
        .resize(
            session,
            TermSize {
                cols: 100,
                rows: 30,
            },
        )
        .await
        .unwrap();
    connection.input(session, b"stty size\n").await.unwrap();
    read_terminal(&mut connection, session, "30 100").await;
}

#[tokio::test]
async fn parent_agent_variables_do_not_reach_sessions() {
    // The sandbox starts the daemon with CLAUDECODE=1.
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace).await;
    connection.attach(session, None).await.unwrap();
    connection
        .input(
            session,
            b"echo \"${CLAUDECODE:-clean}-$((1+1)) $MUSTER_SESSION\"\n",
        )
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "-2 ses_").await;
    assert!(text.contains("clean-2"), "{text:?}");
    assert!(text.contains(&session.to_string()), "{text:?}");
}

#[tokio::test]
async fn the_persons_claude_settings_reach_sessions_and_a_parent_sessions_markers_do_not() {
    let sandbox = Sandbox::new()
        .with_env("CLAUDE_CODE_USE_BEDROCK", "1")
        .with_env("CLAUDE_CODE_CHILD_SESSION", "1")
        .with_env("CLAUDE_PID", "4242");
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    let Reply::ProjectAdded { .. } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let snapshot = connection.subscribe().await.unwrap();
    let session = start(&mut connection, snapshot.workspaces[0].id).await;
    connection.attach(session, None).await.unwrap();
    connection
        .input(
            session,
            b"echo \"[${CLAUDE_CODE_USE_BEDROCK}][${CLAUDE_CODE_CHILD_SESSION}][${CLAUDE_PID}]-$((3+3))\"\n",
        )
        .await
        .unwrap();
    let (text, _) = read_terminal(&mut connection, session, "]-6").await;
    assert!(text.contains("[1][][]-6"), "{text:?}");
}

#[tokio::test]
async fn the_progress_signal_moves_the_status() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace).await;
    connection
        .input(session, b"printf '\\033]9;4;3\\007'\n")
        .await
        .unwrap();
    expect_status(&mut connection, session, |status, _| {
        status == SessionStatus::Running
    })
    .await;
    connection
        .input(session, b"printf '\\033]9;4;0\\007'\n")
        .await
        .unwrap();
    let (_, unseen) = expect_status(&mut connection, session, |status, _| {
        status == SessionStatus::Finished
    })
    .await;
    assert!(unseen);

    // Looking at it clears the mark.
    connection.attach(session, None).await.unwrap();
    expect_status(&mut connection, session, |status, unseen| {
        status == SessionStatus::Finished && !unseen
    })
    .await;
}

#[tokio::test]
async fn an_exit_is_reported_and_its_output_stays_readable() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace).await;
    connection.attach(session, None).await.unwrap();
    connection
        .input(session, b"echo last-$((4*4)); exit 3\n")
        .await
        .unwrap();

    // The status change and the end of the stream travel separately, so
    // either may arrive first.
    let (mut exited, mut detached) = (None, None);
    expect_event(&mut connection, |frame| {
        match frame {
            ServerFrame::Delta(Delta::SessionStatus {
                session: s,
                status,
                unseen,
                ..
            }) if *s == session && status.is_exited() => exited = Some((*status, *unseen)),
            ServerFrame::Detached { session: s, reason } if *s == session => {
                detached = Some(*reason);
            }
            _ => {}
        }
        (exited.is_some() && detached.is_some()).then_some(())
    })
    .await;
    // It exited in front of a client watching it, so it is not news.
    assert_eq!(
        exited,
        Some((SessionStatus::Exited { success: false }, false))
    );
    assert_eq!(detached, Some(DetachReason::Exited));

    let mut later = setup.daemon.connect().await;
    later.attach(session, None).await.unwrap();
    read_terminal(&mut later, session, "last-16").await;
    let reason = expect_event(&mut later, |frame| match frame {
        ServerFrame::Detached { session: s, reason } if *s == session => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, DetachReason::Exited);
    let snapshot = later.subscribe().await.unwrap();
    assert!(snapshot.sessions[0].ended_at.is_some());
}

#[tokio::test]
async fn a_stopped_session_is_not_marked_unseen() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start(&mut connection, setup.workspace).await;
    assert_eq!(
        connection
            .command(Command::StopSession { session })
            .await
            .unwrap(),
        Reply::Done
    );
    let (_, unseen) = expect_status(&mut connection, session, |status, _| status.is_exited()).await;
    assert!(!unseen);
    let again = connection.command(Command::StopSession { session }).await;
    assert!(matches!(again, Err(ClientError::Daemon(error)) if error.code == ErrorCode::Conflict));
}

#[tokio::test]
async fn a_worktree_with_a_running_session_is_not_removed() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    start(&mut connection, setup.workspace).await;
    let removed = connection
        .command(Command::RemoveWorktree {
            workspace: setup.workspace,
            force: true,
        })
        .await;
    assert!(
        matches!(removed, Err(ClientError::Daemon(error)) if error.code == ErrorCode::Conflict)
    );
    assert!(setup.path.exists());
}

#[tokio::test]
async fn unknown_harnesses_and_sessions_are_refused() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let started = connection
        .command(Command::StartSession {
            workspace: setup.workspace,
            harness: "nonsense".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await;
    assert!(
        matches!(started, Err(ClientError::Daemon(error)) if error.code == ErrorCode::BadRequest)
    );

    let unknown = SessionId::new();
    connection.attach(unknown, None).await.unwrap();
    let reason = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Detached { session, reason } if *session == unknown => Some(*reason),
        _ => None,
    })
    .await;
    assert_eq!(reason, DetachReason::NotRunning);
}

#[tokio::test]
async fn sessions_running_when_the_daemon_stopped_are_recorded_as_exited() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace).await;
    drop(connection);
    let Setup {
        _sandbox: sandbox,
        daemon,
        ..
    } = setup;
    assert!(daemon.stop().success());

    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    let stored = &snapshot.sessions[0];
    assert_eq!(stored.id, session);
    assert_eq!(stored.status, SessionStatus::Exited { success: false });
    assert!(stored.unseen);
    assert!(stored.ended_at.is_some());
}

#[tokio::test]
async fn input_modes_reach_the_client() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start(&mut connection, setup.workspace).await;
    connection.attach(session, None).await.unwrap();
    // A fresh shell has the default modes.
    let initial = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Modes { session: s, modes } if *s == session => Some(*modes),
        _ => None,
    })
    .await;
    assert!(!initial.bracketed_paste);

    connection
        .input(session, b"printf '\\033[?2004h\\033[?1h\\033[>1u'\n")
        .await
        .unwrap();
    let changed = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Modes { session: s, modes }
            if *s == session && modes.bracketed_paste && modes.application_cursor =>
        {
            Some(*modes)
        }
        _ => None,
    })
    .await;
    assert_eq!(changed.kitty_flags, 1);
}

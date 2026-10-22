//! End-to-end tests for the daemon's restart in place (`muster reload`):
//! the real binary replaces itself with its sessions handed over, and they
//! run on. A shell keeps what it was told and finishes the work it was
//! doing; a Claude Code session's hooks still reach the daemon with the
//! token it was started with, and it is told it is in focus, whatever the
//! daemon before told it; clients connect again and pick up where they
//! were.

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

use muster_client::Connection;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::{SessionId, SessionStatus, WorkspaceId};
use serde_json::json;

use support::{Sandbox, daemon_pid, expect_event, read_terminal};

fn said(output: &std::process::Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    )
}

async fn worktree(connection: &mut Connection, sandbox: &Sandbox) -> WorkspaceId {
    let root = sandbox.repository("app");
    let Reply::ProjectAdded { id: project } = connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap()
    else {
        panic!("expected ProjectAdded");
    };
    let Reply::WorktreeCreated { id } = connection
        .command(Command::CreateWorktree {
            project,
            branch: Some("work".to_owned()),
            on: None,
            named_for: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    id
}

async fn start(connection: &mut Connection, workspace: WorkspaceId, harness: &str) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: harness.to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap()
    {
        Reply::SessionStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn a_reload_keeps_every_session_running() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let pid = daemon.pid();
    let mut connection = daemon.connect().await;
    let workspace = worktree(&mut connection, &sandbox).await;
    let shell = start(&mut connection, workspace, "shell").await;
    let claude = start(&mut connection, workspace, "claude").await;
    let launch = sandbox.wait_for_launches(claude, 1).remove(0);
    let agent = launch.value("--session-id").unwrap().to_owned();

    // Claude Code asks to be told of focus, and waits for the 3 bytes it
    // is sent; nothing has told it yet.
    connection.attach(claude, None).await.unwrap();
    let probe = "stty -echo -icanon min 1; printf '\\033[?1004h'; echo ready-$((1+2)); \
                 r=$(dd bs=1 count=3 2>/dev/null | tr '\\033' 'E'); stty echo icanon; \
                 echo \"focus=$r=$((2*21))\"\n";
    connection.input(claude, probe.as_bytes()).await.unwrap();
    read_terminal(&mut connection, claude, "ready-3").await;

    connection.attach(shell, None).await.unwrap();
    connection
        .input(shell, b"MARK=kept-$((40+2)); echo before-$((1+1))\n")
        .await
        .unwrap();
    read_terminal(&mut connection, shell, "before-2").await;
    // Work under way as the daemon restarts.
    connection
        .input(shell, b"sleep 2; echo after-$((3*3))\n")
        .await
        .unwrap();

    let reloaded = sandbox.run(&["reload"]);
    assert!(reloaded.status.success(), "{}", said(&reloaded));
    assert!(
        said(&reloaded).contains(&format!(
            "The daemon is now muster {}",
            env!("CARGO_PKG_VERSION")
        )),
        "{}",
        said(&reloaded)
    );

    // The same process, with both sessions running.
    assert_eq!(daemon_pid(daemon.socket()), Some(pid));
    let mut connection = daemon.connect().await;
    let snapshot = connection.subscribe().await.unwrap();
    for id in [shell, claude] {
        let stored = snapshot.sessions.iter().find(|s| s.id == id).unwrap();
        assert_eq!(stored.ended_at, None, "{id} ended");
    }

    // The shell: what it wrote before, the work it was doing, and what it
    // was told.
    connection.attach(shell, None).await.unwrap();
    let (text, _) = read_terminal(&mut connection, shell, "after-9").await;
    assert!(text.contains("before-2"), "{text:?}");
    connection
        .input(shell, b"echo still-$MARK\n")
        .await
        .unwrap();
    read_terminal(&mut connection, shell, "still-kept-42").await;

    // Claude Code: told it is in focus by the daemon that took it over,
    // with no client holding its keys.
    connection.attach(claude, None).await.unwrap();
    let (text, _) = read_terminal(&mut connection, claude, "=42").await;
    assert!(text.contains("focus=E[I=42"), "{text:?}");

    // Its hooks, with the token it was started with, still move its status.
    launch.fire(
        "UserPromptSubmit",
        &json!({"session_id": agent, "hook_event_name": "UserPromptSubmit", "cwd": "/work"}),
    );
    expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session,
            status: SessionStatus::Running,
            ..
        }) if *session == claude => Some(()),
        _ => None,
    })
    .await;

    let log = daemon.log();
    assert!(log.contains("restarting in place"), "{log}");
    assert!(
        log.contains("taking over from the daemon's previous version"),
        "{log}"
    );
}

#[test]
fn reload_with_no_daemon_says_so() {
    let sandbox = Sandbox::new();
    let reloaded = sandbox.run(&["reload"]);
    assert!(reloaded.status.success());
    assert!(said(&reloaded).contains("The daemon is not running"));
}

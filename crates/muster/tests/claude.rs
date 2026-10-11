//! End-to-end tests for the Claude Code harness (M1 step 7): launch
//! arguments, hooks reaching the daemon, and resume after a restart. A
//! stand-in `claude` records how it was started; the hooks run are the
//! exact commands from the `--settings` it was given.

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![allow(clippy::unwrap_used)]

mod support;

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::{SessionId, SessionStatus, WaitReason, WorkspaceId};
use serde_json::json;

use support::{Daemon, Sandbox, expect_event};

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

async fn start_claude(
    connection: &mut Connection,
    workspace: WorkspaceId,
    prompt: Option<&str>,
) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: prompt.map(str::to_owned),
        })
        .await
        .unwrap()
    {
        Reply::SessionStarted { id } => id,
        other => panic!("unexpected {other:?}"),
    }
}

/// A payload shaped like Claude Code's, for `agent_session`.
fn payload(agent_session: &str, event: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut payload = json!({
        "session_id": agent_session,
        "hook_event_name": event,
        "cwd": "/work",
        "transcript_path": "/claude/projects/-work/x.jsonl",
    });
    if let (Some(payload), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        payload.extend(extra.clone());
    }
    payload
}

async fn expect_status(connection: &mut Connection, session: SessionId) -> (SessionStatus, bool) {
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            unseen,
        }) if *changed == session => Some((*status, *unseen)),
        _ => None,
    })
    .await
}

#[tokio::test]
async fn each_claude_session_gets_its_own_id_hooks_and_token() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let first = start_claude(&mut connection, setup.workspace, None).await;
    let second = start_claude(&mut connection, setup.workspace, None).await;

    let one = setup.sandbox.wait_for_launches(first, 1).remove(0);
    let two = setup.sandbox.wait_for_launches(second, 1).remove(0);
    let id = one.value("--session-id").expect("--session-id");
    assert_eq!(id.len(), 36);
    assert_ne!(one.value("--session-id"), two.value("--session-id"));
    assert_ne!(one.token, two.token);
    assert_eq!(one.token.len(), 64);
    assert!(one.socket.ends_with("hooks.sock"));
    for event in [
        "SessionStart",
        "UserPromptSubmit",
        "Stop",
        "PermissionRequest",
        "Notification",
        "PreToolUse",
        "PostToolUse",
        "SubagentStart",
        "SubagentStop",
    ] {
        assert!(one.hook_command(event).contains("--unix-socket"), "{event}");
    }
}

#[tokio::test]
async fn hooks_move_the_status_through_a_turn() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace, None).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra| launch.fire(event, &payload(&id, event, extra));

    fire("UserPromptSubmit", json!({ "prompt": "fix it" }));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );

    fire("PermissionRequest", json!({ "tool_name": "Bash" }));
    let needs_permission = SessionStatus::NeedsYou {
        reason: WaitReason::Permission,
    };
    assert_eq!(
        expect_status(&mut connection, session).await,
        (needs_permission, true)
    );

    fire("PostToolUse", json!({ "tool_name": "Bash" }));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );

    fire("PreToolUse", json!({ "tool_name": "AskUserQuestion" }));
    let needs_answer = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    assert_eq!(
        expect_status(&mut connection, session).await,
        (needs_answer, true)
    );

    fire("PostToolUse", json!({ "tool_name": "AskUserQuestion" }));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );

    fire("Stop", json!({ "stop_hook_active": false }));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, true)
    );
}

#[tokio::test]
async fn the_token_not_the_payload_says_whose_hook_it_is() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let first = start_claude(&mut connection, setup.workspace, None).await;
    let second = start_claude(&mut connection, setup.workspace, None).await;
    let one = setup.sandbox.wait_for_launches(first, 1).remove(0);
    let two = setup.sandbox.wait_for_launches(second, 1).remove(0);
    let two_id = two.value("--session-id").unwrap().to_owned();

    // A payload naming the second session's agent, carried by the first's
    // token, is the first session's hook; its foreign session id is ignored.
    one.fire(
        "UserPromptSubmit",
        &payload(&two_id, "UserPromptSubmit", json!({})),
    );
    // An unknown token is ignored outright.
    two.fire_with_token(
        "UserPromptSubmit",
        &payload(&two_id, "UserPromptSubmit", json!({})),
        "not-a-real-token",
    );
    // The second session's own hook moves only the second session.
    two.fire(
        "UserPromptSubmit",
        &payload(&two_id, "UserPromptSubmit", json!({})),
    );

    let (changed, status) = expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session, status, ..
        }) => Some((*session, *status)),
        _ => None,
    })
    .await;
    assert_eq!((changed, status), (second, SessionStatus::Running));
    let snapshot = connection.subscribe().await.unwrap();
    let first_now = snapshot.sessions.iter().find(|s| s.id == first).unwrap();
    assert_eq!(first_now.status, SessionStatus::Fresh);
}

#[tokio::test]
async fn a_first_prompt_is_passed_and_the_session_starts_running() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start_claude(&mut connection, setup.workspace, Some("fix the login")).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    assert_eq!(
        launch.args.last().map(String::as_str),
        Some("fix the login")
    );
    let snapshot = connection.subscribe().await.unwrap();
    assert_eq!(snapshot.sessions[0].status, SessionStatus::Running);
}

#[tokio::test]
async fn the_shell_takes_no_prompt() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let refused = connection
        .command(Command::StartSession {
            workspace: setup.workspace,
            harness: "shell".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: Some("hello".to_owned()),
        })
        .await;
    assert!(
        matches!(refused, Err(ClientError::Daemon(error)) if error.code == ErrorCode::BadRequest)
    );
}

/// Starts a Claude session, runs one turn's hooks so its id is saved for
/// resuming, and stops the daemon. Returns the session and its agent id.
async fn session_cut_short_by_a_restart(
    setup: Setup,
    transcript: bool,
) -> (Sandbox, SessionId, String) {
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace, None).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    launch.fire(
        "UserPromptSubmit",
        &payload(&id, "UserPromptSubmit", json!({})),
    );
    expect_status(&mut connection, session).await;
    if transcript {
        let slug = setup.sandbox.claude_projects().join("-work");
        std::fs::create_dir_all(&slug).unwrap();
        std::fs::write(slug.join(format!("{id}.jsonl")), "{}\n").unwrap();
    }
    drop(connection);
    let Setup {
        sandbox, daemon, ..
    } = setup;
    assert!(daemon.stop().success());
    (sandbox, session, id)
}

#[tokio::test]
async fn a_session_resumes_after_a_daemon_restart() {
    let (sandbox, session, id) = session_cut_short_by_a_restart(setup().await, true).await;
    let daemon = sandbox.start();

    let launches = sandbox.wait_for_launches(session, 2);
    assert_eq!(launches[1].value("--resume"), Some(id.as_str()));
    assert_eq!(launches[1].value("--session-id"), None);
    assert_ne!(
        launches[1].token, launches[0].token,
        "a new token per launch"
    );

    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    let stored = &snapshot.sessions[0];
    // The restart cut its turn short, so it is waiting to be looked at.
    assert_eq!(
        (stored.status, stored.unseen),
        (SessionStatus::Finished, true)
    );
    assert_eq!(stored.ended_at, None);

    // And its hooks reach the new daemon.
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
    launches[1].fire(
        "UserPromptSubmit",
        &payload(&id, "UserPromptSubmit", json!({})),
    );
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );
}

#[tokio::test]
async fn without_a_transcript_a_session_is_not_resumed() {
    let (sandbox, session, _) = session_cut_short_by_a_restart(setup().await, false).await;
    let daemon = sandbox.start();
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert_eq!(
        snapshot.sessions[0].status,
        SessionStatus::Exited { success: false }
    );
    assert_eq!(sandbox.claude_launches(session).len(), 1);
}

#[tokio::test]
async fn a_resume_that_fails_at_once_starts_fresh() {
    let (sandbox, session, id) = session_cut_short_by_a_restart(setup().await, true).await;
    std::fs::write(sandbox.fake_claude_flag("FAKE_CLAUDE_RESUME_FAILS"), "1").unwrap();
    let daemon = sandbox.start();

    let launches = sandbox.wait_for_launches(session, 3);
    assert_eq!(launches[1].value("--resume"), Some(id.as_str()));
    let fresh = launches[2].value("--session-id").expect("a fresh start");
    assert_ne!(fresh, id);

    let mut connection = daemon.connect().await;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        if snapshot.sessions[0].status == SessionStatus::Fresh {
            assert_eq!(snapshot.sessions[0].ended_at, None);
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "{:?}",
            snapshot.sessions[0]
        );
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
}

//! End-to-end tests of `muster session explain`: why a session has its
//! status, from the real daemon, with a stand-in `claude` running the
//! exact hook commands Muster gave it. Only kinds of signal and tools'
//! names come back, never a prompt or a command's arguments (`PRD-06`).

// Helpers outside `#[test]` functions are not covered by clippy.toml's test exemption.
#![expect(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "end-to-end tests: a failed step is the test failing"
)]
#![expect(
    clippy::disallowed_methods,
    reason = "tests time themselves; ARC-03 binds Muster's own code"
)]

mod support;

use muster_core::explain::OpenWait;
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::session::Origin;
use muster_core::{SessionId, SessionStatus, WaitReason};
use serde_json::json;

use muster_client::Connection;
use support::{Daemon, Sandbox, expect_event};

/// A payload shaped like Claude Code's, for `agent_session`.
#[expect(
    clippy::needless_pass_by_value,
    reason = "callers build the extra fields in place; a borrow would not outlive them"
)]
fn payload(agent_session: &str, event: &str, extra: serde_json::Value) -> serde_json::Value {
    let mut payload = json!({
        "session_id": agent_session,
        "hook_event_name": event,
        "cwd": "/work",
    });
    if let (Some(payload), Some(extra)) = (payload.as_object_mut(), extra.as_object()) {
        payload.extend(extra.clone());
    }
    payload
}

/// A Claude Code session started in a worktree of a new project.
async fn claude_session(sandbox: &Sandbox, daemon: &Daemon) -> (Connection, SessionId) {
    let root = sandbox.repository("app");
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
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
            on: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: None,
            size: TermSize::default(),
            prompt: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    (connection, session)
}

#[tokio::test]
async fn a_waiting_session_says_on_whom_for_what_and_what_it_heard() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let (mut connection, session) = claude_session(&sandbox, &daemon).await;
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra| launch.fire(event, &payload(&id, event, extra));
    fire(
        "UserPromptSubmit",
        json!({ "prompt": "words-the-person-typed", "prompt_id": "p1" }),
    );
    fire(
        "PreToolUse",
        json!({ "tool_name": "Bash", "tool_input": { "command": "TOKEN=hush-hush-value make" } }),
    );
    // Another conversation in the same folder: heard, and ignored.
    launch.fire(
        "Stop",
        &payload("someone-else", "Stop", json!({ "stop_hook_active": false })),
    );
    fire(
        "PermissionRequest",
        json!({ "tool_name": "Bash", "tool_input": { "command": "TOKEN=hush-hush-value make" } }),
    );
    let needs_permission = SessionStatus::NeedsYou {
        reason: WaitReason::Permission,
    };
    expect_event(&mut connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status,
            ..
        }) if *changed == session && *status == needs_permission => Some(()),
        _ => None,
    })
    .await;

    // ARC-07, ARC-09: a typed reply.
    let Reply::Explanation(explanation) = connection
        .command(Command::ExplainSession { session })
        .await
        .unwrap()
    else {
        panic!("expected an explanation");
    };
    assert_eq!(explanation.status, needs_permission);
    assert!(explanation.unseen);
    let machine = explanation.machine.as_ref().expect("its machine is held");
    assert_eq!(
        machine.waits,
        [OpenWait {
            origin: Origin::Main,
            reason: WaitReason::Permission,
        }]
    );
    assert!(machine.hold.is_none() && machine.next.is_none());
    let heard: Vec<(&str, Option<&str>)> = explanation
        .signals
        .iter()
        .map(|seen| (seen.what.as_str(), seen.tool.as_deref()))
        .filter(|(what, _)| *what != "hook: session started")
        .collect();
    assert_eq!(
        heard,
        [
            ("hook: prompt submitted", None),
            ("hook: tool started", Some("Bash")),
            ("hook: turn ended (ignored)", None),
            ("hook: permission requested", Some("Bash")),
        ]
    );

    // The command says the same in words, and nothing that was written.
    let output = sandbox.run(&["session", "explain", &session.to_string()]);
    assert!(output.status.success(), "{output:?}");
    let said = String::from_utf8_lossy(&output.stdout);
    assert!(
        said.starts_with(&format!("Session {session}: needs you: permission, for ")),
        "{said}"
    );
    for line in [
        "Waiting on the main agent for a permission.\n",
        "No deadline: only a signal changes it.\n",
        " ago  hook: permission requested (Bash)\n",
        " ago  hook: turn ended (ignored)\n",
    ] {
        assert!(said.contains(line), "{line:?} in {said}");
    }
    for written in ["words-the-person-typed", "hush-hush-value"] {
        assert!(!said.contains(written), "PRD-06: {said}");
        assert!(!daemon.log().contains(written), "ARC-20: never in the log");
    }
}

#[tokio::test]
async fn a_session_the_daemon_does_not_know_is_refused_by_its_code() {
    let sandbox = Sandbox::new();
    let daemon = sandbox.start();
    let refused = daemon
        .connect()
        .await
        .command(Command::ExplainSession {
            session: SessionId::new(),
        })
        .await
        .unwrap_err();
    let muster_client::ClientError::Daemon(body) = refused else {
        panic!("expected a refusal, got {refused:?}");
    };
    assert_eq!(body.code, ErrorCode::NotFound, "ARC-07");
    let output = sandbox.run(&["session", "explain", &SessionId::new().to_string()]);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no session"),
        "{output:?}"
    );
}

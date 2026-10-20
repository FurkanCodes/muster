//! End-to-end tests of a Claude Code session's waits on the person beyond
//! a tool's own events: a permission the person declines, which Claude
//! Code tells only in its transcript, and an MCP server's form for the
//! person; and of a turn's wait for its subagents, one of which the person
//! stops, which too only the transcript tells. A stand-in `claude` runs the
//! exact hook commands Muster gave it.

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

use std::time::Duration;

use muster_client::Connection;
use muster_core::protocol::{Command, Delta, Reply, ServerFrame, TermSize};
use muster_core::{SessionId, SessionStatus, WaitReason, WorkspaceId};
use serde_json::json;

use support::{Daemon, Sandbox, expect_event};

struct Setup {
    sandbox: Sandbox,
    daemon: Daemon,
    workspace: WorkspaceId,
}

async fn setup() -> Setup {
    // The transcript is read often, so a test need not wait long.
    let sandbox = Sandbox::new().with_env("MUSTER_SPEND_MS", "100");
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
            on: None,
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

async fn start_claude(connection: &mut Connection, workspace: WorkspaceId) -> SessionId {
    match connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
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
            ..
        }) if *changed == session => Some((*status, *unseen)),
        _ => None,
    })
    .await
}

/// What Claude Code 2.1.296 writes to its transcript as the person
/// declines a permission in the turn `prompt` began: the tool's result,
/// then the marker, then the turn's length. Content shortened.
fn declined(prompt: &str) -> String {
    [
        json!({
            "type": "user", "isSidechain": false, "promptId": prompt,
            "toolUseResult": "User rejected tool use", "toolDenialKind": "user-rejected",
            "message": {"role": "user", "content": [{
                "type": "tool_result", "is_error": true, "tool_use_id": "toolu_1",
                "content": "The user doesn't want to proceed with this tool use."
            }]}
        }),
        json!({
            "type": "user", "isSidechain": false, "promptId": prompt,
            "message": {"role": "user", "content": [
                {"type": "text", "text": "[Request interrupted by user for tool use]"}
            ]}
        }),
        json!({"type": "system", "subtype": "turn_duration", "durationMs": 1477}),
    ]
    .map(|line| line.to_string() + "\n")
    .concat()
}

/// What Claude Code 2.1.304 writes to its transcript as background task
/// `task` stops: the notice it queues for the main agent. Content
/// shortened.
fn task_stopped(task: &str, status: &str) -> String {
    json!({
        "type": "queue-operation", "operation": "enqueue",
        "timestamp": "2026-10-20T14:14:14.985Z",
        "content": format!(
            "<task-notification>\n<task-id>{task}</task-id>\n<status>{status}</status>\n\
             <summary>Agent \"Look around\" was stopped by user</summary>\n</task-notification>"
        ),
    })
    .to_string()
        + "\n"
}

/// Waits until the daemon takes `session`'s subagents at work to be
/// `wanted`.
async fn expect_subagents(connection: &mut Connection, session: SessionId, wanted: &[&str]) {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
    loop {
        let Reply::Explanation(explanation) = connection
            .command(Command::ExplainSession { session })
            .await
            .unwrap()
        else {
            panic!("expected an explanation");
        };
        let machine = explanation.machine.expect("its machine is held");
        if machine.subagents == wanted {
            return;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "subagents at work: {:?}",
            machine.subagents
        );
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

fn append(path: &std::path::Path, text: &str) {
    use std::io::Write;
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .unwrap()
        .write_all(text.as_bytes())
        .unwrap();
}

#[tokio::test]
async fn a_permission_the_person_declines_needs_them_no_more() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra| launch.fire(event, &payload(&id, event, extra));
    // An earlier turn the person cut short is in the transcript already.
    let slug = setup.sandbox.claude_projects().join("-work");
    std::fs::create_dir_all(&slug).unwrap();
    let transcript = slug.join(format!("{id}.jsonl"));
    append(&transcript, &declined("p0"));

    fire(
        "UserPromptSubmit",
        json!({ "prompt": "tidy up", "prompt_id": "p1" }),
    );
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );
    fire(
        "PermissionRequest",
        json!({ "tool_name": "Bash", "prompt_id": "p1" }),
    );
    let needs_permission = SessionStatus::NeedsYou {
        reason: WaitReason::Permission,
    };
    assert_eq!(
        expect_status(&mut connection, session).await,
        (needs_permission, true)
    );
    // The earlier turn's interruption is no word about this one.
    tokio::time::sleep(Duration::from_millis(600)).await;
    let snapshot = setup.daemon.connect().await.subscribe().await.unwrap();
    assert_eq!(snapshot.sessions[0].status, needs_permission);

    // The person presses Esc at the dialog: Claude Code runs no hook, and
    // writes the turn's end to its transcript.
    append(&transcript, &declined("p1"));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false),
        "it waits on nobody, and the person, who ended it, has seen it"
    );
}

#[tokio::test]
async fn an_mcp_server_s_form_needs_the_person_until_they_respond() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra| launch.fire(event, &payload(&id, event, extra));
    fire("UserPromptSubmit", json!({ "prompt": "file it" }));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );
    fire("PreToolUse", json!({ "tool_name": "mcp__tracker__create" }));
    fire(
        "Elicitation",
        json!({ "mcp_server_name": "tracker", "message": "Which project?", "mode": "form" }),
    );
    let needs_answer = SessionStatus::NeedsYou {
        reason: WaitReason::Question,
    };
    assert_eq!(
        expect_status(&mut connection, session).await,
        (needs_answer, true)
    );
    // Its notice is the same question; the person's response answers it.
    fire(
        "Notification",
        json!({ "notification_type": "elicitation_dialog", "message": "Claude Code needs your input" }),
    );
    fire(
        "ElicitationResult",
        json!({ "mcp_server_name": "tracker", "action": "cancel", "mode": "form" }),
    );
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );
    fire(
        "Notification",
        json!({ "notification_type": "elicitation_response", "message": "cancel" }),
    );
    fire(
        "PostToolUse",
        json!({ "tool_name": "mcp__tracker__create" }),
    );
    fire("Stop", json!({ "stop_hook_active": false }));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, true),
        "nothing else opened on the way"
    );
}

#[tokio::test]
async fn a_subagent_the_person_stops_holds_no_turn() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra| launch.fire(event, &payload(&id, event, extra));
    let slug = setup.sandbox.claude_projects().join("-work");
    std::fs::create_dir_all(&slug).unwrap();
    let transcript = slug.join(format!("{id}.jsonl"));

    fire(
        "UserPromptSubmit",
        json!({ "prompt": "look around", "prompt_id": "p1" }),
    );
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );
    fire(
        "SubagentStart",
        json!({ "agent_id": "a1", "agent_type": "fork", "prompt_id": "p1" }),
    );
    expect_subagents(&mut connection, session, &["a1"]).await;

    // The person stops the turn, and the subagent working in the
    // background with it: Claude Code runs no hook for either, and writes
    // both to its transcript, after a shell of its own that ended.
    append(
        &transcript,
        &[
            task_stopped("b1", "completed"),
            task_stopped("a1", "killed"),
            declined("p1"),
        ]
        .concat(),
    );
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false),
        "held for no subagent, and the person, who ended it, has seen it"
    );
    expect_subagents(&mut connection, session, &[]).await;

    // The next turn holds for it no more either.
    fire(
        "UserPromptSubmit",
        json!({ "prompt": "go on", "prompt_id": "p2" }),
    );
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

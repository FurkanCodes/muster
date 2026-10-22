//! End-to-end tests for bringing agents back after a daemon restart
//! (`PRD-09`): one conversation is resumed in one session only, and agents
//! start one at a time. The stand-in `claude` records each start; its hooks
//! are the exact commands from the `--settings` it was given.

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

use std::time::{Duration, Instant};

use muster_client::{ClientError, Connection};
use muster_core::protocol::{Command, Delta, ErrorCode, Reply, ServerFrame, TermSize};
use muster_core::{Session, SessionId, SessionStatus, WorkspaceId};
use serde_json::json;

use support::{Daemon, Sandbox, expect_event};

/// A daemon with a project and one worktree in it.
async fn setup() -> (Sandbox, Daemon, WorkspaceId) {
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
            on: None,
            named_for: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected WorktreeCreated");
    };
    (sandbox, daemon, workspace)
}

/// A payload shaped like Claude Code's, for `agent_session`.
fn payload(agent_session: &str, event: &str) -> serde_json::Value {
    json!({
        "session_id": agent_session,
        "hook_event_name": event,
        "cwd": "/work",
        "transcript_path": "/claude/projects/-work/x.jsonl",
    })
}

/// Starts a Claude Code session called `title` and runs a turn on the
/// conversation `on`, or on its own where `on` is `None`, so that
/// conversation is saved for resuming and its transcript is there. Returns
/// the session and the conversation.
async fn turn_in(
    sandbox: &Sandbox,
    connection: &mut Connection,
    workspace: WorkspaceId,
    title: &str,
    on: Option<&str>,
) -> (SessionId, String) {
    let Reply::SessionStarted { id: session } = connection
        .command(Command::StartSession {
            workspace,
            harness: "claude".to_owned(),
            title: Some(title.to_owned()),
            size: TermSize::default(),
            prompt: None,
            started_by: None,
        })
        .await
        .unwrap()
    else {
        panic!("expected SessionStarted");
    };
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let own = launch.value("--session-id").unwrap().to_owned();
    let conversation = on.map_or(own, str::to_owned);
    if on.is_some() {
        // As `/resume` in Claude Code does: the same program goes on with
        // another conversation, which it announces.
        launch.fire("SessionStart", &payload(&conversation, "SessionStart"));
    }
    launch.fire(
        "UserPromptSubmit",
        &payload(&conversation, "UserPromptSubmit"),
    );
    expect_event(connection, |frame| match frame {
        ServerFrame::Delta(Delta::SessionStatus {
            session: changed,
            status: SessionStatus::Running,
            ..
        }) if *changed == session => Some(()),
        _ => None,
    })
    .await;
    let slug = sandbox.claude_projects().join("-work");
    std::fs::create_dir_all(&slug).unwrap();
    std::fs::write(slug.join(format!("{conversation}.jsonl")), "{}\n").unwrap();
    (session, conversation)
}

/// Waits until `session`'s stored state satisfies `done`.
async fn wait_for(
    connection: &mut Connection,
    session: SessionId,
    done: impl Fn(&Session) -> bool,
) -> Session {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let snapshot = connection.subscribe().await.unwrap();
        if let Some(found) = snapshot.sessions.iter().find(|s| s.id == session)
            && done(found)
        {
            return found.clone();
        }
        assert!(Instant::now() < deadline, "{:?}", snapshot.sessions);
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn after_a_restart_one_conversation_is_resumed_in_one_session_only_prd_09() {
    let (sandbox, daemon, workspace) = setup().await;
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
    let (first, conversation) = turn_in(&sandbox, &mut connection, workspace, "First", None).await;
    let (second, taken) = turn_in(
        &sandbox,
        &mut connection,
        workspace,
        "Second",
        Some(&conversation),
    )
    .await;
    assert_eq!(taken, conversation);
    drop(connection);
    assert!(daemon.stop().success());

    let daemon = sandbox.start();
    let launches = sandbox.wait_for_launches(first, 2);
    assert_eq!(launches[1].value("--resume"), Some(conversation.as_str()));

    // The later one is not resumed, and says why.
    let mut connection = daemon.connect().await;
    let stored = wait_for(&mut connection, second, |s| s.ended_at.is_some()).await;
    assert_eq!(stored.status, SessionStatus::Exited { success: false });
    assert!(stored.unseen);
    assert_eq!(
        stored.activity.map(|activity| activity.text).as_deref(),
        Some("not resumed: another has its conversation")
    );
    assert_eq!(sandbox.claude_launches(second).len(), 1);

    // Nor is it, started again by hand, while the first one has it.
    let refused = connection
        .command(Command::RestartSession {
            session: second,
            size: TermSize::default(),
        })
        .await;
    let Err(ClientError::Daemon(error)) = refused else {
        panic!("expected a refusal, got {refused:?}");
    };
    assert_eq!(error.code, ErrorCode::Conflict);
    assert!(error.message.contains("First"), "{}", error.message);
    assert_eq!(sandbox.claude_launches(second).len(), 1);
}

/// When the daemon logged that each of `sessions` resumed, in seconds of
/// the day, as its log says.
fn resumed_at(log: &str, sessions: &[SessionId]) -> Vec<f64> {
    let plain = strip_colours(log);
    sessions
        .iter()
        .map(|session| {
            let wanted = format!("session={session}");
            let line = plain
                .lines()
                .find(|line| line.contains("recovery: resumed") && line.contains(&wanted))
                .unwrap_or_else(|| panic!("{session} did not resume:\n{plain}"));
            // `2026-10-18T14:22:33.123456Z  INFO …`
            let stamp = line.split_whitespace().next().unwrap();
            let time = stamp.split('T').nth(1).unwrap().trim_end_matches('Z');
            let mut parts = time.split(':').map(|part| part.parse::<f64>().unwrap());
            let (hours, minutes, seconds) = (
                parts.next().unwrap(),
                parts.next().unwrap(),
                parts.next().unwrap(),
            );
            hours * 3600.0 + minutes * 60.0 + seconds
        })
        .collect()
}

/// `text` without its terminal colours.
fn strip_colours(text: &str) -> String {
    let mut plain = String::with_capacity(text.len());
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            for c in chars.by_ref() {
                if c == 'm' {
                    break;
                }
            }
        } else {
            plain.push(c);
        }
    }
    plain
}

#[tokio::test]
async fn after_a_restart_agents_resume_one_at_a_time_prd_09() {
    let (sandbox, daemon, workspace) = setup().await;
    let mut connection = daemon.connect().await;
    connection.subscribe().await.unwrap();
    let mut sessions = Vec::new();
    for title in ["One", "Two", "Three"] {
        sessions.push(turn_in(&sandbox, &mut connection, workspace, title, None).await);
    }
    drop(connection);
    assert!(daemon.stop().success());

    let daemon = sandbox.start();
    for (session, conversation) in &sessions {
        let launches = sandbox.wait_for_launches(*session, 2);
        assert_eq!(launches[1].value("--resume"), Some(conversation.as_str()));
    }
    let ids: Vec<SessionId> = sessions.iter().map(|(session, _)| *session).collect();
    let mut times = resumed_at(&daemon.log(), &ids);
    times.sort_by(f64::total_cmp);
    // Each about 150 ms after the one before, never in the same instant.
    for pair in times.windows(2) {
        assert!(pair[1] - pair[0] >= 0.14, "resumed at {times:?}");
    }
}

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

#[tokio::test]
async fn opening_a_claude_session_that_suspended_itself_continues_it() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    let session = start_claude(&mut connection, setup.workspace, None).await;
    setup.sandbox.wait_for_launches(session, 1);
    connection.attach(session, None).await.unwrap();
    // The stand-in is a shell: make it hand the terminal back the way
    // Claude Code does on Ctrl+Z, then wait for a continue.
    let suspend = "trap 'echo got-cont-$((2+5))' CONT; printf '\\033[?2004h\\033[?2004l'; \
                   echo handed-$((3*3)); while :; do sleep 0.1; done\n";
    connection.input(session, suspend.as_bytes()).await.unwrap();
    support::read_terminal(&mut connection, session, "handed-9").await;
    connection.detach(session).await.unwrap();

    // Opening it again continues it.
    let mut viewer = setup.daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    support::read_terminal(&mut viewer, session, "got-cont-7").await;
}

#[tokio::test]
async fn nothing_is_news_while_someone_watches_the_session() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace, None).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str, extra| launch.fire(event, &payload(&id, event, extra));

    let mut viewer = setup.daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    viewer.command(Command::Ping).await.unwrap();

    fire("UserPromptSubmit", json!({}));
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
        (needs_permission, false),
        "a prompt shown to someone watching is still a wait, but not news"
    );
    fire("PostToolUse", json!({ "tool_name": "Bash" }));
    expect_status(&mut connection, session).await;
    fire("Stop", json!({}));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false)
    );

    // Once nobody watches, the next finish is news again.
    viewer.detach(session).await.unwrap();
    viewer.command(Command::Ping).await.unwrap();
    fire("UserPromptSubmit", json!({}));
    expect_status(&mut connection, session).await;
    fire("Stop", json!({}));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, true)
    );

    // Watching from a client that then disconnects counts until it does.
    let mut viewer = setup.daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false)
    );
    drop(viewer);
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    fire("UserPromptSubmit", json!({}));
    expect_status(&mut connection, session).await;
    fire("Stop", json!({}));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, true)
    );
}

#[tokio::test]
async fn a_session_that_exits_while_watched_is_not_news() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace, None).await;
    setup.sandbox.wait_for_launches(session, 1);
    let mut viewer = setup.daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    viewer.input(session, b"exit\n").await.unwrap();
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Exited { success: true }, false)
    );
}

#[tokio::test]
async fn a_watching_client_follows_the_session_to_its_new_program() {
    let (sandbox, session, _) = session_cut_short_by_a_restart(setup().await, true).await;
    std::fs::write(
        sandbox.fake_claude_flag("FAKE_CLAUDE_RESUME_FAILS_LATE"),
        "1",
    )
    .unwrap();
    let daemon = sandbox.start();
    sandbox.wait_for_launches(session, 2);

    // Watching while the resume fails: the stream ends with the old
    // program, then starts over with the new one on a clean screen.
    let mut viewer = daemon.connect().await;
    viewer.attach(session, None).await.unwrap();
    let (_, old_end) = support::read_terminal(&mut viewer, session, "resume-failing-42").await;
    let (seq, reset) = expect_event(&mut viewer, |frame| match frame {
        ServerFrame::Replay {
            session: s,
            seq,
            reset,
            ..
        } if *s == session => Some((*seq, *reset)),
        _ => None,
    })
    .await;
    assert!(reset, "a new program starts on a clean screen");
    assert!(seq > old_end, "its output is numbered after the old one's");
    sandbox.wait_for_launches(session, 3);

    // Keystrokes reach the new program.
    viewer
        .input(session, b"echo rebound-$((6*7))\n")
        .await
        .unwrap();
    support::read_terminal(&mut viewer, session, "rebound-42").await;

    // The fresh start was watched, so it is not news.
    let snapshot = daemon.connect().await.subscribe().await.unwrap();
    assert_eq!(
        (snapshot.sessions[0].status, snapshot.sessions[0].unseen),
        (SessionStatus::Fresh, false)
    );
}

#[tokio::test]
async fn a_client_looking_away_stops_watching_until_it_looks_back() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace, None).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str| launch.fire(event, &payload(&id, event, json!({})));

    let mut viewer = setup.daemon.connect().await;
    // Watching before attaching does nothing.
    viewer.watch(session, true).await.unwrap();
    viewer.attach(session, None).await.unwrap();
    // Its window lost focus.
    viewer.watch(session, false).await.unwrap();
    viewer.command(Command::Ping).await.unwrap();
    fire("UserPromptSubmit");
    expect_status(&mut connection, session).await;
    fire("Stop");
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, true),
        "finished while nobody looked"
    );

    // Looking back sees it, and watches again.
    viewer.watch(session, true).await.unwrap();
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false)
    );
    viewer.watch(session, true).await.unwrap();
    viewer.command(Command::Ping).await.unwrap();
    fire("UserPromptSubmit");
    expect_status(&mut connection, session).await;
    fire("Stop");
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false)
    );

    // Looking away and detaching ends the watch once, not twice.
    viewer.watch(session, false).await.unwrap();
    viewer.detach(session).await.unwrap();
    let mut other = setup.daemon.connect().await;
    other.attach(session, None).await.unwrap();
    other.command(Command::Ping).await.unwrap();
    fire("UserPromptSubmit");
    expect_status(&mut connection, session).await;
    fire("Stop");
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false),
        "the other client still watches"
    );
}

#[tokio::test]
async fn a_claude_that_never_announces_its_start_is_asking_before_it_starts() {
    let sandbox = Sandbox::new().with_env("MUSTER_START_GRACE_MS", "300");
    std::fs::write(sandbox.fake_claude_flag("FAKE_CLAUDE_SILENT"), "1").unwrap();
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
    let snapshot = connection.subscribe().await.unwrap();
    let checkout = snapshot
        .workspaces
        .iter()
        .find(|workspace| workspace.project == project)
        .unwrap()
        .id;

    // Given a first prompt but silent, as at Claude Code's trust question.
    let session = start_claude(&mut connection, checkout, Some("fix it")).await;
    let launch = sandbox.wait_for_launches(session, 1).remove(0);
    let starting = SessionStatus::NeedsYou {
        reason: WaitReason::Startup,
    };
    assert_eq!(
        expect_status(&mut connection, session).await,
        (starting, true)
    );

    // Answered: it announces its start and gets on with the prompt.
    let id = launch.value("--session-id").unwrap().to_owned();
    launch.fire("SessionStart", &payload(&id, "SessionStart", json!({})));
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Running, false)
    );
}

#[tokio::test]
async fn a_claude_that_announces_its_start_is_never_taken_to_be_asking() {
    // Long enough for the stand-in to announce itself under a loaded test run.
    let sandbox = Sandbox::new().with_env("MUSTER_START_GRACE_MS", "3000");
    let root = sandbox.repository("app");
    let daemon = sandbox.start();
    let mut connection = daemon.connect().await;
    connection
        .command(Command::AddProject { root, name: None })
        .await
        .unwrap();
    let snapshot = connection.subscribe().await.unwrap();
    let checkout = snapshot.workspaces[0].id;
    let session = start_claude(&mut connection, checkout, None).await;
    sandbox.wait_for_launches(session, 1);
    tokio::time::sleep(std::time::Duration::from_millis(3500)).await;
    let snapshot = connection.subscribe().await.unwrap();
    let stored = snapshot.sessions.iter().find(|s| s.id == session).unwrap();
    assert_eq!(
        (stored.status, stored.unseen),
        (SessionStatus::Fresh, false)
    );
}

#[tokio::test]
async fn attaching_without_watching_leaves_the_news_for_later() {
    let setup = setup().await;
    let mut connection = setup.daemon.connect().await;
    connection.subscribe().await.unwrap();
    let session = start_claude(&mut connection, setup.workspace, None).await;
    let launch = setup.sandbox.wait_for_launches(session, 1).remove(0);
    let id = launch.value("--session-id").unwrap().to_owned();
    let fire = |event: &str| launch.fire(event, &payload(&id, event, json!({})));
    fire("UserPromptSubmit");
    expect_status(&mut connection, session).await;
    fire("Stop");
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, true)
    );

    // A client attaching while its window is out of focus does not see it.
    let mut viewer = setup.daemon.connect().await;
    viewer.attach_unwatched(session, None).await.unwrap();
    viewer.command(Command::Ping).await.unwrap();
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    let snapshot = connection.subscribe().await.unwrap();
    assert!(
        snapshot
            .sessions
            .iter()
            .any(|s| s.id == session && s.unseen)
    );
    // Until it looks.
    viewer.watch(session, true).await.unwrap();
    assert_eq!(
        expect_status(&mut connection, session).await,
        (SessionStatus::Finished, false)
    );
}
